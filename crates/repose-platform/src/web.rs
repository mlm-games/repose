//! Web runner (wasm32) using winit + repose-render-wgpu (async init).
use crate::common as rc;
use crate::common_web as rc_web;
use crate::render::RenderContext;
use crate::*;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use web_sys::DragEvent;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::{ImePurpose, Window};

use repose_ui::TextFieldState;
use repose_ui::textfield::{
    TF_FONT_DP, TF_PADDING_X_DP, caret_xy_for_byte, index_for_x_bytes, index_for_xy_bytes,
    move_caret_vertical,
};

enum ClipboardAction {
    PasteText(String),
}

enum ExternalDropAction {
    DroppedFiles {
        names: Vec<String>,
        pos_px: (f32, f32),
    },
}

#[wasm_bindgen]
pub struct WebOptions {
    canvas_id: Option<String>,
    fullscreen: bool,

    /// If true, request redraw continuously (needed for animations).
    continuous_redraw: bool,
}

#[wasm_bindgen]
impl WebOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: Option<String>) -> Self {
        Self {
            canvas_id,
            fullscreen: true,
            continuous_redraw: true,
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

    #[wasm_bindgen(getter)]
    pub fn continuous_redraw(&self) -> bool {
        self.continuous_redraw
    }

    #[wasm_bindgen(setter)]
    pub fn set_continuous_redraw(&mut self, v: bool) {
        self.continuous_redraw = v;
    }
}

#[wasm_bindgen]
pub fn run_app(options: WebOptions) -> Result<(), JsValue> {
    run_web_app(
        |_sched, _rc| repose_core::View::new(0, repose_core::ViewKind::Surface),
        options,
    )
}

pub fn run_web_app(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
    options: WebOptions,
) -> Result<(), JsValue> {
    run_web_app_with_snackbar(root, options, None)
}

pub fn run_web_app_with_snackbar(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
    options: WebOptions,
    snackbar_tick: Option<Rc<dyn Fn(u32)>>,
) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Info);

    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = EventLoop::new().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
    let app = App::new_with_snackbar(Box::new(root), options, snackbar_tick);

    event_loop.spawn_app(app);
    Ok(())
}

#[derive(Clone)]
struct WebDropListeners {
    _drag_over: Closure<dyn FnMut(DragEvent)>,
    _drop: Closure<dyn FnMut(DragEvent)>,
}

struct App {
    root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
    options: WebOptions,

    window: Option<Arc<Window>>,
    backend: Rc<RefCell<Option<repose_render_wgpu::WgpuBackend>>>,

    sched: Scheduler,
    frame_cache: Option<Frame>,
    render: RenderContext,

    // pointer + focus
    mouse_pos_px: (f32, f32),
    modifiers: Modifiers,
    hover_id: Option<u64>,
    capture_id: Option<u64>,
    pressed_ids: HashSet<u64>,

    mouse_down_pos_px: Option<(f32, f32)>,
    drag: Option<rc::DragSession>,

    // touch click-cancel after scroll
    touch_scrolled: bool,
    touch_scroll_accum_y_px: f32,
    prev_touch_px: Option<(f32, f32)>,

    // text
    ime_preedit: bool,
    textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,

    // clipboard async results
    clipboard_actions: Rc<RefCell<Vec<ClipboardAction>>>,

    external_drop_actions: Rc<RefCell<Vec<ExternalDropAction>>>,

    // keep DOM listener closures alive
    drop_listeners: Option<WebDropListeners>,

    // multi-touch for pinch
    active_touches: HashMap<u64, (f32, f32)>,
    primary_touch_id: Option<u64>,
    pinch_last_dist: Option<f32>,

    // swipe tracking
    touch_start: Option<(web_time::Instant, (f32, f32))>,

    key_pressed_active: Option<u64>,

    snackbar_tick: Option<Rc<dyn Fn(u32)>>,
    last_redraw: web_time::Instant,
}

impl App {
    fn new(
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        options: WebOptions,
    ) -> Self {
        Self {
            root,
            options,
            window: None,
            backend: Rc::new(RefCell::new(None)),
            sched: Scheduler::new(),
            frame_cache: None,

            render: RenderContext::new(),

            mouse_pos_px: (0.0, 0.0),
            modifiers: Modifiers::default(),
            hover_id: None,
            capture_id: None,
            pressed_ids: HashSet::new(),

            mouse_down_pos_px: None,
            drag: None,

            touch_scrolled: false,
            touch_scroll_accum_y_px: 0.0,
            prev_touch_px: None,

            ime_preedit: false,
            textfield_states: HashMap::new(),

            clipboard_actions: Rc::new(RefCell::new(Vec::new())),

            external_drop_actions: Rc::new(RefCell::new(Vec::new())),
            drop_listeners: None,

            active_touches: HashMap::new(),
            primary_touch_id: None,
            pinch_last_dist: None,
            touch_start: None,

            key_pressed_active: None,

            snackbar_tick: None,
            last_redraw: web_time::Instant::now(),
        }
    }

    fn new_with_snackbar(
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        options: WebOptions,
        snackbar_tick: Option<Rc<dyn Fn(u32)>>,
    ) -> Self {
        let mut app = Self::new(root, options);
        app.snackbar_tick = snackbar_tick;
        app
    }

