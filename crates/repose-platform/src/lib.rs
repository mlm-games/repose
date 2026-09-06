//! Platform runners
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use accesskit_winit::Adapter;
use repose_core::a11y::ReposeActionHandler;
use repose_core::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod web;

mod common;
pub mod render;
pub mod runner_common;
pub mod window_v2;

use common as rc;

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
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static ABOUT_TO_WAIT_CALLBACK: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

pub use repose_app::lifecycle::{
    AppLifecycle, current_lifecycle, process_deeplinks, process_lifecycle, run_pre_redraw,
    set_on_deeplink, set_on_lifecycle, set_pre_redraw,
};

/// Queue a lifecycle transition and wake the UI loop. Thin wrapper over `repose_app`.
#[cfg(target_os = "android")]
pub(crate) fn push_lifecycle(state: AppLifecycle) {
    repose_app::lifecycle::push_lifecycle(state);
    #[cfg(not(target_arch = "wasm32"))]
    wake_event_loop();
}

/// Push a deeplink payload and wake the UI loop. Thin wrapper over `repose_app`.
pub fn push_deeplink(data: Vec<u8>) {
    repose_app::lifecycle::push_deeplink(data);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(proxy) = EVENT_LOOP_PROXY.get() {
        let _ = proxy.send_event(());
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
    *ABOUT_TO_WAIT_CALLBACK
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(cb);
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

/// Helper: ensure caret visibility - now in `repose_ui::textfield::tf_ensure_visible_in_rect`.
pub use repose_ui::textfield::tf_ensure_visible_in_rect;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use common::map_cursor;

pub use repose_app::{AndroidOptions, AppConfig, ReposeOptions};

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
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, MouseButton, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Window, WindowAttributes};

    use repose_app::ReposeRuntime;
    use repose_core::a11y::A11yTree;

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

        // Last applied OS window theme (dark/light) to avoid spamming set_theme.
        last_window_theme: Option<bool>,

        last_redraw: Instant,
        pending_redraw: bool,

        // Tracks whether a redraw was requested by app code
        redraw_requested: Cell<bool>,

        // Shared touch-scroll / pinch / swipe gesture state (touchscreens)
        touch_gestures: rc::TouchGestureState,
    }

    impl App {
        fn process_a11y_actions(&mut self) {
            let mut actions = self.a11y_actions.lock().unwrap_or_else(|e| e.into_inner());
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
                        // Assistive tech focus should show the focus ring.
                        let _ = repose_core::request_input_mode(repose_core::InputMode::Keyboard);
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
                last_window_theme: None,
                redraw_requested: Cell::new(false),
                touch_gestures: rc::TouchGestureState::default(),
            }
        }

        fn request_redraw(&self) {
            self.redraw_requested.set(true);
            repose_core::request_frame();
            rc::request_redraw(&self.window);
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            if self.rt.dispatch_action(action) {
                if let Some(win) = &self.window {
                    rc::set_ime_for_textfield(
                        win,
                        self.rt
                            .sched
                            .focused
                            .is_some_and(|id| self.rt.is_textfield(id)),
                    );
                }
                return true;
            }
            false
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
            repose_render_wgpu::apply_render_commands(backend, self.render.drain());
        }

        fn reset_pointer_state(&mut self) {
            self.rt.capture_id = None;
            self.rt.pressed_ids.clear();
            self.rt.hover_id = None;
        }
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            self.clipboard = rc::setup_clipboard();

            if self.window.is_none() {
                match el.create_window(
                    WindowAttributes::default()
                        .with_title(self.window_title.clone())
                        .with_inner_size(PhysicalSize::new(self.window_size.0, self.window_size.1))
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
                            Ok(mut b) => {
                                b.set_pixels_per_point(sf);
                                repose_render_wgpu::offscreen::set_shared_device(
                                    b.device.clone(),
                                    b.queue.clone(),
                                );
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
            if let (Some(adapter), Some(window)) = (&mut self.accesskit_adapter, &self.window) {
                adapter.process_event(window, &event);
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
                        rc::set_ime_for_textfield(w, false);
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
                    let sf = self
                        .window
                        .as_ref()
                        .map(|w| w.scale_factor() as f32)
                        .unwrap_or(1.0);
                    rc::sync_viewport(&mut self.rt, &mut self.backend, size, sf);
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

                    // Cursor icon via winit window
                    if let Some(win) = &self.window
                        && let Some(c) = result.cursor
                    {
                        win.set_cursor(winit::window::Cursor::Icon(map_cursor(c)));
                    }

                    self.request_redraw();
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    let scale = self
                        .window
                        .as_ref()
                        .map(|w| w.scale_factor() as f32)
                        .unwrap_or(1.0);
                    if crate::runner_common::on_mouse_wheel(&mut self.rt, delta, scale) {
                        self.request_redraw();
                    }
                }

                WindowEvent::MouseInput { state, button, .. } => {
                    let pos = Vec2 {
                        x: self.rt.mouse_pos_px.0,
                        y: self.rt.mouse_pos_px.1,
                    };

                    let mapped = match button {
                        MouseButton::Left => PointerButton::Primary,
                        MouseButton::Right => PointerButton::Secondary,
                        MouseButton::Middle => PointerButton::Tertiary,
                        // Forward/Back/other buttons are not dispatched by the runtime.
                        _ => return,
                    };

                    match state {
                        ElementState::Pressed => {
                            let result = self.rt.handle_pointer_press(pos, mapped);

                            // Platform-specific IME setup for focused textfields
                            if let Some(fid) = result.focused
                                && let Some(win) = &self.window
                                && let Some(f) = &self.rt.frame_cache
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                let sf = win.scale_factor();
                                rc::set_ime_for_textfield_ex(
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

                            // Click outside - no focus result from runtime, drop IME
                            if result.focused.is_none() && self.rt.ime_preedit {
                                if let Some(win) = &self.window {
                                    rc::set_ime_for_textfield(win, false);
                                }
                                self.rt.ime_preedit = false;
                            }

                            if result.needs_a11y_announce {
                                self.announce_focus_change();
                            }

                            // Middle-click: paste the primary selection into a textfield.
                            if matches!(mapped, PointerButton::Tertiary)
                                && let Some(f) = &self.rt.frame_cache
                                && let Some(cid) = self.rt.capture_id
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                                && self.rt.is_textfield(hit.id)
                                && let Some(txt) = self.paste_from_primary()
                            {
                                self.rt.paste_into_focused(&txt);
                            }

                            // Inspector: click-to-select topmost widget under cursor.
                            if matches!(mapped, PointerButton::Primary | PointerButton::Secondary)
                                && let Some(inspector) = &mut self.inspector
                                && inspector.hud.inspector_enabled
                                && let Some(f) = &self.rt.frame_cache
                                && let Some(hit) =
                                    f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                            {
                                let info =
                                    f.semantics_nodes.iter().find(|s| s.id == hit.id).map(|s| {
                                        repose_devtools::HoveredInfo {
                                            id: s.id,
                                            role: format!("{:?}", s.role),
                                            label: s.label.clone(),
                                        }
                                    });
                                inspector
                                    .hud
                                    .select_widget(repose_devtools::SelectedWidget {
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

                        ElementState::Released => {
                            let result = self.rt.handle_pointer_release(pos, mapped);

                            // A11y: announce activation when a click fires on release.
                            // The runtime reports the clicked id before clearing its
                            // capture state, so this cannot race with `capture_id = None`.
                            if let Some(cid) = result.clicked_id
                                && let Some(f) = &self.rt.frame_cache
                                && let Some(node) = f.semantics_nodes.iter().find(|n| n.id == cid)
                            {
                                let label = node.label.as_deref().unwrap_or("");
                                self.a11y.announce(&format!("Activated {}", label));
                            }

                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::Touch(t) => {
                    let scale = self
                        .window
                        .as_ref()
                        .map(|w| w.scale_factor() as f32)
                        .unwrap_or(1.0);
                    let r = crate::runner_common::handle_touch_raw(
                        &mut self.rt,
                        &mut self.touch_gestures,
                        &t,
                        scale,
                    );
                    let mut dirty = r.dirty;
                    if let Some((delta_scale, center)) = r.pinch
                        && self.dispatch_action(repose_core::shortcuts::Action::Gesture(
                            repose_core::shortcuts::Gesture::PinchWithCenter {
                                delta_scale,
                                center,
                            },
                        )) {
                            dirty = true;
                        }
                    if let Some(delta) = r.pan
                        && self.dispatch_action(repose_core::shortcuts::Action::Gesture(
                            repose_core::shortcuts::Gesture::Pan { delta },
                        )) {
                            dirty = true;
                        }
                    if let Some(right) = r.swipe_right {
                        let g = if right {
                            repose_core::shortcuts::Gesture::SwipeRight
                        } else {
                            repose_core::shortcuts::Gesture::SwipeLeft
                        };
                        if self.dispatch_action(repose_core::shortcuts::Action::Gesture(g)) {
                            dirty = true;
                        }
                    }
                    if dirty {
                        self.request_redraw();
                    }
                }

                WindowEvent::ModifiersChanged(new_mods) => {
                    crate::runner_common::on_modifiers_changed(&mut self.rt, &new_mods.state());
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

                    // Escape / BrowserBack: when the runtime didn't cancel a
                    // drag / dispatch focus, fall back to navigation back.
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::BrowserBack)
                            | PhysicalKey::Code(KeyCode::Escape) => {
                                use repose_navigation::back;
                                if !back::handle() {
                                    // el.exit();
                                }
                                return;
                            }
                            _ => {}
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
                    crate::runner_common::on_ime(&mut self.rt, &ime);
                    self.request_redraw();
                }

                WindowEvent::RedrawRequested => {
                    // Allow media (etc.) to queue texture uploads without compose.
                    crate::run_pre_redraw(&self.render);

                    // 1. Check our redraw flag before processing a11y.
                    if !self.redraw_requested.replace(false) {
                        self.process_a11y_actions();
                        self.process_render_commands();
                        // Present-only: redraw last cached scene with updated textures
                        if let (Some(backend), Some(frame)) =
                            (self.backend.as_mut(), self.rt.frame_cache.as_ref())
                        {
                            let scale = self
                                .window
                                .as_ref()
                                .map(|w| w.scale_factor() as f32)
                                .unwrap_or(1.0);
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

                    if let Some(cursor) = &output.platform.cursor {
                        win.set_cursor(winit::window::Cursor::Icon(map_cursor(*cursor)));
                    }

                    // Sync OS window chrome (titlebar) to the app theme, deduped.
                    if let Some(dark) = output.platform.window_theme_dark
                        && self.last_window_theme != Some(dark)
                    {
                        win.set_theme(Some(if dark {
                            winit::window::Theme::Dark
                        } else {
                            winit::window::Theme::Light
                        }));
                        self.last_window_theme = Some(dark);
                    }

                    // Apply IME keyboard hints
                    if output.platform.ime_allowed {
                        rc::set_ime_for_textfield_ex(
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
                        rc::set_ime_for_textfield_ex(
                            win,
                            false,
                            repose_core::ImePurposeHint::Normal,
                            true,
                            repose_core::KeyboardCapitalization::Unspecified,
                        );
                        self.rt.ime_preedit = false;
                    }

                    // Apply IME state based on wants_keyboard
                    if !output.wants_keyboard
                        && focused.is_some()
                        && self.rt.sched.focused.is_none()
                        && self.rt.ime_preedit
                    {
                        rc::set_ime_for_textfield(win, false);
                        self.rt.ime_preedit = false;
                    }

                    let frame = output.into_frame();

                    let build_layout_ms = (Instant::now() - t0).as_secs_f32() * 1000.0;

                    // UPDATE ACCESSIBILITY TREE
                    if let (Some(adapter), Some(win)) = (&mut self.accesskit_adapter, &self.window)
                    {
                        let scale = win.scale_factor();
                        if let Some(update) = self.a11y_tree.update(
                            &frame.semantics_nodes,
                            scale,
                            self.rt.sched.focused,
                        ) {
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
                    let Some(win) = self.window.as_ref() else {
                        return;
                    };
                    let scale = win.scale_factor() as f32;
                    if let Some(backend) = self.backend.as_mut() {
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                    }

                    // Initialize TextFieldState for any focused TextField that
                    // doesn't have one yet (e.g. after FocusRequester::request_focus),
                    // reconcile hover, and publish the DnD frame/scale.
                    self.rt.after_compose(&frame, scale);

                    // NOTE: hover was already reconciled inside `compose()`.
                    // `cache_frame` rebuilds the retained hover-leave map.
                    self.rt.cache_frame(frame);

                    self.dispatch_file_drop_now();

                    self.rt.tick_overlays();
                    self.last_redraw = Instant::now();
                }

                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            // Process cross-thread commands (e.g. tray toggles, deeplinks) before any
            // redraw check, so hide/show commands work even when hidden
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if let Some(cb) = ABOUT_TO_WAIT_CALLBACK
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .as_ref()
            {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(cb));
                if let Err(e) = res {
                    log::error!(
                        "ABOUT_TO_WAIT_CALLBACK panicked: {}",
                        e.downcast_ref::<String>()
                            .map(|s| s.as_str())
                            .or_else(|| e.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown")
                    );
                }
            }
            process_deeplinks();

            // On Wayland, wgpu creates an xdg_surface from the winit window and it shouldn't be recreated with a new id?
            // It doesn't take a lot of resources anyway, so let the backend be present.
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if WINDOW_VISIBLE.load(Ordering::Relaxed)
                && self.backend.is_none()
                && let Some(w) = &self.window
            {
                log::info!("about_to_wait: recreating GPU backend");
                match repose_render_wgpu::WgpuBackend::new_with_options(
                    w.clone(),
                    self.msaa_samples,
                    self.present_mode,
                ) {
                    Ok(b) => {
                        repose_render_wgpu::offscreen::set_shared_device(
                            b.device.clone(),
                            b.queue.clone(),
                        );
                        self.backend = Some(b)
                    }
                    Err(e) => log::error!("about_to_wait: failed to recreate backend: {e:?}"),
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
                let deadline = self.rt.next_frame_deadline(now, idle_cap);

                if now.saturating_duration_since(self.last_redraw) >= idle_cap
                    || self.rt.is_wakeup_due(now)
                {
                    self.redraw_requested.set(true);
                    request_frame();
                    rc::request_redraw(&self.window);
                    self.last_redraw = now;
                }
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(Ord::min(
                    deadline,
                    now + idle_cap,
                )));
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
        fn announce_focus_change(&mut self) {
            if let Some(f) = &self.rt.frame_cache {
                let focused_node = self
                    .rt
                    .sched
                    .focused
                    .and_then(|id| f.semantics_nodes.iter().find(|n| n.id == id));
                self.a11y.focus_changed(focused_node);
            }
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

#[cfg_attr(target_os = "linux", allow(dead_code))]
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
