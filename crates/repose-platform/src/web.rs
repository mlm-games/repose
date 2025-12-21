//! Web runner (wasm32) using winit + wgpu.
//!
//! Note: This runner currently clears the screen using `frame.scene.clear_color`.
//!
//! Need to make `repose-render-wgpu` support async init on wasm (due to Web-sync api)

use crate::*;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::{ImePurpose, Window};

use repose_core::locals::dp_to_px;
use repose_ui::TextFieldState;
use repose_ui::textfield::{TF_FONT_DP, TF_PADDING_X_DP, index_for_x_bytes, measure_text};

#[wasm_bindgen]
pub struct WebOptions {
    canvas_id: Option<String>,
    antialiasing: bool,
}

#[wasm_bindgen]
impl WebOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: Option<String>) -> Self {
        Self {
            canvas_id,
            antialiasing: true,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn antialiasing(&self) -> bool {
        self.antialiasing
    }

    /// Returns a cloned canvas_id (Option<String>) for JS callers.
    #[wasm_bindgen(getter)]
    pub fn canvas_id(&self) -> Option<String> {
        self.canvas_id.clone()
    }
}

/// JS-friendly entrypoint. Uses a placeholder root view.
/// Prefer calling `run_web_app(root, options)` from a Rust wasm crate.
#[wasm_bindgen]
pub fn run_app(options: WebOptions) -> Result<(), JsValue> {
    run_web_app(
        |_sched| repose_core::View::new(0, repose_core::ViewKind::Surface),
        options,
    )
}

/// Rust-friendly entrypoint that accepts your actual root closure.
pub fn run_web_app(
    root: impl FnMut(&mut Scheduler) -> View + 'static,
    options: WebOptions,
) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Debug);

    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = EventLoop::new().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

    let mut app = App::new(Box::new(root), options);
    event_loop.spawn_app(app);

    Ok(())
}

struct App {
    // user code
    root: Box<dyn FnMut(&mut Scheduler) -> View>,
    options: WebOptions,

    // window + gpu
    window: Option<Arc<Window>>,
    gpu: Rc<RefCell<Option<WgpuState>>>,

    // runtime state
    sched: Scheduler,
    frame_cache: Option<Frame>,

    // pointer + focus
    mouse_pos_px: (f32, f32),
    modifiers: Modifiers,
    hover_id: Option<u64>,
    capture_id: Option<u64>,
    pressed_ids: HashSet<u64>,
    key_pressed_active: Option<u64>,

    // text input
    ime_preedit: bool,
    textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
    prev_touch_px: Option<(f32, f32)>,
}