    fn tick_snackbar(&mut self) {
        let Some(cb) = &self.snackbar_tick else {
            return;
        };
        let now = web_time::Instant::now();
        let elapsed = now.saturating_duration_since(self.last_redraw);
        let ms = elapsed.as_millis().min(u32::MAX as u128) as u32;
        if ms > 0 {
            cb(ms);
        }
    }

    fn request_redraw(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn scale(&self, window: &Window) -> f32 {
        window.scale_factor() as f32
    }

    fn padding_px(&self, window: &Window) -> f32 {
        TF_PADDING_X_DP * self.scale(window)
    }

    fn touch_slop_px(&self, window: &Window) -> f32 {
        rc::touch_slop_px(self.scale(window))
    }

    fn tf_key_of(&self, visual_id: u64) -> u64 {
        if let Some(f) = &self.frame_cache {
            return rc::tf_key_of(f, visual_id);
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

    fn is_textfield(&self, id: u64) -> bool {
        if let Some(f) = &self.frame_cache {
            f.semantics_nodes
                .iter()
                .any(|n| n.id == id && n.role == Role::TextField)
        } else {
            false
        }
    }

    fn tf_ensure_caret_visible_in_hit(&self, state: &mut TextFieldState, is_multiline: bool) {
        rc::tf_ensure_caret_visible(state, is_multiline);
    }

    fn inject_fullscreen_css_if_needed(&self, window: &Window) {
        if !self.options.fullscreen {
            return;
        }
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };

        if let Some(el) = doc
            .document_element()
            .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let style = el.style();
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
            let _ = style.set_property("overflow", "hidden");
        }
        if let Some(body) = doc.body() {
            let style = body.style();
            let _ = style.set_property("margin", "0");
            let _ = style.set_property("padding", "0");
            let _ = style.set_property("width", "100%");
            let _ = style.set_property("height", "100%");
            let _ = style.set_property("overflow", "hidden");
        }

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
        let current = window.inner_size();
        if current.width != desired.width || current.height != desired.height {
            let _ = window.request_inner_size(desired);
        }
    }

    fn sync_size_from_window(&mut self, window: &Window) {
        let s = window.inner_size();
        if (s.width, s.height) != self.sched.size {
            self.sched.size = (s.width, s.height);
            if let Some(b) = self.backend.borrow_mut().as_mut() {
                b.configure_surface(s.width, s.height);
            }
        }
    }

    fn copy_to_clipboard_async(&self, text: String) {
        spawn_local(async move {
            if let Ok(mut cb) = clipawl::Clipboard::new() {
                let _ = cb.set_text(&text).await;
            }
        });
    }

    fn request_paste_async(&self) {
        let actions = self.clipboard_actions.clone();
        let win = self.window.clone();

        spawn_local(async move {
            if let Ok(mut cb) = clipawl::Clipboard::new() {
                if let Ok(t) = cb.get_text().await {
                    actions.borrow_mut().push(ClipboardAction::PasteText(t));
                    if let Some(w) = win.as_ref() {
                        w.request_redraw();
                    }
                }
            }
        });
    }

    fn apply_clipboard_actions(&mut self, window: &Window) {
        if self.clipboard_actions.borrow().is_empty() {
            return;
        }
        let actions = std::mem::take(&mut *self.clipboard_actions.borrow_mut());

        for a in actions {
            match a {
                ClipboardAction::PasteText(mut txt) => {
                    let multiline = if let Some(fid) = self.sched.focused {
                        self.frame_cache
                            .as_ref()
                            .and_then(|f| rc::hit_index_by_id(f, fid))
                            .map(|i| self.frame_cache.as_ref().unwrap().hit_regions[i].tf_multiline)
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    txt.retain(|c| !c.is_control() && c != '\r' && (multiline || c != '\n'));
                    if txt.is_empty() {
                        continue;
                    }

                    if let Some(fid) = self.sched.focused {
                        let key = self.tf_key_of(fid);
                        if let Some(st_rc) = self.textfield_states.get(&key).cloned() {
                            let mut st = st_rc.borrow_mut();
                            st.insert_text(&txt);
                            self.notify_text_change(fid, st.text.clone());

                            if let Some(f) = &self.frame_cache
                                && let Some(i) = rc::hit_index_by_id(f, fid)
                            {
                                self.tf_ensure_caret_visible_in_hit(
                                    &mut st,
                                    f.hit_regions[i].tf_multiline,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn dispatch_action(&mut self, window: &Window, action: repose_core::shortcuts::Action) -> bool {
        use repose_core::shortcuts;

        if let (Some(f), Some(fid)) = (&self.frame_cache, self.sched.focused) {
            if let Some(i) = rc::hit_index_by_id(f, fid) {
                if let Some(cb) = &f.hit_regions[i].on_action {
                    if cb(action.clone()) {
                        return true;
                    }
                }
            }
        }

        if shortcuts::handle(action.clone()) {
            return true;
        }

        self.dispatch_default_action(window, action)
    }

    fn dispatch_default_action(
        &mut self,
        window: &Window,
        action: repose_core::shortcuts::Action,
    ) -> bool {
        use repose_core::shortcuts::Action;

        let Some(fid) = self.sched.focused else {
            return false;
        };
        let key = self.tf_key_of(fid);
        let Some(state_rc) = self.textfield_states.get(&key).cloned() else {
            return false;
        };

        match action {
            Action::Copy => {
                let txt = state_rc.borrow().selected_text();
                if txt.is_empty() {
                    return false;
                }
                self.copy_to_clipboard_async(txt);
                true
            }
            Action::Cut => {
                let txt = state_rc.borrow().selected_text();
                if txt.is_empty() {
                    return false;
                }
                self.copy_to_clipboard_async(txt);
                {
                    let mut st = state_rc.borrow_mut();
                    st.insert_text("");
                    self.notify_text_change(fid, st.text.clone());
                    if let Some(f) = &self.frame_cache
                        && let Some(i) = rc::hit_index_by_id(f, fid)
                    {
                        self.tf_ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].tf_multiline);
                    }
                }
                true
            }
            Action::Paste => {
                self.request_paste_async();
                true
            }
            Action::SelectAll => {
                {
                    let mut st = state_rc.borrow_mut();
                    st.selection = 0..st.text.len();
                    if let Some(f) = &self.frame_cache
                        && let Some(i) = rc::hit_index_by_id(f, fid)
                    {
                        self.tf_ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].tf_multiline);
                    }
                }
                true
            }
            _ => false,
        }
    }

    fn drain_render_commands(&self) {
        let cmds = self.render.drain();
        if cmds.is_empty() {
            return;
        }
        let mut backend_ref = self.backend.borrow_mut();
        let Some(backend) = backend_ref.as_mut() else {
            return;
        };
        rc::process_render_commands(backend, cmds);
    }
    fn dnd_slop_px(&self, window: &Window) -> f32 {
        rc::touch_slop_px(self.scale(window))
    }

    fn dnd_update_over(&mut self, pos: Vec2) {
        let Some(f) = &self.frame_cache else {
            return;
        };
        let Some(session) = self.drag.as_mut() else {
            return;
        };
        rc::dnd_update_over(f, session, self.modifiers, pos);
    }

    fn dnd_try_begin_mouse(&mut self, window: &Window, pos: Vec2) -> bool {
        if self.drag.is_some() {
            return true;
        }

        let Some((sx, sy)) = self.mouse_down_pos_px else {
            return false;
        };
        let Some(cid) = self.capture_id else {
            return false;
        };
        if !self.pressed_ids.contains(&cid) {
            return false;
        }

        let dx = pos.x - sx;
        let dy = pos.y - sy;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < self.dnd_slop_px(window) {
            return false;
        }

        let Some(f) = &self.frame_cache else {
            return false;
        };
        let Some(i) = rc::hit_index_by_id(f, cid) else {
            return false;
        };
        let Some(cb) = &f.hit_regions[i].on_drag_start else {
            return false;
        };

        let payload = cb(repose_core::dnd::DragStart {
            source_id: cid,
            position: pos,
            modifiers: self.modifiers,
        });
        let Some(payload) = payload else {
            return false;
        };

        self.drag = Some(rc::DragSession {
            source_id: cid,
            payload,
            start_px: (sx, sy),
            over_id: None,
        });
        self.pressed_ids.remove(&cid);
        self.request_redraw();
        true
    }

    fn dnd_finish(&mut self, pos: Vec2, accept_if_possible: bool) {
        let Some(f) = &self.frame_cache else {
            self.drag = None;
            self.capture_id = None;
            self.mouse_down_pos_px = None;
            self.request_redraw();
            return;
        };

        let Some(session) = self.drag.take() else {
            return;
        };

        let _accepted = rc::dnd_finish(f, session, self.modifiers, pos, accept_if_possible);
        self.capture_id = None;
        self.mouse_down_pos_px = None;
        self.request_redraw();
    }

    fn dnd_cancel(&mut self) {
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        self.dnd_finish(pos, false);
    }
    fn dispatch_dropped_files(&mut self, window: &Window, names: Vec<String>, pos_px: (f32, f32)) {
        let Some(f) = &self.frame_cache else {
            return;
        };

        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };

        let files = names
            .into_iter()
            .map(|name| repose_core::dnd::DroppedFile { name, path: None })
            .collect::<Vec<_>>();

        let payload: repose_core::dnd::DragPayload =
            std::rc::Rc::new(repose_core::dnd::DroppedFiles { files });

        let Some(target_id) = rc::dnd_target_id_at(f, pos) else {
            return;
        };

        if let Some(i) = rc::hit_index_by_id(f, target_id) {
            if let Some(cb) = &f.hit_regions[i].on_drop {
                let _accepted = cb(repose_core::dnd::DropEvent {
                    source_id: 0,
                    target_id,
                    position: pos,
                    modifiers: self.modifiers,
                    payload,
                });
                self.request_redraw();
            }
        }
    }

    fn apply_external_drop_actions(&mut self, window: &Window) {
        if self.external_drop_actions.borrow().is_empty() {
            return;
        }
        let actions = std::mem::take(&mut *self.external_drop_actions.borrow_mut());
        for a in actions {
            match a {
                ExternalDropAction::DroppedFiles { names, pos_px } => {
                    self.dispatch_dropped_files(window, names, pos_px);
                }
            }
        }
    }
}

impl ApplicationHandler<()> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let mut attrs = Window::default_attributes()
            .with_title("Repose (Web)")
            .with_inner_size(PhysicalSize::new(1280u32, 800u32))
            .with_prevent_default(true)
            .with_focusable(true);

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

        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
        }

        self.ensure_fullscreen_size(&window);
        self.sync_size_from_window(&window);

        self.window = Some(window.clone());

        if let Some(canvas) = window.canvas() {
            use wasm_bindgen::JsCast;

            let actions = self.external_drop_actions.clone();
            let win = self.window.clone();

            let drag_over = Closure::wrap(Box::new(move |e: DragEvent| {
                e.prevent_default(); // required to allow drop
                if let Some(dt) = e.data_transfer() {
                    dt.set_drop_effect("copy");
                }
            }) as Box<dyn FnMut(_)>);

            let actions2 = self.external_drop_actions.clone();
            let win2 = self.window.clone();

            let drop = Closure::wrap(Box::new(move |e: DragEvent| {
                e.prevent_default();
                let Some(dt) = e.data_transfer() else {
                    return;
                };
                let Some(list) = dt.files() else {
                    return;
                };

                let mut names = Vec::new();
                for i in 0..list.length() {
                    if let Some(f) = list.get(i) {
                        names.push(f.name());
                    }
                }

                let mut pos_px = (0.0f32, 0.0f32);
                if let Some(target) = e
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlCanvasElement>().ok())
                {
                    let rect = target.get_bounding_client_rect();
                    let x_css = e.client_x() as f64 - rect.left();
                    let y_css = e.client_y() as f64 - rect.top();
                    let dpr = web_sys::window()
                        .map(|w| w.device_pixel_ratio())
                        .unwrap_or(1.0);
                    pos_px = ((x_css * dpr) as f32, (y_css * dpr) as f32);
                }

                actions2
                    .borrow_mut()
                    .push(ExternalDropAction::DroppedFiles { names, pos_px });

                if let Some(w) = win2.as_ref() {
                    w.request_redraw();
                }
            }) as Box<dyn FnMut(_)>);

            let _ = canvas
                .add_event_listener_with_callback("dragover", drag_over.as_ref().unchecked_ref());
            let _ = canvas.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref());

            self.drop_listeners = Some(WebDropListeners {
                _drag_over: drag_over,
                _drop: drop,
            });
        }

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
        let Some(window) = self.window.clone() else {
            return;
        };

        // Apply any async clipboard results (paste)
        self.apply_clipboard_actions(&window);
        self.apply_external_drop_actions(&window);

        match event {
            WindowEvent::CloseRequested => el.exit(),

            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);
                self.request_redraw();
            }

