//! Platform runners
use crate::a11y::ReposeActionHandler;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use accesskit_winit::Adapter;
use repose_core::locals::dp_to_px;
use repose_core::*;
use repose_ui::textfield::{
    self, TF_FONT_DP, TF_PADDING_X_DP, TextFieldState, TextMeasureConfig, caret_xy_for_byte, measure_text,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub mod a11y;
mod common;
#[cfg(not(target_arch = "wasm32"))]
mod common_android;
mod common_web;
pub mod render;

use common as rc;
#[cfg(not(target_arch = "wasm32"))]
use common_android as rc_android;
use common_web as rc_web;

pub use render::{ImageHandleGuard, RenderCommand, RenderContext};

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static APP_WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
static EVENT_LOOP_PROXY: OnceLock<winit::event_loop::EventLoopProxy<()>> = OnceLock::new();

/// Optional callback invoked on every AboutToWait, regardless of redraw state.
/// Used for draining cross-thread commands (e.g. tray toggles) that must be
/// processed even when the window is hidden.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static ABOUT_TO_WAIT_CALLBACK: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

static DEEPLINK_CB: Mutex<Option<Box<dyn Fn(Vec<u8>) + Send>>> = Mutex::new(None);
static PENDING_DEEPLINKS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Coarse application lifecycle state, derived from the runner's
/// `suspended` or `resumed` callbacks (eg. Android activity pause/resume).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLifecycle {
    /// Surface available and the activity is interactive (after `resumed`).
    Foreground,
    /// Surface torn down / activity no longer interactive (after `suspended`).
    Background,
}

// 0 = unknown, 1 = Foreground, 2 = Background
static CURRENT_LIFECYCLE: AtomicU8 = AtomicU8::new(0);
static LIFECYCLE_CB: Mutex<Option<Box<dyn Fn(AppLifecycle) + Send>>> = Mutex::new(None);
static PENDING_LIFECYCLE: Mutex<Vec<AppLifecycle>> = Mutex::new(Vec::new());

/// Register a callback for coarse app lifecycle (foreground/background).
///
/// Safe to call from any thread. Deliveries are coalesced to the latest state
/// and dispatched on the UI loop via `about_to_wait` (same pattern as deeplinks).
pub fn set_on_lifecycle(callback: Box<dyn Fn(AppLifecycle) + Send>) {
    *LIFECYCLE_CB.lock().unwrap() = Some(callback);
}

/// Current lifecycle state, if the runner has reported one yet.
pub fn current_lifecycle() -> Option<AppLifecycle> {
    match CURRENT_LIFECYCLE.load(Ordering::Relaxed) {
        1 => Some(AppLifecycle::Foreground),
        2 => Some(AppLifecycle::Background),
        _ => None,
    }
}

/// Queue a lifecycle transition and wake the UI loop. Called by platform runners
/// (e.g. from `suspended` / `resumed`), which already run on the UI thread.
pub(crate) fn push_lifecycle(state: AppLifecycle) {
    let code = match state {
        AppLifecycle::Foreground => 1,
        AppLifecycle::Background => 2,
    };
    CURRENT_LIFECYCLE.store(code, Ordering::Relaxed);
    PENDING_LIFECYCLE.lock().unwrap().push(state);
    #[cfg(not(target_arch = "wasm32"))]
    wake_event_loop();
}

/// Drain queued lifecycle transitions and dispatch the latest to the callback.
/// Called from each platform runner's `about_to_wait` handler.
pub(crate) fn process_lifecycle() {
    let batch = std::mem::take(&mut *PENDING_LIFECYCLE.lock().unwrap());
    if batch.is_empty() {
        return;
    }
    // Coalesce to the last state if multiple transitions fired in one pump.
    if let Some(last) = batch.last().copied()
        && let Some(cb) = LIFECYCLE_CB.lock().unwrap().as_ref()
    {
        cb(last);
    }
}

/// Register a callback to receive deeplink payloads (raw bytes)
pub fn set_on_deeplink(callback: Box<dyn Fn(Vec<u8>) + Send>) {
    *DEEPLINK_CB.lock().unwrap() = Some(callback);
}

/// Push a deeplink payload from any thread (JNI callback, CLI watcher, etc).
pub fn push_deeplink(data: Vec<u8>) {
    PENDING_DEEPLINKS.lock().unwrap().push(data);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(proxy) = EVENT_LOOP_PROXY.get() {
        let _ = proxy.send_event(());
    }
}

/// Drain queued deeplinks and dispatch them to the registered callback.
/// Called from each platform runner's `about_to_wait` handler.
pub(crate) fn process_deeplinks() {
    let mut queue = PENDING_DEEPLINKS.lock().unwrap();
    if queue.is_empty() {
        return;
    }
    let batch = std::mem::take(&mut *queue);
    drop(queue);

    if let Some(cb) = DEEPLINK_CB.lock().unwrap().as_ref() {
        for data in batch {
            cb(data);
        }
    }
}

/// Store the application window handle (called once during app setup).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_app_window(window: Arc<Window>) {
    let _ = APP_WINDOW.set(window);
}

/// Store the event loop proxy so tray commands / deeplinks can wake the event loop.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_event_loop_proxy(proxy: winit::event_loop::EventLoopProxy<()>) {
    let _ = EVENT_LOOP_PROXY.set(proxy);
}

/// Register a callback invoked on every AboutToWait (used for draining tray commands).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_about_to_wait_callback(cb: Box<dyn Fn() + Send>) {
    *ABOUT_TO_WAIT_CALLBACK.lock().unwrap() = Some(cb);
}

/// Wake the winit event loop from another thread (e.g. tray's GTK thread, JNI callback).
#[cfg(not(target_arch = "wasm32"))]
pub fn wake_event_loop() {
    if let Some(proxy) = EVENT_LOOP_PROXY.get() {
        let _ = proxy.send_event(());
    }
}

/// Show the application window.
///
/// On Wayland, unminimizing might not be supported by the protocol?
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn show_app_window() {
    WINDOW_VISIBLE.store(true, Ordering::Relaxed);
    if let Some(w) = APP_WINDOW.get() {
        log::info!("show_app_window: calling set_visible(true)");
        w.set_visible(true);
        #[allow(deprecated)]
        w.focus_window();
    }
    repose_core::frame_clock::request_frame();
    wake_event_loop();
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn hide_app_window() {
    WINDOW_VISIBLE.store(false, Ordering::Relaxed);
    if let Some(w) = APP_WINDOW.get() {
        log::info!("hide_app_window: calling set_visible(false)");
        w.set_visible(false);
    }
    repose_core::frame_clock::request_frame();
    wake_event_loop();
}

/// Returns whether the application window is currently visible.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn window_is_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Relaxed)
}

