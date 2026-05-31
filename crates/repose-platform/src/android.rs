use crate::common as rc;
use crate::common_android as rc_android;
use crate::common_web as rc_web;
use crate::render::{RenderCommand, RenderContext};
use crate::*;

use repose_ui::TextFieldState;
use repose_ui::textfield::{TF_FONT_DP, TF_PADDING_X_DP, index_for_x_bytes};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use repose_core::shortcuts::{Action, Gesture};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{ImePurpose, Window, WindowAttributes};

#[derive(Clone)]
struct DragSession {
    source_id: u64,
    payload: repose_core::dnd::DragPayload,
    start_px: (f32, f32),
    over_id: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct AndroidOptions {
    /// If true, runner keeps requesting frames (good for animations, costs battery).
    pub continuous_redraw: bool,

    /// If true, runner wraps the app root in a ScrollV container.
    /// Useful for "webpage-like" apps; off by default to avoid nested scroll surprises.
    pub auto_root_scroll: bool,

    /// IME (soft keyboard) inset height in physical pixels.
    /// When the keyboard opens on Android, `set_ime_inset()` is called with this value.
    /// If `None`, the runner estimates ~40% of the window's shorter dimension.
    pub ime_height_px: Option<f32>,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            // Keep behavior close to your original runner: always ticking.
            continuous_redraw: true,
            auto_root_scroll: false,
            ime_height_px: None,
        }
    }
}

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

    struct AppState {
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        render: RenderContext,
        options: AndroidOptions,

        window: Option<Arc<Window>>,
        backend: Option<repose_render_wgpu::WgpuBackend>,
        sched: Scheduler,
        frame_cache: Option<Frame>,

        // input state
        last_pos_px: (f32, f32),
        modifiers: Modifiers,
        capture_id: Option<u64>,
        pressed_ids: HashSet<u64>,

        // touch scroll cancel-click
        touch_scrolled: bool,
        touch_scroll_accum_y_px: f32,
        prev_touch_px: Option<(f32, f32)>,

        // TextFields
        textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
        ime_preedit: bool,

        // auto root scroll state
        root_scroll: Rc<RefCell<rc::RootScrollState>>,

        // IME (soft keyboard) tracking
        ime_visible: bool,

        // redraw control
        dirty: bool,

        // clipboard
        clipboard: Option<clipawl::Clipboard>,

        active_touches: HashMap<u64, (f32, f32)>,
        primary_touch_id: Option<u64>,
        pinch_last_dist: Option<f32>,

        // swipe tracking
        touch_start: Option<(web_time::Instant, (f32, f32))>,

        drag: Option<DragSession>,
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
                sched: Scheduler::new(),
                frame_cache: None,

                last_pos_px: (0.0, 0.0),
                modifiers: Modifiers::default(),
                capture_id: None,
                pressed_ids: HashSet::new(),

                touch_scrolled: false,
                touch_scroll_accum_y_px: 0.0,
                prev_touch_px: None,

                textfield_states: HashMap::new(),
                ime_preedit: false,

                root_scroll: Rc::new(RefCell::new(rc::RootScrollState::default())),
                ime_visible: false,
                dirty: true,

                clipboard: None,
                active_touches: HashMap::new(),
                primary_touch_id: None,
                pinch_last_dist: None,
                touch_start: None,

                drag: None,
            }
        }

        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
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

        fn padding_px(&self) -> f32 {
            self.dp_px(TF_PADDING_X_DP)
        }

        fn touch_slop_px(&self) -> f32 {
            rc::touch_slop_px(self.scale())
        }

        fn tf_key_of(&self, visual_id: u64) -> u64 {
            if let Some(f) = &self.frame_cache {
                return rc::tf_key_of(f, visual_id);
            }
            visual_id
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

        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.frame_cache
                && let Some(i) = rc::hit_index_by_id(f, id)
                && let Some(cb) = &f.hit_regions[i].on_text_change
            {
                cb(text);
            }
        }

        fn update_ime_state(&mut self) {
            let Some(win) = &self.window else { return };

            let allow = self.sched.focused.map_or(false, |id| self.is_textfield(id));

            win.set_ime_allowed(allow);

            if allow {
                win.set_ime_purpose(ImePurpose::Normal);
                self.update_ime_cursor_area(win);
            } else {
                self.ime_preedit = false;
            }
        }

        fn update_ime_cursor_area(&self, win: &Window) {
            let Some(fid) = self.sched.focused else {
                return;
            };
            let Some(f) = &self.frame_cache else { return };
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

        fn ensure_caret_visible_in_hit(&self, st: &mut TextFieldState, hit_rect: Rect) {
            let is_multiline = self
                .frame_cache
                .as_ref()
                .and_then(|f| f.hit_regions.iter().find(|h| h.rect == hit_rect))
                .map(|h| h.tf_multiline)
                .unwrap_or(false);
            rc::tf_ensure_caret_visible(st, is_multiline);
        }

        fn update_ime_inset(&self) {
            let h = if self.ime_visible {
                self.options.ime_height_px.unwrap_or_else(|| {
                    // Estimate ~40% of window's shorter dimension as default IME height
                    let size = self.window.as_ref().map(|w| w.inner_size()).unwrap_or_default();
                    (size.width.min(size.height) as f32 * 0.4).max(200.0)
                })
            } else {
                0.0
            };
            set_ime_inset(h);
        }

        fn sync_window_size(&mut self, size: PhysicalSize<u32>) {
            self.sched.size = (size.width, size.height);
            if let Some(b) = &mut self.backend {
                b.configure_surface(size.width, size.height);
            }
            // Recompute IME inset estimate when window size changes
            self.update_ime_inset();
        }

        fn copy_to_clipboard(&mut self, text: &str) {
            if let Some(cb) = &mut self.clipboard {
                let _ = pollster::block_on(cb.set_text(text));
            }
        }

        fn paste_from_clipboard(&mut self) -> Option<String> {
            if let Some(cb) = &mut self.clipboard {
                pollster::block_on(cb.get_text()).ok()
            } else {
                None
            }
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = &mut self.backend else {
                return;
            };

            for cmd in self.render.drain() {
                match cmd {
                    RenderCommand::SetImageEncoded {
                        handle,
                        bytes,
                        srgb,
                    } => {
                        let _ = backend.set_image_from_bytes(handle, &bytes, srgb);
                    }
                    RenderCommand::SetImageRgba8 {
                        handle,
                        w,
                        h,
                        rgba,
                        srgb,
                    } => {
                        let _ = backend.set_image_rgba8(handle, w, h, &rgba, srgb);
                    }
                    RenderCommand::SetImageNv12 {
                        handle,
                        w,
                        h,
                        y,
                        uv,
                        full_range,
                    } => {
                        let _ = backend.set_image_nv12(handle, w, h, &y, &uv, full_range);
                    }
                    RenderCommand::RemoveImage { handle } => {
                        backend.remove_image(handle);
                    }
                }
            }
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
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

            self.dispatch_default_action(action)
        }

        fn dispatch_default_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
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
                    self.copy_to_clipboard(&txt);
                    true
                }
                Action::Cut => {
                    let txt = state_rc.borrow().selected_text();
                    if txt.is_empty() {
                        return false;
                    }
                    self.copy_to_clipboard(&txt);
                    {
                        let mut st = state_rc.borrow_mut();
                        st.insert_text("");
                        self.notify_text_change(fid, st.text.clone());
                        if let Some(f) = &self.frame_cache
                            && let Some(i) = rc::hit_index_by_id(f, fid)
                        {
                            self.ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].rect);
                        }
                    }
                    true
                }
                Action::Paste => {
                    let Some(mut txt) = self.paste_from_clipboard() else {
                        return false;
                    };
                    txt.retain(|c| !c.is_control() && c != '\n' && c != '\r');
                    if txt.is_empty() {
                        return false;
                    }
                    {
                        let mut st = state_rc.borrow_mut();
                        st.insert_text(&txt);
                        self.notify_text_change(fid, st.text.clone());
                        if let Some(f) = &self.frame_cache
                            && let Some(i) = rc::hit_index_by_id(f, fid)
                        {
                            self.ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].rect);
                        }
                    }
                    true
                }
                Action::SelectAll => {
                    {
                        let mut st = state_rc.borrow_mut();
                        st.selection = 0..st.text.len();
                        if let Some(f) = &self.frame_cache
                            && let Some(i) = rc::hit_index_by_id(f, fid)
                        {
                            self.ensure_caret_visible_in_hit(&mut st, f.hit_regions[i].rect);
                        }
                    }
                    true
                }
                _ => false,
            }
        }
        fn dnd_slop_px(&self) -> f32 {
            rc::touch_slop_px(self.scale())
        }

        fn dnd_update_over(&mut self, pos: Vec2) {
            let Some(f) = &self.frame_cache else {
                return;
            };
            let Some(session) = self.drag.as_mut() else {
                return;
            };

            let new_over = rc::dnd_target_id_at(f, pos);

            if new_over != session.over_id {
                if let Some(prev) = session.over_id {
                    if let Some(i) = rc::hit_index_by_id(f, prev) {
                        if let Some(cb) = &f.hit_regions[i].on_drag_leave {
                            cb(repose_core::dnd::DragOver {
                                source_id: session.source_id,
                                target_id: prev,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }

                if let Some(now) = new_over {
                    if let Some(i) = rc::hit_index_by_id(f, now) {
                        if let Some(cb) = &f.hit_regions[i].on_drag_enter {
                            cb(repose_core::dnd::DragOver {
                                source_id: session.source_id,
                                target_id: now,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }

                session.over_id = new_over;
            }

            if let Some(over) = session.over_id {
                if let Some(i) = rc::hit_index_by_id(f, over) {
                    if let Some(cb) = &f.hit_regions[i].on_drag_over {
                        cb(repose_core::dnd::DragOver {
                            source_id: session.source_id,
                            target_id: over,
                            position: pos,
                            modifiers: self.modifiers,
                            payload: session.payload.clone(),
                        });
                    }
                }
            }
        }

        fn dnd_try_begin_touch(&mut self, pos: Vec2) -> bool {
            if self.drag.is_some() {
                return true;
            }
            let Some(cid) = self.capture_id else {
                return false;
            };
            let Some((_t0, (sx, sy))) = self.touch_start else {
                return false;
            };

            let dx = pos.x - sx;
            let dy = pos.y - sy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < self.dnd_slop_px() {
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

            self.drag = Some(DragSession {
                source_id: cid,
                payload,
                start_px: (sx, sy),
                over_id: None,
            });

            // Prevent click-on-release behavior
            self.touch_scrolled = true;
            true
        }

        fn dnd_finish(&mut self, pos: Vec2, accept_if_possible: bool) {
            let Some(f) = &self.frame_cache else {
                self.drag = None;
                self.capture_id = None;
                self.request_redraw();
                return;
            };
            let Some(session) = self.drag.take() else {
                return;
            };

            let mut accepted = false;
            if accept_if_possible {
                let drop_target = rc::dnd_target_id_at(f, pos);
                if let Some(tid) = drop_target {
                    if let Some(i) = rc::hit_index_by_id(f, tid) {
                        if let Some(cb) = &f.hit_regions[i].on_drop {
                            accepted = cb(repose_core::dnd::DropEvent {
                                source_id: session.source_id,
                                target_id: tid,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }
            }

            if let Some(i) = rc::hit_index_by_id(f, session.source_id) {
                if let Some(cb) = &f.hit_regions[i].on_drag_end {
                    cb(repose_core::dnd::DragEnd { accepted });
                }
            }

            self.capture_id = None;
            self.request_redraw();
        }
    }

    impl ApplicationHandler<()> for AppState {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            if self.window.is_some() {
                return;
            }

            match el.create_window(WindowAttributes::default().with_title("Repose Android")) {
                Ok(win) => {
                    let w = Arc::new(win);
                    let sz = w.inner_size();
                    self.sync_window_size(sz);

                    match repose_render_wgpu::WgpuBackend::new(w.clone()) {
                        Ok(b) => {
                            self.backend = Some(b);
                            self.window = Some(w);
                            self.clipboard = clipawl::Clipboard::new().ok();
                            self.dirty = true;
                            self.request_redraw();
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
                    self.sync_window_size(size);
                    self.dirty = true;
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

                // Touch handling (Android primary)
                WindowEvent::Touch(t) => {
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    self.last_pos_px = pos_px;
                    let pos = Vec2 {
                        x: pos_px.0,
                        y: pos_px.1,
                    };

                    let tid = t.id;
                    self.active_touches.insert(tid, pos_px);

                    match t.phase {
                        winit::event::TouchPhase::Started => {
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

                                    // focus + IME for textfields
                                    if self.is_textfield(hit.id) {
                                        self.sched.focused = Some(hit.id);
                                        let key = self.tf_key_of(hit.id);
                                        self.textfield_states.entry(key).or_insert_with(|| {
                                            Rc::new(RefCell::new(TextFieldState::new()))
                                        });

                                        if let Some(win) = &self.window {
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

                                        // caret placement on touch down
                                        let key = self.tf_key_of(hit.id);
                                        if let Some(state_rc) = self.textfield_states.get(&key) {
                                            let mut st = state_rc.borrow_mut();
                                            let inner_x_px = hit.rect.x + self.padding_px();
                                            let content_x_px =
                                                pos_px.0 - inner_x_px + st.scroll_offset;
                                            let font_px = dp_to_px(TF_FONT_DP)
                                                * repose_core::locals::text_scale().0;
                                            let idx = index_for_x_bytes(
                                                &st.text,
                                                font_px,
                                                content_x_px.max(0.0),
                                            );
                                            st.begin_drag(idx, self.modifiers.shift);
                                            self.ensure_caret_visible_in_hit(&mut st, hit.rect);
                                        }
                                    } else {
                                        self.sched.focused = None;
                                        self.ime_preedit = false;
                                        if let Some(win) = &self.window {
                                            win.set_ime_allowed(false);
                                        }
                                    }

                                    // pointer down callback
                                    if let Some(cb) = &hit.on_pointer_down {
                                        cb(rc::pe_down_primary(
                                            repose_core::input::PointerKind::Touch,
                                            pos,
                                            self.modifiers,
                                        ));
                                    }
                                } else {
                                    self.sched.focused = None;
                                    self.ime_preedit = false;
                                    if let Some(win) = &self.window {
                                        win.set_ime_allowed(false);
                                    }
                                }
                            }

                            self.prev_touch_px = Some(pos_px);
                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Moved => {
                            if self.drag.is_some() {
                                self.dnd_update_over(pos);
                                self.dirty = true;
                                self.request_redraw();
                                return;
                            }

                            if self.dnd_try_begin_touch(pos) {
                                self.dnd_update_over(pos);
                                self.dirty = true;
                                self.request_redraw();
                                return;
                            }
                            // Pinch gesture detection
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

                            if let (Some(prev), Some(f)) = (self.prev_touch_px, &self.frame_cache) {
                                let dy_px = pos_px.1 - prev.1;

                                // Always attempt to scroll the best consumer under the finger.
                                if dy_px.abs() > 0.0 {
                                    self.touch_scroll_accum_y_px += dy_px;

                                    let consumed =
                                        rc::dispatch_scroll(f, pos, Vec2 { x: 0.0, y: -dy_px });

                                    if consumed
                                        && self.touch_scroll_accum_y_px.abs() > self.touch_slop_px()
                                    {
                                        self.touch_scrolled = true;
                                    }
                                }

                                // still deliver pointer_move to captured widget if present
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
                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
                            if self.drag.is_some() {
                                self.dnd_finish(pos, true);
                                self.capture_id = None;
                                self.prev_touch_px = None;
                                self.pressed_ids.clear();
                                self.dirty = true;
                                self.request_redraw();
                                return;
                            }

                            self.active_touches.remove(&tid);
                            if self.active_touches.len() < 2 {
                                self.pinch_last_dist = None;
                            }

                            // Swipe gesture detection
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
                                            self.capture_id = None;
                                            self.prev_touch_px = None;
                                            self.pressed_ids.clear();
                                            self.dirty = true;
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
                                    if t.phase == winit::event::TouchPhase::Ended
                                        && !self.touch_scrolled
                                        && hit.rect.contains(pos)
                                        && let Some(cb) = &hit.on_click
                                    {
                                        cb();
                                    }

                                    // end drag selection for textfields
                                    if self.is_textfield(cid) {
                                        let key = self.tf_key_of(cid);
                                        if let Some(st) = self.textfield_states.get(&key) {
                                            st.borrow_mut().end_drag();
                                        }
                                    }
                                }
                            }

                            self.capture_id = None;
                            self.prev_touch_px = None;
                            self.pressed_ids.clear();
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
                            if let Some(focused_id) = self.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state_rc) = self.textfield_states.get(&key) {
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
                            if let Some(focused_id) = self.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state_rc) = self.textfield_states.get(&key) {
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
                            if let Some(focused_id) = self.sched.focused {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state) = self.textfield_states.get(&key) {
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
                                // If you use repose_navigation::back on Android too, call it here.
                                // use repose_navigation::back;
                                // if !back::handle() { el.exit(); }
                                return;
                            }
                            _ => {}
                        }
                    }

                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        if let Some(action) = repose_core::shortcuts::resolve_action(
                            repose_core::shortcuts::KeyChord::new(
                                rc::map_key(key_event.physical_key),
                                self.modifiers,
                            ),
                        ) {
                            if self.dispatch_action(action) {
                                self.dirty = true;
                                self.request_redraw();
                                return;
                            }
                        }
                    }

                    // Tab traversal
                    if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                        if key_event.state == ElementState::Pressed && !key_event.repeat {
                            if let Some(f) = &self.frame_cache {
                                if let Some(next) = rc::focus_next_in_chain(
                                    &f.focus_chain,
                                    self.sched.focused,
                                    self.modifiers.shift,
                                ) {
                                    self.sched.focused = Some(next);
                                    if let Some(win) = &self.window {
                                        win.set_ime_allowed(self.is_textfield(next));
                                    }
                                    self.dirty = true;
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
                                }
                            }
                        }
                    }
                }

                // IME (Preedit/Commit)
                WindowEvent::Ime(ime) => {
                    if let Some(focused_id) = self.sched.focused {
                        let key = self.tf_key_of(focused_id);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut state = state_rc.borrow_mut();

                            let hit_rect = if let Some(f) = self.frame_cache.as_ref() {
                                rc::hit_index_by_id(f, focused_id)
                                    .map(|i| f.hit_regions[i].rect)
                                    .unwrap_or_default()
                            } else {
                                Rect::default()
                            };

                            match ime {
                                Ime::Enabled => {
                                    self.ime_preedit = false;
                                    if !self.ime_visible {
                                        self.ime_visible = true;
                                        self.update_ime_inset();
                                    }
                                }
                                Ime::Preedit(text, cursor) => {
                                    let cursor_usize =
                                        cursor.map(|(a, b)| (a as usize, b as usize));
                                    state.set_composition(text.clone(), cursor_usize);
                                    self.ime_preedit = !text.is_empty();
                                    self.notify_text_change(focused_id, state.text.clone());
                                    let font_px =
                                        dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                                    let m = repose_ui::textfield::measure_text(
                                        &state.text,
                                        font_px,
                                        None,
                                    );
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x_px,
                                        hit_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                        dp_to_px(2.0),
                                    );
                                }
                                Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.ime_preedit = false;
                                    self.notify_text_change(focused_id, state.text.clone());
                                    let font_px =
                                        dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                                    let m = repose_ui::textfield::measure_text(
                                        &state.text,
                                        font_px,
                                        None,
                                    );
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x_px,
                                        hit_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                        dp_to_px(2.0),
                                    );
                                }
                                Ime::Disabled => {
                                    self.ime_preedit = false;
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
                                            None,
                                        );
                                        let caret_x_px = m
                                            .positions
                                            .get(state.caret_index())
                                            .copied()
                                            .unwrap_or(0.0);
                                        state.ensure_caret_visible(
                                            caret_x_px,
                                            hit_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
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
                    self.process_render_commands();

                    let (Some(backend), Some(win)) = (self.backend.as_mut(), self.window.as_ref())
                    else {
                        return;
                    };

                    let scale = win.scale_factor() as f32;
                    let size_px_u32 = self.sched.size;
                    let focused = self.sched.focused;

                    let auto_root_scroll = self.options.auto_root_scroll;
                    let root_scroll = self.root_scroll.clone();
                    let rc = self.render.clone();
                    let root_fn = &mut self.root;

                    let mut composed_root = move |s: &mut Scheduler| {
                        let v = (root_fn)(s, &rc);
                        if auto_root_scroll {
                            rc::wrap_root_scroll(v, root_scroll.clone())
                        } else {
                            v
                        }
                    };

                    let frame = compose_frame(
                        &mut self.sched,
                        &mut composed_root,
                        scale,
                        size_px_u32,
                        None, // hover_id (no mouse on Android usually)
                        &self.pressed_ids,
                        &self.textfield_states,
                        focused,
                    );

                    backend.frame(&frame.scene, GlyphRasterConfig { px: 18.0 * scale });
                    self.frame_cache = Some(frame);

                    self.dirty = false;

                    if self.options.continuous_redraw {
                        win.request_redraw();
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            if self.options.continuous_redraw || self.dirty || take_frame_request() {
                self.request_redraw();
            }
        }
    }

    let mut app_state = AppState::new(Box::new(root), options);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