            WindowEvent::ModifiersChanged(new_mods) => {
                self.modifiers.shift = new_mods.state().shift_key();
                self.modifiers.ctrl = new_mods.state().control_key();
                self.modifiers.alt = new_mods.state().alt_key();
                self.modifiers.meta = new_mods.state().super_key();
                self.modifiers.command = if cfg!(target_os = "macos") {
                    self.modifiers.meta
                } else {
                    self.modifiers.ctrl
                };
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.mouse_pos_px = (position.x as f32, position.y as f32);

                let pos = Vec2 {
                    x: self.mouse_pos_px.0,
                    y: self.mouse_pos_px.1,
                };

                if self.drag.is_some() {
                    self.dnd_update_over(pos);
                    self.request_redraw();
                    return;
                }

                // DnD (mouse)
                if self.dnd_try_begin_mouse(&window, pos) {
                    self.dnd_update_over(pos);
                    return;
                }

                // TextField drag selection (if captured)
                if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                    && self.is_textfield(cid)
                {
                    let key = self.tf_key_of(cid);
                    if let Some(state_rc) = self.textfield_states.get(&key) {
                        let mut state = state_rc.borrow_mut();
                        let pad = self.padding_px(&window);

                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                            let inner_x_px = hit.rect.x + pad;
                            let inner_y_px = hit.rect.y + 8.0 * self.scale(&window);
                            let content_x_px =
                                (self.mouse_pos_px.0 - inner_x_px + state.scroll_offset).max(0.0);
                            let content_y_px =
                                (self.mouse_pos_px.1 - inner_y_px + state.scroll_offset_y).max(0.0);
                            let font_px =
                                dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;

                            let idx = if hit.tf_multiline {
                                rc::index_for_xy_bytes_vt(
                                    &state,
                                    font_px,
                                    hit.rect.w - 2.0 * pad,
                                    content_x_px,
                                    content_y_px,
                                )
                            } else {
                                rc::index_for_x_bytes_vt(&state, font_px, content_x_px)
                            };

                            state.drag_to(idx);

                            // Ensure caret visible
                            if hit.tf_multiline {
                                let caret_idx = state.caret_index();
                                let wrap_w = hit.rect.w - 2.0 * pad;
                                let (cx, cy, _) =
                                    caret_xy_for_byte(&state.text, font_px, wrap_w, caret_idx);
                                let iw = state.inner_width;
                                let ih = state.inner_height;
                                state.ensure_caret_visible_xy(
                                    cx,
                                    cy,
                                    iw,
                                    ih,
                                    2.0 * self.scale(&window),
                                );
                            } else {
                                self.tf_ensure_caret_visible_in_hit(&mut state, hit.tf_multiline);
                            }
                            self.request_redraw();
                        }
                    }
                }