/// The close button hides the window (via ``set_visible(false)``) instead of
/// closing. The tray "Quit" action still exits the process regardless.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Relaxed);
}

/// Compose a single frame with density and text-scale applied, returning Frame.
pub fn compose_frame<F>(
    sched: &mut Scheduler,
    root_fn: &mut F,
    scale: f32,
    size_px_u32: (u32, u32),
    hover_id: Option<u64>,
    pressed_ids: &std::collections::HashSet<u64>,
    tf_states: &std::collections::HashMap<u64, Rc<RefCell<repose_ui::TextFieldState>>>,
    _focused: Option<u64>,
) -> Frame
where
    F: FnMut(&mut Scheduler) -> View,
{
    // Process any programmatic focus request from FocusRequester
    if let Some(requested_id) = repose_core::take_focus_request() {
        if requested_id == repose_core::runtime::CLEAR_FOCUS_MARKER {
            sched.focused = None;
        } else {
            sched.focused = Some(requested_id);
        }
    }

    set_density_default(Density { scale });

    // Use scheduler's focused state (which may have been updated by focus request)
    let current_focused = sched.focused;

    let frame = sched.repose(
        {
            let scale = scale;
            move |s: &mut Scheduler| with_density(Density { scale }, || (root_fn)(s))
        },
        {
            let hover_id = hover_id;
            let pressed_ids = pressed_ids.clone();
            move |view, _size| {
                let interactions = repose_ui::Interactions {
                    hover: hover_id,
                    pressed: pressed_ids.clone(),
                };

                with_density(Density { scale }, || {
                    repose_ui::layout_and_paint(
                        view,
                        size_px_u32,
                        tf_states,
                        &interactions,
                        current_focused,
                    )
                })
            }
        },
    );

    if let Some(fid) = sched.focused {
        if !frame.focus_chain.contains(&fid) {
            sched.focused = None;
        }
    }

    frame
}

pub(crate) fn next_caret_blink_deadline(
    sched: &Scheduler,
    frame_cache: &Option<Frame>,
    textfield_states: &std::collections::HashMap<u64, Rc<RefCell<TextFieldState>>>,
) -> Option<Instant> {
    let fid = sched.focused?;
    let frame = frame_cache.as_ref()?;
    let hit = frame.hit_regions.iter().find(|h| h.id == fid)?;
    let key = hit.tf_state_key?;
    textfield_states.get(&key)?.borrow().next_blink_deadline()
}

/// Helper: ensure caret visibility for a TextFieldState inside a given rect (px).
pub fn tf_ensure_visible_in_rect(state: &mut repose_ui::TextFieldState, inner_rect: Rect) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let m = measure_text(&state.text, font_px, TextMeasureConfig::default());
    let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
    state.ensure_caret_visible(
        caret_x_px,
        inner_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
        dp_to_px(2.0),
    );
}

/// Convert a winit `KeyEvent` + mapped `Key` + modifiers into a repose `KeyEvent`.
fn winit_key_to_repose(
    ev: &winit::event::KeyEvent,
    mapped_key: &repose_core::input::Key,
    mods: &repose_core::input::Modifiers,
) -> repose_core::input::KeyEvent {
    let utf16 = match mapped_key {
        repose_core::input::Key::Character(c) => *c as u16,
        _ => 0,
    };
    repose_core::input::KeyEvent {
        key: mapped_key.clone(),
        modifiers: *mods,
        is_repeat: ev.repeat,
        event_type: if ev.state == winit::event::ElementState::Pressed {
            repose_core::input::KeyEventType::Down
        } else {
            repose_core::input::KeyEventType::Up
        },
        utf16_code_point: utf16,
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn map_cursor(c: repose_core::CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
    match c {
        repose_core::CursorIcon::Default => W::Default,
        repose_core::CursorIcon::Pointer => W::Pointer,
        repose_core::CursorIcon::Text => W::Text,
        repose_core::CursorIcon::EwResize => W::EwResize,
        repose_core::CursorIcon::NsResize => W::NsResize,
        repose_core::CursorIcon::Grab => W::Grab,
        repose_core::CursorIcon::Grabbing => W::Grabbing,
    }
}

/// Options common to all platforms.
#[derive(Clone, Copy, Debug)]
pub struct ReposeOptions {
    /// MSAA sample count for the UI surface pass. The renderer falls back to
    /// the largest supported count <= this value.
    pub msaa_samples: u32,
    /// CPU-side frame rate cap. `None` = uncapped: redraws are issued as fast
    /// as the event loop allows (the GPU may still vsync via the present
    /// mode). eg: `Some(60.0)`, `Some(30.0)`.
    pub max_fps: Option<f32>,
    /// Preferred GPU present mode for the swapchain.
    pub present_mode: PresentModePref,
}

impl Default for ReposeOptions {
    fn default() -> Self {
        Self {
            msaa_samples: 4,
            max_fps: None,
            present_mode: PresentModePref::Auto,
        }
    }
}

/// Configuration for [`run_desktop_app`].
///
/// Uses [`Default`] so new options can be added without breaking existing
/// callers. Configure via struct update syntax, e.g.
/// `AppConfig { window_title: "My Game".into(), ..Default::default() }`.
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// Common options shared with other platforms.
    pub common: ReposeOptions,
    /// Window title.
    pub window_title: String,
    /// Initial window size in physical pixels.
    pub window_size: (u32, u32),
    /// Enable the devtools inspector (hover + HUD). Disable for release builds.
    pub enable_inspector: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            common: ReposeOptions::default(),
            window_title: "Repose".to_string(),
            window_size: (1280, 800),
            enable_inspector: true,
        }
    }
}

/// Run a desktop app with default [`AppConfig`].
///
/// Deprecated: use [`run_desktop_app_with_config`] with
/// `AppConfig::default()` instead. This may be removed in a future release.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
#[deprecated(
    note = "use run_desktop_app_with_config(root, AppConfig) instead; this may be removed in a future release"
)]
pub fn run_desktop_app(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
) -> anyhow::Result<()> {
    run_desktop_app_with_config(root, AppConfig::default())
}

