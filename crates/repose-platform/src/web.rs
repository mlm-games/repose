//! Web runner (wasm32) using winit + repose-render-wgpu (async init).
use crate::common as rc;
use crate::common_web as rc_web;
use crate::render::RenderContext;
use crate::*;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use wasm_bindgen::JsCast;
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

use repose_app::ReposeRuntime;
use repose_ui::TextFieldState;
use repose_ui::textfield::{
    TF_FONT_DP, caret_xy_for_byte, index_for_x_bytes, index_for_xy_bytes,
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

    /// Common options shared with other platforms.
    common: ReposeOptions,
}

#[wasm_bindgen]
impl WebOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(canvas_id: Option<String>) -> Self {
        Self {
            canvas_id,
            fullscreen: true,
            continuous_redraw: true,
            common: ReposeOptions::default(),
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

    #[wasm_bindgen(getter)]
    pub fn msaa_samples(&self) -> u32 {
        self.common.msaa_samples
    }

    #[wasm_bindgen(setter)]
    pub fn set_msaa_samples(&mut self, v: u32) {
        self.common.msaa_samples = v;
    }
}

#[wasm_bindgen]
pub fn run_app(options: WebOptions) -> Result<(), JsValue> {
    run_web_app(
        |_sched, _rc| repose_core::View::new(0, repose_core::ViewKind::Box),
        options,
    )
}

pub fn run_web_app(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
    options: WebOptions,
) -> Result<(), JsValue> {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Info);

    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    // Deeplink from page URL on startup.
    if let Some(w) = web_sys::window() {
        if let Ok(hash) = w.location().hash() {
            let hash = hash.trim_start_matches('#');
            if !hash.is_empty() {
                crate::push_deeplink(hash.as_bytes().to_vec());
            }
        }
    }

    let event_loop = EventLoop::new().map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
    let mut app = App::new(Box::new(root), options);

    // Listen for hash changes
    if let Some(w) = web_sys::window() {
        let location = w.location();
        let cb = Closure::wrap(Box::new(move || {
            if let Ok(hash) = location.hash() {
                let hash = hash.trim_start_matches('#');
                if !hash.is_empty() {
                    crate::push_deeplink(hash.as_bytes().to_vec());
                }
            }
        }) as Box<dyn FnMut()>);
        w.set_onhashchange(Some(cb.as_ref().unchecked_ref()));
        app.deeplink_listener = Some(WebDeeplinkListener { _hash_change: cb });
    }

    event_loop.spawn_app(app);
    Ok(())
}

struct WebDropListeners {
    _drag_over: Closure<dyn FnMut(DragEvent)>,
    _drop: Closure<dyn FnMut(DragEvent)>,
}

struct WebDeeplinkListener {
    _hash_change: Closure<dyn FnMut()>,
}

struct App {
    root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
    options: WebOptions,

    window: Option<Arc<Window>>,
    backend: Rc<RefCell<Option<repose_render_wgpu::WgpuBackend>>>,

    rt: ReposeRuntime,
    render: RenderContext,

    inspector: Option<repose_devtools::Inspector>,

    // touch click-cancel after scroll
    touch_scrolled: bool,
    scroll_capture_id: Option<u64>,
    touch_scroll_accum_x_px: f32,
    touch_scroll_accum_y_px: f32,
    prev_touch_px: Option<(f32, f32)>,

    // clipboard async results
    clipboard_actions: Rc<RefCell<Vec<ClipboardAction>>>,

    external_drop_actions: Rc<RefCell<Vec<ExternalDropAction>>>,

    // keep DOM listener closures alive
    drop_listeners: Option<WebDropListeners>,
    deeplink_listener: Option<WebDeeplinkListener>,

    // multi-touch for pinch
    active_touches: HashMap<u64, (f32, f32)>,
    primary_touch_id: Option<u64>,
    pinch_last_dist: Option<f32>,

    // swipe tracking
    touch_start: Option<(web_time::Instant, (f32, f32))>,

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
            rt: ReposeRuntime::new(),

            render: RenderContext::new(),

            inspector: Some(repose_devtools::Inspector::new()),

            touch_scrolled: false,
            scroll_capture_id: None,
            touch_scroll_accum_x_px: 0.0,
            touch_scroll_accum_y_px: 0.0,
            prev_touch_px: None,