                // Hover/move
                if let Some(f) = &self.frame_cache {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };
                    let top_i = rc::top_hit_index(f, pos);
                    let new_hover = top_i.map(|i| f.hit_regions[i].id);

                    if new_hover != self.hover_id {
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
                let scale = self.scale(&window);
                let (dx_px, dy_px) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        let unit_px = 60.0 * scale;
                        (x * unit_px, y * unit_px)
                    }
                    MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
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
                            self.mouse_down_pos_px = Some(self.mouse_pos_px);
                            self.drag = None;

                            // Find top-most hit for capture
                            if let Some(i) = rc::top_hit_index(f, pos) {
                                let hit = &f.hit_regions[i];
                                self.capture_id = Some(hit.id);
                                self.pressed_ids.insert(hit.id);

                                if hit.focusable {
                                    self.sched.focused = Some(hit.id);

                                    let key = self.tf_key_of(hit.id);
                                    self.textfield_states.entry(key).or_insert_with(|| {
                                        Rc::new(RefCell::new(TextFieldState::new()))
                                    });

                                    rc_web::set_ime_for_textfield(
                                        &window,
                                        self.is_textfield(hit.id),
                                    );
                                }

                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(rc::pe_down_primary(
                                        repose_core::input::PointerKind::Mouse,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                // TextField begin selection
                                if self.is_textfield(hit.id) {
                                    let key = self.tf_key_of(hit.id);
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        let mut st = state_rc.borrow_mut();
                                        rc::tf_place_caret_at_pointer(
                                            &mut st,
                                            hit.rect,
                                            hit.tf_multiline,
                                            self.mouse_pos_px,
                                            self.scale(&window),
                                            self.modifiers.shift,
                                        );
                                    }
                                }
                            } else {
                                self.sched.focused = None;
                                rc_web::set_ime_for_textfield(&window, false);
                            }
                            self.request_redraw();
                        }