impl App {
    fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>, options: WebOptions) -> Self {
        Self {
            root,
            options,
            window: None,
            gpu: Rc::new(RefCell::new(None)),
            sched: Scheduler::new(),
            frame_cache: None,

            mouse_pos_px: (0.0, 0.0),
            modifiers: Modifiers::default(),
            hover_id: None,
            capture_id: None,
            pressed_ids: HashSet::new(),
            key_pressed_active: None,

            ime_preedit: false,
            textfield_states: HashMap::new(),
            prev_touch_px: None,
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn tf_key_of(&self, visual_id: u64) -> u64 {
        if let Some(f) = &self.frame_cache
            && let Some(hr) = f.hit_regions.iter().find(|h| h.id == visual_id)
        {
            return hr.tf_state_key.unwrap_or(hr.id);
        }
        visual_id
    }

    fn notify_text_change(&self, id: u64, text: String) {
        if let Some(f) = &self.frame_cache
            && let Some(h) = f.hit_regions.iter().find(|h| h.id == id)
            && let Some(cb) = &h.on_text_change
        {
            cb(text);
        }
    }

    fn tf_ensure_caret_visible_in_hit(&self, state: &mut TextFieldState, hit_rect: Rect) {
        let font_dp = TF_FONT_DP as u32;
        let m = measure_text(&state.text, font_dp);
        let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
        state.ensure_caret_visible(caret_x_px, hit_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP));
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        // Build window attributes
        let mut attrs = Window::default_attributes()
            .with_title("Repose (Web)")
            .with_inner_size(PhysicalSize::new(1280u32, 800u32))
            // Web-only:
            .with_prevent_default(true)
            .with_focusable(true);

        // Attach to existing canvas (if provided), else create one and append.
        if let Some(id) = self.options.canvas_id.clone() {
            let document = web_sys::window()
                .and_then(|w| w.document())
                .ok_or_else(|| JsValue::from_str("No document"))
                .unwrap();

            let canvas = document
                .get_element_by_id(&id)
                .ok_or_else(|| JsValue::from_str(&format!("Canvas id '{id}' not found")))
                .unwrap()
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .map_err(|_| JsValue::from_str("Element is not a canvas"))
                .unwrap();

            attrs = attrs.with_canvas(Some(canvas)).with_append(false);
        } else {
            attrs = attrs.with_canvas(None).with_append(true);
        }

        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("create_window failed: {e:?}");
                return;
            }
        };

        // Focus canvas for keyboard input
        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
        }

        // Initial size
        let size = window.inner_size();
        self.sched.size = (size.width, size.height);

        self.window = Some(window.clone());

        // Async init wgpu
        let gpu_cell = self.gpu.clone();
        spawn_local(async move {
            match WgpuState::new(window.clone()).await {
                Ok(mut gpu) => {
                    let size = window.inner_size();
                    gpu.resize(size);
                    *gpu_cell.borrow_mut() = Some(gpu);
                    window.request_redraw();
                    log::info!("WGPU initialized");
                }
                Err(e) => {
                    log::error!("WGPU init failed: {e}");
                }
            }
        });
    }

    fn window_event(
        &mut self,
        el: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                // On web this usually won't happen, but keep parity.
                el.exit();
            }

            WindowEvent::Resized(size) => {
                self.sched.size = (size.width, size.height);
                if let Some(gpu) = self.gpu.borrow_mut().as_mut() {
                    gpu.resize(size);
                }
                self.request_redraw();
            }

            WindowEvent::ScaleFactorChanged {
                scale_factor: _, ..
            } => {
                // winit handles canvas scaling; just redraw.
                self.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos_px = (position.x as f32, position.y as f32);

                // Pointer routing (hover + move/capture), matching desktop semantics.
                if let Some(f) = &self.frame_cache {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };
                    let top = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos));
                    let new_hover = top.map(|h| h.id);

                    if new_hover != self.hover_id {
                        // leave old
                        if let Some(prev_id) = self.hover_id
                            && let Some(prev) = f.hit_regions.iter().find(|h| h.id == prev_id)
                            && let Some(cb) = &prev.on_pointer_leave
                        {
                            cb(repose_core::input::PointerEvent {
                                id: repose_core::input::PointerId(0),
                                kind: repose_core::input::PointerKind::Mouse,
                                event: repose_core::input::PointerEventKind::Leave,
                                position: pos,
                                pressure: 1.0,
                                modifiers: self.modifiers,
                            });
                        }
                        // enter new
                        if let Some(h) = top
                            && let Some(cb) = &h.on_pointer_enter
                        {
                            cb(repose_core::input::PointerEvent {
                                id: repose_core::input::PointerId(0),
                                kind: repose_core::input::PointerKind::Mouse,
                                event: repose_core::input::PointerEventKind::Enter,
                                position: pos,
                                pressure: 1.0,
                                modifiers: self.modifiers,
                            });
                        }

                        self.hover_id = new_hover;
                    }

                    // Move
                    let pe = repose_core::input::PointerEvent {
                        id: repose_core::input::PointerId(0),
                        kind: repose_core::input::PointerKind::Mouse,
                        event: repose_core::input::PointerEventKind::Move,
                        position: pos,
                        pressure: 1.0,
                        modifiers: self.modifiers,
                    };

                    if let Some(cid) = self.capture_id {
                        if let Some(h) = f.hit_regions.iter().find(|h| h.id == cid)
                            && let Some(cb) = &h.on_pointer_move
                        {
                            cb(pe);
                        }
                    } else if let Some(h) = top
                        && let Some(cb) = &h.on_pointer_move
                    {
                        cb(pe);
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let (dx_px, dy_px) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        let unit_px = dp_to_px(60.0);
                        (-(x * unit_px), -(y * unit_px))
                    }
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };

                if let Some(f) = &self.frame_cache {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };
                    for hit in f.hit_regions.iter().rev().filter(|h| h.rect.contains(pos)) {
                        if let Some(cb) = &hit.on_scroll {
                            let before = Vec2 { x: dx_px, y: dy_px };
                            let leftover = cb(before);
                            let consumed_x = (before.x - leftover.x).abs() > 0.001;
                            let consumed_y = (before.y - leftover.y).abs() > 0.001;
                            if consumed_x || consumed_y {
                                self.request_redraw();
                                break;
                            }
                        }
                    }
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if let Some(f) = &self.frame_cache {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };

                    match state {
                        ElementState::Pressed => {
                            if let Some(hit) =
                                f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                            {
                                self.capture_id = Some(hit.id);
                                self.pressed_ids.insert(hit.id);

                                // Focus & IME for focusables
                                if hit.focusable {
                                    self.sched.focused = Some(hit.id);
                                    let key = self.tf_key_of(hit.id);
                                    self.textfield_states.entry(key).or_insert_with(|| {
                                        Rc::new(RefCell::new(TextFieldState::new()))
                                    });

                                    window.set_ime_allowed(true);
                                    window.set_ime_purpose(ImePurpose::Normal);
                                    // Cursor area positioning on web is best-effort; keep it simple.
                                }

                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(repose_core::input::PointerEvent {
                                        id: repose_core::input::PointerId(0),
                                        kind: repose_core::input::PointerKind::Mouse,
                                        event: repose_core::input::PointerEventKind::Down(
                                            repose_core::input::PointerButton::Primary,
                                        ),
                                        position: pos,
                                        pressure: 1.0,
                                        modifiers: self.modifiers,
                                    });
                                }

                                // TextField caret placement
                                if f.semantics_nodes
                                    .iter()
                                    .any(|n| n.id == hit.id && n.role == Role::TextField)
                                {
                                    let key = self.tf_key_of(hit.id);
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        let mut state = state_rc.borrow_mut();
                                        let inner_x_px = hit.rect.x + dp_to_px(TF_PADDING_X_DP);
                                        let content_x_px =
                                            self.mouse_pos_px.0 - inner_x_px + state.scroll_offset;
                                        let idx = index_for_x_bytes(
                                            &state.text,
                                            TF_FONT_DP as u32,
                                            content_x_px.max(0.0),
                                        );
                                        state.begin_drag(idx, self.modifiers.shift);
                                        self.tf_ensure_caret_visible_in_hit(&mut state, hit.rect);
                                    }
                                }
                            } else {
                                // click outside: drop focus
                                self.sched.focused = None;
                                window.set_ime_allowed(false);
                            }
                            self.request_redraw();
                        }

                        ElementState::Released => {
                            if let Some(cid) = self.capture_id {
                                self.pressed_ids.remove(&cid);

                                // click-on-release if still inside
                                if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                                    && hit.rect.contains(pos)
                                    && let Some(cb) = &hit.on_click
                                {
                                    cb();
                                }

                                // end textfield drag
                                if f.semantics_nodes
                                    .iter()
                                    .any(|n| n.id == cid && n.role == Role::TextField)
                                {
                                    let key = self.tf_key_of(cid);
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        state_rc.borrow_mut().end_drag();
                                    }
                                }
                            }

                            self.capture_id = None;
                            self.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::Touch(t) => {
                // Touch -> similar mapping as Android runner.
                let pos_px = (t.location.x as f32, t.location.y as f32);
                self.mouse_pos_px = pos_px;

                let pos = Vec2 {
                    x: pos_px.0,
                    y: pos_px.1,
                };

                match t.phase {
                    TouchPhase::Started => {
                        if let Some(f) = &self.frame_cache {
                            if let Some(hit) =
                                f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                            {
                                self.capture_id = Some(hit.id);
                                self.pressed_ids.insert(hit.id);

                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(repose_core::input::PointerEvent {
                                        id: repose_core::input::PointerId(0),
                                        kind: repose_core::input::PointerKind::Touch,
                                        event: repose_core::input::PointerEventKind::Down(
                                            repose_core::input::PointerButton::Primary,
                                        ),
                                        position: pos,
                                        pressure: 1.0,
                                        modifiers: self.modifiers,
                                    });
                                }

                                // focus textfields
                                if f.semantics_nodes
                                    .iter()
                                    .any(|n| n.id == hit.id && n.role == Role::TextField)
                                {
                                    self.sched.focused = Some(hit.id);
                                    let key = self.tf_key_of(hit.id);
                                    self.textfield_states.entry(key).or_insert_with(|| {
                                        Rc::new(RefCell::new(TextFieldState::new()))
                                    });
                                    window.set_ime_allowed(true);
                                    window.set_ime_purpose(ImePurpose::Normal);
                                }
                            }
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }
                    TouchPhase::Moved => {
                        if let (Some(prev), Some(f), Some(cid)) =
                            (self.prev_touch_px, &self.frame_cache, self.capture_id)
                        {
                            // deliver pointer move to captured
                            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                                if let Some(cb) = &hit.on_pointer_move {
                                    cb(repose_core::input::PointerEvent {
                                        id: repose_core::input::PointerId(0),
                                        kind: repose_core::input::PointerKind::Touch,
                                        event: repose_core::input::PointerEventKind::Move,
                                        position: pos,
                                        pressure: 1.0,
                                        modifiers: self.modifiers,
                                    });
                                }

                                // natural scroll via dy
                                let dy_px = pos_px.1 - prev.1;
                                if dy_px.abs() > 0.0 {
                                    if let Some(cb) = &hit.on_scroll {
                                        let _ = cb(Vec2 { x: 0.0, y: -dy_px });
                                    }
                                }
                            }
                            self.prev_touch_px = Some(pos_px);
                            self.request_redraw();
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                                if let Some(cb) = &hit.on_pointer_up {
                                    cb(repose_core::input::PointerEvent {
                                        id: repose_core::input::PointerId(0),
                                        kind: repose_core::input::PointerKind::Touch,
                                        event: repose_core::input::PointerEventKind::Up(
                                            repose_core::input::PointerButton::Primary,
                                        ),
                                        position: pos,
                                        pressure: 1.0,
                                        modifiers: self.modifiers,
                                    });
                                }

                                if t.phase == TouchPhase::Ended
                                    && hit.rect.contains(pos)
                                    && let Some(cb) = &hit.on_click
                                {
                                    cb();
                                }
                            }
                        }

                        self.capture_id = None;
                        self.prev_touch_px = None;
                        self.pressed_ids.clear();
                        self.request_redraw();
                    }
                }
            }

            WindowEvent::ModifiersChanged(new_mods) => {
                self.modifiers.shift = new_mods.state().shift_key();
                self.modifiers.ctrl = new_mods.state().control_key();
                self.modifiers.alt = new_mods.state().alt_key();
                self.modifiers.meta = new_mods.state().super_key();
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                // Keep this minimal (no clipboard on web yet).
                // Tab traversal, Enter submit, and basic navigation can be added similarly to desktop runner.

                // Enter submits focused TextField if callback exists.
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                        if let Some(focused_id) = self.sched.focused
                            && let Some(f) = &self.frame_cache
                            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == focused_id)
                            && let Some(on_submit) = &hit.on_text_submit
                        {
                            let key = self.tf_key_of(focused_id);
                            if let Some(state) = self.textfield_states.get(&key) {
                                on_submit(state.borrow().text.clone());
                                self.request_redraw();
                            }
                        }
                    }
                }

                // Plain text input when IME isn't composing
                if key_event.state == ElementState::Pressed
                    && !self.ime_preedit
                    && !self.modifiers.ctrl
                    && !self.modifiers.alt
                    && !self.modifiers.meta
                    && let Some(raw) = key_event.text.as_deref()
                {
                    let text: String = raw
                        .chars()
                        .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                        .collect();
                    if !text.is_empty()
                        && let Some(fid) = self.sched.focused
                    {
                        let key = self.tf_key_of(fid);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut st = state_rc.borrow_mut();
                            st.insert_text(&text);
                            self.notify_text_change(fid, st.text.clone());

                            if let Some(f) = &self.frame_cache
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                self.tf_ensure_caret_visible_in_hit(&mut st, hit.rect);
                            }
                            self.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::Ime(ime) => {
                if let Some(focused_id) = self.sched.focused {
                    let key = self.tf_key_of(focused_id);
                    if let Some(state_rc) = self.textfield_states.get(&key) {
                        let mut state = state_rc.borrow_mut();
                        match ime {
                            Ime::Enabled => {
                                self.ime_preedit = false;
                            }
                            Ime::Preedit(text, cursor) => {
                                let cursor_usize = cursor.map(|(a, b)| (a, b));
                                state.set_composition(text.clone(), cursor_usize);
                                self.ime_preedit = !text.is_empty();

                                if let Some(f) = &self.frame_cache
                                    && let Some(hit) =
                                        f.hit_regions.iter().find(|h| h.id == focused_id)
                                {
                                    crate::tf_ensure_visible_in_rect(&mut state, hit.rect);
                                }

                                self.notify_text_change(focused_id, state.text.clone());
                                self.request_redraw();
                            }
                            Ime::Commit(text) => {
                                state.commit_composition(text);
                                self.ime_preedit = false;

                                if let Some(f) = &self.frame_cache
                                    && let Some(hit) =
                                        f.hit_regions.iter().find(|h| h.id == focused_id)
                                {
                                    crate::tf_ensure_visible_in_rect(&mut state, hit.rect);
                                }

                                self.notify_text_change(focused_id, state.text.clone());
                                self.request_redraw();
                            }
                            Ime::Disabled => {
                                self.ime_preedit = false;
                                if state.composition.is_some() {
                                    state.cancel_composition();
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == focused_id)
                                    {
                                        crate::tf_ensure_visible_in_rect(&mut state, hit.rect);
                                    }
                                    self.notify_text_change(focused_id, state.text.clone());
                                }
                                self.request_redraw();
                            }
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                // Build a Repose frame regardless; if GPU isn't ready, just skip drawing.
                let scale = window.scale_factor() as f32;
                let size_px_u32 = self.sched.size;
                let focused = self.sched.focused;

                let frame = compose_frame(
                    &mut self.sched,
                    &mut self.root,
                    scale,
                    size_px_u32,
                    self.hover_id,
                    &self.pressed_ids,
                    &self.textfield_states,
                    focused,
                );

                // Draw (currently: clear only)
                if let Some(gpu) = self.gpu.borrow_mut().as_mut() {
                    gpu.clear(frame.scene.clear_color);
                }

                self.frame_cache = Some(frame);

                // Schedule next frame
                window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        // Keep pumping frames (simple).
        self.request_redraw();
    }
}

struct WgpuState {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl WgpuState {
    async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();

        let instance = wgpu::Instance::default();

        let surface = instance
            .create_surface(window)
            .map_err(|e| format!("create_surface failed: {e:?}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|e| format!("request_adapter failed: {e:?}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Repose Web Device"),
                required_features: wgpu::Features::empty(),
                // conservative default for web compatibility (esp WebGL2 fallback):
                required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("request_device failed: {e:?}"))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        Ok(Self {
            surface,
            device,
            queue,
            config,
        })
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn clear(&mut self, color: repose_core::Color) {
        let output = match self.surface.get_current_texture() {
            Ok(tex) => tex,
            Err(wgpu::SurfaceError::Lost) | Err(wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(wgpu::SurfaceError::OutOfMemory) => return,
            Err(wgpu::SurfaceError::Other) => return,
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Repose Web Clear Encoder"),
            });

        {
            let _rp = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Repose Web Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: color.0 as f64 / 255.0,
                            g: color.1 as f64 / 255.0,
                            b: color.2 as f64 / 255.0,
                            a: color.3 as f64 / 255.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}
