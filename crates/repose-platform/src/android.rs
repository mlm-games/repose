//! Android runner (winit native-activity).
//!
//! winit can show and hide the Android soft keyboard, but it does not create
//! an Android `InputConnection` or a native editable view. Soft-keyboard text,
//! composing text, selection, and key events must therefore be bridged by the
//! host application: implement the activity's `onCreateInputConnection`, keep
//! an editable buffer synchronized with the focused Repose field, and forward
//! `BaseInputConnection` `commitText`/`setComposingText`/
//! `finishComposingText`/`deleteSurroundingText`/`setSelection`/
//! `performEditorAction` results to the runtime; winit does not turn Android
//! soft-keyboard edits into `WindowEvent::Ime` for this canvas. The
//! `set_ime_allowed` calls below only control keyboard visibility; they are not
//! an editor implementation.

use crate::common as rc;

use crate::render::RenderContext;
use crate::*;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use repose_app::ReposeRuntime;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::keyboard::PhysicalKey;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowAttributes};

pub use repose_app::AndroidOptions;

/// Runtime override for [`AndroidOptions::continuous_redraw`].
///
/// Useful for a settings toggle. Takes precedence over the static option.
#[cfg(target_os = "android")]
static CONTINUOUS_REDRAW: AtomicBool = AtomicBool::new(false);

/// Toggle continuous redraw at runtime (e.g. from a settings switch).
#[cfg(target_os = "android")]
pub fn set_continuous_redraw(enabled: bool) {
    CONTINUOUS_REDRAW.store(enabled, Ordering::Relaxed);
    if enabled {
        repose_core::request_frame();
        crate::wake_event_loop();
    }
}

/// Run an Android app with default [`AndroidOptions`].
///
/// Deprecated: use [`run_android_app_with_options`] with
/// `AndroidOptions::default()` instead. This may be removed in a future release.
#[deprecated(
    note = "use run_android_app_with_options(app, root, AndroidOptions) instead; this may be removed in a future release"
)]
pub fn run_android_app(
    app: AndroidApp,
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
) -> anyhow::Result<()> {
    run_android_app_with_options(app, root, AndroidOptions::default())
}