            clipboard_actions: Rc::new(RefCell::new(Vec::new())),

            external_drop_actions: Rc::new(RefCell::new(Vec::new())),
            drop_listeners: None,
            deeplink_listener: None,

            active_touches: HashMap::new(),
            primary_touch_id: None,
            pinch_last_dist: None,
            touch_start: None,

            last_redraw: web_time::Instant::now(),
        }
    }

    fn request_redraw(&self) {
        repose_core::request_frame();
        rc::request_redraw(&self.window);
    }

    fn scale(&self, window: &Window) -> f32 {
        window.scale_factor() as f32
    }

    fn touch_slop_px(&self, window: &Window) -> f32 {
        6.0 * self.scale(window)
    }

    fn tf_key_of(&self, visual_id: u64) -> u64 {
        rc::tf_key_of_in_frame(&self.rt.frame_cache, visual_id)
    }

    fn notify_text_change(&self, id: u64, text: String) {
        if let Some(f) = &self.rt.frame_cache
            && let Some(i) = rc::hit_index_by_id(f, id)
            && let Some(cb) = &f.hit_regions[i].on_text_change
        {
            cb(text);
        }
    }

    fn is_textfield(&self, id: u64) -> bool {
        rc::is_textfield_in_frame(&self.rt.frame_cache, id)
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
        let sf = window.scale_factor() as f32;
        if (s.width, s.height) != self.rt.sched.size {
            self.rt.set_viewport_and_scale(s.width, s.height, sf);
            if let Some(b) = self.backend.borrow_mut().as_mut() {
                b.configure_surface(s.width, s.height);
            }
        }
    }

    fn copy_to_clipboard_async(&self, text: String) {
        spawn_local(async move {
            if let Ok(cb) = clipawl::Clipboard::new() {
                let _ = cb.write(&text).await;
            }
        });
    }

    fn request_paste_async(&self) {
        let actions = self.clipboard_actions.clone();
        let win = self.window.clone();

        spawn_local(async move {
            if let Ok(cb) = clipawl::Clipboard::new() {
                if let Ok(t) = cb.read().await {
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
                    let multiline = if let Some(fid) = self.rt.sched.focused {
                        self.rt.frame_cache
                            .as_ref()
                            .and_then(|f| rc::hit_index_by_id(f, fid))
                            .map(|i| self.rt.frame_cache.as_ref().unwrap().hit_regions[i].tf_multiline)
                            .unwrap_or(false)
                    } else {
                        false
                    };
                    txt.retain(|c| !c.is_control() && c != '\r' && (multiline || c != '\n'));
                    if txt.is_empty() {
                        continue;
                    }

                    if let Some(fid) = self.rt.sched.focused {
                        let key = self.tf_key_of(fid);
                        if let Some(st_rc) = self.rt.textfield_states.get(&key).cloned() {
                            let mut st = st_rc.borrow_mut();
                            st.insert_text(&txt);
                            self.notify_text_change(fid, st.text.clone());

                            if let Some(f) = &self.rt.frame_cache
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

        if let (Some(f), Some(fid)) = (&self.rt.frame_cache, self.rt.sched.focused) {
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

        // Focus navigation (Tab/arrows)
        if let Some(f) = &self.rt.frame_cache {
            if let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.rt.sched, f) {
                let tf_state_key = f
                    .hit_regions
                    .iter()
                    .find(|h| h.id == new_id)
                    .and_then(|h| h.tf_state_key);
                if let Some(key) = tf_state_key {
                    self.rt.textfield_states
                        .entry(key)
                        .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())));
                    if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                        state_rc.borrow_mut().reset_caret_blink();
                    }
                }
                rc_web::set_ime_for_textfield(&window, self.is_textfield(new_id));
                return true;
            }
        }

        // Web clipboard read is async, so Paste needs a platform fallback
        if matches!(action, repose_core::shortcuts::Action::Paste) {
            self.request_paste_async();
            return true;
        }

        false
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
    fn dispatch_dropped_files(&mut self, window: &Window, names: Vec<String>, pos_px: (f32, f32)) {
        let Some(f) = &self.rt.frame_cache else {
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

        let Some(target_id) = repose_core::dnd::dnd_target_id_at(f, pos) else {
            return;
        };

        if let Some(i) = rc::hit_index_by_id(f, target_id) {
            if let Some(cb) = &f.hit_regions[i].on_drop {
                let _accepted = cb(repose_core::dnd::DropEvent {
                    source_id: 0,
                    target_id,
                    position: pos,
                    modifiers: self.rt.modifiers,
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
            .with_prevent_default(false)
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
        let msaa_samples = self.options.common.msaa_samples;
        spawn_local(async move {
            match repose_render_wgpu::WgpuBackend::new_async_with_msaa(
                window_for_async.clone(),
                msaa_samples,
            )
            .await
            {
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

        // Clipboard read is async on web; register a no-op and handle Paste in platform fallback
        repose_core::clipboard::set_clipboard_read_fn(Box::new(|| None));

        repose_core::clipboard::set_clipboard_fn(Box::new(|text| {
            let text = text.to_string();
            spawn_local(async move {
                if let Ok(cb) = clipawl::Clipboard::new() {
                    let _ = cb.write(&text).await;
                }
            });
        }));

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
                rc::update_modifiers(&mut self.rt.modifiers, &new_mods.state());
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.rt.pointer_inside = true;
                let pos = Vec2 {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                self.rt.handle_pointer_move(pos);

                // Inspector hover (web).
                if let (Some(inspector), Some(f)) = (&mut self.inspector, &self.rt.frame_cache)
                    && inspector.hud.inspector_enabled
                {
                    let hit = f.hit_regions.iter().find(|h| h.rect.contains(pos));
                    let hover_rect = hit.map(|h| h.rect);
                    let hover_info = hit.and_then(|h| {
                        f.semantics_nodes.iter().find(|s| s.id == h.id).map(|s| {
                            repose_devtools::HoveredInfo {
                                id: s.id,
                                role: format!("{:?}", s.role),
                                label: s.label.clone(),
                            }
                        })
                    });
                    inspector.hud.set_hovered(hover_rect, hover_info);
                }

                self.request_redraw();
            }

            WindowEvent::CursorLeft { .. } => {
                self.rt.pointer_inside = false;
                self.rt.clear_hover();
                self.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.scale(&window);
                let (dx_px, dy_px) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        let unit_px = 60.0 * scale;
                        (-(x * unit_px), -(y * unit_px))
                    }
                    MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
                };
                if self.rt.handle_scroll(Vec2 { x: dx_px, y: dy_px }) {
                    self.request_redraw();
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                let pos = Vec2 {
                    x: self.rt.mouse_pos_px.0,
                    y: self.rt.mouse_pos_px.1,
                };

                match state {
                    ElementState::Pressed => {
                        let press_result = self.rt.handle_pointer_press(pos, PointerButton::Primary);
                        // Platform-specific IME setup for focused textfields
                        if let Some(fid) = press_result.focused {
                            if let Some(f) = &self.rt.frame_cache
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                rc_web::set_ime_for_textfield(&window, self.is_textfield(fid));
                            }
                        } else {
                            rc_web::set_ime_for_textfield(&window, false);
                        }
                        self.request_redraw();
                    }

                    ElementState::Released => {
                        self.rt.handle_pointer_release(pos, PointerButton::Primary);
                        self.request_redraw();
                    }
                }
            }

            WindowEvent::MouseInput {
                state,
                button: MouseButton::Middle,
                ..
            } => {
                if let Some(f) = &self.rt.frame_cache {
                    let pos = Vec2 {
                        x: self.rt.mouse_pos_px.0,
                        y: self.rt.mouse_pos_px.1,
                    };
                    match state {
                        ElementState::Pressed => {
                            if let Some(i) = rc::top_hit_index(f, pos) {
                                let hit = &f.hit_regions[i];
                                if let Some(cb) = &hit.on_pointer_down {
                                    cb(PointerEvent::new(
                                        PointerId(0),
                                        PointerKind::Mouse,
                                        PointerEventKind::Down(PointerButton::Tertiary),
                                        pos,
                                        1.0,
                                        self.rt.modifiers,
                                    ));
                                }
                                // Paste from clipboard as a best-effort for middle-click
                                if self.is_textfield(hit.id) {
                                    self.request_paste_async();
                                }
                            }
                            self.request_redraw();
                        }
                        ElementState::Released => {
                            let pos = Vec2 {
                                x: self.rt.mouse_pos_px.0,
                                y: self.rt.mouse_pos_px.1,
                            };
                            if let Some(i) = rc::top_hit_index(f, pos)
                                && let Some(cb) = &f.hit_regions[i].on_pointer_up
                            {
                                cb(PointerEvent::new(
                                    PointerId(0),
                                    PointerKind::Mouse,
                                    PointerEventKind::Up(PointerButton::Tertiary),
                                    pos,
                                    1.0,
                                    self.rt.modifiers,
                                ));
                            }
                        }
                    }
                }
            }

            WindowEvent::Touch(t) => {
                use repose_core::shortcuts::{Action, Gesture};

                let pos_px = (t.location.x as f32, t.location.y as f32);
                let pos = Vec2 {
                    x: pos_px.0,
                    y: pos_px.1,
                };

                let tid = t.id;
                self.active_touches.insert(tid, pos_px);

                match t.phase {
                    TouchPhase::Started => {
                        self.touch_scrolled = false;
                        self.scroll_capture_id = None;
                        self.touch_scroll_accum_x_px = 0.0;
                        self.touch_scroll_accum_y_px = 0.0;

                        if self.primary_touch_id.is_none() {
                            self.primary_touch_id = Some(tid);
                            self.touch_start = Some((web_time::Instant::now(), pos_px));
                        }

                        self.rt.handle_pointer_press(pos, PointerButton::Primary);

                        // Platform-specific IME setup for focused textfields
                        if let Some(fid) = self.rt.sched.focused
                            && self.is_textfield(fid)
                        {
                            rc_web::set_ime_for_textfield(&window, true);
                        } else {
                            rc_web::set_ime_for_textfield(&window, false);
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }

                    TouchPhase::Moved => {
                        // Handle pinch gesture with two touches (platform-specific)
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

                        // Touch-scroll detection + dispatch
                        if let Some(prev) = self.prev_touch_px {
                            let dx_px = pos_px.0 - prev.0;
                            let dy_px = pos_px.1 - prev.1;

                            if dx_px.abs() > 0.0 || dy_px.abs() > 0.0 {
                                self.touch_scroll_accum_x_px += dx_px;
                                self.touch_scroll_accum_y_px += dy_px;

                                if let Some(f) = &self.rt.frame_cache {
                                    let (consumed, cap) = rc::dispatch_scroll(
                                        f,
                                        pos,
                                        Vec2 {
                                            x: -dx_px,
                                            y: -dy_px,
                                        },
                                        self.scroll_capture_id,
                                    );
                                    self.scroll_capture_id = cap;

                                    if consumed
                                        && (self.touch_scroll_accum_x_px.abs()
                                            > self.touch_slop_px(&window)
                                            || self.touch_scroll_accum_y_px.abs()
                                                > self.touch_slop_px(&window))
                                    {
                                        self.touch_scrolled = true;
                                    }
                                }
                            }

                            // Delegate pointer-move to runtime for enter/leave/move dispatch
                            self.rt.handle_pointer_move(pos);
                        }

                        self.prev_touch_px = Some(pos_px);
                        self.request_redraw();
                    }

                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if t.phase == TouchPhase::Cancelled {
                            self.rt.handle_pointer_cancel();
                        } else {
                            self.rt.handle_pointer_release(pos, PointerButton::Primary);
                        }

                        self.active_touches.remove(&tid);
                        if self.active_touches.len() < 2 {
                            self.pinch_last_dist = None;
                        }

                        // Handle swipe gesture for primary touch (platform-specific)
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
                                        self.scroll_capture_id = None;
                                        self.prev_touch_px = None;
                                        self.request_redraw();
                                        return;
                                    }
                                }
                            }
                        }

                        self.scroll_capture_id = None;
                        self.prev_touch_px = None;
                        self.request_redraw();
                    }
                }
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                // Toggle devtools inspector with Ctrl+Shift+I.
                if key_event.state == ElementState::Pressed
                    && !key_event.repeat
                    && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyI)
                    && self.rt.modifiers.ctrl
                    && self.rt.modifiers.shift
                    && let Some(inspector) = &mut self.inspector
                {
                    inspector.hud.toggle_inspector();
                    self.request_redraw();
                    return;
                }

                // Convert to KeyEvent for runtime dispatch
                let mapped_key = rc::map_key(key_event.physical_key);
                let utf16 = match mapped_key {
                    repose_core::input::Key::Character(c) => c as u16,
                    _ => 0,
                };
                let ev_type = if key_event.state == ElementState::Pressed {
                    repose_core::input::KeyEventType::Down
                } else {
                    repose_core::input::KeyEventType::Up
                };

                let ke = repose_core::input::KeyEvent {
                    key: mapped_key,
                    modifiers: self.rt.modifiers,
                    is_repeat: key_event.repeat,
                    event_type: ev_type,
                    utf16_code_point: utf16,
                };

                if self.rt.handle_key(&ke) {
                    self.request_redraw();
                    return;
                }
            }

            WindowEvent::Ime(ime) => {
                let ime_event = match &ime {
                    Ime::Enabled => repose_core::input::ImeEvent::Start,
                    Ime::Preedit(text, cursor) => {
                        repose_core::input::ImeEvent::Update {
                            text: text.clone(),
                            cursor: cursor.map(|(a, b)| (a as usize, b as usize)),
                        }
                    }
                    Ime::Commit(text) => repose_core::input::ImeEvent::Commit(text.clone()),
                    Ime::Disabled => repose_core::input::ImeEvent::Cancel,
                };
                self.rt.handle_ime(&ime_event);
            }

            WindowEvent::RedrawRequested => {
                rc::tick_snackbar(self.last_redraw);

                // Advance animations before composition (Compose pattern)
                repose_core::animation_driver::tick();

                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);

                self.drain_render_commands();

                if self.backend.borrow().is_none() {
                    window.request_redraw();
                    return;
                }

                let scale = self.scale(&window);

                // Compose frame through runtime
                let root_fn = &mut self.root;
                let rc = self.render.clone();
                let frame = self.rt.compose(root_fn, &rc);

                // Drain upload commands queued during compose before presenting
                self.drain_render_commands();

                let output = repose_app::FrameOutput {
                    scene: frame.scene.clone(),
                    hit_regions: frame.hit_regions.clone(),
                    semantics_nodes: frame.semantics_nodes.clone(),
                    focus_chain: frame.focus_chain.clone(),
                    platform: repose_app::PlatformOutput {
                        cursor: self.rt.take_cursor_suggestion(),
                        ime_allowed: false,
                        ime_cursor_area: None,
                        clipboard_text: None,
                    },
                    wants_pointer: !frame.hit_regions.is_empty() || self.rt.hover_id.is_some() || self.rt.capture_id.is_some(),
                    wants_keyboard: !self.rt.textfield_states.is_empty() || self.rt.ime_preedit,
                };

                if !output.wants_keyboard && self.rt.sched.focused.is_some() && self.rt.ime_preedit {
                    rc_web::set_ime_for_textfield(&window, false);
                    self.rt.ime_preedit = false;
                }

                if let Some(backend) = self.backend.borrow_mut().as_mut() {
                    let mut scene = frame.scene.clone();
                    if let Some(inspector) = &mut self.inspector {
                        inspector.frame(&mut scene);
                    }
                    backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                }

                self.rt.reconcile_hover_from_mouse_pos(&frame);
                repose_core::dnd::set_dnd_frame(Some(frame.clone()));
                repose_core::dnd::set_dnd_scale(scale);
                self.rt.cache_frame(frame);
                self.last_redraw = web_time::Instant::now();

                if self.options.continuous_redraw {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        crate::process_deeplinks();
        if !self.options.continuous_redraw {
            if take_frame_request() {
                self.request_redraw();
            } else if take_present_request() && self.rt.frame_cache.is_some() {
                self.request_redraw();
            } else if crate::next_caret_blink_deadline(
                &self.rt.sched,
                &self.rt.frame_cache,
                &self.rt.textfield_states,
            )
            .is_some_and(|d| d <= web_time::Instant::now())
            {
                self.request_redraw();
            }
        }
    }
}