                        ElementState::Released => {
                            let pos = Vec2 {
                                x: self.mouse_pos_px.0,
                                y: self.mouse_pos_px.1,
                            };

                            if self.drag.is_some() {
                                self.dnd_finish(pos, true);
                                self.capture_id = None;
                                self.pressed_ids.clear();
                                self.request_redraw();
                                return;
                            }

                            if let Some(cid) = self.capture_id {
                                self.pressed_ids.remove(&cid);

                                if let Some(i) = rc::hit_index_by_id(f, cid)
                                    && let Some(cb) = &f.hit_regions[i].on_pointer_up
                                {
                                    cb(rc::pe_up_primary(
                                        repose_core::input::PointerKind::Mouse,
                                        pos,
                                        self.modifiers,
                                    ));
                                }

                                // Robust click search: find the top-most region with this ID
                                // that actually contains the point and has a click handler.
                                // NOTE: We search in reverse (top-to-bottom) because overlays
                                // are usually drawn last (at the end of the list).
                                let click_hit = f.hit_regions.iter().rev().find(|h| {
                                    h.id == cid && h.rect.contains(pos) && h.on_click.is_some()
                                });

                                if let Some(hit) = click_hit {
                                    log::info!("MouseUp: Clicked! id={}", hit.id);
                                    if let Some(cb) = &hit.on_click {
                                        cb();
                                    }
                                } else {
                                    log::info!(
                                        "MouseUp: No click match for captured id={} at pos={:?}",
                                        cid,
                                        pos
                                    );
                                }

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
                use repose_core::shortcuts::{Action, Gesture};

                let pos_px = (t.location.x as f32, t.location.y as f32);
                self.mouse_pos_px = pos_px;
                let pos = Vec2 {
                    x: pos_px.0,
                    y: pos_px.1,
                };

                let tid = t.id;
                self.active_touches.insert(tid, pos_px);

                match t.phase {
                    TouchPhase::Started => {
                        self.touch_scrolled = false;
                        self.touch_scroll_accum_y_px = 0.0;

                        if self.primary_touch_id.is_none() {
                            self.primary_touch_id = Some(tid);
                            self.touch_start = Some((web_time::Instant::now(), pos_px));
                        }

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
                                    rc_web::set_ime_for_textfield(&window, true);

                                    // Place caret at touch position
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        let mut st = state_rc.borrow_mut();
                                        rc::tf_place_caret_at_pointer(
                                            &mut st,
                                            hit.rect,
                                            hit.tf_multiline,
                                            pos_px,
                                            self.scale(&window),
                                            self.modifiers.shift,
                                        );
                                    }
                                }
                            }
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }

                    TouchPhase::Moved => {
                        // Handle pinch gesture with two touches
                        if self.active_touches.len() == 2 {
                            let mut it = self.active_touches.values();
                            let a = it.next().copied().unwrap();
                            let b = it.next().copied().unwrap();
                            let dx = a.0 - b.0;
                            let dy = a.1 - b.1;
                            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                            if let Some(prev) = self.pinch_last_dist.replace(dist) {
                                let delta_scale = (dist / prev).clamp(0.5, 2.0);
                                if self.dispatch_action(
                                    &window,
                                    Action::Gesture(Gesture::Pinch { delta_scale }),
                                ) {
                                    self.request_redraw();
                                }
                            }
                        }

                        // Skip scroll handling for non-primary touch
                        if self.primary_touch_id != Some(tid) {
                            self.prev_touch_px = Some(pos_px);
                            return;
                        }

                        if let (Some(prev), Some(f)) = (self.prev_touch_px, &self.frame_cache) {
                            let dy_px = pos_px.1 - prev.1;
                            if dy_px.abs() > 0.0 {
                                self.touch_scroll_accum_y_px += dy_px;

                                let consumed =
                                    rc::dispatch_scroll(f, pos, Vec2 { x: 0.0, y: -dy_px });

                                if consumed
                                    && self.touch_scroll_accum_y_px.abs()
                                        > self.touch_slop_px(&window)
                                {
                                    self.touch_scrolled = true;
                                }
                            }

                            // still deliver pointer_move to captured widget (if any)
                            if let Some(cid) = self.capture_id
                                && let Some(i) = rc::hit_index_by_id(f, cid)
                                && let Some(cb) = &f.hit_regions[i].on_pointer_move
                            {
                                cb(rc::pe_touch(
                                    repose_core::input::PointerEventKind::Move,
                                    pos,
                                    self.modifiers,
                                ));
                            }
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }

                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        self.active_touches.remove(&tid);
                        if self.active_touches.len() < 2 {
                            self.pinch_last_dist = None;
                        }

                        // Handle swipe gesture for primary touch
                        if self.primary_touch_id == Some(tid) {
                            self.primary_touch_id = None;

                            if let Some((t0, p0)) = self.touch_start.take() {
                                let dt = (web_time::Instant::now() - t0).as_secs_f32();
                                let dx = pos_px.0 - p0.0;
                                let dy = pos_px.1 - p0.1;

                                if dt < 0.35
                                    && dy.abs() < 40.0
                                    && dx.abs() > 80.0
                                    && !self.touch_scrolled
                                {
                                    let g = if dx > 0.0 {
                                        Gesture::SwipeRight
                                    } else {
                                        Gesture::SwipeLeft
                                    };

                                    // try gesture first, then common "swipe right = back"
                                    if self.dispatch_action(&window, Action::Gesture(g.clone()))
                                        || (dx > 0.0 && self.dispatch_action(&window, Action::Back))
                                    {
                                        self.capture_id = None;
                                        self.prev_touch_px = None;
                                        self.pressed_ids.clear();
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            }
                        }

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

                                // click only if we didn't scroll-drag
                                if t.phase == TouchPhase::Ended
                                    && !self.touch_scrolled
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

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                use repose_core::shortcuts::Action;

                if key_event.state == ElementState::Pressed
                    && !key_event.repeat
                    && matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Escape))
                    && self.drag.is_some()
                {
                    self.dnd_cancel();
                    return;
                }

                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    if let Some(action) = repose_core::shortcuts::resolve_action(
                        repose_core::shortcuts::KeyChord::new(
                            rc::map_key(key_event.physical_key),
                            self.modifiers,
                        ),
                    ) {
                        if self.dispatch_action(&window, action) {
                            self.request_redraw();
                            return;
                        }
                    }

                    if handle_text_undo_redo!(self, key_event) {
                        if let Some(fid) = self.sched.focused {
                            let key = self.tf_key_of(fid);
                            if let Some(state_rc) = self.textfield_states.get(&key) {
                                let mut st = state_rc.borrow_mut();
                                if let Some(f) = &self.frame_cache
                                    && let Some(i) = rc::hit_index_by_id(f, fid)
                                {
                                    self.tf_ensure_caret_visible_in_hit(
                                        &mut st,
                                        f.hit_regions[i].tf_multiline,
                                    );
                                }
                            }
                        }
                        self.request_redraw();
                        return;
                    }
                }

