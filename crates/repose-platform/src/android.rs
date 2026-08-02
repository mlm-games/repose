use crate::common as rc;
use crate::common_android as rc_android;
use crate::common_web as rc_web;
use crate::render::RenderContext;
use crate::*;

use repose_ui::TextFieldState;
use repose_ui::textfield::{TF_FONT_DP, index_for_x_bytes};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
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
use winit::window::{ImePurpose, Window, WindowAttributes};

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
/// Useful for a settings toggle; takes precedence over the static option.
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

        // touch scroll cancel-click
        touch_scrolled: bool,
        scroll_capture_id: Option<u64>,
        touch_scroll_accum_x_px: f32,
        touch_scroll_accum_y_px: f32,
        prev_touch_px: Option<(f32, f32)>,

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

        active_touches: HashMap<u64, (f32, f32)>,
        primary_touch_id: Option<u64>,
        pinch_last_dist: Option<f32>,

        // swipe tracking
        touch_start: Option<(web_time::Instant, (f32, f32))>,

        touch_long_press_pending: bool,

        last_redraw: web_time::Instant,
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

                touch_scrolled: false,
                scroll_capture_id: None,
                touch_scroll_accum_x_px: 0.0,
                touch_scroll_accum_y_px: 0.0,
                prev_touch_px: None,

                ime_visible: false,
                dirty: true,
                surface_active: false,
                in_foreground: false,

                clipboard: None,
                active_touches: HashMap::new(),
                primary_touch_id: None,
                pinch_last_dist: None,
                touch_start: None,

                touch_long_press_pending: false,

                last_redraw: web_time::Instant::now(),
            }
        }

        fn request_redraw(&self) {
            repose_core::request_frame();
            rc::request_redraw(&self.window);
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

        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.rt.frame_cache
                && let Some(i) = rc::hit_index_by_id(f, id)
                && let Some(cb) = &f.hit_regions[i].on_text_change
            {
                cb(text);
            }
        }

        fn tf_key_of(&self, visual_id: u64) -> u64 {
            rc::tf_key_of_in_frame(&self.rt.frame_cache, visual_id)
        }

        fn is_textfield(&self, id: u64) -> bool {
            rc::is_textfield_in_frame(&self.rt.frame_cache, id)
        }

        fn update_ime_state(&mut self) {
            let Some(win) = &self.window else { return };

            let allow = self.rt.sched.focused.map_or(false, |id| self.rt.is_textfield(id));

            win.set_ime_allowed(allow);

            if allow {
                win.set_ime_purpose(ImePurpose::Normal);
                self.update_ime_cursor_area(win);
            } else {
                self.rt.ime_preedit = false;
            }
        }

        fn update_ime_cursor_area(&self, win: &Window) {
            let Some(fid) = self.rt.sched.focused else {
                return;
            };
            let Some(f) = &self.rt.frame_cache else { return };
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
            self.rt.set_viewport_and_scale(size.width, size.height, scale);
            if let Some(b) = &mut self.backend {
                b.configure_surface(size.width, size.height);
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
            rc::process_render_commands(backend, self.render.drain());
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
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
                if let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.rt.sched, f)
                {
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
                    if let Some(win) = &self.window {
                        rc_web::set_ime_for_textfield(win, self.rt.is_textfield(new_id));
                    }
                    return true;
                }
            }

            false
        }
        fn overlay_drag_indicator(&self, scene: &mut Scene) {
            repose_core::dnd::overlay_drag_indicator(
                scene,
                self.rt.mouse_pos_px,
                false,
            );
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

                    match repose_render_wgpu::WgpuBackend::new_with_msaa(
                        w.clone(),
                        self.options.common.msaa_samples,
                    ) {
                        Ok(b) => {
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

                // Touch handling (Android primary)
                WindowEvent::Touch(t) => {
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    self.rt.mouse_pos_px = pos_px;
                    let pos = Vec2 {
                        x: pos_px.0,
                        y: pos_px.1,
                    };

                    let tid = t.id;
                    self.active_touches.insert(tid, pos_px);

                    match t.phase {
                        winit::event::TouchPhase::Started => {
                            self.touch_scrolled = false;
                            self.scroll_capture_id = None;
                            self.touch_scroll_accum_x_px = 0.0;
                            self.touch_scroll_accum_y_px = 0.0;

                            if self.primary_touch_id.is_none() {
                                self.primary_touch_id = Some(tid);
                                self.touch_start = Some((web_time::Instant::now(), pos_px));
                                self.touch_long_press_pending = true;
                            }

                            // Delegate common pointer-press logic to the runtime
                            let press_result = self.rt.handle_pointer_press(pos, PointerButton::Primary);

                            // Platform-specific IME setup for focused textfields
                            if let Some(fid) = press_result.focused
                                && self.is_textfield(fid)
                            {
                                if let Some(win) = &self.window
                                    && let Some(f) = &self.rt.frame_cache
                                    && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                                {
                                    let sf = win.scale_factor() as f32;
                                    win.set_ime_allowed(true);
                                    win.set_ime_purpose(ImePurpose::Normal);
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

                            self.prev_touch_px = Some(pos_px);
                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Moved => {
                            // Pinch gesture detection (platform-specific)
                            if self.active_touches.len() == 2 {
                                let mut it = self.active_touches.values();
                                let a = it.next().copied().unwrap();
                                let b = it.next().copied().unwrap();
                                let dx = a.0 - b.0;
                                let dy = a.1 - b.1;
                                let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                                if let Some(prev) = self.pinch_last_dist.replace(dist) {
                                    let delta_scale = (dist / prev).clamp(0.5, 2.0);
                                    if self.dispatch_action(Action::Gesture(Gesture::Pinch {
                                        delta_scale,
                                    })) {
                                        self.dirty = true;
                                        self.request_redraw();
                                    }
                                }
                            }

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
                                            Vec2 { x: -dx_px, y: -dy_px },
                                            self.scroll_capture_id,
                                        );
                                        self.scroll_capture_id = cap;

                                        if consumed
                                            && (self.touch_scroll_accum_x_px.abs()
                                                > 6.0 * self.scale()
                                                || self.touch_scroll_accum_y_px.abs()
                                                    > 6.0 * self.scale())
                                        {
                                            self.touch_scrolled = true;
                                        }
                                    }
                                }

                                // Delegate pointer-move to runtime for enter/leave/move dispatch
                                self.rt.handle_pointer_move(pos);
                            }

                            self.prev_touch_px = Some(pos_px);
                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                            self.touch_long_press_pending = false;

                            if t.phase == winit::event::TouchPhase::Cancelled {
                                self.rt.handle_pointer_cancel();
                            } else {
                                self.rt.handle_pointer_release(pos, PointerButton::Primary);
                            }

                            self.active_touches.remove(&tid);
                            if self.active_touches.len() < 2 {
                                self.pinch_last_dist = None;
                            }

                            // Swipe gesture detection (platform-specific)
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

                                        if self.dispatch_action(Action::Gesture(g.clone()))
                                            || (dx > 0.0 && self.dispatch_action(Action::Back))
                                        {
                                            self.dirty = true;
                                            self.request_redraw();
                                            return;
                                        }
                                    }
                                }
                            }

                            self.scroll_capture_id = None;
                            self.prev_touch_px = None;
                            self.dirty = true;
                            self.request_redraw();
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

                    // Handle text from Android soft keyboard (fallback when IME events don't work)
                    // Filter out backspace character (\u{8}) which should be handled as delete, not text
                    if let Some(text) = &key_event.text {
                        let is_backspace_char = text == "\u{8}" || text == "\u{7f}"; // BS or DEL
                        if !text.is_empty()
                            && !is_backspace_char
                            && key_event.state == ElementState::Pressed
                        {
                            if let Some(focused_id) = self.rt.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                    let mut state = state_rc.borrow_mut();
                                    state.insert_text(text);
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.dirty = true;
                                    self.request_redraw();
                                }
                            }
                        }
                    }

                    // Handle Backspace for textfields (Android soft keyboard fallback)
                    if key_event.state == ElementState::Pressed {
                        let is_backspace = matches!(
                            key_event.physical_key,
                            PhysicalKey::Code(KeyCode::Backspace)
                        ) || matches!(
                            key_event.logical_key,
                            winit::keyboard::Key::Named(winit::keyboard::NamedKey::Backspace)
                        );
                        if is_backspace {
                            if let Some(focused_id) = self.rt.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                                    let mut state = state_rc.borrow_mut();
                                    state.delete_backward();
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.dirty = true;
                                    self.request_redraw();
                                }
                            }
                        }

                        // Handle Enter for textfields (commit composition or submit)
                        if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Enter)) {
                            if let Some(focused_id) = self.rt.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state) = self.rt.textfield_states.get(&key) {
                                    let mut st = state.borrow_mut();
                                    // If we have a composition, commit it
                                    if st.composition.is_some() {
                                        st.commit_composition(String::new());
                                        self.notify_text_change(focused_id, st.text.clone());
                                    }
                                }
                            }
                        }
                    }

                    // Back key / Escape handling (optional)
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::Escape)
                            | PhysicalKey::Code(KeyCode::BrowserBack) => {
                                if repose_core::dnd::handle_drag_action(
                                    &repose_core::shortcuts::DragAction::Cancel,
                                ) {
                                    self.request_redraw();
                                    return;
                                }
                                return;
                            }
                            _ => {}
                        }
                    }

                    // Dispatch key event through focus ancestor chain (Compose-compatible)
                    let mapped_key = rc::map_key(key_event.physical_key);
                    let utf16 = match mapped_key {
                        repose_core::input::Key::Character(c) => c as u16,
                        _ => 0,
                    };
                    let mods = self.rt.modifiers;
                    let repeat = key_event.repeat;
                    let ev_type = if key_event.state == ElementState::Pressed {
                        repose_core::input::KeyEventType::Down
                    } else {
                        repose_core::input::KeyEventType::Up
                    };
                    let consumed = self.rt
                        .frame_cache
                        .as_ref()
                        .and_then(|f| {
                            let focused = self.rt.sched.focused.or_else(|| {
                                f.semantics_nodes
                                    .iter()
                                    .find(|n| n.parent.is_none())
                                    .map(|n| n.id)
                            })?;
                            let sem_parent_of: std::collections::HashMap<u64, u64> = f
                                .semantics_nodes
                                .iter()
                                .filter_map(|n| n.parent.map(|p| (n.id, p)))
                                .collect();
                            let hit_by_id: std::collections::HashMap<u64, &HitRegion> =
                                f.hit_regions.iter().map(|h| (h.id, h)).collect();
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
                            // Top-down preview: root → focused
                            for &id in ancestors.iter().rev() {
                                if let Some(hit) = hit_by_id.get(&id) {
                                    if let Some(cb) = &hit.on_preview_key_event {
                                        if cb(make_ke()) {
                                            return Some(true);
                                        }
                                    }
                                }
                            }
                            // Bottom-up normal: focused → root
                            for &id in ancestors.iter() {
                                if let Some(hit) = hit_by_id.get(&id) {
                                    if let Some(cb) = &hit.on_key_event {
                                        if cb(make_ke()) {
                                            return Some(true);
                                        }
                                    }
                                }
                            }
                            None
                        })
                        .unwrap_or(false);
                    if consumed {
                        self.dirty = true;
                        self.request_redraw();
                        return;
                    }

                    if key_event.state == ElementState::Pressed {
                        if let Some(action) = repose_core::shortcuts::resolve_action(
                            repose_core::shortcuts::KeyChord::new(
                                rc::map_key(key_event.physical_key),
                                self.rt.modifiers,
                            ),
                        ) {
                            if self.dispatch_action(action) {
                                self.dirty = true;
                                self.request_redraw();
                                return;
                            }
                        }
                    }

                    // Keyboard activation for focused buttons (Space/Enter)
                    if let Some(fid) = self.rt.sched.focused {
                        let is_textfield = if let Some(f) = &self.rt.frame_cache {
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
                                    if key_event.state == ElementState::Pressed && !key_event.repeat
                                    {
                                        self.rt.pressed_ids.insert(fid);
                                        self.rt.key_pressed_active = Some(fid);
                                        self.dirty = true;
                                        self.request_redraw();
                                        return;
                                    } else if key_event.state == ElementState::Released {
                                        if let Some(active_id) = self.rt.key_pressed_active.take() {
                                            self.rt.pressed_ids.remove(&active_id);
                                            if let Some(f) = &self.rt.frame_cache
                                                && let Some(hit) =
                                                    f.hit_regions.iter().find(|h| h.id == active_id)
                                            {
                                                if let Some(cb) = &hit.on_click {
                                                    cb();
                                                } else if let Some(cb) = &hit.on_pointer_down {
                                                    let pe = PointerEvent::new(
                                                        PointerId(0),
                                                        PointerKind::Mouse,
                                                        PointerEventKind::Down(
                                                            PointerButton::Primary,
                                                        ),
                                                        Vec2 { x: 0.0, y: 0.0 },
                                                        1.0,
                                                        self.rt.modifiers,
                                                    );
                                                    cb(pe);
                                                }
                                            }
                                            self.dirty = true;
                                            self.request_redraw();
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Enter submits focused TextField
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                            if let Some(focused_id) = self.rt.sched.focused
                                && let Some(f) = &self.rt.frame_cache
                                && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                && let Some(on_submit) = &f.hit_regions[i].on_text_submit
                            {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state) = self.rt.textfield_states.get(&key) {
                                    on_submit(state.borrow().text.clone());
                                }
                            }
                        }
                    }
                }

                // IME (Preedit/Commit)
                WindowEvent::Ime(ime) => {
                    if let Some(focused_id) = self.rt.sched.focused {
                        let key = self.tf_key_of(focused_id);
                        if let Some(state_rc) = self.rt.textfield_states.get(&key) {
                            let mut state = state_rc.borrow_mut();

                            let hit_rect = if let Some(f) = self.rt.frame_cache.as_ref() {
                                rc::hit_index_by_id(f, focused_id)
                                    .map(|i| f.hit_regions[i].rect)
                                    .unwrap_or_default()
                            } else {
                                Rect::default()
                            };

                            match ime {
                                Ime::Enabled => {
                                    self.rt.ime_preedit = false;
                                    if !self.ime_visible {
                                        self.ime_visible = true;
                                        // Only estimates if rlobkit's real ime_bottom
                                        // is still 0 (see update_ime_inset).
                                        self.update_ime_inset();
                                    }
                                }
                                Ime::Preedit(text, cursor) => {
                                    let cursor_usize =
                                        cursor.map(|(a, b)| (a as usize, b as usize));
                                    state.set_composition(text.clone(), cursor_usize);
                                    self.rt.ime_preedit = !text.is_empty();
                                    self.notify_text_change(focused_id, state.text.clone());
                                    let font_px =
                                        dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                                    let m = repose_ui::textfield::measure_text(
                                        &state.text,
                                        font_px,
                                        repose_ui::textfield::TextMeasureConfig::default(),
                                    );
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x_px,
                                        hit_rect.w,
                                        dp_to_px(2.0),
                                    );
                                }
                                Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.rt.ime_preedit = false;
                                    self.notify_text_change(focused_id, state.text.clone());
                                    let font_px =
                                        dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                                    let m = repose_ui::textfield::measure_text(
                                        &state.text,
                                        font_px,
                                        repose_ui::textfield::TextMeasureConfig::default(),
                                    );
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x_px,
                                        hit_rect.w,
                                        dp_to_px(2.0),
                                    );
                                }
                                Ime::Disabled => {
                                    self.rt.ime_preedit = false;
                                    if self.ime_visible {
                                        self.ime_visible = false;
                                        self.update_ime_inset();
                                    }
                                    if state.composition.is_some() {
                                        state.cancel_composition();
                                        self.notify_text_change(focused_id, state.text.clone());
                                        let font_px = dp_to_px(TF_FONT_DP)
                                            * repose_core::locals::text_scale().0;
                                        let m = repose_ui::textfield::measure_text(
                                            &state.text,
                                            font_px,
                                            repose_ui::textfield::TextMeasureConfig::default(),
                                        );
                                        let caret_x_px = m
                                            .positions
                                            .get(state.caret_index())
                                            .copied()
                                            .unwrap_or(0.0);
                                        state.ensure_caret_visible(
                                            caret_x_px,
                                            hit_rect.w,
                                            dp_to_px(2.0),
                                        );
                                    }
                                }
                            }

                            if let Some(win) = &self.window {
                                self.update_ime_cursor_area(win);
                            }

                            self.dirty = true;
                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    if !self.surface_active {
                        return; // surface gone; never touch the GPU
                    }

                    rc::tick_snackbar(self.last_redraw);

                    // Advance animations before composition (Compose pattern).
                    // `tick()` already calls `request_frame()` if any are running.
                    let animating = repose_core::animation_driver::tick();

                    self.process_render_commands();

                    let scale = {
                        let Some(win) = self.window.as_ref() else { return; };
                        win.scale_factor() as f32
                    };
                    let size_px_u32 = self.rt.sched.size;
                    let focused = self.rt.sched.focused;

                    let rc = self.render.clone();
                    let root_fn = &mut self.root;

                    let mut composed_root = move |s: &mut Scheduler| (root_fn)(s, &rc);

                    let sched = &mut self.rt.sched;
                    let pressed_ids = &self.rt.pressed_ids;
                    let textfield_states = &self.rt.textfield_states;

                    let mut frame = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                        move || {
                            compose_frame(
                                sched,
                                &mut composed_root,
                                scale,
                                size_px_u32,
                                None, // hover_id (no mouse on Android usually)
                                pressed_ids,
                                textfield_states,
                                focused,
                            )
                        },
                    )) {
                        Ok(frame) => frame,
                        Err(_) => {
                            log::error!("compose panicked; presenting last good frame");
                            if let (Some(backend), Some(cached)) = (
                                self.backend.as_mut(),
                                self.rt.frame_cache.as_ref(),
                            ) {
                                let mut scene = cached.scene.clone();
                                backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                            }
                            return;
                        }
                    };

                    // Drain upload commands queued during compose before presenting
                    self.process_render_commands();

                    let output = repose_app::FrameOutput {
                        scene: frame.scene.clone(),
                        hit_regions: frame.hit_regions.clone(),
                        semantics_nodes: frame.semantics_nodes.clone(),
                        focus_chain: frame.focus_chain.clone(),
                        platform: repose_app::PlatformOutput {
                            cursor: None,
                            ime_allowed: false,
                            ime_cursor_area: None,
                            clipboard_text: None,
                        },
                        wants_pointer: !frame.hit_regions.is_empty() || self.rt.capture_id.is_some(),
                        wants_keyboard: !self.rt.textfield_states.is_empty() || self.rt.ime_preedit,
                    };

                    if !output.wants_keyboard && focused.is_some() && self.rt.sched.focused.is_none() && self.rt.ime_preedit {
                        self.rt.ime_preedit = false;
                        if let Some(win) = self.window.as_ref() {
                            win.set_ime_allowed(false);
                        }
                    }

                    repose_core::dnd::set_dnd_frame(Some(frame.clone()));
                    repose_core::dnd::set_dnd_scale(scale);
                    self.overlay_drag_indicator(&mut frame.scene);

                    let Some(backend) = self.backend.as_mut() else { return; };
                    backend.frame(&frame.scene, GlyphRasterConfig { px: 18.0 * scale });

                    if let Some(fid) = self.rt.sched.focused {
                        if let Some(hit) = frame.hit_regions.iter().find(|h| h.id == fid)
                            && let Some(key) = hit.tf_state_key
                            && !self.rt.textfield_states.contains_key(&key)
                        {
                            self.rt.textfield_states
                                .entry(key)
                                .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())))
                                .borrow_mut()
                                .reset_caret_blink();
                        }
                    }

                    self.rt.reconcile_hover_from_mouse_pos(&frame);
                    self.rt.frame_cache = Some(frame);
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

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            crate::process_deeplinks();
            crate::process_lifecycle();

            if !self.surface_active {
                return;
            }

            let frame_requested = take_frame_request();
            let present_requested = take_present_request();

            let needs = if !self.in_foreground {
                // Surface active but app backgrounded: only honor explicit
                // frame requests (e.g. a bg worker wanting a toast), not caret
                // blink or animations.
                self.dirty || frame_requested || present_requested
            } else {
                self.continuous_redraw()
                    || self.dirty
                    || frame_requested
                    || (present_requested && self.rt.frame_cache.is_some())
                    || crate::next_caret_blink_deadline(
                        &self.rt.sched,
                        &self.rt.frame_cache,
                        &self.rt.textfield_states,
                    )
                    .is_some_and(|d| d <= web_time::Instant::now())
                    || repose_core::animation_driver::is_active()
            };

            if needs {
                self.request_redraw();
            }
        }
    }

    let mut app_state = AppState::new(Box::new(root), options);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
