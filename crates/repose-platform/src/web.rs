//! Web runner (wasm32) using winit + repose-render-wgpu (async init).
//!
//! winit's web backend does not provide an editor or an IME input connection;
//! its `set_ime_allowed` and `set_ime_cursor_area` methods are no-ops. Mobile
//! and soft-keyboard editing therefore requires an application-owned hidden
//! `<input>`/`<textarea>` (or `contenteditable`) bridge. The bridge must mirror
//! the focused Repose field's value and selection, focus it on the canvas tap,
//! and forward `beforeinput`/`input`/composition events (or the resulting text
//! and selection) to the runtime; winit does not synthesize those editor events
//! for a canvas. Canvas attributes alone do not provide that bridge and are
//! not treated as text-editor support here.
use crate::common as rc;

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
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys, WindowExtWebSys};
use winit::window::Window;

use repose_app::ReposeRuntime;

#[derive(Clone, Copy, PartialEq, Eq)]
struct ClipboardPasteTarget {
    focus_id: u64,
    tf_state_key: u64,
    focus_generation: u64,
    window_focused: bool,
}

struct ClipboardAction {
    text: String,
    target: ClipboardPasteTarget,
}

enum ClipboardPasteState {
    Pending,
    Ready(ClipboardAction),
    Failed,
}

struct ClipboardPasteRequest {
    id: u64,
    target: ClipboardPasteTarget,
    state: ClipboardPasteState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BackendState {
    Pending,
    Ready,
    Failed,
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
    backend_state: Rc<Cell<BackendState>>,
    backend_generation: Rc<Cell<u64>>,
    backend_retry_at: Rc<Cell<Option<web_time::Instant>>>,
    surface_retry_pending: bool,
    surface_retry_at: Option<web_time::Instant>,
    present_retry_pending: bool,
    present_retry_at: Option<web_time::Instant>,

    rt: ReposeRuntime,
    render: RenderContext,

    inspector: Option<repose_devtools::Inspector>,

    // Shared touch-scroll / pinch / swipe gesture state
    touch_gestures: rc::TouchGestureState,

    paste_requests: Rc<RefCell<Vec<ClipboardPasteRequest>>>,
    paste_apply_index: usize,
    paste_request_generation: u64,
    focus_generation: u64,
    focused_id: Option<u64>,
    last_window_focused: bool,
    os_focused: bool,
    occluded: bool,

    external_drop_actions: Rc<RefCell<Vec<ExternalDropAction>>>,

    // keep DOM listener closures alive
    drop_listeners: Option<WebDropListeners>,
    deeplink_listener: Option<WebDeeplinkListener>,

    last_redraw: web_time::Instant,

    compose_requested: Rc<Cell<bool>>,
}

impl App {
    fn sync_focus_generation(&mut self) {
        let focused = self.rt.sched.focused;
        let window_focused = self.rt.sched.window_focused;
        if focused != self.focused_id || window_focused != self.last_window_focused {
            self.focused_id = focused;
            self.last_window_focused = window_focused;
            self.focus_generation = self.focus_generation.wrapping_add(1);
        }
    }

    fn clear_focus_state(&mut self) {
        let touch_ids = self
            .touch_gestures
            .active_touches()
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for touch_id in touch_ids {
            self.rt.handle_touch_cancel(Some(touch_id));
        }
        self.touch_gestures = rc::TouchGestureState::default();
        self.rt.handle_focus_lost();
        self.rt.touch_paths.clear();
        self.rt.scroll_capture_id = None;
        self.rt.pointer_inside = false;
        self.rt.hover_id = None;
        self.rt.hover_ancestors.clear();
        self.rt.last_focus = None;
        self.rt.sched.pointer_pos_px = None;
        self.rt.modifiers = Default::default();
        self.rt.sched.held_keys.clear();
        self.rt.sched.mouse_primary = false;
        self.rt.sched.mouse_secondary = false;
        self.rt.sched.mouse_middle = false;
        self.rt.pressed_ids.clear();
        self.rt.key_pressed_active = None;
        self.paste_requests.borrow_mut().clear();
        self.paste_apply_index = 0;
        self.paste_request_generation = self.paste_request_generation.wrapping_add(1);
        self.external_drop_actions.borrow_mut().clear();
    }

