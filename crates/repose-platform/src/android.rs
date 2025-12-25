use crate::common as rc;
use crate::*;

use repose_ui::TextFieldState;
use repose_ui::textfield::{TF_FONT_DP, TF_PADDING_X_DP, index_for_x_bytes, measure_text};

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{ImePurpose, Window, WindowAttributes};

#[derive(Clone, Copy, Debug)]
pub struct AndroidOptions {
    /// If true, runner keeps requesting frames (good for animations, costs battery).
    pub continuous_redraw: bool,

    /// If true, runner wraps the app root in a ScrollV container.
    /// Useful for "webpage-like" apps; off by default to avoid nested scroll surprises.
    pub auto_root_scroll: bool,
}

impl Default for AndroidOptions {
    fn default() -> Self {
        Self {
            // Keep behavior close to your original runner: always ticking.
            continuous_redraw: true,
            auto_root_scroll: false,
        }
    }
}

pub fn run_android_app(
    app: AndroidApp,
    root: impl FnMut(&mut Scheduler) -> View + 'static,
) -> anyhow::Result<()> {
    run_android_app_with_options(app, root, AndroidOptions::default())
}

pub fn run_android_app_with_options(
    app: AndroidApp,
    root: impl FnMut(&mut Scheduler) -> View + 'static,
    options: AndroidOptions,
) -> anyhow::Result<()> {
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = winit::event_loop::EventLoopBuilder::new()
        .with_android_app(app)
        .build()?;

    struct AppState {
        root: Box<dyn FnMut(&mut Scheduler) -> View>,
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

        // redraw control
        dirty: bool,
    }

    impl AppState {
        fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>, options: AndroidOptions) -> Self {
            Self {
                root,
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
                dirty: true,
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
            self.dp_px(6.0)
        }

        fn tf_key_of(&self, visual_id: u64) -> u64 {
            if let Some(f) = &self.frame_cache
                && let Some(i) = rc::hit_index_by_id(f, visual_id)
            {
                let hr = &f.hit_regions[i];
                return hr.tf_state_key.unwrap_or(hr.id);
            }
            visual_id
        }

        fn is_textfield(&self, id: u64) -> bool {
            self.frame_cache
                .as_ref()
                .map(|f| {
                    f.semantics_nodes
                        .iter()
                        .any(|n| n.id == id && n.role == Role::TextField)
                })
                .unwrap_or(false)
        }

        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.frame_cache
                && let Some(i) = rc::hit_index_by_id(f, id)
                && let Some(cb) = &f.hit_regions[i].on_text_change
            {
                cb(text);
            }
        }

        fn ensure_caret_visible_in_hit(&self, st: &mut TextFieldState, hit_rect: Rect) {
            let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
            let m = measure_text(&st.text, font_px);
            let caret_x_px = m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
            st.ensure_caret_visible(
                caret_x_px,
                hit_rect.w - 2.0 * self.padding_px(),
                dp_to_px(2.0),
            );
        }

        fn sync_window_size(&mut self, size: PhysicalSize<u32>) {
            self.sched.size = (size.width, size.height);
            if let Some(b) = &mut self.backend {
                b.configure_surface(size.width, size.height);
            }
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

                // Touch handling (Android primary)
                WindowEvent::Touch(t) => {
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    self.last_pos_px = pos_px;
                    let pos = Vec2 {
                        x: pos_px.0,
                        y: pos_px.1,
                    };

                    match t.phase {
                        winit::event::TouchPhase::Started => {
                            self.touch_scrolled = false;
                            self.touch_scroll_accum_y_px = 0.0;

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
                                    }

                                    // pointer down callback
                                    if let Some(cb) = &hit.on_pointer_down {
                                        cb(rc::pe_down_primary(
                                            repose_core::input::PointerKind::Touch,
                                            pos,
                                            self.modifiers,
                                        ));
                                    }
                                }
                            }

                            self.prev_touch_px = Some(pos_px);
                            self.dirty = true;
                            self.request_redraw();
                        }

                        winit::event::TouchPhase::Moved => {
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

                // Basic keyboard support (hardware keyboards / Tab focus)
                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
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

                    // Tab traversal
                    if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                        if key_event.state == ElementState::Pressed && !key_event.repeat {
                            if let Some(f) = &self.frame_cache {
                                let chain = &f.focus_chain;
                                if !chain.is_empty() {
                                    let shift = self.modifiers.shift;
                                    let current = self.sched.focused;

                                    let next = if let Some(cur) = current {
                                        if let Some(idx) = chain.iter().position(|&id| id == cur) {
                                            if shift {
                                                if idx == 0 {
                                                    chain[chain.len() - 1]
                                                } else {
                                                    chain[idx - 1]
                                                }
                                            } else {
                                                chain[(idx + 1) % chain.len()]
                                            }
                                        } else {
                                            chain[0]
                                        }
                                    } else {
                                        chain[0]
                                    };

                                    self.sched.focused = Some(next);
                                    if let Some(win) = &self.window {
                                        if self.is_textfield(next) {
                                            win.set_ime_allowed(true);
                                            win.set_ime_purpose(ImePurpose::Normal);
                                        } else {
                                            win.set_ime_allowed(false);
                                        }
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
                                    self.dirty = true;
                                    self.request_redraw();
                                    return;
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
                            match ime {
                                winit::event::Ime::Enabled => self.ime_preedit = false,

                                winit::event::Ime::Preedit(text, cursor) => {
                                    let cursor_usize =
                                        cursor.map(|(a, b)| (a as usize, b as usize));
                                    state.set_composition(text.clone(), cursor_usize);
                                    self.ime_preedit = !text.is_empty();

                                    if let Some(f) = &self.frame_cache
                                        && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                    {
                                        self.ensure_caret_visible_in_hit(
                                            &mut state,
                                            f.hit_regions[i].rect,
                                        );
                                    }

                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.dirty = true;
                                    self.request_redraw();
                                }

                                winit::event::Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.ime_preedit = false;

                                    if let Some(f) = &self.frame_cache
                                        && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                    {
                                        self.ensure_caret_visible_in_hit(
                                            &mut state,
                                            f.hit_regions[i].rect,
                                        );
                                    }

                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.dirty = true;
                                    self.request_redraw();
                                }

                                winit::event::Ime::Disabled => {
                                    self.ime_preedit = false;
                                    if state.composition.is_some() {
                                        state.cancel_composition();

                                        if let Some(f) = &self.frame_cache
                                            && let Some(i) = rc::hit_index_by_id(f, focused_id)
                                        {
                                            self.ensure_caret_visible_in_hit(
                                                &mut state,
                                                f.hit_regions[i].rect,
                                            );
                                        }

                                        self.notify_text_change(focused_id, state.text.clone());
                                        self.dirty = true;
                                        self.request_redraw();
                                    }
                                }
                            }
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    let (Some(backend), Some(win)) = (self.backend.as_mut(), self.window.as_ref())
                    else {
                        return;
                    };

                    let scale = win.scale_factor() as f32;
                    let size_px_u32 = self.sched.size;
                    let focused = self.sched.focused;

                    let auto_root_scroll = self.options.auto_root_scroll;
                    let root_scroll = self.root_scroll.clone();
                    let root_fn = &mut self.root;

                    let mut composed_root = move |s: &mut Scheduler| {
                        let v = (root_fn)(s);
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
            // Only redraw if needed (unless continuous_redraw is enabled).
            if self.options.continuous_redraw || self.dirty {
                self.request_redraw();
            }
        }
    }

    let mut app_state = AppState::new(Box::new(root), options);
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
