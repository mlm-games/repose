use crate::common as rc;
use crate::common_web as rc_web;
use crate::render::RenderContext;
use crate::*;

use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use repose_app::ReposeRuntime;
use repose_core::shortcuts::{Action, Gesture};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{Window, WindowAttributes};

#[derive(Clone, Copy, Debug)]
pub struct AndroidOptions {
    /// If true, runner keeps requesting frames (good for animations, costs battery).
    pub continuous_redraw: bool,

    /// IME (soft keyboard) inset height in physical pixels.
    /// When the keyboard opens on Android, `set_ime_inset()` is called with this value.
    /// If `None`, the runner estimates ~40% of the window's shorter dimension.
    pub ime_height_px: Option<f32>,

    /// Common options shared with other platforms.
    pub common: ReposeOptions,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            // Reactive by default (egui-style): frames are only requested on
            // demand (dirty, caret blink, running animations). Set this to true
            // only for always-animating UIs that want to burn battery.
            continuous_redraw: false,
            ime_height_px: None,
            common: ReposeOptions::default(),
        }
    }
}

/// Runtime override for [`AndroidOptions::continuous_redraw`].
///
/// Useful for a settings toggle. Takes precedence over the static option.
#[cfg(target_os = "android")]
static CONTINUOUS_REDRAW: AtomicBool = AtomicBool::new(false);