pub fn run_android_app_with_options(
    app: AndroidApp,
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
    options: AndroidOptions,
) -> anyhow::Result<()> {
    // Android logcat init is owned by the app: call
    // `rlobkit_app_events::android_log::init` from `android_main` before
    // this runner starts. The runner stays logger-agnostic so apps keep
    // one shared tracing backend with no `log`-global races.
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = winit::event_loop::EventLoopBuilder::new()
        .with_android_app(app)
        .build()?;
    crate::set_event_loop_proxy(event_loop.create_proxy());

    struct AppState {
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        render: RenderContext,
        options: AndroidOptions,

        window: Option<Arc<Window>>,
        backend: Option<repose_render_wgpu::WgpuBackend>,
        rt: ReposeRuntime,

        // Shared touch-scroll / pinch / swipe gesture state
        touch_gestures: rc::TouchGestureState,

        ime_visible: bool,
        /// Focused id the keyboard was last shown for.
        ime_shown_for: Option<u64>,

        /// Buttons arrive as native keycodes, axes have no source yet.
        #[cfg(feature = "gamepad")]
        gamepad: crate::gamepad::AndroidBackend,

        surface_active: bool,
        in_foreground: bool,
        occluded: bool,
        os_focused: bool,
        ime_output_allowed: bool,
        surface_retry_pending: bool,
        surface_retry_at: Option<web_time::Instant>,
        render_retry_pending: bool,
        render_retry_at: Option<web_time::Instant>,

        frame_pacer: rc::FramePacer,
        redraw_deferred: bool,
    }

    impl AppState {
        fn new(
            root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
            options: AndroidOptions,
        ) -> Self {
            Self {
                root,
                render: RenderContext::new(),
                options,
                window: None,
                backend: None,
                rt: ReposeRuntime::new(),

                touch_gestures: rc::TouchGestureState::default(),

                ime_visible: false,
                ime_shown_for: None,
                #[cfg(feature = "gamepad")]
                gamepad: crate::gamepad::create_android_backend().expect("android gamepad backend"),
                surface_active: false,
                in_foreground: false,
                occluded: false,
                os_focused: true,
                ime_output_allowed: false,
                surface_retry_pending: false,
                surface_retry_at: None,
                render_retry_pending: false,
                render_retry_at: None,

                frame_pacer: rc::FramePacer::new(options.common.max_fps),
                redraw_deferred: false,
            }
        }

        fn request_redraw(&self) {
            repose_core::request_frame();
        }

        /// Whether frames should be forced continuously (static option or a
        /// runtime override from `set_continuous_redraw`).
        fn continuous_redraw(&self) -> bool {
            self.options.continuous_redraw || CONTINUOUS_REDRAW.load(Ordering::Relaxed)
        }

        fn notify_lifecycle(&mut self, state: AppLifecycle) {
            self.rt.push_lifecycle(state);
        }

        fn set_foreground(&mut self, foreground: bool) {
            if self.in_foreground == foreground {
                return;
            }
            self.in_foreground = foreground;
            self.notify_lifecycle(if foreground {
                AppLifecycle::Foreground
            } else {
                AppLifecycle::Background
            });
        }

        fn scale(&self) -> f32 {
            self.window
                .as_ref()
                .map(|w| w.scale_factor() as f32)
                .unwrap_or(1.0)
        }

        fn clear_input_state(&mut self) {
            let active_touches = self
                .touch_gestures
                .active_touches()
                .iter()
                .map(|(id, pos)| (*id, *pos))
                .collect::<Vec<_>>();
            for (touch_id, pos) in active_touches {
                self.touch_gestures
                    .touch_ended(&mut self.rt, touch_id, pos, true);
                self.touch_gestures.contact_up(touch_id);
            }
            self.touch_gestures = rc::TouchGestureState::default();
            self.rt.handle_focus_lost();
            self.rt.touch_paths.clear();
            self.rt.scroll_capture_id = None;
            self.rt.pointer_inside = false;
            self.rt.hover_id = None;
            self.rt.hover_ancestors.clear();
            self.rt.last_focus = None;
            self.rt.sched.window_focused = false;
            self.rt.sched.pointer_pos_px = None;
            self.rt.sched.held_keys.clear();
            self.rt.sched.touch_points.clear();
            self.rt.sched.mouse_primary = false;
            self.rt.sched.mouse_secondary = false;
            self.rt.sched.mouse_middle = false;
            self.rt.held_keys.clear();
            self.rt.held_mouse.clear();
            self.rt.pressed_ids.clear();
            self.rt.key_pressed_active = None;
            self.rt.ime_preedit = false;
            self.rt.modifiers = Default::default();
            #[cfg(feature = "gamepad")]
            {
                use repose_core::input::{GamepadEvent, GamepadId};
                let mut releases = Vec::new();
                for (id, pad) in &self.rt.gamepads {
                    let id = GamepadId(*id);
                    for button in &pad.pressed {
                        releases.push(GamepadEvent::Button {
                            id,
                            button: *button,
                            pressed: false,
                        });
                    }
                    for axis in pad.axes.keys() {
                        releases.push(GamepadEvent::Axis {
                            id,
                            axis: *axis,
                            value: 0.0,
                        });
                    }
                }
                for event in releases {
                    self.rt.handle_gamepad(&event);
                }
            }
            self.ime_visible = false;
            self.ime_shown_for = None;
            if let Some(win) = &self.window {
                rc::set_ime_for_textfield(win, false);
            }
        }

        fn try_recreate_surface(&mut self) -> bool {
            let Some(window) = self.window.clone() else {
                return false;
            };
            let size = window.inner_size();
            if size.width == 0 || size.height == 0 {
                return false;
            }
            let result = match self.backend.as_mut() {
                Some(backend) => backend.recreate_surface(&window),
                None => return false,
            };
            match result {
                Ok(()) => {
                    let scale = window.scale_factor() as f32;
                    self.sync_window_size(size, scale);
                    true
                }
                Err(e) => {
                    log::warn!("surface recreate failed: {e:?}");
                    false
                }
            }
        }

        fn defer_render_retry(&mut self) {
            let now = web_time::Instant::now();
            let retry_at = now + web_time::Duration::from_millis(100);
            self.render_retry_pending = true;
            self.render_retry_at = self
                .frame_pacer
                .deadline(now)
                .into_iter()
                .chain(Some(retry_at))
                .max();
        }

        fn handle_frame_result(&mut self, presented: bool) {
            if presented {
                self.render_retry_pending = false;
                self.render_retry_at = None;
            } else if self
                .backend
                .as_ref()
                .is_some_and(|backend| backend.surface.is_some())
            {
                self.defer_render_retry();
            } else {
                self.defer_surface_retry();
            }
        }

        fn defer_surface_retry(&mut self) {
            let now = web_time::Instant::now();
            let retry_at = now + web_time::Duration::from_millis(100);
            self.surface_retry_pending = true;
            self.render_retry_pending = false;
            self.render_retry_at = None;
            self.surface_retry_at = self
                .frame_pacer
                .deadline(now)
                .into_iter()
                .chain(Some(retry_at))
                .max();
            self.surface_active = false;
        }

        fn activate_surface(&mut self) {
            self.surface_retry_pending = false;
            self.surface_retry_at = None;
            self.render_retry_pending = false;
            self.render_retry_at = None;
            self.surface_active = true;
            self.request_redraw();
        }

        fn dp_px(&self, dp: f32) -> f32 {
            dp * self.scale()
        }

        /// Sync the soft keyboard with the currently focused textfield.
        /// When `force` is set, re-show the keyboard even if it is already
        /// marked visible.
        fn update_ime_state(&mut self, force: bool, ime_allowed: bool) {
            let Some(win) = self.window.clone() else {
                return;
            };

            let focused_tf = if self.in_foreground && self.rt.sched.window_focused && ime_allowed {
                self.rt.sched.focused.filter(|id| {
                    self.rt
                        .frame_cache
                        .as_ref()
                        .is_some_and(|frame| rc::is_editable_textfield_hit(frame, *id))
                })
            } else {
                None
            };
            let ime_active = focused_tf.is_some();
            if !force && focused_tf == self.ime_shown_for && ime_active == self.ime_visible {
                return;
            }

            win.set_ime_allowed(ime_active);
            self.ime_visible = ime_active;
            self.ime_shown_for = focused_tf;
            if !ime_active {
                self.rt.finish_compositions();
            }
        }

        // IME inset is normally supplied by the app itself, which forwards
        // rlobkit-app-events' real system-bar + IME insets into
        // repose_core::locals. This estimate is only a fallback for apps that
        // have not wired an insets source yet.
        fn update_ime_inset(&self) {
            // Prefer live insets (filled by the app, check mlm-games/retorrent for eg). If the keyboard is
            // closed, keep the authoritative 0 (or clear a stale estimate).
            let current = repose_core::locals::window_insets();
            if current.ime_bottom > 0.0 || !self.ime_visible {
                if !self.ime_visible && current.ime_bottom != 0.0 {
                    repose_core::locals::set_ime_inset(0.0);
                }
                return;
            }

            // Fallback only when the IME is visible but the app hasn't
            // supplied real insets yet.
            let h = self.options.ime_height_px.unwrap_or_else(|| {
                // Estimate ~40% of window's shorter dimension as default IME height
                let size = self
                    .window
                    .as_ref()
                    .map(|w| w.inner_size())
                    .unwrap_or_default();
                (size.width.min(size.height) as f32 * 0.4).max(200.0)
            });
            repose_core::locals::set_ime_inset(h);
        }

        fn sync_window_size(&mut self, size: PhysicalSize<u32>, scale: f32) {
            rc::sync_viewport(&mut self.rt, &mut self.backend, size, scale);
            // Recompute IME inset estimate when window size changes
            self.update_ime_inset();
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = &mut self.backend else {
                return;
            };
            repose_render_wgpu::apply_render_commands(backend, self.render.drain());
        }

        fn current_ime_allowed(&self) -> bool {
            self.in_foreground
                && self.ime_output_allowed
                && self.rt.sched.window_focused
                && self.rt.sched.focused.is_some_and(|id| {
                    self.rt
                        .frame_cache
                        .as_ref()
                        .is_some_and(|frame| rc::is_editable_textfield_hit(frame, id))
                })
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            if self.rt.dispatch_action(action) {
                let ime_allowed = self.current_ime_allowed();
                self.update_ime_state(false, ime_allowed);
                return true;
            }

            false
        }
        fn overlay_drag_indicator(&self, scene: &mut Scene) {
            let _dnd_guard = self.rt.dnd_context.enter();
            Self::overlay_drag_indicator_static(scene, self.rt.mouse_pos_px);
        }

        fn overlay_drag_indicator_static(scene: &mut Scene, pos: (f32, f32)) {
            repose_core::dnd::overlay_drag_indicator(scene, pos, false);
        }
    }

    impl ApplicationHandler<()> for AppState {
        fn suspended(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            let _event_scope =
                repose_app::lifecycle::enter_dispatchers(self.rt.event_dispatchers());
            self.surface_active = false;
            self.set_foreground(false);
            self.os_focused = false;
            self.surface_retry_pending = false;
            self.surface_retry_at = None;
            rc::flush_clipboard_writes();
            self.render_retry_pending = false;
            self.render_retry_at = None;
            if let Some(backend) = self.backend.as_mut() {
                backend.take_surface();
            }
            self.clear_input_state();
        }

        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            let _event_scope =
                repose_app::lifecycle::enter_dispatchers(self.rt.event_dispatchers());
            self.set_foreground(true);
            if self.os_focused {
                self.rt.sched.window_focused = !self.occluded;
            }
            let ime_allowed = self.current_ime_allowed();
            self.update_ime_state(true, ime_allowed);
            if self.window.is_some() {
                if self.try_recreate_surface() {
                    self.activate_surface();
                } else {
                    self.defer_surface_retry();
                }
                return;
            }

            match el.create_window(WindowAttributes::default().with_title("Repose Android")) {
                Ok(win) => {
                    let w = Arc::new(win);
                    let sz = w.inner_size();
                    let sf = w.scale_factor() as f32;
                    self.sync_window_size(sz, sf);

                    match repose_render_wgpu::WgpuBackend::new_with_options(
                        w.clone(),
                        self.options.common.msaa_samples,
                        self.options.common.present_mode,
                    ) {
                        Ok(mut b) => {
                            b.set_pixels_per_point(sf);
                            repose_render_wgpu::offscreen::set_shared_device(
                                b.device.clone(),
                                b.queue.clone(),
                            );
                            self.backend = Some(b);
                            self.window = Some(w);
                            let _ = rc::setup_clipboard();
                        }
                        Err(e) => {
                            log::error!("WGPU backend init failed: {e:?}");
                            el.exit();
                        }
                    }
                }
                Err(e) => {
                    log::error!("Window create failed: {e:?}");
                    el.exit();
                }
            }

            if self.backend.is_some() {
                self.activate_surface();
            }
        }

        fn window_event(
            &mut self,
            el: &winit::event_loop::ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            let _event_scope =
                repose_app::lifecycle::enter_dispatchers(self.rt.event_dispatchers());
            let _dnd_guard = self.rt.dnd_context.enter();
            match event {
                WindowEvent::CloseRequested => el.exit(),

                WindowEvent::Resized(size) => {
                    self.sync_window_size(size, self.scale());

                    if size.width == 0 || size.height == 0 {
                        if self.backend.is_some() {
                            if let Some(backend) = self.backend.as_mut() {
                                backend.take_surface();
                            }
                            self.defer_surface_retry();
                        }
                    } else if !self.surface_retry_pending {
                        self.request_redraw();
                    }
                }

                WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                    let size = self
                        .window
                        .as_ref()
                        .map(|w| w.inner_size())
                        .unwrap_or_default();
                    self.sync_window_size(size, scale_factor as f32);

                    if size.width == 0 || size.height == 0 {
                        if self.backend.is_some() {
                            if let Some(backend) = self.backend.as_mut() {
                                backend.take_surface();
                            }
                            self.defer_surface_retry();
                        }
                    } else if !self.surface_retry_pending {
                        self.request_redraw();
                    }
                }

                WindowEvent::Focused(focused) => {
                    self.os_focused = focused;
                    let focused = focused && !self.occluded;
                    self.rt.sched.window_focused = focused;
                    if !focused {
                        self.clear_input_state();
                    } else {
                        let ime_allowed = self.current_ime_allowed();
                        self.update_ime_state(true, ime_allowed);
                    }

                    self.request_redraw();
                }

                WindowEvent::Occluded(occluded) => {
                    self.occluded = occluded;
                    if occluded {
                        self.rt.sched.window_focused = false;
                        self.clear_input_state();
                    } else {
                        self.rt.sched.window_focused = self.os_focused;
                        if self.os_focused {
                            let ime_allowed = self.current_ime_allowed();
                            self.update_ime_state(true, ime_allowed);
                        }
                    }

                    self.request_redraw();
                }

                WindowEvent::ModifiersChanged(new_mods) => {
                    crate::runner_common::on_modifiers_changed(&mut self.rt, &new_mods.state());
                }

                // Touch handling (Android primary). Scroll / pinch / swipe
                // recognition lives in common.rs, shared with web + desktop.
                WindowEvent::Touch(t) => {
                    if t.phase == winit::event::TouchPhase::Started {
                        let pos_px = (t.location.x as f32, t.location.y as f32);
                        self.touch_gestures.contact_down(t.id, pos_px);
                        self.touch_gestures
                            .touch_started(&mut self.rt, t.id, pos_px);
                        crate::runner_common::sync_touch_points(&mut self.rt, &self.touch_gestures);
                        let ime_allowed = self.current_ime_allowed();
                        self.update_ime_state(true, ime_allowed);

                        self.request_redraw();
                    } else {
                        let scale = self.scale();
                        let r = crate::runner_common::handle_touch_raw(
                            &mut self.rt,
                            &mut self.touch_gestures,
                            &t,
                            scale,
                        );
                        if t.phase == winit::event::TouchPhase::Ended && r.press.is_some() {
                            let ime_allowed = self.current_ime_allowed();
                            self.update_ime_state(true, ime_allowed);
                        }
                        let mut dirty = r.dirty;
                        if let Some((delta_scale, center)) = r.pinch {
                            if self.dispatch_action(repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::PinchWithCenter {
                                    delta_scale,
                                    center,
                                },
                            )) {
                                dirty = true;
                            }
                        }
                        if let Some((delta, center)) = r.pan {
                            if self.dispatch_action(repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::Pan { delta, center },
                            )) {
                                dirty = true;
                            }
                        }
                        if let Some((delta_rotation, center)) = r.rotation {
                            if self.dispatch_action(repose_core::shortcuts::Action::Gesture(
                                repose_core::shortcuts::Gesture::Rotate {
                                    delta_rotation,
                                    center,
                                },
                            )) {
                                dirty = true;
                            }
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
                }

                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    // Controller buttons arrive as native Android keycodes
                    // (winit maps AKEYCODE_BUTTON_* to Unidentified).
                    #[cfg(feature = "gamepad")]
                    if let PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Android(
                        code,
                    )) = key_event.physical_key
                    {
                        let pressed = key_event.state == ElementState::Pressed;
                        let events = self.gamepad.key_button(code, pressed);
                        if !events.is_empty() {
                            for ev in events {
                                self.rt.handle_gamepad(&ev);
                            }

                            self.request_redraw();
                            return;
                        }
                    }
                    let mut no_inspector: Option<repose_devtools::Inspector> = None;
                    if crate::runner_common::on_keyboard_input(
                        &mut self.rt,
                        &key_event,
                        &mut no_inspector,
                    ) {
                        self.request_redraw();
                        return;
                    }
                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && (rc::is_back_key(&key_event) || rc::is_escape_key(&key_event))
                    {
                        if self.rt.overlay.handle_back() {
                            self.request_redraw();
                            return;
                        }
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

                    self.request_redraw();
                }

                WindowEvent::RedrawRequested => {
                    if !self.surface_active || self.backend.is_none() {
                        return; // surface gone; never touch the GPU
                    }
                    let zero_size = self.window.as_ref().is_some_and(|window| {
                        let size = window.inner_size();
                        size.width == 0 || size.height == 0
                    });
                    if zero_size {
                        if let Some(backend) = self.backend.as_mut() {
                            backend.take_surface();
                        }
                        self.defer_surface_retry();
                        return;
                    }

                    crate::run_pre_redraw(&self.render);

                    let now = web_time::Instant::now();
                    let compose_requested = repose_core::frame_clock::peek_frame_request();
                    let should_compose = compose_requested || self.continuous_redraw();
                    let has_frame = self.rt.frame_cache.is_some();
                    if (should_compose
                        || has_frame
                        || repose_core::frame_clock::peek_present_request())
                        && !self.frame_pacer.due(now)
                    {
                        self.redraw_deferred = true;
                        if let Some(deadline) = self.frame_pacer.deadline(now) {
                            el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                                deadline,
                            ));
                        }
                        return;
                    }
                    self.redraw_deferred = false;
                    let compose_requested = take_frame_request();
                    let do_compose = compose_requested || self.continuous_redraw();

                    if !do_compose {
                        self.process_render_commands();
                        let scale = self.scale();
                        let dragging = repose_core::dnd::is_dragging();
                        let (presented, has_frame) = match (&mut self.backend, &self.rt.frame_cache)
                        {
                            (Some(backend), Some(frame)) => {
                                let mut overlay = dragging.then(|| frame.scene.clone());
                                if let Some(overlay) = &mut overlay {
                                    Self::overlay_drag_indicator_static(
                                        overlay,
                                        self.rt.mouse_pos_px,
                                    );
                                }
                                let _ = take_present_request();
                                (
                                    backend.frame(
                                        overlay.as_ref().unwrap_or(&frame.scene),
                                        GlyphRasterConfig {
                                            px: Px(18.0 * scale),
                                        },
                                    ),
                                    true,
                                )
                            }
                            _ => (false, has_frame),
                        };
                        if presented {
                            self.frame_pacer.rendered(web_time::Instant::now());
                        }
                        if has_frame {
                            self.handle_frame_result(presented);
                        }
                        return;
                    }

                    self.rt.tick_overlays();
                    repose_core::animation_driver::tick();
                    self.process_render_commands();

                    let scale = self.scale();
                    self.rt.scale = scale;

                    let output = self.rt.frame(&mut self.root, &self.render);
                    self.process_render_commands();

                    self.ime_output_allowed = output.platform.ime_allowed;
                    let ime_allowed = output.platform.ime_allowed
                        && self.rt.sched.window_focused
                        && self.rt.sched.focused.is_some_and(|id| {
                            rc::editable_textfield_hit(
                                &output.hit_regions,
                                &output.semantics_nodes,
                                id,
                            )
                            .is_some()
                        });
                    let frame = output.into_shared_frame();
                    self.rt.after_compose_shared(frame.clone(), scale);

                    let mut overlay = repose_core::dnd::is_dragging().then(|| frame.scene.clone());
                    if let Some(overlay) = &mut overlay {
                        self.overlay_drag_indicator(overlay);
                    }

                    let _ = take_present_request();
                    let presented = self.backend.as_mut().is_some_and(|backend| {
                        backend.frame(
                            overlay.as_ref().unwrap_or(&frame.scene),
                            GlyphRasterConfig {
                                px: Px(18.0 * scale),
                            },
                        )
                    });

                    self.rt.cache_frame(frame);
                    self.update_ime_state(false, ime_allowed);
                    if presented {
                        self.frame_pacer.rendered(web_time::Instant::now());
                    }
                    self.handle_frame_result(presented);
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            let _event_scope =
                repose_app::lifecycle::enter_dispatchers(self.rt.event_dispatchers());
            let _dnd_guard = self.rt.dnd_context.enter();
            self.rt.process_deeplinks();
            self.rt.process_lifecycle();
            let redraw_deferred = self.redraw_deferred;

            #[cfg(feature = "gamepad")]
            {
                use crate::gamepad::GamepadBackend as _;
                for (id, low, high, duration_ms) in self.rt.take_rumble_requests() {
                    use repose_core::input::GamepadId;
                    let gid = GamepadId(id);
                    if duration_ms == 0 {
                        self.gamepad.stop_rumble(gid);
                    } else if !self.gamepad.set_rumble(gid, low, high, duration_ms) {
                        log::warn!("gamepad: rumble not supported on Android (pad {id})");
                    }
                }
            }
            #[cfg(not(feature = "gamepad"))]
            {
                self.rt.take_rumble_requests();
            }

            if self.surface_retry_pending {
                let now = web_time::Instant::now();
                if let Some(retry_at) = self.surface_retry_at
                    && now < retry_at
                {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                    return;
                }
                if self.backend.is_none() {
                    self.surface_retry_pending = false;
                    self.surface_retry_at = None;
                    return;
                }
                if self.try_recreate_surface() {
                    self.activate_surface();
                } else {
                    self.defer_surface_retry();
                    if let Some(retry_at) = self.surface_retry_at {
                        el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                    }
                    return;
                }
            }

            if self.render_retry_pending {
                let now = web_time::Instant::now();
                if let Some(retry_at) = self.render_retry_at
                    && now < retry_at
                {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(retry_at));
                    return;
                }
                self.render_retry_pending = false;
                self.render_retry_at = None;
                if self.frame_pacer.due(now) {
                    rc::request_redraw(&self.window);
                } else if let Some(deadline) = self.frame_pacer.deadline(now) {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                }
                return;
            }

            if !self.surface_active {
                return;
            }
            if redraw_deferred {
                let now = web_time::Instant::now();
                if self.frame_pacer.due(now) {
                    self.redraw_deferred = false;
                    rc::request_redraw(&self.window);
                } else if let Some(deadline) = self.frame_pacer.deadline(now) {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                } else {
                    self.redraw_deferred = false;
                    rc::request_redraw(&self.window);
                }
                return;
            }

            let now = web_time::Instant::now();
            let wakeup_due = self.in_foreground && self.rt.is_wakeup_due(now);
            if wakeup_due {
                request_frame();
                rc::request_redraw(&self.window);
                return;
            }
            let compose_requested = repose_core::frame_clock::peek_frame_request();
            let should_compose = compose_requested
                || (self.in_foreground
                    && (self.continuous_redraw() || repose_core::animation_driver::is_active()));
            let wakeup_deadline = self
                .rt
                .next_wakeup_deadline()
                .filter(|deadline| *deadline > now);
            if should_compose {
                if self.frame_pacer.due(now) {
                    rc::request_redraw(&self.window);
                } else if let Some(deadline) = self
                    .frame_pacer
                    .deadline(now)
                    .into_iter()
                    .chain(wakeup_deadline)
                    .min()
                {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                }
                return;
            }

            if repose_core::frame_clock::peek_present_request() && self.rt.frame_cache.is_some() {
                if self.frame_pacer.due(now) {
                    rc::request_redraw(&self.window);
                } else if let Some(deadline) = self
                    .frame_pacer
                    .deadline(now)
                    .into_iter()
                    .chain(wakeup_deadline)
                    .min()
                {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                }
                return;
            }

            match wakeup_deadline {
                Some(deadline) => {
                    el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
                }
                None => el.set_control_flow(winit::event_loop::ControlFlow::Wait),
            }
        }

        fn user_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: ()) {
            request_frame();
        }
    }

    let mut app_state = AppState::new(Box::new(root), options);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
