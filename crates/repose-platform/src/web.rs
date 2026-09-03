//! Web runner (wasm32) using winit + repose-render-wgpu (async init).
use crate::common as rc;
use crate::common_web as rc_web;
use crate::render::RenderContext;
use crate::*;

use std::cell::{Cell, RefCell};
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

    /// If true, winit-web calls `preventDefault()` on the browser events it
    /// processes (mousedown/move/up, wheel, keydown, ...), suppressing text
    /// selection, touch scrolling, and similar default browser actions on the
    /// canvas. Defaults to false.
    prevent_default: bool,

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
            prevent_default: false,
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
    pub fn prevent_default(&self) -> bool {
        self.prevent_default
    }

    #[wasm_bindgen(setter)]
    pub fn set_prevent_default(&mut self, v: bool) {
        self.prevent_default = v;
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
    repose_text::ensure_web_fallback_initialized();

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
    /// Suppresses the browser context menu on the canvas so right-click reaches
    /// app code as `PointerButton::Secondary`.
    _context_menu: Closure<dyn FnMut(web_sys::MouseEvent)>,
    /// Suppresses browser middle-click autoscroll on the canvas.
    _middle_down: Closure<dyn FnMut(web_sys::MouseEvent)>,
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

    // Shared touch-scroll / pinch / swipe gesture state
    touch_gestures: rc::TouchGestureState,

    // clipboard async results
    clipboard_actions: Rc<RefCell<Vec<ClipboardAction>>>,

    external_drop_actions: Rc<RefCell<Vec<ExternalDropAction>>>,

    // keep DOM listener closures alive
    drop_listeners: Option<WebDropListeners>,
    deeplink_listener: Option<WebDeeplinkListener>,

    last_redraw: web_time::Instant,

    compose_requested: Cell<bool>,
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

            inspector: None, //  Some(repose_devtools::Inspector::new()),// Incomplete / doesn't work, so better disable it

            touch_gestures: rc::TouchGestureState::default(),

            clipboard_actions: Rc::new(RefCell::new(Vec::new())),

            external_drop_actions: Rc::new(RefCell::new(Vec::new())),
            drop_listeners: None,
            deeplink_listener: None,

            last_redraw: web_time::Instant::now(),

            compose_requested: Cell::new(false),
        }
    }

    fn request_redraw(&self) {
        self.compose_requested.set(true);
        repose_core::request_frame();
        rc::request_redraw(&self.window);
    }

    fn request_present_only(&self) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }

    fn scale(&self, window: &Window) -> f32 {
        window.scale_factor() as f32
    }

    fn is_textfield(&self, id: u64) -> bool {
        rc::is_textfield_in_frame(&self.rt.frame_cache, id)
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
                b.set_pixels_per_point(sf);
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
                ClipboardAction::PasteText(txt) => {
                    self.rt.insert_text_into_focused(&txt);
                }
            }
        }
    }

    fn dispatch_action(&mut self, window: &Window, action: repose_core::shortcuts::Action) -> bool {
        if self.rt.dispatch_action(action.clone()) {
            rc_web::set_ime_for_textfield(
                window,
                self.rt
                    .sched
                    .focused
                    .map_or(false, |id| self.rt.is_textfield(id)),
            );
            return true;
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
        repose_render_wgpu::apply_render_commands(backend, cmds);
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
            .with_prevent_default(self.options.prevent_default)
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

            // Right-click: suppress the OS/browser context menu so
            // `PointerButton::Secondary` reaches app code.
            let context_menu = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                e.prevent_default();
            }) as Box<dyn FnMut(_)>);

            // Middle-click: suppress browser autoscroll.
            let middle_down = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
                if e.button() == 1 {
                    e.prevent_default();
                }
            }) as Box<dyn FnMut(_)>);

            let _ = canvas.add_event_listener_with_callback(
                "contextmenu",
                context_menu.as_ref().unchecked_ref(),
            );
            let _ = canvas.add_event_listener_with_callback(
                "mousedown",
                middle_down.as_ref().unchecked_ref(),
            );

            self.drop_listeners = Some(WebDropListeners {
                _drag_over: drag_over,
                _drop: drop,
                _context_menu: context_menu,
                _middle_down: middle_down,
            });
        }

        let backend_cell = self.backend.clone();
        let window_for_async = window.clone();
        let msaa_samples = self.options.common.msaa_samples;
        let present_mode = self.options.common.present_mode;
        spawn_local(async move {
            match repose_render_wgpu::WgpuBackend::new_async_with_options(
                window_for_async.clone(),
                msaa_samples,
                present_mode,
            )
            .await
            {
                Ok(mut b) => {
                    let s = window_for_async.inner_size();
                    let sf = window_for_async.scale_factor() as f32;
                    b.configure_surface(s.width, s.height);
                    b.set_pixels_per_point(sf);
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

            WindowEvent::MouseInput { state, button, .. } => {
                let mapped = match button {
                    MouseButton::Left => PointerButton::Primary,
                    MouseButton::Right => PointerButton::Secondary,
                    MouseButton::Middle => PointerButton::Tertiary,
                    // Forward/Back/other buttons are not dispatched by the runtime.
                    _ => return,
                };

                let pos = Vec2 {
                    x: self.rt.mouse_pos_px.0,
                    y: self.rt.mouse_pos_px.1,
                };

                match state {
                    ElementState::Pressed => {
                        let press_result = self.rt.handle_pointer_press(pos, mapped);
                        // Platform-specific IME setup for focused textfields
                        if let Some(fid) = press_result.focused {
                            if let Some(f) = &self.rt.frame_cache
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                rc_web::set_ime_for_textfield_ex(
                                    &window,
                                    self.is_textfield(fid),
                                    hit.keyboard_type.ime_purpose_hint(),
                                    hit.auto_correct.unwrap_or(true),
                                    hit.capitalization,
                                );
                            }
                        } else {
                            rc_web::set_ime_for_textfield(&window, false);
                        }

                        if matches!(mapped, PointerButton::Tertiary)
                            && let Some(f) = &self.rt.frame_cache
                            && let Some(cid) = self.rt.capture_id
                            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                            && self.is_textfield(hit.id)
                        {
                            self.request_paste_async();
                        }

                        self.request_redraw();
                    }

                    ElementState::Released => {
                        self.rt.handle_pointer_release(pos, mapped);
                        self.request_redraw();
                    }
                }
            }

            WindowEvent::Touch(t) => {
                use repose_core::shortcuts::{Action, Gesture};

                let pos_px = (t.location.x as f32, t.location.y as f32);
                let tid = t.id;

                match t.phase {
                    TouchPhase::Started => {
                        let focused = self.touch_gestures.touch_started(&mut self.rt, tid, pos_px);

                        // Platform-specific IME setup for focused textfields
                        if let Some(fid) = focused
                            && self.is_textfield(fid)
                        {
                            let (purpose, ac, cap) = self.rt.focused_keyboard_hints();
                            rc_web::set_ime_for_textfield_ex(&window, true, purpose, ac, cap);
                        } else {
                            rc_web::set_ime_for_textfield(&window, false);
                        }

                        self.request_redraw();
                    }

                    TouchPhase::Moved => {
                        let scale = self.scale(&window);
                        let (mut dirty, pinch, pan) =
                            self.touch_gestures
                                .touch_moved(&mut self.rt, tid, pos_px, scale);

                        if let Some((delta_scale, center)) = pinch
                            && self.dispatch_action(
                                &window,
                                Action::Gesture(Gesture::PinchWithCenter {
                                    delta_scale,
                                    center,
                                }),
                            )
                        {
                            dirty = true;
                        }
                        if let Some(delta) = pan
                            && self
                                .dispatch_action(&window, Action::Gesture(Gesture::Pan { delta }))
                        {
                            dirty = true;
                        }

                        if dirty {
                            self.request_redraw();
                        }
                    }

                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        let cancelled = t.phase == TouchPhase::Cancelled;
                        let swipe_right =
                            self.touch_gestures
                                .touch_ended(&mut self.rt, tid, pos_px, cancelled);

                        let mut dirty = false;
                        if let Some(right) = swipe_right {
                            let g = if right {
                                Gesture::SwipeRight
                            } else {
                                Gesture::SwipeLeft
                            };
                            if self.dispatch_action(&window, Action::Gesture(g)) {
                                dirty = true;
                            }
                        }

                        if dirty {
                            self.request_redraw();
                        }
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
                let mapped_key = rc::map_key(key_event.physical_key, &self.rt.modifiers);
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

                if self.rt.handle_key_with_text(&ke, key_event.text.as_deref()) {
                    self.request_redraw();
                    return;
                }
            }

            WindowEvent::Ime(ime) => {
                let ime_event = match &ime {
                    Ime::Enabled => repose_core::input::ImeEvent::Start,
                    Ime::Preedit(text, cursor) => repose_core::input::ImeEvent::Update {
                        text: text.clone(),
                        cursor: cursor.map(|(a, b)| (a as usize, b as usize)),
                    },
                    Ime::Commit(text) => repose_core::input::ImeEvent::Commit(text.clone()),
                    Ime::Disabled => repose_core::input::ImeEvent::Cancel,
                };
                self.rt.handle_ime(&ime_event);
            }

            WindowEvent::RedrawRequested => {
                crate::run_pre_redraw(&self.render);

                let compose_needed = self.compose_requested.replace(false);
                // Present-only: no compose needed, just present cached scene with updated textures
                if !self.options.continuous_redraw && !compose_needed {
                    self.drain_render_commands();
                    if let (Some(backend), Some(frame)) = (
                        self.backend.borrow_mut().as_mut(),
                        self.rt.frame_cache.as_ref(),
                    ) {
                        let scale = self.scale(&window);
                        let mut scene = frame.scene.clone();
                        if let Some(inspector) = &mut self.inspector {
                            inspector.frame(&mut scene);
                        }
                        repose_core::dnd::overlay_drag_indicator(
                            &mut scene,
                            self.rt.mouse_pos_px,
                            false,
                        );
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                    } else if self.backend.borrow().is_none() {
                        window.request_redraw();
                    }
                    self.last_redraw = web_time::Instant::now();
                    return;
                }

                self.rt.tick_overlays(self.last_redraw);

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

                let output = self.rt.frame(&mut self.root, &self.render);

                // Drain upload commands queued during compose before presenting
                self.drain_render_commands();

                if !output.wants_keyboard && self.rt.sched.focused.is_some() && self.rt.ime_preedit
                {
                    rc_web::set_ime_for_textfield(&window, false);
                    self.rt.ime_preedit = false;
                }

                let frame = output.into_frame();

                if let Some(backend) = self.backend.borrow_mut().as_mut() {
                    let mut scene = frame.scene.clone();
                    if let Some(inspector) = &mut self.inspector {
                        inspector.frame(&mut scene);
                    }
                    repose_core::dnd::overlay_drag_indicator(
                        &mut scene,
                        self.rt.mouse_pos_px,
                        false,
                    );
                    backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                }

                self.rt.after_compose(&frame, scale);
                self.rt.cache_frame(frame);
                self.last_redraw = web_time::Instant::now();

                if self.options.continuous_redraw {
                    window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        crate::process_deeplinks();
        // Noto fallback manager may have fetched a font and cleared caches; trigger recompose
        if repose_text::take_fallback_dirty() {
            self.request_redraw();
        }
        if !self.options.continuous_redraw {
            let frame_requested = take_frame_request();
            let present_requested = take_present_request();
            if frame_requested {
                self.request_redraw();
            } else if present_requested && self.rt.frame_cache.is_some() {
                self.request_present_only();
            } else if let Some(deadline) = self.rt.next_wakeup_deadline() {
                let now = web_time::Instant::now();
                if self.rt.is_wakeup_due(now) {
                    self.request_redraw();
                } else {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                    return;
                }
            } else if repose_core::animation_driver::is_active() {
                self.request_redraw();
            }
        }
    }
}