                // focus traversal: Tab / Shift+Tab
                if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        if let Some(f) = &self.frame_cache {
                            if let Some(next) = rc::focus_in_direction(
                                &f.focus_chain,
                                &f.hit_regions,
                                self.sched.focused,
                                if self.modifiers.shift {
                                    FocusDirection::Previous
                                } else {
                                    FocusDirection::Next
                                },
                            ) {
                                // If a button was "pressed" via keyboard, clear it when we move focus
                                if let Some(active) = self.key_pressed_active.take() {
                                    self.pressed_ids.remove(&active);
                                }

                                self.sched.focused = Some(next);

                                let tf_state_key = f.hit_regions.iter()
                                    .find(|h| h.id == next)
                                    .and_then(|h| h.tf_state_key);
                                if let Some(key) = tf_state_key {
                                    self.textfield_states.entry(key).or_insert_with(|| {
                                        Rc::new(RefCell::new(TextFieldState::new()))
                                    });
                                    if let Some(state_rc) = self.textfield_states.get(&key) {
                                        state_rc.borrow_mut().reset_caret_blink();
                                    }
                                }

                                rc_web::set_ime_for_textfield(&window, self.is_textfield(next));
                                self.request_redraw();
                            }
                        }
                    }
                    return;
                }

                handle_arrow_key_spatial_nav!(self, key_event, f, next, {
                    if let Some(active) = self.key_pressed_active.take() {
                        self.pressed_ids.remove(&active);
                    }
                    let tf_state_key = f.hit_regions.iter()
                        .find(|h| h.id == next)
                        .and_then(|h| h.tf_state_key);
                    if let Some(key) = tf_state_key {
                        self.textfield_states.entry(key).or_insert_with(|| {
                            Rc::new(RefCell::new(TextFieldState::new()))
                        });
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            state_rc.borrow_mut().reset_caret_blink();
                        }
                    }
                    rc_web::set_ime_for_textfield(&window, self.is_textfield(next));
                });

                if let Some(fid) = self.sched.focused {
                    // If focused is NOT a TextField, allow Space/Enter activation
                    let is_textfield = if let Some(f) = &self.frame_cache {
                        f.semantics_nodes
                            .iter()
                            .any(|n| n.id == fid && n.role == Role::TextField)
                    } else {
                        false
                    };

                    if !is_textfield {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::Space)
                            | PhysicalKey::Code(KeyCode::Enter) => {
                                if key_event.state == ElementState::Pressed && !key_event.repeat {
                                    self.pressed_ids.insert(fid);
                                    self.key_pressed_active = Some(fid);
                                    self.request_redraw();
                                    return;
                                } else if key_event.state == ElementState::Released {
                                    if self.pressed_ids.contains(&fid) {
                                        self.pressed_ids.remove(&fid);
                                        self.key_pressed_active = None;

                                        // Execute click
                                        if let Some(f) = &self.frame_cache {
                                            if let Some(hit) = f
                                                .hit_regions
                                                .iter()
                                                .rev()
                                                .find(|h| h.id == fid)
                                            {
                                                if let Some(cb) = &hit.on_click {
                                                    cb();
                                                } else if let Some(cb) = &hit.on_pointer_down {
                                                    let pe = repose_core::input::PointerEvent {
                                                        id: repose_core::input::PointerId(0),
                                                        kind: repose_core::input::PointerKind::Mouse,
                                                        event: repose_core::input::PointerEventKind::Down(
                                                            repose_core::input::PointerButton::Primary,
                                                        ),
                                                        position: repose_core::Vec2 {
                                                            x: 0.0,
                                                            y: 0.0,
                                                        },
                                                        pressure: 1.0,
                                                        modifiers: self.modifiers,
                                                    };
                                                    cb(pe);
                                                }
                                            }
                                        }
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }

                // Enter behavior for focused TextField
                if key_event.state == ElementState::Pressed && !key_event.repeat {
                    if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                        if let Some(focused_id) = self.sched.focused
                            && let Some(f) = &self.frame_cache
                            && let Some(i) = rc::hit_index_by_id(f, focused_id)
                        {
                            let hit = &f.hit_regions[i];
                            let is_multiline = hit.tf_multiline;
                            let should_submit = if is_multiline {
                                self.modifiers.ctrl || self.modifiers.meta
                            } else {
                                true
                            };

                            if should_submit {
                                if let Some(on_submit) = &hit.on_text_submit {
                                    let key = self.tf_key_of(focused_id);
                                    if let Some(state) = self.textfield_states.get(&key) {
                                        on_submit(state.borrow().text.clone());
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            } else {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state_rc) = self.textfield_states.get(&key) {
                                    let mut st = state_rc.borrow_mut();
                                    st.insert_text("\n");
                                    self.notify_text_change(focused_id, st.text.clone());
                                    self.tf_ensure_caret_visible_in_hit(&mut st, hit.tf_multiline);
                                    self.request_redraw();
                                    return;
                                }
                            }
                        }
                    }
                }

                // Basic TextField edit + plaintext input
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
                                    st.move_cursor(-1, self.modifiers.shift);
                                    st.preferred_x_px = None;
                                }
                                PhysicalKey::Code(KeyCode::ArrowRight) => {
                                    st.move_cursor(1, self.modifiers.shift);
                                    st.preferred_x_px = None;
                                }
                                PhysicalKey::Code(KeyCode::ArrowUp) => {
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == fid)
                                        && hit.tf_multiline
                                    {
                                        let font_px = dp_to_px(TF_FONT_DP)
                                            * repose_core::locals::text_scale().0;
                                        let pad = self.padding_px(&window);
                                        let wrap_w = hit.rect.w - 2.0 * pad;
                                        let cur = st.caret_index();
                                        let (new_pos, px) = move_caret_vertical(
                                            &st.text,
                                            font_px,
                                            wrap_w,
                                            cur,
                                            -1,
                                            st.preferred_x_px,
                                        );
                                        if self.modifiers.shift {
                                            st.selection.end = new_pos;
                                        } else {
                                            st.selection = new_pos..new_pos;
                                        }
                                        st.preferred_x_px = Some(px);
                                        // Use multiline-aware caret visibility
                                        let (cx, cy, _) = caret_xy_for_byte(
                                            &st.text,
                                            font_px,
                                            wrap_w,
                                            st.caret_index(),
                                        );
                                        let iw = st.inner_width;
                                        let ih = st.inner_height;
                                        st.ensure_caret_visible_xy(
                                            cx,
                                            cy,
                                            iw,
                                            ih,
                                            2.0 * self.scale(&window),
                                        );
                                    }
                                }
                                PhysicalKey::Code(KeyCode::ArrowDown) => {
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == fid)
                                        && hit.tf_multiline
                                    {
                                        let font_px = dp_to_px(TF_FONT_DP)
                                            * repose_core::locals::text_scale().0;
                                        let pad = self.padding_px(&window);
                                        let wrap_w = hit.rect.w - 2.0 * pad;
                                        let cur = st.caret_index();
                                        let (new_pos, px) = move_caret_vertical(
                                            &st.text,
                                            font_px,
                                            wrap_w,
                                            cur,
                                            1,
                                            st.preferred_x_px,
                                        );
                                        if self.modifiers.shift {
                                            st.selection.end = new_pos;
                                        } else {
                                            st.selection = new_pos..new_pos;
                                        }
                                        st.preferred_x_px = Some(px);
                                        // Use multiline-aware caret visibility
                                        let (cx, cy, _) = caret_xy_for_byte(
                                            &st.text,
                                            font_px,
                                            wrap_w,
                                            st.caret_index(),
                                        );
                                        let iw = st.inner_width;
                                        let ih = st.inner_height;
                                        st.ensure_caret_visible_xy(
                                            cx,
                                            cy,
                                            iw,
                                            ih,
                                            2.0 * self.scale(&window),
                                        );
                                    }
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
                                self.tf_ensure_caret_visible_in_hit(
                                    &mut st,
                                    f.hit_regions[i].tf_multiline,
                                );
                            }
                            self.request_redraw();
                        }
                    }

                    if !self.ime_preedit
                        && !self.modifiers.ctrl
                        && !self.modifiers.alt
                        && !self.modifiers.meta
                        && let Some(raw) = key_event.text.as_deref()
                    {
                        let multiline = if let Some(fid) = self.sched.focused {
                            self.frame_cache
                                .as_ref()
                                .and_then(|f| rc::hit_index_by_id(f, fid))
                                .map(|i| {
                                    self.frame_cache.as_ref().unwrap().hit_regions[i].tf_multiline
                                })
                                .unwrap_or(false)
                        } else {
                            false
                        };
                        let text: String = raw
                            .chars()
                            .filter(|c| !c.is_control() && *c != '\r' && (multiline || *c != '\n'))
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
                                        f.hit_regions[i].tf_multiline,
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
                                self.notify_text_change(focused_id, state.text.clone());

                                if let Some(f) = &self.frame_cache
                                    && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                {
                                    self.tf_ensure_caret_visible_in_hit(
                                        &mut state,
                                        f.hit_regions[i].tf_multiline,
                                    );
                                }
                                self.request_redraw();
                            }
                            Ime::Commit(text) => {
                                state.commit_composition(text);
                                self.ime_preedit = false;
                                self.notify_text_change(focused_id, state.text.clone());

                                if let Some(f) = &self.frame_cache
                                    && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                {
                                    self.tf_ensure_caret_visible_in_hit(
                                        &mut state,
                                        f.hit_regions[i].tf_multiline,
                                    );
                                }
                                self.request_redraw();
                            }
                            Ime::Disabled => {
                                self.ime_preedit = false;
                                if state.composition.is_some() {
                                    state.cancel_composition();
                                    self.notify_text_change(focused_id, state.text.clone());

                                    if let Some(f) = &self.frame_cache
                                        && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                    {
                                        self.tf_ensure_caret_visible_in_hit(
                                            &mut state,
                                            f.hit_regions[i].tf_multiline,
                                        );
                                    }
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                self.tick_snackbar();
                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);

                self.drain_render_commands();

                if self.backend.borrow().is_none() {
                    window.request_redraw();
                    return;
                }

                let scale = self.scale(&window);
                let size_px_u32 = self.sched.size;
                let focused = self.sched.focused;

                let root_fn = &mut self.root;
                let rc = self.render.clone();

                let mut composed_root = move |s: &mut Scheduler| (root_fn)(s, &rc);

                let frame = compose_frame(
                    &mut self.sched,
                    &mut composed_root,
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

                if let Some(fid) = self.sched.focused {
                    if let Some(hit) = frame.hit_regions.iter().find(|h| h.id == fid)
                        && let Some(key) = hit.tf_state_key
                        && !self.textfield_states.contains_key(&key)
                    {
                        self.textfield_states
                            .entry(key)
                            .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())))
                            .borrow_mut()
                            .reset_caret_blink();
                    }
                }

                self.frame_cache = Some(frame);
                self.last_redraw = web_time::Instant::now();

                if self.options.continuous_redraw {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if !self.options.continuous_redraw && take_frame_request() {
            self.request_redraw();
        }
    }
}