/// Run a desktop app with the given [`AppConfig`].
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn run_desktop_app_with_config(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
    config: AppConfig,
) -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Window, WindowAttributes};

    use crate::a11y::A11yTree;
    use repose_app::ReposeRuntime;

    struct ReposeActivationHandler {
        initial_tree: Option<accesskit::TreeUpdate>,
    }

    impl accesskit::ActivationHandler for ReposeActivationHandler {
        fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.initial_tree.take()
        }
    }

    struct ReposeDeactivationHandler;

    impl accesskit::DeactivationHandler for ReposeDeactivationHandler {
        fn deactivate_accessibility(&mut self) {
            // Nothing to clean up for now
        }
    }

    struct App {
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        render: RenderContext,
        window: Option<Arc<Window>>,
        backend: Option<repose_render_wgpu::WgpuBackend>,
        rt: ReposeRuntime,
        inspector: Option<repose_devtools::Inspector>,
        msaa_samples: u32,
        max_fps: Option<f32>,
        present_mode: PresentModePref,
        window_title: String,
        window_size: (u32, u32),

        // Files
        pending_dropped_files: Vec<std::path::PathBuf>,
        pending_drop_pos_px: Option<(f32, f32)>,

        // External file drag hover (HoveredFile / Cancelled)
        external_file_drag: bool,
        hovered_files: Vec<std::path::PathBuf>,

        clipboard: Option<clipawl::Clipboard>,
        a11y: Box<dyn A11yBridge>,

        accesskit_adapter: Option<Adapter>,
        a11y_actions: Arc<Mutex<Vec<accesskit::ActionRequest>>>,
        a11y_tree: A11yTree,

        last_redraw: Instant,
        pending_redraw: bool,

        // Tracks whether a redraw was requested by app code
        redraw_requested: Cell<bool>,
    }

    impl App {
        fn process_a11y_actions(&mut self) {
            let mut actions = self.a11y_actions.lock().unwrap();
            if actions.is_empty() {
                return;
            }
            let pending = actions.drain(..).collect::<Vec<_>>();
            drop(actions);

            let Some(f) = &self.rt.frame_cache else {
                return;
            };

            for req in pending {
                let target_id = req.target_node.0;
                match req.action {
                    accesskit::Action::Click => {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id)
                            && let Some(cb) = &hit.on_click
                        {
                            cb();
                            self.request_redraw();
                        }
                    }
                    accesskit::Action::Focus => {
                        self.rt.sched.focused = Some(target_id);
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
        }

        fn new(
            root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
            config: AppConfig,
        ) -> Self {
            Self {
                root,
                render: RenderContext::new(),
                window: None,
                backend: None,
                rt: ReposeRuntime::new(),
                inspector: if config.enable_inspector {
                    Some(repose_devtools::Inspector::new())
                } else {
                    None
                },
                msaa_samples: config.common.msaa_samples,
                max_fps: config.common.max_fps,
                present_mode: config.common.present_mode,
                window_title: config.window_title,
                window_size: config.window_size,
                pending_dropped_files: Vec::new(),
                pending_drop_pos_px: None,

                external_file_drag: false,
                hovered_files: Vec::new(),

                clipboard: None,
                a11y: {
                    #[cfg(target_os = "linux")]
                    {
                        Box::new(LinuxAtspiStub) as Box<dyn A11yBridge>
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Box::new(NoopA11y) as Box<dyn A11yBridge>
                    }
                },

                accesskit_adapter: None,
                a11y_actions: Arc::new(Mutex::new(Vec::new())),
                a11y_tree: A11yTree::default(),

                last_redraw: Instant::now(),
                pending_redraw: false,
                redraw_requested: Cell::new(false),
            }
        }

        fn request_redraw(&self) {
            self.redraw_requested.set(true);
            repose_core::request_frame();
            rc::request_redraw(&self.window);
        }

        /// Minimum time between CPU-side redraw requests derived from
        /// `max_fps`. `Duration::ZERO` means uncapped (redraw immediately).
        fn frame_interval(&self) -> web_time::Duration {
            match self.max_fps.filter(|f| *f > 0.0) {
                Some(fps) => {
                    let secs = (1.0 / fps as f64).clamp(0.0, 1.0);
                    web_time::Duration::from_secs_f64(secs)
                }
                None => web_time::Duration::ZERO,
            }
        }

        // Ensure caret is visible after edits/moves (all units in px)
        fn tf_ensure_caret_visible(st: &mut TextFieldState, is_multiline: bool) {
            rc::tf_ensure_caret_visible(st, is_multiline);
        }

        fn paste_from_primary(&self) -> Option<String> {
            let mut opts = clipawl::ClipboardOptions::default();
            opts.linux.selection = clipawl::LinuxSelection::Primary;
            if let Ok(cb) = clipawl::Clipboard::new_with_options(opts) {
                match pollster::block_on(cb.read()) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        eprintln!("Primary paste error: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            rc::process_render_commands(backend, self.render.drain());
        }

        fn reset_pointer_state(&mut self) {
            self.rt.capture_id = None;
            self.rt.pressed_ids.clear();
            self.rt.hover_id = None;
        }

        fn is_textfield(&self, id: u64) -> bool {
            rc::is_textfield_in_frame(&self.rt.frame_cache, id)
        }

        fn is_multiline_id(&self, id: u64) -> bool {
            if let Some(f) = &self.rt.frame_cache {
                f.hit_regions
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.tf_multiline)
                    .unwrap_or(false)
            } else {
                false
            }
        }

        fn hit_by_id(f: &Frame, id: u64) -> Option<&HitRegion> {
            f.hit_regions.iter().find(|h| h.id == id)
        }

        fn dp_px(&self, dp: f32) -> f32 {
            dp_to_px(dp)
        }
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            self.clipboard = clipawl::Clipboard::new()
                .map_err(|e| {
                    eprintln!("clipawl clipboard init failed: {e}");
                    e
                })
                .ok();
            repose_core::clipboard::set_clipboard_read_fn(Box::new(|| {
                clipawl::blocking::read().ok()
            }));
            // Register for SelectableText (Ctrl+C) - use blocking API directly
            repose_core::clipboard::set_clipboard_fn(Box::new(move |text| {
                if let Err(e) = clipawl::blocking::write(text) {
                    eprintln!("clipboard write error: {e}");
                }
            }));

            repose_core::clipboard::set_primary_fn(Box::new(|text| {
                let mut opts = clipawl::ClipboardOptions::default();
                opts.linux.selection = clipawl::LinuxSelection::Primary;
                match clipawl::Clipboard::new_with_options(opts) {
                    Ok(cb) => {
                        if let Err(e) = pollster::block_on(cb.write(text)) {
                            eprintln!("primary selection write error: {e}");
                        }
                    }
                    Err(e) => eprintln!("primary clipboard init error: {e}"),
                }
            }));

            if self.window.is_none() {
                match el.create_window(
                    WindowAttributes::default()
                        .with_title(self.window_title.clone())
                        .with_inner_size(PhysicalSize::new(
                            self.window_size.0,
                            self.window_size.1,
                        ))
                        .with_visible(false),
                ) {
                    Ok(win) => {
                        let w = Arc::new(win);

                        let activation_handler = ReposeActivationHandler {
                            initial_tree: Some(A11yTree::initial_tree()),
                        };

                        let action_handler = ReposeActionHandler {
                            pending_actions: self.a11y_actions.clone(),
                        };

                        let deactivation_handler = ReposeDeactivationHandler;

                        let adapter = Adapter::with_direct_handlers(
                            el,
                            &w,
                            activation_handler,
                            action_handler,
                            deactivation_handler,
                        );

                        self.accesskit_adapter = Some(adapter);

                        w.set_visible(true);

                        let size = w.inner_size();
                        let sf = w.scale_factor() as f32;
                        self.rt.set_viewport_and_scale(size.width, size.height, sf);

                        match repose_render_wgpu::WgpuBackend::new_with_options(
                            w.clone(),
                            self.msaa_samples,
                            self.present_mode,
                        ) {
                            Ok(b) => {
                                self.backend = Some(b);
                                set_app_window(w.clone());
                                self.window = Some(w);
                                self.request_redraw();
                            }
                            Err(e) => {
                                log::error!("Failed to create WGPU backend: {e:?}");
                                el.exit();
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create window: {e:?}");
                        el.exit();
                    }
                }
            }
        }

        fn window_event(
            &mut self,
            el: &winit::event_loop::ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            // Process AccessKit events first!
            if let Some(adapter) = &mut self.accesskit_adapter {
                adapter.process_event(self.window.as_ref().unwrap(), &event);
            }

            match event {
                WindowEvent::CloseRequested => {
                    if CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                        // Drop GPU backend before null-buffer unmap.
                        self.backend = None;
                        if let Some(w) = &self.window {
                            w.set_visible(false);
                        }
                        WINDOW_VISIBLE.store(false, Ordering::Relaxed);
                    } else {
                        el.exit();
                    }
                }

                WindowEvent::Focused(false) => {
                    // Delegate all common focus-lost cleanup to the runtime
                    self.rt.handle_focus_lost();

                    // Platform-specific cleanup
                    self.external_file_drag = false;
                    self.hovered_files.clear();

                    if let Some(w) = &self.window {
                        rc_web::set_ime_for_textfield(w, false);
                    }

                    self.request_redraw();
                }

                WindowEvent::CursorLeft { .. } => {
                    self.rt.pointer_inside = false;
                    self.rt.clear_hover();
                    self.external_file_drag = false;
                    self.hovered_files.clear();
                    self.request_redraw();
                }

                WindowEvent::HoveredFile(path) => {
                    // Mark external drag active and keep a small bounded list
                    self.external_file_drag = true;
                    if self.hovered_files.len() < 32 {
                        self.hovered_files.push(path);
                    }
                    // Update drop position (best effort)
                    if self.pending_drop_pos_px.is_none() {
                        self.pending_drop_pos_px = Some(self.rt.mouse_pos_px);
                    }
                    self.request_redraw();
                }

                WindowEvent::HoveredFileCancelled => {
                    self.external_file_drag = false;
                    self.hovered_files.clear();

                    // Defensive: cancel any internal capture/drag that might be left stuck
                    self.reset_pointer_state();

                    self.request_redraw();
                }

                WindowEvent::DroppedFile(path) => {
                    // DroppedFile is emitted once per file. Batch them.
                    self.pending_dropped_files.push(path);
                    if self.pending_drop_pos_px.is_none() {
                        self.pending_drop_pos_px = Some(self.rt.mouse_pos_px);
                    }

                    // Drop ends the external file drag session.
                    self.external_file_drag = false;
                    self.hovered_files.clear();

                    self.request_redraw();
                }

                WindowEvent::Resized(size) => {
                    let sf = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
                    self.rt.set_viewport_and_scale(size.width, size.height, sf);
                    if let Some(b) = self.backend.as_mut() {
                        b.configure_surface(size.width, size.height);
                    }
                    if let Some(w) = &self.window {
                        let sf = w.scale_factor() as f32;
                        let dp_w = size.width as f32 / sf;
                        let dp_h = size.height as f32 / sf;
                        log::info!(
                            "Resized: fb={}x{} px, scale_factor={}, ~{}x{} dp",
                            size.width,
                            size.height,
                            sf,
                            dp_w as i32,
                            dp_h as i32
                        );
                    }
                    self.request_redraw();
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.rt.pointer_inside = true;

                    if self.external_file_drag {
                        self.pending_drop_pos_px = Some((position.x as f32, position.y as f32));
                    }

                    let pos = Vec2 {
                        x: position.x as f32,
                        y: position.y as f32,
                    };

                    // Delegate pointer-move to the host runtime
                    let result = self.rt.handle_pointer_move(pos);

                    // Inspector hover (platform-specific - devtools inspect)
                    if let (Some(inspector), Some(f)) =
                        (&mut self.inspector, &self.rt.frame_cache)
                        && inspector.hud.inspector_enabled
                    {
                        let hit = f.hit_regions.iter().find(|h| {
                            h.rect.contains(pos)
                        });
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

                    // Cursor icon via winit window
                    if let Some(win) = &self.window {
                        if let Some(c) = result.cursor {
                            win.set_cursor(winit::window::Cursor::Icon(map_cursor(c)));
                        }
                    }

                    self.request_redraw();
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let (dx_px, dy_px) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            let unit_px = dp_to_px(60.0);
                            (-(x * unit_px), -(y * unit_px))
                        }
                        MouseScrollDelta::PixelDelta(lp) => (-(lp.x as f32), -(lp.y as f32)),
                    };
                    log::debug!("MouseWheel: dx={}, dy={}", dx_px, dy_px);

                    if self.rt.handle_scroll(Vec2 { x: dx_px, y: dy_px }) {
                        self.request_redraw();
                    }
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let pos = Vec2 {
                        x: self.rt.mouse_pos_px.0,
                        y: self.rt.mouse_pos_px.1,
                    };

                    let result = self.rt.handle_pointer_press(pos, PointerButton::Primary);

                    // Platform-specific IME setup for focused textfields
                    if let Some(fid) = result.focused {
                        if let Some(win) = &self.window
                            && let Some(f) = &self.rt.frame_cache
                            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                        {
                            let sf = win.scale_factor();
                            rc_web::set_ime_for_textfield_ex(
                                win,
                                true,
                                hit.keyboard_type.ime_purpose_hint(),
                                hit.auto_correct.unwrap_or(true),
                                hit.capitalization,
                            );
                            win.set_ime_cursor_area(
                                LogicalPosition::new(
                                    hit.rect.x as f64 / sf,
                                    hit.rect.y as f64 / sf,
                                ),
                                LogicalSize::new(
                                    hit.rect.w as f64 / sf,
                                    hit.rect.h as f64 / sf,
                                ),
                            );
                        }
                    }

                    // Click outside - no focus result from runtime, drop IME
                    if result.focused.is_none() && self.rt.ime_preedit {
                        if let Some(win) = &self.window {
                            rc_web::set_ime_for_textfield(win, false);
                        }
                        self.rt.ime_preedit = false;
                    }

                    if result.needs_a11y_announce {
                        self.announce_focus_change();
                    }

                    // Inspector: click-to-select topmost widget under cursor.
                    if let Some(inspector) = &mut self.inspector
                        && inspector.hud.inspector_enabled
                        && let Some(f) = &self.rt.frame_cache
                        && let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                    {
                        let info = f
                            .semantics_nodes
                            .iter()
                            .find(|s| s.id == hit.id)
                            .map(|s| repose_devtools::HoveredInfo {
                                id: s.id,
                                role: format!("{:?}", s.role),
                                label: s.label.clone(),
                            });
                        inspector.hud.select_widget(repose_devtools::SelectedWidget {
                            id: hit.id,
                            role: info
                                .as_ref()
                                .map(|i| i.role.clone())
                                .unwrap_or_default(),
                            label: info.as_ref().and_then(|i| i.label.clone()),
                            bounds: hit.rect,
                        });
                    }

                    self.request_redraw();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Middle,
                    ..
                } => {
                    let Some(f) = &self.rt.frame_cache else {
                        return;
                    };
                    let pos = Vec2 {
                        x: self.rt.mouse_pos_px.0,
                        y: self.rt.mouse_pos_px.1,
                    };
                    if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos)) {
                        // Dispatch Tertiary pointer event
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
                        // Paste primary selection into textfield
                        if self.is_textfield(hit.id) {
                            let key = self.tf_key_of(hit.id);
                            if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                if let Some(txt) = self.paste_from_primary() {
                                    let mut st = state_rc.borrow_mut();
                                    st.insert_text_atomic(&txt);
                                    self.notify_text_change(hit.id, st.text.clone());
                                    if let Some(f) = &self.rt.frame_cache
                                        && let Some(h) =
                                            f.hit_regions.iter().find(|h| h.id == hit.id)
                                    {
                                        App::tf_ensure_caret_visible(&mut st, h.tf_multiline);
                                    }
                                }
                            }
                        }
                    }
                    self.request_redraw();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    let pos = Vec2 {
                        x: self.rt.mouse_pos_px.0,
                        y: self.rt.mouse_pos_px.1,
                    };

                    self.rt.handle_pointer_release(pos, PointerButton::Primary);

                    // A11y: announce activation when a click fires on release
                    if let (Some(f), Some(cid)) = (&self.rt.frame_cache, self.rt.capture_id) {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                            && hit.rect.contains(pos)
                            && hit.on_click.is_some()
                            && let Some(node) = f.semantics_nodes.iter().find(|n| n.id == cid)
                        {
                            let label = node.label.as_deref().unwrap_or("");
                            self.a11y.announce(&format!("Activated {}", label));
                        }
                    }

                    repose_core::request_frame();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Middle,
                    ..
                } => {
                    if let Some(f) = &self.rt.frame_cache {
                        let pos = Vec2 {
                            x: self.rt.mouse_pos_px.0,
                            y: self.rt.mouse_pos_px.1,
                        };
                        if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                        {
                            if let Some(cb) = &hit.on_pointer_up {
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

                WindowEvent::ModifiersChanged(new_mods) => {
                    let state = new_mods.state();
                    self.rt.modifiers.shift = state.shift_key();
                    self.rt.modifiers.ctrl = state.control_key();
                    self.rt.modifiers.alt = state.alt_key();
                    self.rt.modifiers.meta = state.super_key();
                    self.rt.modifiers.command = if cfg!(target_os = "macos") {
                        self.rt.modifiers.meta
                    } else {
                        self.rt.modifiers.ctrl
                    };
                }

                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    // --- Platform-specific shortcuts (before generic dispatch) ---

                    // Escape / BrowserBack: cancel DnD, try focus chain, then navigation back
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::BrowserBack)
                            | PhysicalKey::Code(KeyCode::Escape) => {
                                use repose_navigation::back;

                                if repose_core::dnd::handle_drag_action(
                                    &repose_core::shortcuts::DragAction::Cancel,
                                ) {
                                    return;
                                }

                                // Try focus-ancestor dispatch without handle_key's always-true return
                                let mapped = rc::map_key(key_event.physical_key);
                                if self.dispatch_focus_key_event(&key_event, &mapped) {
                                    self.request_redraw();
                                    return;
                                }

                                if !back::handle() {
                                    // el.exit();
                                }
                                return;
                            }
                            _ => {}
                        }
                    }

                    // Inspector hotkey: Ctrl+Shift+I
                    if let Some(inspector) = &mut self.inspector
                        && key_event.state == ElementState::Pressed
                        && self.rt.modifiers.ctrl
                        && self.rt.modifiers.shift
                        && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyI)
                    {
                        inspector.hud.toggle_inspector();
                        self.request_redraw();
                        return;
                    }

                    // Text undo/redo (Ctrl+Z / Ctrl+Shift+Z)
                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && self.rt.modifiers.command
                    {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::KeyZ) if self.rt.modifiers.shift => {
                                if let Some(fid) = self.rt.sched.focused {
                                    let key = self.tf_key_of(fid);
                                    if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                        let mut st = state_rc.borrow_mut();
                                        if st.can_redo() {
                                            st.redo();
                                            self.notify_text_change(fid, st.text.clone());
                                            self.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }
                            PhysicalKey::Code(KeyCode::KeyZ) => {
                                if let Some(fid) = self.rt.sched.focused {
                                    let key = self.tf_key_of(fid);
                                    if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                        let mut st = state_rc.borrow_mut();
                                        if st.can_undo() {
                                            st.undo();
                                            self.notify_text_change(fid, st.text.clone());
                                            self.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }

                    // --- Delegate all generic keyboard dispatch to the runtime ---

                    let mapped = rc::map_key(key_event.physical_key);
                    let ke = winit_key_to_repose(&key_event, &mapped, &self.rt.modifiers);
                    let consumed = self.rt.handle_key(&ke);
                    if consumed {
                        self.request_redraw();
                        return;
                    }

                    // --- Platform-specific text input (winit key_event.text) ---
                    // The runtime handles text via Key::Character, but we ALSO try
                    // winit's composed `key_event.text` for proper IME-less input
                    // on international keyboard layouts.
                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && !self.rt.ime_preedit
                        && !self.rt.modifiers.ctrl
                        && !self.rt.modifiers.alt
                        && !self.rt.modifiers.meta
                        && let Some(raw) = key_event.text.as_deref()
                    {
                        let text: String = raw
                            .chars()
                            .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                            .collect();
                        if !text.is_empty()
                            && let Some(fid) = self.rt.sched.focused
                        {
                            let key = self.tf_key_of(fid);
                            if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                let mut st = state_rc.borrow_mut();
                                st.insert_text(&text);
                                self.notify_text_change(fid, st.text.clone());
                                if let Some(f) = &self.rt.frame_cache
                                    && let Some(hit) =
                                        f.hit_regions.iter().find(|h| h.id == fid)
                                {
                                    App::tf_ensure_caret_visible(&mut st, hit.tf_multiline);
                                }
                                self.request_redraw();
                                return;
                            }
                        }
                    }

                    // --- A11y: keyboard activation announcement ---
                    if key_event.state == ElementState::Released
                        && let Some(active_id) = self.rt.key_pressed_active
                    {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::Space)
                            | PhysicalKey::Code(KeyCode::Enter) => {
                                if let Some(f) = &self.rt.frame_cache
                                    && let Some(node) =
                                        f.semantics_nodes.iter().find(|n| n.id == active_id)
                                {
                                    let label = node.label.as_deref().unwrap_or("");
                                    self.a11y.announce(&format!("Activated {}", label));
                                }
                            }
                            _ => {}
                        }
                    }
                }

                WindowEvent::Ime(ime) => {
                    if let Some(focused_id) = self.rt.sched.focused {
                        let key = self.tf_key_of(focused_id);
                        if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                            let mut state = state_rc.borrow_mut();
                            let on_text_change = self.rt
                                .frame_cache
                                .as_ref()
                                .and_then(|f| f.hit_regions.iter().find(|h| h.id == focused_id))
                                .and_then(|h| h.on_text_change.clone());
                            let mut notify = |text: String| {
                                if let Some(cb) = &on_text_change {
                                    cb(text);
                                }
                            };
                            rc_android::handle_ime_event(
                                ime,
                                &mut state,
                                &mut notify,
                                &mut self.rt.ime_preedit,
                            );
                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    // 1. Check our redraw flag before processing a11y.
                    if !self.redraw_requested.replace(false) {
                        self.process_a11y_actions();
                        self.process_render_commands();
                        // Present-only: redraw last cached scene with updated textures
                        if let (Some(backend), Some(frame)) =
                            (self.backend.as_mut(), self.rt.frame_cache.as_ref())
                        {
                            let scale = self.window.as_ref().map(|w| w.scale_factor() as f32).unwrap_or(1.0);
                            let mut scene = frame.scene.clone();
                            if let Some(inspector) = &mut self.inspector {
                                inspector.frame(&mut scene);
                            }
                            backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                        }
                        log::trace!("RedrawRequested: no frame request, skipping compose");
                        return;
                    }
                    log::trace!("RedrawRequested: frame request pending, composing");

                    // 2. Process a11y actions and render commands before compose.
                    self.process_a11y_actions();
                    self.process_render_commands();

                    let Some(win) = self.window.as_ref() else {
                        return;
                    };
                    if self.backend.is_none() {
                        return;
                    }

                    // Advance animations before composition (Compose pattern).
                    // Mirrors broadcastFrameClock.sendFrame() before performRecompose().
                    repose_core::animation_driver::tick();

                    let t0 = Instant::now();
                    let scale = win.scale_factor() as f32;
                    self.rt.scale = scale;
                    let focused = self.rt.sched.focused;

                    let output = self.rt.frame(&mut self.root, &self.render);

                    // Apply cursor from platform output
                    if let Some(cursor) = &output.platform.cursor {
                        win.set_cursor(winit::window::Cursor::Icon(map_cursor(*cursor)));
                    }

                    // Apply IME keyboard hints
                    if output.platform.ime_allowed {
                        rc_web::set_ime_for_textfield_ex(
                            win,
                            true,
                            output.platform.ime_purpose,
                            output.platform.ime_auto_correct,
                            output.platform.ime_capitalization,
                        );
                        if let Some((x, y, w, h)) = output.platform.ime_cursor_area {
                            win.set_ime_cursor_area(
                                LogicalPosition::new(x, y),
                                LogicalSize::new(w, h),
                            );
                        }
                    } else if self.rt.ime_preedit {
                        rc_web::set_ime_for_textfield_ex(
                            win,
                            false,
                            repose_core::ImePurposeHint::Normal,
                            true,
                            repose_core::KeyboardCapitalization::Unspecified,
                        );
                        self.rt.ime_preedit = false;
                    }

                    // Apply IME state based on wants_keyboard
                    if !output.wants_keyboard && focused.is_some() && self.rt.sched.focused.is_none() && self.rt.ime_preedit {
                        rc_web::set_ime_for_textfield(win, false);
                        self.rt.ime_preedit = false;
                    }

                    let frame = Frame {
                        scene: output.scene,
                        hit_regions: output.hit_regions,
                        semantics_nodes: output.semantics_nodes,
                        focus_chain: output.focus_chain,
                    };

                    let build_layout_ms = (Instant::now() - t0).as_secs_f32() * 1000.0;

                    // UPDATE ACCESSIBILITY TREE
                    if let Some(adapter) = &mut self.accesskit_adapter {
                        let win = self.window.as_ref().unwrap();
                        let scale = win.scale_factor();
                        if let Some(update) =
                            self.a11y_tree
                                .update(&frame.semantics_nodes, scale, self.rt.sched.focused)
                        {
                            adapter.update_if_active(|| update);
                        }
                    }

                    // Render
                    let mut scene = frame.scene.clone();
                    // Update HUD metrics before overlay draws
                    if let Some(inspector) = &mut self.inspector {
                        let widget_count = frame.semantics_nodes.len() + frame.hit_regions.len();
                        let signal_count = self.rt.sched.id_count() as usize;
                        let ls = repose_ui::last_layout_stats();
                        inspector.hud.metrics = Some(repose_devtools::Metrics {
                            build_ms: build_layout_ms,
                            layout_ms: ls.layout_time_ms,
                            paint_ms: ls.paint_time_ms,
                            scene_nodes: scene.nodes.len(),
                            widget_count,
                            signal_count,
                            taffy_created: ls.taffy_created,
                            taffy_reused: ls.taffy_reused,
                            layout_hits: ls.layout_hits,
                            layout_misses: ls.layout_misses,
                            paint_cache_hits: ls.paint_cache_hits,
                            paint_cache_misses: ls.paint_cache_misses,
                            paint_culled: ls.paint_culled,
                        });
                        inspector.frame(&mut scene);
                    }

                    // Drag indicator overlay (internal + file drop)
                    repose_core::dnd::overlay_drag_indicator(
                        &mut scene,
                        self.rt.mouse_pos_px,
                        self.external_file_drag,
                    );

                    // Drain upload commands queued during compose (e.g. VideoSink set_image_*)
                    // before presenting to avoid 1-frame GPU texture lag.
                    self.process_render_commands();

                    // Now borrow backend mutably only for the frame() call
                    let win = self.window.as_ref().unwrap();
                    let scale = win.scale_factor() as f32;
                    if let Some(backend) = self.backend.as_mut() {
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                    }

                    // Initialize TextFieldState for any focused TextField that
                    // doesn't have one yet (e.g. after FocusRequester::request_focus)
                    if let Some(fid) = self.rt.sched.focused {
                        if let Some(hit) = frame.hit_regions.iter().find(|h| h.id == fid)
                            && let Some(key) = hit.tf_state_key
                            && !self.rt.textfield_states.contains_key(&key)
                        {
                            self.rt.textfield_states
                                .entry(key)
                                .or_insert_with(|| {
                                    Rc::new(RefCell::new(repose_ui::TextFieldState::new()))
                                })
                                .borrow_mut()
                                .reset_caret_blink();
                        }
                    }

                    self.rt.reconcile_hover_from_mouse_pos(&frame);
                    repose_core::dnd::set_dnd_frame(Some(frame.clone()));
                    self.rt.frame_cache = Some(frame);
                    repose_core::dnd::set_dnd_scale(scale);

                    self.dispatch_file_drop_now();

                    rc::tick_snackbar(self.last_redraw);
                    self.last_redraw = Instant::now();
                }

                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            // Process cross-thread commands (e.g. tray toggles, deeplinks) before any
            // redraw check, so hide/show commands work even when hidden
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if let Some(cb) = ABOUT_TO_WAIT_CALLBACK.lock().unwrap().as_ref() {
                cb();
            }
            process_deeplinks();

            // On Wayland, wgpu creates an xdg_surface from the winit window and it shouldn't be recreated with a new id?
            // It doesn't take a lot of resources anyway, so let the backend be present.
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if WINDOW_VISIBLE.load(Ordering::Relaxed) && self.backend.is_none() {
                if let Some(w) = &self.window {
                    log::info!("about_to_wait: recreating GPU backend");
                    match repose_render_wgpu::WgpuBackend::new_with_options(
                        w.clone(),
                        self.msaa_samples,
                        self.present_mode,
                    ) {
                        Ok(b) => self.backend = Some(b),
                        Err(e) => log::error!("about_to_wait: failed to recreate backend: {e:?}"),
                    }
                }
            }

            let needs_compose = take_frame_request();
            let needs_present = take_present_request();

            if needs_compose {
                self.pending_redraw = true;
            }

            // Present-only: texture was updated, redraw last cached scene without compose.
            if !self.pending_redraw && needs_present && self.rt.frame_cache.is_some() {
                let now = Instant::now();
                let interval = self.frame_interval();
                if now.saturating_duration_since(self.last_redraw) >= interval {
                    rc::request_redraw(&self.window);
                    self.last_redraw = now;
                } else {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                        self.last_redraw + interval,
                    ));
                }
                return;
            }

            if !self.pending_redraw {
                let now = Instant::now();
                let idle_cap = web_time::Duration::from_millis(1000);
                let deadline = self
                    .next_caret_blink_deadline()
                    .unwrap_or(now + idle_cap);

                if now.saturating_duration_since(self.last_redraw) >= idle_cap || now >= deadline {
                    self.redraw_requested.set(true);
                    request_frame();
                    rc::request_redraw(&self.window);
                    self.last_redraw = now;
                }
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    Ord::min(deadline, now + idle_cap),
                ));
                return;
            }

            let now = Instant::now();
            let interval = self.frame_interval();

            if now.saturating_duration_since(self.last_redraw) >= interval {
                self.pending_redraw = false;
                self.redraw_requested.set(true);
                rc::request_redraw(&self.window);
                self.last_redraw = now;
            } else {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    self.last_redraw + interval,
                ));
            }
        }

        fn new_events(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::event::StartCause,
        ) {
        }
        fn user_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: ()) {
            self.pending_redraw = true;
        }
        fn device_event(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::event::DeviceId,
            _: winit::event::DeviceEvent,
        ) {
        }
        fn suspended(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
        fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
        fn memory_warning(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
    }

    impl App {
        /// Dispatch a key event through the focus ancestor chain.
        /// Returns true if the event was consumed by a handler.
        fn dispatch_focus_key_event(
            &self,
            key_event: &winit::event::KeyEvent,
            mapped_key: &repose_core::input::Key,
        ) -> bool {
            let Some(f) = &self.rt.frame_cache else {
                return false;
            };
            let Some(focused) = self.rt.sched.focused else {
                return false;
            };
            let utf16 = match mapped_key {
                repose_core::input::Key::Character(c) => *c as u16,
                _ => 0,
            };
            let mods = self.rt.modifiers;
            let repeat = key_event.repeat;
            let ev_type = if key_event.state == ElementState::Pressed {
                repose_core::input::KeyEventType::Down
            } else {
                repose_core::input::KeyEventType::Up
            };
            let hit_by_id: std::collections::HashMap<u64, &HitRegion> =
                f.hit_regions.iter().map(|h| (h.id, h)).collect();
            let sem_parent_of: std::collections::HashMap<u64, u64> = f
                .semantics_nodes
                .iter()
                .filter_map(|n| n.parent.map(|p| (n.id, p)))
                .collect();
            let mut ancestors = Vec::new();
            let mut cur = focused;
            loop {
                ancestors.push(cur);
                if let Some(&p) = sem_parent_of.get(&cur) {
                    cur = p;
                } else {
                    break;
                }
            }
            let make_ke = || repose_core::input::KeyEvent {
                key: mapped_key.clone(),
                modifiers: mods,
                is_repeat: repeat,
                event_type: ev_type,
                utf16_code_point: utf16,
            };
            // Top-down preview: root -> focused
            for &id in ancestors.iter().rev() {
                if let Some(hit) = hit_by_id.get(&id) {
                    if let Some(cb) = &hit.on_preview_key_event {
                        if cb(make_ke()) {
                            return true;
                        }
                    }
                }
            }
            // Bottom-up normal: focused -> root
            for &id in ancestors.iter() {
                if let Some(hit) = hit_by_id.get(&id) {
                    if let Some(cb) = &hit.on_key_event {
                        if cb(make_ke()) {
                            return true;
                        }
                    }
                }
            }
            false
        }

        fn announce_focus_change(&mut self) {
            if let Some(f) = &self.rt.frame_cache {
                let focused_node = self.rt
                    .sched
                    .focused
                    .and_then(|id| f.semantics_nodes.iter().find(|n| n.id == id));
                self.a11y.focus_changed(focused_node);
            }
        }

        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.rt.frame_cache
                && let Some(h) = f.hit_regions.iter().find(|h| h.id == id)
                && let Some(cb) = &h.on_text_change
            {
                cb(text);
            }
        }

        fn tf_key_of(&self, visual_id: u64) -> u64 {
            rc::tf_key_of_in_frame(&self.rt.frame_cache, visual_id)
        }

        /// If a text field is focused with a collapsed selection (caret blinking),
        /// return the [`Instant`] of the next 500 ms blink edge.
        fn next_caret_blink_deadline(&self) -> Option<Instant> {
            next_caret_blink_deadline(&self.rt.sched, &self.rt.frame_cache, &self.rt.textfield_states)
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            use repose_core::shortcuts;

            if let (Some(f), Some(fid)) = (&self.rt.frame_cache, self.rt.sched.focused)
                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                && let Some(cb) = &hit.on_action
                && cb(action.clone())
            {
                return true;
            }

            if shortcuts::handle(action.clone()) {
                return true;
            }

            // Focus navigation (Tab/arrows)
            if let Some(f) = &self.rt.frame_cache {
                if let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.rt.sched, f)
                {
                    if let Some(active) = self.rt.key_pressed_active.take() {
                        self.rt.pressed_ids.remove(&active);
                    }
                    let tf_state_key = f
                        .hit_regions
                        .iter()
                        .find(|h| h.id == new_id)
                        .and_then(|h| h.tf_state_key);
                    if let Some(key) = tf_state_key {
                        self.rt.textfield_states.entry(key).or_insert_with(|| {
                            Rc::new(RefCell::new(repose_ui::TextFieldState::new()))
                        });
                        if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                            state_rc.borrow_mut().reset_caret_blink();
                        }
                    }
                    if let Some(win) = &self.window {
                        let is_textfield = f.semantics_nodes.iter().any(|n| {
                            n.id == new_id && n.role == repose_core::semantics::Role::TextField
                        });
                        rc_web::set_ime_for_textfield(win, is_textfield);
                    }
                    self.announce_focus_change();
                    return true;
                }
            }

            false
        }

        fn dispatch_file_drop_now(&mut self) {
            let Some(f) = &self.rt.frame_cache else {
                self.pending_dropped_files.clear();
                self.pending_drop_pos_px = None;
                return;
            };

            if self.pending_dropped_files.is_empty() {
                return;
            }

            let pos_px = self.pending_drop_pos_px.unwrap_or(self.rt.mouse_pos_px);
            let pos = Vec2 {
                x: pos_px.0,
                y: pos_px.1,
            };

            let mut files = Vec::new();
            for p in self.pending_dropped_files.drain(..) {
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file")
                    .to_string();
                files.push(repose_core::dnd::DroppedFile {
                    name,
                    path: Some(p),
                });
            }

            let payload: repose_core::dnd::DragPayload =
                std::rc::Rc::new(repose_core::dnd::DroppedFiles { files });

            let Some(target_id) = repose_core::dnd::dnd_target_id_at(f, pos) else {
                self.pending_drop_pos_px = None;
                return;
            };

            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id)
                && let Some(cb) = &hit.on_drop
            {
                let accepted = cb(repose_core::dnd::DropEvent {
                    source_id: 0, // external source (OS)
                    target_id,
                    position: pos,
                    modifiers: self.rt.modifiers,
                    payload: payload.clone(),
                });

                if accepted && let Some(node) = f.semantics_nodes.iter().find(|n| n.id == target_id)
                {
                    let label = node.label.as_deref().unwrap_or("");
                    self.a11y.announce(&format!("Dropped files on {}", label));
                }
            }

            self.pending_drop_pos_px = None;
            self.request_redraw();
        }
    }

    let event_loop = EventLoop::new()?;
    set_event_loop_proxy(event_loop.create_proxy());
    let mut app = App::new(Box::new(root), config);
    // Install system clock once
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));
    event_loop.run_app(&mut app)?;
    Ok(())
}