    fn poll_visibility_lifecycle(&self) {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let hidden = doc.hidden() || self.occluded;
        let want = if hidden {
            repose_app::lifecycle::AppLifecycle::Background
        } else {
            repose_app::lifecycle::AppLifecycle::Foreground
        };
        if repose_app::lifecycle::current_lifecycle() != Some(want) {
            repose_app::lifecycle::push_lifecycle(want);
        }
    }

    fn new(
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        options: WebOptions,
    ) -> Self {
        Self {
            root,
            options,
            window: None,
            backend: Rc::new(RefCell::new(None)),
            backend_state: Rc::new(Cell::new(BackendState::Pending)),
            backend_generation: Rc::new(Cell::new(0)),
            backend_retry_at: Rc::new(Cell::new(None)),
            surface_retry_pending: false,
            surface_retry_at: None,
            present_retry_pending: false,
            present_retry_at: None,
            rt: ReposeRuntime::new(),

            render: RenderContext::new(),

            inspector: None, //  Some(repose_devtools::Inspector::new()),// Incomplete / doesn't work, so better disable it

            touch_gestures: rc::TouchGestureState::default(),

            paste_requests: Rc::new(RefCell::new(Vec::new())),
            paste_apply_index: 0,
            paste_request_generation: 0,
            focus_generation: 0,
            focused_id: None,
            last_window_focused: true,
            os_focused: true,
            occluded: false,

            external_drop_actions: Rc::new(RefCell::new(Vec::new())),
            drop_listeners: None,
            deeplink_listener: None,

            last_redraw: web_time::Instant::now(),

            compose_requested: Rc::new(Cell::new(false)),
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

    /// Mirror the desktop runner's cursor handling onto the web canvas:
    /// `Some` applies the matching CSS keyword (`Hidden` -> `none`), `None`
    /// restores the default arrow. Cached per value so the DOM style is only
    /// touched on change. `Custom` encodes the pixels as a PNG data URL
    /// (browsers cap cursors ~128px, so bigger art scales down to fit);
    /// encode failures fall back to `default`, never a panic.
    fn apply_frame_cursor(&self, window: &Window, cursor: &Option<repose_core::CursorIcon>) {
        use std::cell::RefCell;
        thread_local! {
            static LAST_CURSOR: RefCell<Option<String>> = const { RefCell::new(None) };
        }
        let next: String = match cursor {
            Some(repose_core::CursorIcon::Custom(img)) => match Self::custom_cursor_css(img) {
                Some(url) => url,
                None => "default".to_string(),
            },
            other => other
                .as_ref()
                .map(rc::cursor_css)
                .unwrap_or("default")
                .to_string(),
        };
        let changed = LAST_CURSOR.with(|last| {
            if last.borrow().as_deref() != Some(next.as_str()) {
                *last.borrow_mut() = Some(next.clone());
                true
            } else {
                false
            }
        });
        if !changed {
            return;
        }
        let Some(canvas) = window.canvas() else {
            return;
        };
        let _ = canvas.style().set_property("cursor", &next);
    }

    /// Encode a `Custom` cursor as a CSS `url(data:image/png;base64,…)`
    /// value with the GML hotspot. Browsers cap cursors ~128px, so art
    /// bigger than that scales down (hotspot scales with it). Returns
    /// `None` when the pixels are malformed or PNG/base64 encoding is
    /// unavailable — callers fall back to the default arrow.
    fn custom_cursor_css(img: &repose_core::CustomCursorImage) -> Option<String> {
        const MAX: u32 = 128;
        let (w, h) = (img.size[0] as u32, img.size[1] as u32);
        if w == 0 || h == 0 || w > 2048 || h > 2048 {
            return None;
        }
        if img.rgba.len() != (w * h * 4) as usize {
            return None;
        }
        let scale = (MAX as f32 / w.max(h) as f32).min(1.0);
        let (dw, dh) = (
            (w as f32 * scale).round().max(1.0) as u32,
            (h as f32 * scale).round().max(1.0) as u32,
        );
        let mut px = Vec::with_capacity((dw * dh * 4) as usize);
        for y in 0..dh {
            for x in 0..dw {
                let sx = ((x as f32 / scale).floor() as u32).min(w - 1);
                let sy = ((y as f32 / scale).floor() as u32).min(h - 1);
                let i = ((sy * w + sx) * 4) as usize;
                px.extend_from_slice(&img.rgba[i..i + 4]);
            }
        }
        let (hx, hy) = (
            (img.hotspot[0] as f32 * scale)
                .round()
                .clamp(0.0, dw.saturating_sub(1) as f32) as u32,
            (img.hotspot[1] as f32 * scale)
                .round()
                .clamp(0.0, dh.saturating_sub(1) as f32) as u32,
        );
        let mut png = Vec::new();
        {
            use image::ImageEncoder as _;
            let enc = image::codecs::png::PngEncoder::new(&mut png);
            enc.write_image(&px, dw, dh, image::ExtendedColorType::Rgba8)
                .ok()?;
        }
        let b64 = Self::image_base64_encode(&png);
        Some(format!(
            "url(data:image/png;base64,{b64}) {hx} {hy}, default"
        ))
    }

    /// Minimal base64 (RFC 4648, padded) over bytes. Vendored so the web
    /// runner needs no new dependency for one CSS data URL.
    fn image_base64_encode(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
        for chunk in bytes.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = *chunk.get(1).unwrap_or(&0) as u32;
            let b2 = *chunk.get(2).unwrap_or(&0) as u32;
            let n = (b0 << 16) | (b1 << 8) | b2;
            out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
            out.push(if chunk.len() > 1 {
                ALPHABET[((n >> 6) & 63) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                ALPHABET[(n & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    fn is_editable_textfield(&self, id: u64) -> bool {
        self.rt
            .frame_cache
            .as_ref()
            .is_some_and(|frame| rc::is_editable_textfield_hit(frame, id))
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

    fn sync_size_from_window_at_scale(&mut self, window: &Window, scale: f32) {
        let size = window.inner_size();
        if (size.width, size.height) != self.rt.sched.size || self.rt.scale != scale {
            let mut backend = self.backend.borrow_mut();
            rc::sync_viewport(&mut self.rt, &mut *backend, size, scale);
        }
    }

    fn sync_size_from_window(&mut self, window: &Window) {
        let scale = window.scale_factor() as f32;
        self.sync_size_from_window_at_scale(window, scale);
    }

    fn handle_size_change(&mut self, window: &Window) {
        self.ensure_fullscreen_size(window);
        self.sync_size_from_window(window);
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            if let Some(backend) = self.backend.borrow_mut().as_mut() {
                backend.take_surface();
            }
            self.defer_surface_retry();
        } else if !self.surface_retry_pending {
            self.request_redraw();
        }
    }

    fn copy_to_clipboard_async(&self, text: String) {
        spawn_local(async move {
            if let Ok(cb) = clipawl::Clipboard::new() {
                let _ = cb.write(&text).await;
            }
        });
    }

    fn focused_paste_target(&mut self) -> Option<ClipboardPasteTarget> {
        self.sync_focus_generation();
        let focus_id = self.rt.sched.focused?;
        let window_focused = self.rt.sched.window_focused;
        if !window_focused || !self.is_editable_textfield(focus_id) {
            return None;
        }
        Some(ClipboardPasteTarget {
            focus_id,
            tf_state_key: self.rt.tf_key_of(focus_id),
            focus_generation: self.focus_generation,
            window_focused,
        })
    }

    fn request_paste_async(&mut self) -> bool {
        let Some(target) = self.focused_paste_target() else {
            return false;
        };
        self.paste_request_generation = self.paste_request_generation.wrapping_add(1);
        let request_id = self.paste_request_generation;
        let requests = self.paste_requests.clone();
        let request_index = {
            let mut requests = requests.borrow_mut();
            requests.push(ClipboardPasteRequest {
                id: request_id,
                target,
                state: ClipboardPasteState::Pending,
            });
            requests.len() - 1
        };
        let win = self.window.clone();

        spawn_local(async move {
            let request_target = target;
            let state = if let Ok(cb) = clipawl::Clipboard::new() {
                match cb.read().await {
                    Ok(text) => ClipboardPasteState::Ready(ClipboardAction {
                        text,
                        target: request_target,
                    }),
                    Err(error) => {
                        log::warn!("web clipboard read failed: {error:?}");
                        ClipboardPasteState::Failed
                    }
                }
            } else {
                log::warn!("web clipboard initialization failed");
                ClipboardPasteState::Failed
            };
            if let Some(request) = requests.borrow_mut().get_mut(request_index)
                && request.id == request_id
                && request.target == request_target
            {
                request.state = state;
            }
            if let Some(w) = win.as_ref() {
                w.request_redraw();
            }
        });
        true
    }

    fn apply_clipboard_actions(&mut self) {
        self.sync_focus_generation();
        let mut apply_index = self.paste_apply_index;
        let actions = {
            let mut requests = self.paste_requests.borrow_mut();
            let mut actions = Vec::new();
            while apply_index < requests.len() {
                if matches!(&requests[apply_index].state, ClipboardPasteState::Pending) {
                    break;
                }
                let target = requests[apply_index].target;
                let state = std::mem::replace(
                    &mut requests[apply_index].state,
                    ClipboardPasteState::Pending,
                );
                if let ClipboardPasteState::Ready(action) = state
                    && action.target == target
                {
                    actions.push(action);
                }
                apply_index += 1;
            }
            if apply_index == requests.len() {
                requests.clear();
                apply_index = 0;
            }
            actions
        };
        self.paste_apply_index = apply_index;

        let mut changed = false;
        for action in actions {
            self.sync_focus_generation();
            let target = action.target;
            if target.window_focused
                && self.rt.sched.window_focused
                && self.rt.sched.focused == Some(target.focus_id)
                && self.focus_generation == target.focus_generation
                && self.rt.tf_key_of(target.focus_id) == target.tf_state_key
                && self.is_editable_textfield(target.focus_id)
            {
                self.rt.paste_into_focused(&action.text);
                changed = true;
            }
        }
        if changed {
            self.request_redraw();
        }
    }

    fn dispatch_action(
        &mut self,
        _window: &Window,
        action: repose_core::shortcuts::Action,
    ) -> bool {
        if self.rt.dispatch_action(action.clone()) {
            return true;
        }

        if matches!(action, repose_core::shortcuts::Action::Paste)
            && self
                .rt
                .sched
                .focused
                .is_some_and(|id| self.is_editable_textfield(id))
        {
            return self.request_paste_async();
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
    fn dispatch_dropped_files(&mut self, _window: &Window, names: Vec<String>, pos_px: (f32, f32)) {
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

    fn setup_web_clipboard() {
        repose_core::clipboard::set_clipboard_read_fn(Box::new(|| None));
        repose_core::clipboard::set_clipboard_fn(Box::new(|text| {
            let text = text.to_string();
            spawn_local(async move {
                if let Ok(cb) = clipawl::Clipboard::new() {
                    let _ = cb.write(&text).await;
                }
            });
        }));
    }

    fn try_recreate_surface(&mut self) -> bool {
        let Some(window) = self.window.clone() else {
            return false;
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return false;
        }
        let result = {
            let mut backend_ref = self.backend.borrow_mut();
            let Some(backend) = backend_ref.as_mut() else {
                return false;
            };
            backend.recreate_surface(&window)
        };
        match result {
            Ok(()) => {
                let scale = window.scale_factor() as f32;
                let mut backend_ref = self.backend.borrow_mut();
                rc::sync_viewport(&mut self.rt, &mut backend_ref, size, scale);
                true
            }
            Err(error) => {
                log::warn!("web surface recreation failed: {error:?}");
                false
            }
        }
    }

    fn recover_missing_surface(&mut self) -> bool {
        let missing = self
            .backend
            .borrow()
            .as_ref()
            .is_some_and(|backend| backend.surface.is_none());
        if !missing {
            if self.surface_retry_pending {
                self.surface_retry_pending = false;
                self.surface_retry_at = None;
                self.present_retry_pending = false;
                self.present_retry_at = None;
                self.compose_requested.set(true);
                repose_core::request_frame();
                self.request_redraw();
            }
            return true;
        }
        if self.try_recreate_surface() {
            self.surface_retry_pending = false;
            self.surface_retry_at = None;
            self.present_retry_pending = false;
            self.present_retry_at = None;
            self.compose_requested.set(true);
            repose_core::request_frame();
            self.request_redraw();
            true
        } else {
            self.defer_surface_retry();
            false
        }
    }

    fn defer_surface_retry(&mut self) {
        take_frame_request();
        self.surface_retry_pending = true;
        self.surface_retry_at =
            Some(web_time::Instant::now() + web_time::Duration::from_millis(100));
        self.present_retry_pending = false;
        self.present_retry_at = None;
        self.compose_requested.set(false);
    }

    fn defer_present_retry(&mut self) {
        take_frame_request();
        self.present_retry_pending = true;
        self.present_retry_at =
            Some(web_time::Instant::now() + web_time::Duration::from_millis(100));
        self.compose_requested.set(false);
    }

    fn handle_frame_result(&mut self, presented: bool) {
        if presented {
            self.surface_retry_pending = false;
            self.surface_retry_at = None;
            self.present_retry_pending = false;
            self.present_retry_at = None;
            return;
        }
        let missing = self
            .backend
            .borrow()
            .as_ref()
            .is_some_and(|backend| backend.surface.is_none());
        if missing {
            self.recover_missing_surface();
        } else {
            self.defer_present_retry();
        }
    }

    fn start_backend(&mut self, window: Arc<Window>) {
        self.surface_retry_pending = false;
        self.surface_retry_at = None;
        self.present_retry_pending = false;
        self.present_retry_at = None;
        self.backend_generation
            .set(self.backend_generation.get().wrapping_add(1));
        let request_generation = self.backend_generation.get();
        self.backend_state.set(BackendState::Pending);
        self.backend_retry_at.set(None);
        *self.backend.borrow_mut() = None;
        self.compose_requested.set(true);
        window.request_redraw();
        let backend_cell = self.backend.clone();
        let backend_state = self.backend_state.clone();
        let backend_generation = self.backend_generation.clone();
        let backend_retry_at = self.backend_retry_at.clone();
        let compose_requested = self.compose_requested.clone();
        let msaa_samples = self.options.common.msaa_samples;
        let present_mode = self.options.common.present_mode;
        spawn_local(async move {
            let result = repose_render_wgpu::WgpuBackend::new_async_with_options(
                window.clone(),
                msaa_samples,
                present_mode,
            )
            .await;
            if backend_generation.get() != request_generation {
                return;
            }
            match result {
                Ok(mut b) => {
                    let size = window.inner_size();
                    let scale = window.scale_factor() as f32;
                    b.configure_surface(size.width, size.height);
                    b.set_pixels_per_point(scale);
                    repose_render_wgpu::offscreen::set_shared_device(
                        b.device.clone(),
                        b.queue.clone(),
                    );
                    *backend_cell.borrow_mut() = Some(b);
                    backend_state.set(BackendState::Ready);
                    backend_retry_at.set(None);
                    compose_requested.set(true);
                    repose_core::request_frame();
                    window.request_redraw();
                    log::info!("WGPU backend initialized");
                }
                Err(e) => {
                    *backend_cell.borrow_mut() = None;
                    backend_state.set(BackendState::Failed);
                    backend_retry_at.set(Some(
                        web_time::Instant::now() + web_time::Duration::from_millis(250),
                    ));
                    compose_requested.set(false);
                    log::error!("WGPU init failed: {e:?}");
                    window.request_redraw();
                }
            }
        });
    }

    fn install_dom_listeners(&mut self, window: &Window) {
        let Some(canvas) = window.canvas() else {
            return;
        };
        let drag_over = Closure::wrap(Box::new(move |e: DragEvent| {
            e.prevent_default();
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

        let _ =
            canvas.add_event_listener_with_callback("dragover", drag_over.as_ref().unchecked_ref());
        let _ = canvas.add_event_listener_with_callback("drop", drop.as_ref().unchecked_ref());

        let context_menu = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
            e.prevent_default();
        }) as Box<dyn FnMut(_)>);
        let middle_down = Closure::wrap(Box::new(move |e: web_sys::MouseEvent| {
            if e.button() == 1 {
                e.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);

        let _ = canvas
            .add_event_listener_with_callback("contextmenu", context_menu.as_ref().unchecked_ref());
        let _ = canvas
            .add_event_listener_with_callback("mousedown", middle_down.as_ref().unchecked_ref());

        self.drop_listeners = Some(WebDropListeners {
            _drag_over: drag_over,
            _drop: drop,
            _context_menu: context_menu,
            _middle_down: middle_down,
        });
    }

    fn finish_window_setup(&mut self, window: Arc<Window>) {
        self.inject_fullscreen_css_if_needed(&window);
        if let Some(canvas) = window.canvas() {
            let _ = canvas.focus();
        }
        self.ensure_fullscreen_size(&window);
        self.sync_size_from_window(&window);
        self.window = Some(window.clone());
        self.install_dom_listeners(&window);
        self.start_backend(window);
        Self::setup_web_clipboard();
        self.request_redraw();
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
            let canvas = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| document.get_element_by_id(&id))
                .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
            match canvas {
                Some(canvas) => attrs = attrs.with_canvas(Some(canvas)).with_append(false),
                None => {
                    log::error!(
                        "Canvas id '{id}' is unavailable or not a canvas; using a new canvas"
                    );
                    attrs = attrs.with_canvas(None).with_append(true);
                }
            }
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
        self.finish_window_setup(window);
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

        self.sync_focus_generation();
        if !matches!(
            &event,
            WindowEvent::Focused(false) | WindowEvent::Occluded(true)
        ) {
            self.apply_clipboard_actions();
        }
        self.apply_external_drop_actions(&window);

        match event {
            WindowEvent::CloseRequested => el.exit(),

            WindowEvent::Resized(_) => {
                self.handle_size_change(&window);
            }

            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window_at_scale(&window, scale_factor as f32);
                let size = window.inner_size();
                if size.width == 0 || size.height == 0 {
                    if let Some(backend) = self.backend.borrow_mut().as_mut() {
                        backend.take_surface();
                    }
                    self.defer_surface_retry();
                } else if !self.surface_retry_pending {
                    self.request_redraw();
                }
            }

            WindowEvent::Focused(focused) => {
                self.os_focused = focused;
                let focused = focused && !self.occluded;
                self.rt.sched.window_focused = focused;
                if !focused {
                    self.clear_focus_state();
                }
                self.sync_focus_generation();
                self.request_redraw();
            }

            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                if occluded {
                    self.rt.sched.window_focused = false;
                    self.clear_focus_state();
                } else {
                    self.rt.sched.window_focused = self.os_focused;
                }
                self.poll_visibility_lifecycle();
                self.sync_focus_generation();
                self.request_redraw();
            }

            WindowEvent::ModifiersChanged(new_mods) => {
                crate::runner_common::on_modifiers_changed(&mut self.rt, &new_mods.state());
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.rt.pointer_inside = true;
                let pos = Vec2 {
                    x: position.x as f32,
                    y: position.y as f32,
                };
                crate::runner_common::on_cursor_moved(&mut self.rt, pos, &mut self.inspector);
                self.request_redraw();
            }

            WindowEvent::CursorLeft { .. } => {
                self.rt.pointer_inside = false;
                self.rt.handle_pointer_cancel();
                self.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.scale(&window);
                if crate::runner_common::on_mouse_wheel(&mut self.rt, delta, scale) {
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
                match (mapped, state) {
                    (PointerButton::Primary, ElementState::Pressed) => {
                        self.rt.sched.mouse_primary = true;
                    }
                    (PointerButton::Primary, ElementState::Released) => {
                        self.rt.sched.mouse_primary = false;
                    }
                    (PointerButton::Secondary, ElementState::Pressed) => {
                        self.rt.sched.mouse_secondary = true;
                    }
                    (PointerButton::Secondary, ElementState::Released) => {
                        self.rt.sched.mouse_secondary = false;
                    }
                    (PointerButton::Tertiary, ElementState::Pressed) => {
                        self.rt.sched.mouse_middle = true;
                    }
                    (PointerButton::Tertiary, ElementState::Released) => {
                        self.rt.sched.mouse_middle = false;
                    }
                }

                let pos = Vec2 {
                    x: self.rt.mouse_pos_px.0,
                    y: self.rt.mouse_pos_px.1,
                };

                match state {
                    ElementState::Pressed => {
                        self.rt.handle_pointer_press(pos, mapped);

                        if matches!(mapped, PointerButton::Tertiary)
                            && let Some(f) = &self.rt.frame_cache
                            && let Some(cid) = self.rt.capture_id
                            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                            && self.rt.sched.focused == Some(hit.id)
                            && self.is_editable_textfield(hit.id)
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
                let scale = self.scale(&window);
                if t.phase == winit::event::TouchPhase::Started {
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    self.touch_gestures.contact_down(t.id, pos_px);
                    self.touch_gestures
                        .touch_started(&mut self.rt, t.id, pos_px);
                    crate::runner_common::sync_touch_points(&mut self.rt, &self.touch_gestures);
                    self.request_redraw();
                } else {
                    let r = crate::runner_common::handle_touch_raw(
                        &mut self.rt,
                        &mut self.touch_gestures,
                        &t,
                        scale,
                    );
                    let mut dirty = r.dirty;
                    if let Some((delta_scale, center)) = r.pinch {
                        if self.dispatch_action(
                            &window,
                            repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::PinchWithCenter {
                                    delta_scale,
                                    center,
                                },
                            ),
                        ) {
                            dirty = true;
                        }
                    }
                    if let Some((delta, center)) = r.pan {
                        if self.dispatch_action(
                            &window,
                            repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::Pan { delta, center },
                            ),
                        ) {
                            dirty = true;
                        }
                    }
                    if let Some((delta_rotation, center)) = r.rotation {
                        if self.dispatch_action(
                            &window,
                            repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::Rotate {
                                    delta_rotation,
                                    center,
                                },
                            ),
                        ) {
                            dirty = true;
                        }
                    }
                    if let Some(right) = r.swipe_right {
                        let g = if right {
                            repose_core::shortcuts::Gesture::SwipeRight
                        } else {
                            repose_core::shortcuts::Gesture::SwipeLeft
                        };
                        if self.dispatch_action(&window, repose_core::shortcuts::Action::Gesture(g))
                        {
                            dirty = true;
                        }
                    }
                    if dirty {
                        self.request_redraw();
                    }
                }
            }

            WindowEvent::KeyboardInput {
                event: key_event, ..
            } => {
                if crate::runner_common::on_keyboard_input(
                    &mut self.rt,
                    &key_event,
                    &mut self.inspector,
                ) {
                    self.request_redraw();
                    return;
                }

                if key_event.state == ElementState::Pressed
                    && !key_event.repeat
                    && self
                        .rt
                        .shortcuts
                        .resolve_action(&repose_core::shortcuts::KeyChord::new(
                            rc::map_key(key_event.physical_key, &self.rt.modifiers),
                            self.rt.modifiers,
                        ))
                        == Some(repose_core::shortcuts::Action::Paste)
                    && self
                        .rt
                        .sched
                        .focused
                        .is_some_and(|id| self.is_editable_textfield(id))
                    && self.request_paste_async()
                {
                    self.request_redraw();
                    return;
                }

                if key_event.state == ElementState::Pressed
                    && !key_event.repeat
                    && (rc::is_back_key(&key_event) || rc::is_escape_key(&key_event))
                {
                    use repose_navigation::back;
                    if back::handle() {
                        self.request_redraw();
                        return;
                    }
                    if rc::is_back_key(&key_event) {
                        el.exit();
                        return;
                    }
                }
            }

            WindowEvent::Ime(ime) => {
                crate::runner_common::on_ime(&mut self.rt, &ime);
            }

            WindowEvent::RedrawRequested => {
                crate::run_pre_redraw(&self.render);

                match self.backend_state.get() {
                    BackendState::Pending => {
                        self.compose_requested.set(true);
                        return;
                    }
                    BackendState::Failed => {
                        self.compose_requested.set(false);
                        return;
                    }
                    BackendState::Ready => {}
                }

                let surface_missing = self
                    .backend
                    .borrow()
                    .as_ref()
                    .is_some_and(|backend| backend.surface.is_none());
                if surface_missing {
                    self.recover_missing_surface();
                    return;
                }

                let compose_needed = self.compose_requested.replace(false);
                if !self.options.continuous_redraw && !compose_needed {
                    self.drain_render_commands();
                    let presented = if let (Some(backend), Some(frame)) = (
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
                        Some(backend.frame(
                            &scene,
                            GlyphRasterConfig {
                                px: Px(18.0 * scale),
                            },
                        ))
                    } else {
                        None
                    };
                    if let Some(presented) = presented {
                        self.handle_frame_result(presented);
                        if !presented {
                            return;
                        }
                    }
                    self.last_redraw = web_time::Instant::now();
                    return;
                }

                self.rt.tick_overlays();

                repose_core::animation_driver::tick();

                self.ensure_fullscreen_size(&window);
                self.sync_size_from_window(&window);

                self.drain_render_commands();

                if self.backend.borrow().is_none() {
                    return;
                }

                let scale = self.scale(&window);

                let output = self.rt.frame(&mut self.root, &self.render);
                self.drain_render_commands();

                self.apply_frame_cursor(&window, &output.platform.cursor);

                let frame = output.into_frame();

                let presented = if let Some(backend) = self.backend.borrow_mut().as_mut() {
                    let mut scene = frame.scene.clone();
                    if let Some(inspector) = &mut self.inspector {
                        inspector.frame(&mut scene);
                    }
                    repose_core::dnd::overlay_drag_indicator(
                        &mut scene,
                        self.rt.mouse_pos_px,
                        false,
                    );
                    backend.frame(
                        &scene,
                        GlyphRasterConfig {
                            px: Px(18.0 * scale),
                        },
                    )
                } else {
                    false
                };

                self.rt.after_compose(&frame, scale);
                self.rt.cache_frame(frame);
                self.handle_frame_result(presented);
                if !presented {
                    return;
                }
                self.sync_focus_generation();
                self.last_redraw = web_time::Instant::now();

                if self.options.continuous_redraw {
                    window.request_redraw();
                }
            }

            _ => {}
        }
        self.sync_focus_generation();
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        crate::process_deeplinks();
        crate::process_lifecycle();
        self.poll_visibility_lifecycle();
        if !self.rt.take_rumble_requests().is_empty() {
            log::warn!("gamepad: rumble not supported on web");
        }
        if repose_text::take_fallback_dirty() {
            self.request_redraw();
        }

        match self.backend_state.get() {
            BackendState::Pending => {
                self.compose_requested.set(true);
                el.set_control_flow(winit::event_loop::ControlFlow::Wait);
                return;
            }
            BackendState::Failed => {
                let now = web_time::Instant::now();
                if let Some(retry_at) = self.backend_retry_at.get()
                    && now >= retry_at
                    && let Some(window) = self.window.clone()
                {
                    self.start_backend(window);
                } else if let Some(retry_at) = self.backend_retry_at.get() {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                } else {
                    self.compose_requested.set(false);
                }
                if self.window.is_none() {
                    self.backend_retry_at.set(None);
                }
                return;
            }
            BackendState::Ready => {}
        }

        if self.surface_retry_pending {
            let now = web_time::Instant::now();
            if let Some(retry_at) = self.surface_retry_at
                && now < retry_at
            {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                return;
            }
            if self.backend.borrow().is_none() {
                self.surface_retry_pending = false;
                self.surface_retry_at = None;
                return;
            }
            if self.recover_missing_surface() {
                return;
            }
            if let Some(retry_at) = self.surface_retry_at {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
            }
            return;
        }

        if self.present_retry_pending {
            let now = web_time::Instant::now();
            if let Some(retry_at) = self.present_retry_at
                && now < retry_at
            {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                return;
            }
            self.present_retry_pending = false;
            self.present_retry_at = None;
            self.compose_requested.set(true);
            repose_core::request_frame();
            self.request_redraw();
            return;
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
