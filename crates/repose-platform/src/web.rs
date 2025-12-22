//! Web runner (wasm32) using winit + repose-render-wgpu (async init).
use crate::common as rc;
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
    fullscreen: bool,
}

#[wasm_bindgen]
impl WebOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: Option<String>) -> Self {
        Self {
            canvas_id,
            fullscreen: true,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn canvas_id(&self) -> Option<String> {
        self.canvas_id.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn fullscreen(&self) -> bool {
        self.fullscreen
    }

    #[wasm_bindgen(setter)]
    pub fn set_fullscreen(&mut self, v: bool) {
        self.fullscreen = v;
    }
}

#[wasm_bindgen]
pub fn run_app(options: WebOptions) -> Result<(), JsValue> {
    run_web_app(
        |_sched| repose_core::View::new(0, repose_core::ViewKind::Surface),
        options,
    )
}

pub fn run_web_app(
    root: impl FnMut(&mut Scheduler) -> View + 'static,
    options: WebOptions,
) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Info);

    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = EventLoop::new().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
    let app = App::new(Box::new(root), options);

    event_loop.spawn_app(app);
    Ok(())
}

struct App {
    root: Box<dyn FnMut(&mut Scheduler) -> View>,
    options: WebOptions,

    window: Option<Arc<Window>>,
    backend: Rc<RefCell<Option<repose_render_wgpu::WgpuBackend>>>,

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
            backend: Rc::new(RefCell::new(None)),
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
            && let Some(i) = rc::hit_index_by_id(f, visual_id)
        {
            let hr = &f.hit_regions[i];
            return hr.tf_state_key.unwrap_or(hr.id);
        }
        visual_id
    }

    fn notify_text_change(&self, id: u64, text: String) {
        if let Some(f) = &self.frame_cache
            && let Some(i) = rc::hit_index_by_id(f, id)
            && let Some(cb) = &f.hit_regions[i].on_text_change
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

    fn is_textfield(&self, id: u64) -> bool {
        self.frame_cache
            .as_ref()
            .map(|f| {
                f.semantics_nodes
                    .iter()
                    .any(|n| n.id == id && n.role == Role::TextField)
            })
            .unwrap_or(false)
    }

    // --- Fullscreen helpers (no index.html CSS required) ---
    fn inject_fullscreen_css_if_needed(&self, window: &Window) {
        if !self.options.fullscreen {
            return;
        }
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };

        // set html/body styles
        if let Some(el) = doc
            .document_element()
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let style = el.style();
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
        }
        if let Some(body) = doc.body() {
            let style = body.style();
            let _ = style.set_property("margin", "0");
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
            let _ = style.set_property("overflow", "hidden");
        }

        // set canvas style
        if let Some(canvas) = window.canvas() {
            let style = canvas.style();
            let _ = style.set_property("display", "block");
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
        }
    }

    fn desired_physical_size_from_browser(&self) -> Option<PhysicalSize<u32>> {
        if !self.options.fullscreen {
            return None;
        }
        let w = web_sys::window()?;
        let dpr = w.device_pixel_ratio();
        let css_w = w.inner_width().ok()?.as_f64()?;
        let css_h = w.inner_height().ok()?.as_f64()?;
        let px_w = (css_w * dpr).round().max(1.0) as u32;
        let px_h = (css_h * dpr).round().max(1.0) as u32;
        Some(PhysicalSize::new(px_w, px_h))
    }

    fn ensure_fullscreen_size(&mut self, window: &Window) {
        let Some(desired) = self.desired_physical_size_from_browser() else {
            return;
        };

        // request resize if needed
        let current = window.inner_size();
        if current.width != desired.width || current.height != desired.height {
            let _ = window.request_inner_size(desired);
        }
    }

    // Ensure internal scheduler/backend match the actual window size.
    fn sync_size_from_window(&mut self, window: &Window) {
        let s = window.inner_size();
        if (s.width, s.height) != self.sched.size {
            self.sched.size = (s.width, s.height);
            if let Some(b) = self.backend.borrow_mut().as_mut() {
                b.configure_surface(s.width, s.height);
            }
        }
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
            .with_prevent_default(true)
            .with_focusable(true);

        // Attach to existing canvas (if provided), else create one and append.
        if let Some(id) = self.options.canvas_id.clone() {
            let document = web_sys::window()
                .and_then(|w| w.document())
                .expect("No document");
            let canvas = document
                .get_element_by_id(&id)
                .unwrap_or_else(|| panic!("Canvas id '{id}' not found"))
                .dyn_into::<web_sys::HtmlCanvasElement>()
                .expect("Element is not a canvas");
            attrs = attrs.with_canvas(Some(canvas)).with_append(false);
        } else {
            attrs = attrs.with_canvas(None).with_append(true);
        }

        let window = Arc::new(el.create_window(attrs).expect("create_window failed"));
        self.inject_fullscreen_css_if_needed(&window);

        // Focus canvas for keyboard input
        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
        }

        // Force a good initial size (prevents 1x1)
        self.ensure_fullscreen_size(&window);
        self.sync_size_from_window(&window);

        self.window = Some(window.clone());

        // Async init backend
        let backend_cell = self.backend.clone();
        let window_for_async = window.clone();
        spawn_local(async move {
            match repose_render_wgpu::WgpuBackend::new_async(window_for_async.clone()).await {
                Ok(mut b) => {
                    let s = window_for_async.inner_size();
                    b.configure_surface(s.width, s.height);
                    *backend_cell.borrow_mut() = Some(b);
                    window_for_async.request_redraw();
                    log::info!("WGPU backend initialized");
                }
                Err(e) => {
                    log::error!("WGPU init failed: {e:?}");
                }
            }
        });

        self.request_redraw();
    }

    fn window_event(
        &mut self,
        el: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        // Clone the Arc<Window> to avoid borrow conflicts with &mut self methods
        let Some(window) = self.window.clone() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => el.exit(),

            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);
                self.request_redraw();
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos_px = (position.x as f32, position.y as f32);

                // TextField drag selection (same behavior as desktop)
                if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                    && self.is_textfield(cid)
                {
                    let key = self.tf_key_of(cid);
                    if let Some(state_rc) = self.textfield_states.get(&key) {
                        let mut state = state_rc.borrow_mut();
                        let inner_x_px = f
                            .hit_regions
                            .iter()
                            .find(|h| h.id == cid)
                            .map(|h| h.rect.x + dp_to_px(TF_PADDING_X_DP))
                            .unwrap_or(0.0);
                        let content_x_px = self.mouse_pos_px.0 - inner_x_px + state.scroll_offset;
                        let idx = index_for_x_bytes(
                            &state.text,
                            TF_FONT_DP as u32,
                            content_x_px.max(0.0),
                        );
                        state.drag_to(idx);

                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                            self.tf_ensure_caret_visible_in_hit(&mut state, hit.rect);
                        }
                        self.request_redraw();
                    }
                }

                // Hover + move/capture callbacks
                if let Some(f) = &self.frame_cache {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };
                    let top_i = rc::top_hit_index(f, pos);
                    let new_hover = top_i.map(|i| f.hit_regions[i].id);

                    if new_hover != self.hover_id {
                        // leave
                        if let Some(prev_id) = self.hover_id
                            && let Some(pi) = rc::hit_index_by_id(f, prev_id)
                            && let Some(cb) = &f.hit_regions[pi].on_pointer_leave
                        {
                            cb(rc::pe_mouse(
                                repose_core::input::PointerEventKind::Leave,
                                pos,
                                self.modifiers,
                            ));
                        }
                        // enter
                        if let Some(i) = top_i
                            && let Some(cb) = &f.hit_regions[i].on_pointer_enter
                        {
                            cb(rc::pe_mouse(
                                repose_core::input::PointerEventKind::Enter,
                                pos,
                                self.modifiers,
                            ));
                        }
                        self.hover_id = new_hover;
                    }

                    // move
                    let pe = rc::pe_mouse(
                        repose_core::input::PointerEventKind::Move,
                        pos,
                        self.modifiers,
                    );
                    if let Some(cid) = self.capture_id {
                        if let Some(i) = rc::hit_index_by_id(f, cid)
                            && let Some(cb) = &f.hit_regions[i].on_pointer_move
                        {
                            cb(pe);
                        }
                    } else if let Some(i) = top_i
                        && let Some(cb) = &f.hit_regions[i].on_pointer_move
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
                    if rc::dispatch_scroll(f, pos, Vec2 { x: dx_px, y: dy_px }) {
                        self.request_redraw();
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
                            if let Some(i) = rc::top_hit_index(f, pos) {
                                let hit = &f.hit_regions[i];

                                self.capture_id = Some(hit.id);
                                self.pressed_ids.insert(hit.id);

                                // Focus & IME
                                if hit.focusable {
                                    self.sched.focused = Some(hit.id);

                                    let key = self.tf_key_of(hit.id);
                                    self.textfield_states.entry(key).or_insert_with(|| {
                                        Rc::new(RefCell::new(TextFieldState::new()))
                                    });

                                    if self.is_textfield(hit.id) {
                                        window.set_ime_allowed(true);
                                        window.set_ime_purpose(ImePurpose::Normal);
                                    } else {
                                        window.set_ime_allowed(false);
                                    }
                                }

                                // Pointer down (legacy)
                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(rc::pe_down_primary(
                                        repose_core::input::PointerKind::Mouse,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                // TextField caret placement + begin selection
                                if self.is_textfield(hit.id) {
                                    let key = self.tf_key_of(hit.id);
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        let mut st = state_rc.borrow_mut();
                                        let inner_x_px = hit.rect.x + dp_to_px(TF_PADDING_X_DP);
                                        let content_x_px =
                                            self.mouse_pos_px.0 - inner_x_px + st.scroll_offset;
                                        let idx = index_for_x_bytes(
                                            &st.text,
                                            TF_FONT_DP as u32,
                                            content_x_px.max(0.0),
                                        );
                                        st.begin_drag(idx, self.modifiers.shift);
                                        self.tf_ensure_caret_visible_in_hit(&mut st, hit.rect);
                                    }
                                }
                            } else {
                                self.sched.focused = None;
                                window.set_ime_allowed(false);
                            }
                            self.request_redraw();
                        }

                        ElementState::Released => {
                            if let Some(cid) = self.capture_id {
                                self.pressed_ids.remove(&cid);

                                // on_pointer_up
                                if let Some(i) = rc::hit_index_by_id(f, cid)
                                    && let Some(cb) = &f.hit_regions[i].on_pointer_up
                                {
                                    cb(rc::pe_up_primary(
                                        repose_core::input::PointerKind::Mouse,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                // click-on-release
                                if let Some(i) = rc::hit_index_by_id(f, cid) {
                                    let hit = &f.hit_regions[i];
                                    if hit.rect.contains(pos)
                                        && let Some(cb) = &hit.on_click
                                    {
                                        cb();
                                    }
                                }

                                // end TextField drag
                                if self.is_textfield(cid) {
                                    let key = self.tf_key_of(cid);
                                    if let Some(st) = self.textfield_states.get(&key) {
                                        st.borrow_mut().end_drag();
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
                let pos_px = (t.location.x as f32, t.location.y as f32);
                self.mouse_pos_px = pos_px;
                let pos = Vec2 {
                    x: pos_px.0,
                    y: pos_px.1,
                };

                match t.phase {
                    TouchPhase::Started => {
                        if let Some(f) = &self.frame_cache {
                            if let Some(i) = rc::top_hit_index(f, pos) {
                                let hit = &f.hit_regions[i];
                                self.capture_id = Some(hit.id);
                                self.pressed_ids.insert(hit.id);

                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(rc::pe_down_primary(
                                        repose_core::input::PointerKind::Touch,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                if self.is_textfield(hit.id) {
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
                            if let Some(i) = rc::hit_index_by_id(f, cid) {
                                let hit = &f.hit_regions[i];

                                if let Some(cb) = &hit.on_pointer_move {
                                    cb(rc::pe_touch(
                                        repose_core::input::PointerEventKind::Move,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                // natural scroll
                                let dy_px = pos_px.1 - prev.1;
                                if dy_px.abs() > 0.0 {
                                    if let Some(cb) = &hit.on_scroll {
                                        let _ = cb(Vec2 { x: 0.0, y: -dy_px });
                                    }
                                }
                            }
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }

                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                            if let Some(i) = rc::hit_index_by_id(f, cid) {
                                let hit = &f.hit_regions[i];

                                if let Some(cb) = &hit.on_pointer_up {
                                    cb(rc::pe_up_primary(
                                        repose_core::input::PointerKind::Touch,
                                        pos,
                                        self.modifiers,
                                    ));
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
                // Focus traversal: Tab / Shift+Tab
                if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        if let Some(f) = &self.frame_cache {
                            let chain = &f.focus_chain;
                            if !chain.is_empty() {
                                if let Some(active) = self.key_pressed_active.take() {
                                    self.pressed_ids.remove(&active);
                                }
                                let shift = self.modifiers.shift;
                                let current = self.sched.focused;

                                let next = if let Some(cur) = current {
                                    if let Some(idx) = chain.iter().position(|&id| id == cur) {
                                        if shift {
                                            if idx == 0 {
                                                chain[chain.len() - 1]
                                            } else {
                                                chain[idx - 1]
                                            }
                                        } else {
                                            chain[(idx + 1) % chain.len()]
                                        }
                                    } else {
                                        chain[0]
                                    }
                                } else {
                                    chain[0]
                                };

                                self.sched.focused = Some(next);
                                if self.is_textfield(next) {
                                    window.set_ime_allowed(true);
                                    window.set_ime_purpose(ImePurpose::Normal);
                                } else {
                                    window.set_ime_allowed(false);
                                }
                                self.request_redraw();
                            }
                        }
                    }
                    return;
                }

                // Enter submits focused TextField
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                        if let Some(focused_id) = self.sched.focused
                            && let Some(f) = &self.frame_cache
                            && let Some(i) = rc::hit_index_by_id(f, focused_id)
                            && let Some(on_submit) = &f.hit_regions[i].on_text_submit
                        {
                            let key = self.tf_key_of(focused_id);
                            if let Some(state) = self.textfield_states.get(&key) {
                                on_submit(state.borrow().text.clone());
                                self.request_redraw();
                                return;
                            }
                        }
                    }
                }

                // Basic TextField editing (backspace/delete/arrows/home/end/ctrl+a)
                if key_event.state == ElementState::Pressed {
                    if let Some(fid) = self.sched.focused {
                        let key = self.tf_key_of(fid);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut st = state_rc.borrow_mut();
                            match key_event.physical_key {
                                PhysicalKey::Code(KeyCode::Backspace) => {
                                    st.delete_backward();
                                    self.notify_text_change(fid, st.text.clone());
                                }
                                PhysicalKey::Code(KeyCode::Delete) => {
                                    st.delete_forward();
                                    self.notify_text_change(fid, st.text.clone());
                                }
                                PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                    st.move_cursor(-1, self.modifiers.shift)
                                }
                                PhysicalKey::Code(KeyCode::ArrowRight) => {
                                    st.move_cursor(1, self.modifiers.shift)
                                }
                                PhysicalKey::Code(KeyCode::Home) => st.selection = 0..0,
                                PhysicalKey::Code(KeyCode::End) => {
                                    let end = st.text.len();
                                    st.selection = end..end;
                                }
                                PhysicalKey::Code(KeyCode::KeyA)
                                    if self.modifiers.ctrl || self.modifiers.meta =>
                                {
                                    st.selection = 0..st.text.len();
                                }
                                _ => {}
                            }

                            if let Some(f) = &self.frame_cache
                                && let Some(i) = rc::hit_index_by_id(f, fid)
                            {
                                self.tf_ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].rect);
                            }
                            self.request_redraw();
                        }
                    }

                    // Plain text input when IME isn't composing
                    if !self.ime_preedit
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
                                    && let Some(i) = rc::hit_index_by_id(f, fid)
                                {
                                    self.tf_ensure_caret_visible_in_hit(
                                        &mut st,
                                        f.hit_regions[i].rect,
                                    );
                                }
                                self.request_redraw();
                            }
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
                            Ime::Enabled => self.ime_preedit = false,
                            Ime::Preedit(text, cursor) => {
                                let cursor_usize = cursor.map(|(a, b)| (a, b));
                                state.set_composition(text.clone(), cursor_usize);
                                self.ime_preedit = !text.is_empty();

                                if let Some(f) = &self.frame_cache
                                    && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                {
                                    tf_ensure_visible_in_rect(&mut state, f.hit_regions[i].rect);
                                }

                                self.notify_text_change(focused_id, state.text.clone());
                                self.request_redraw();
                            }
                            Ime::Commit(text) => {
                                state.commit_composition(text);
                                self.ime_preedit = false;

                                if let Some(f) = &self.frame_cache
                                    && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                {
                                    tf_ensure_visible_in_rect(&mut state, f.hit_regions[i].rect);
                                }

                                self.notify_text_change(focused_id, state.text.clone());
                                self.request_redraw();
                            }
                            Ime::Disabled => {
                                self.ime_preedit = false;
                                if state.composition.is_some() {
                                    state.cancel_composition();
                                    if let Some(f) = &self.frame_cache
                                        && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                    {
                                        tf_ensure_visible_in_rect(
                                            &mut state,
                                            f.hit_regions[i].rect,
                                        );
                                    }
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);

                // If backend not ready yet, keep pumping redraws
                if self.backend.borrow().is_none() {
                    window.request_redraw();
                    return;
                }

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

                if let Some(backend) = self.backend.borrow_mut().as_mut() {
                    backend.frame(&frame.scene, GlyphRasterConfig { px: 18.0 * scale });
                }

                self.frame_cache = Some(frame);
                window.request_redraw();
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        self.request_redraw();
    }
}