// Accessibility bridge stub (Noop by default; logs on Linux for now)
/// Bridge from Repose's semantics tree to platform accessibility APIs.
///
/// Implementations are responsible for:
/// - Exposing nodes to the OS (AT‑SPI, Android accessibility, etc.).
/// - Updating focus when `focus_changed` is called.
/// - Announcing transient messages (e.g. button activation) via screen readers.
pub trait A11yBridge: Send {
    /// Publish (or update) the full semantics tree for the current frame.
    fn publish_tree(&mut self, nodes: &[repose_core::runtime::SemNode]);

    /// Notify that the focused node has changed. `None` means focus cleared.
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>);

    /// Announce a one‑off message via the platform's accessibility channel.
    fn announce(&mut self, msg: &str);
}

struct NoopA11y;
impl A11yBridge for NoopA11y {
    fn publish_tree(&mut self, _nodes: &[repose_core::runtime::SemNode]) {
        // no-op
    }
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>) {
        if let Some(n) = node {
            log::info!("A11y focus: {:?} {:?}", n.role, n.label);
        } else {
            log::info!("A11y focus: None");
        }
    }
    fn announce(&mut self, msg: &str) {
        log::info!("A11y announce: {msg}");
    }
}

#[cfg(target_os = "linux")]
struct LinuxAtspiStub;
#[cfg(target_os = "linux")]
impl A11yBridge for LinuxAtspiStub {
    fn publish_tree(&mut self, nodes: &[repose_core::runtime::SemNode]) {
        log::debug!("AT-SPI stub: publish {} nodes", nodes.len());
    }
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>) {
        if let Some(n) = node {
            log::info!("AT-SPI stub focus: {:?} {:?}", n.role, n.label);
        } else {
            log::info!("AT-SPI stub focus: None");
        }
    }
    fn announce(&mut self, msg: &str) {
        log::info!("AT-SPI stub announce: {msg}");
    }
}