/// Toggle continuous redraw at runtime (e.g. from a settings switch).
#[cfg(target_os = "android")]
pub fn set_continuous_redraw(enabled: bool) {
    CONTINUOUS_REDRAW.store(enabled, Ordering::Relaxed);
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

        // IME (soft keyboard) tracking
        ime_visible: bool,

        // redraw control
        dirty: bool,

        /// True while the Activity surface is usable (between resumed and suspended).
        surface_active: bool,
        /// App-level foreground-ish flag (mirrors the last lifecycle transition).
        in_foreground: bool,

        // clipboard
        clipboard: Option<clipawl::Clipboard>,

        last_redraw: web_time::Instant,

        /// Tracks whether a redraw was requested by app code that needs compose.
        compose_requested: Cell<bool>,
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
                dirty: true,
                surface_active: false,
                in_foreground: false,

                clipboard: None,

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
            // Do NOT set compose_requested  - present-only
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        /// Whether frames should be forced continuously (static option or a
        /// runtime override from `set_continuous_redraw`).
        fn continuous_redraw(&self) -> bool {
            self.options.continuous_redraw || CONTINUOUS_REDRAW.load(Ordering::Relaxed)
        }

        fn notify_lifecycle(&self, state: AppLifecycle) {
            crate::push_lifecycle(state);
        }

        fn scale(&self) -> f32 {
            self.window
                .as_ref()
                .map(|w| w.scale_factor() as f32)
                .unwrap_or(1.0)
        }

        fn dp_px(&self, dp: f32) -> f32 {
            dp * self.scale()
        }

        fn is_textfield(&self, id: u64) -> bool {
            rc::is_textfield_in_frame(&self.rt.frame_cache, id)
        }

        fn update_ime_state(&mut self) {
            let Some(win) = &self.window else { return };

            let allow = self
                .rt
                .sched
                .focused
                .map_or(false, |id| self.rt.is_textfield(id));
            let (purpose, auto_correct, capitalization) = self.rt.focused_keyboard_hints();

            rc_web::set_ime_for_textfield_ex(win, allow, purpose, auto_correct, capitalization);

            if allow {
                self.update_ime_cursor_area(win);
            } else {
                self.rt.ime_preedit = false;
            }
        }

        fn update_ime_cursor_area(&self, win: &Window) {
            let Some(fid) = self.rt.sched.focused else {
                return;
            };
            let Some(f) = &self.rt.frame_cache else {
                return;
            };
            let Some(i) = rc::hit_index_by_id(f, fid) else {
                return;
            };

            let hit = &f.hit_regions[i];
            let sf = win.scale_factor() as f32;

            win.set_ime_cursor_area(
                PhysicalPosition::new((hit.rect.x * sf) as i32, (hit.rect.y * sf) as i32),
                PhysicalSize::new((hit.rect.w * sf) as u32, (hit.rect.h * sf) as u32),
            );
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
            self.rt
                .set_viewport_and_scale(size.width, size.height, scale);
            if let Some(b) = &mut self.backend {
                b.configure_surface(size.width, size.height);
                b.set_pixels_per_point(scale);
            }
            // Recompute IME inset estimate when window size changes
            self.update_ime_inset();
        }

        fn copy_to_clipboard(&self, text: &str) {
            if let Some(cb) = &self.clipboard {
                let _ = pollster::block_on(cb.write(text));
            }
        }

        fn paste_from_clipboard(&self) -> Option<String> {
            if let Some(cb) = &self.clipboard {
                pollster::block_on(cb.read()).ok()
            } else {
                None
            }
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = &mut self.backend else {
                return;
            };
            repose_render_wgpu::apply_render_commands(backend, self.render.drain());
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            if self.rt.dispatch_action(action) {
                if let Some(win) = &self.window {
                    rc_web::set_ime_for_textfield(
                        win,
                        self.rt
                            .sched
                            .focused
                            .map_or(false, |id| self.rt.is_textfield(id)),
                    );
                }
                return true;
            }

            false
        }
        fn overlay_drag_indicator(&self, scene: &mut Scene) {
            repose_core::dnd::overlay_drag_indicator(scene, self.rt.mouse_pos_px, false);
        }
    }

    impl ApplicationHandler<()> for AppState {
        fn suspended(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            self.surface_active = false;
            self.in_foreground = false;
            self.notify_lifecycle(AppLifecycle::Background);
            self.backend = None;
            self.window = None;
            // Do NOT request a redraw here: the surface is gone.
        }

        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            if self.window.is_some() {
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
                        Ok(b) => {
                            repose_render_wgpu::offscreen::set_shared_device(
                                b.device.clone(),
                                b.queue.clone(),
                            );
                            self.backend = Some(b);
                            self.window = Some(w);
                            self.clipboard = clipawl::Clipboard::new().ok();
                            repose_core::clipboard::set_clipboard_read_fn(Box::new(|| {
                                clipawl::blocking::read().ok()
                            }));
                            repose_core::clipboard::set_clipboard_fn(Box::new(|text| {
                                if let Ok(cb) = clipawl::Clipboard::new() {
                                    let _ = pollster::block_on(cb.write(text));
                                }
                            }));
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

            // After a successful backend init the surface is usable again.
            if self.backend.is_some() {
                self.surface_active = true;
                self.in_foreground = true;
                self.notify_lifecycle(AppLifecycle::Foreground);
                self.dirty = true;
                self.request_redraw();
            }
        }

        fn window_event(
            &mut self,
            el: &winit::event_loop::ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::CloseRequested => el.exit(),

                WindowEvent::Resized(size) => {
                    self.sync_window_size(size, self.scale());
                    self.dirty = true;
                    self.request_redraw();
                }

                WindowEvent::ModifiersChanged(new_mods) => {
                    rc::update_modifiers(&mut self.rt.modifiers, &new_mods.state());
                }

                // Touch handling (Android primary). Scroll / pinch / swipe
                // recognition lives in common.rs, shared with web + desktop.
                WindowEvent::Touch(t) => {
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    let tid = t.id;

                    match t.phase {
                        winit::event::TouchPhase::Started => {
                            let focused =
                                self.touch_gestures.touch_started(&mut self.rt, tid, pos_px);

                            // Platform-specific IME setup for focused textfields
                            if let Some(fid) = focused
                                && self.is_textfield(fid)
                            {
                                if let Some(win) = &self.window
                                    && let Some(f) = &self.rt.frame_cache
                                    && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                                {
                                    let sf = win.scale_factor() as f32;
                                    rc_web::set_ime_for_textfield_ex(
                                        win,
                                        true,
                                        hit.keyboard_type.ime_purpose_hint(),
                                        hit.auto_correct.unwrap_or(true),
                                        hit.capitalization,
                                    );
                                    win.set_ime_cursor_area(
                                        PhysicalPosition::new(
                                            (hit.rect.x * sf) as i32,
                                            (hit.rect.y * sf) as i32,
                                        ),
                                        PhysicalSize::new(
                                            (hit.rect.w * sf) as u32,
                                            (hit.rect.h * sf) as u32,
                                        ),
                                    );
                                }
                            } else {
                                // Click outside - no focus, drop IME
                                if let Some(win) = &self.window {
                                    win.set_ime_allowed(false);
                                }
                            }

                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Moved => {
                            let scale = self.scale();
                            let (mut dirty, pinch, pan) =
                                self.touch_gestures
                                    .touch_moved(&mut self.rt, tid, pos_px, scale);

                            if let Some((delta_scale, center)) = pinch
                                && self.dispatch_action(Action::Gesture(Gesture::PinchWithCenter {
                                    delta_scale,
                                    center,
                                }))
                            {
                                dirty = true;
                            }
                            if let Some(delta) = pan
                                && self.dispatch_action(Action::Gesture(Gesture::Pan { delta }))
                            {
                                dirty = true;
                            }

                            if dirty {
                                self.dirty = true;
                                self.request_redraw();
                            }
                        }

                        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                            let cancelled = t.phase == winit::event::TouchPhase::Cancelled;
                            let swipe_right = self.touch_gestures.touch_ended(
                                &mut self.rt,
                                tid,
                                pos_px,
                                cancelled,
                            );

                            let mut dirty = false;
                            if let Some(right) = swipe_right {
                                let g = if right {
                                    Gesture::SwipeRight
                                } else {
                                    Gesture::SwipeLeft
                                };
                                if self.dispatch_action(Action::Gesture(g)) {
                                    dirty = true;
                                }
                            }

                            if dirty {
                                self.dirty = true;
                                self.request_redraw();
                            }
                        }
                    }
                }

                // Basic keyboard support (hardware keyboards / Tab focus / Android soft keyboard fallback)
                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    // DO NOT REMOVE, USE FOR TESTING: log ALL keyboard events to see what's arriving from Android keyboard
                    log::info!(
                        "KeyboardInput: physical_key={:?}, logical_key={:?}, text={:?}, state={:?}, repeat={}",
                        key_event.physical_key,
                        key_event.logical_key,
                        key_event.text,
                        key_event.state,
                        key_event.repeat
                    );

                    // Route everything through the runtime: focus-chain dispatch,
                    // deletion/navigation keys, Enter submit/newline, Space/Enter
                    // activation, and composed soft-keyboard text.
                    let mapped_key = rc::map_key(key_event.physical_key, &self.rt.modifiers);
                    let utf16 = match mapped_key {
                        repose_core::input::Key::Character(c) => c as u16,
                        _ => 0,
                    };
                    let ke = repose_core::input::KeyEvent {
                        key: mapped_key,
                        modifiers: self.rt.modifiers,
                        is_repeat: key_event.repeat,
                        event_type: if key_event.state == ElementState::Pressed {
                            repose_core::input::KeyEventType::Down
                        } else {
                            repose_core::input::KeyEventType::Up
                        },
                        utf16_code_point: utf16,
                    };
                    if self.rt.handle_key_with_text(&ke, key_event.text.as_deref()) {
                        self.dirty = true;
                        self.request_redraw();
                        return;
                    }

                    // Back key / Escape handling (optional, should probably handle this via rlobkit)
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::Escape)
                            | PhysicalKey::Code(KeyCode::BrowserBack) => {
                                return;
                            }
                            _ => {}
                        }
                    }
                }

                // IME (Preedit/Commit)
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
                    if let Some(win) = &self.window {
                        self.update_ime_cursor_area(win);
                    }
                    self.dirty = true;
                    self.request_redraw();
                }

                WindowEvent::RedrawRequested => {
                    if !self.surface_active {
                        return; // surface gone; never touch the GPU
                    }

                    crate::run_pre_redraw(&self.render);

                    let do_compose = self.compose_requested.replace(false)
                        || self.dirty
                        || self.continuous_redraw();

                    if !do_compose {
                        // Present-only: no compose, just present cached scene with updated textures
                        self.process_render_commands();
                        let scale = self.scale();
                        let mut scene_opt = None;
                        if let Some(frame) = self.rt.frame_cache.as_ref() {
                            let mut scene = frame.scene.clone();
                            self.overlay_drag_indicator(&mut scene);
                            scene_opt = Some(scene);
                        }
                        if let (Some(backend), Some(scene)) =
                            (self.backend.as_mut(), scene_opt.as_ref())
                        {
                            backend.frame(scene, GlyphRasterConfig { px: 18.0 * scale });
                        }
                        self.last_redraw = web_time::Instant::now();
                        return;
                    }

                    self.rt.tick_overlays(self.last_redraw);

                    // Advance animations before composition (Compose pattern).
                    // `tick()` already calls `request_frame()` if any are running.
                    let animating = repose_core::animation_driver::tick();

                    self.process_render_commands();

                    let scale = {
                        let Some(win) = self.window.as_ref() else {
                            return;
                        };
                        win.scale_factor() as f32
                    };
                    let focused = self.rt.sched.focused;

                    self.rt.scale = scale;

                    let output = self.rt.frame(&mut self.root, &self.render);

                    // Drain upload commands queued during compose before presenting
                    self.process_render_commands();

                    if !output.wants_keyboard
                        && focused.is_some()
                        && self.rt.sched.focused.is_none()
                        && self.rt.ime_preedit
                    {
                        self.rt.ime_preedit = false;
                        if let Some(win) = self.window.as_ref() {
                            win.set_ime_allowed(false);
                        }
                    }

                    let frame = output.into_frame();

                    let scale = self.scale();
                    self.rt.after_compose(&frame, scale);

                    let mut scene = frame.scene.clone();
                    self.overlay_drag_indicator(&mut scene);

                    let Some(backend) = self.backend.as_mut() else {
                        return;
                    };
                    backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });

                    self.rt.cache_frame(frame);
                    self.last_redraw = web_time::Instant::now();

                    self.dirty = false;

                    if self.continuous_redraw() || animating {
                        if let Some(win) = self.window.as_ref() {
                            win.request_redraw();
                        }
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            crate::process_deeplinks();
            crate::process_lifecycle();

            if !self.surface_active {
                return;
            }

            let frame_requested = take_frame_request();
            let present_requested = take_present_request();

            // Compose needed ? Unified via ReposeRuntime wakeup helpers.
            let needs_compose = if !self.in_foreground {
                self.dirty || frame_requested
            } else {
                self.continuous_redraw()
                    || self.dirty
                    || frame_requested
                    || self.rt.is_wakeup_due(web_time::Instant::now())
                    || repose_core::animation_driver::is_active()
            };

            if needs_compose {
                self.request_redraw();
                return;
            }

            // Present-only: texture was updated, redraw cached scene without compose.
            let needs_present = if !self.in_foreground {
                present_requested && self.rt.frame_cache.is_some()
            } else {
                present_requested && self.rt.frame_cache.is_some()
            };
            if needs_present {
                self.request_present_only();
                return;
            }

            if let Some(deadline) = self.rt.next_wakeup_deadline() {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(deadline));
            }
        }
    }

    let mut app_state = AppState::new(Box::new(root), options);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
