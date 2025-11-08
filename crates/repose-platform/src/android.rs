use crate::*;
use repose_core::locals::dp_to_px;
use repose_ui::layout_and_paint;
use repose_ui::textfield::{
    TF_FONT_DP, TF_PADDING_X_DP, byte_to_char_index, index_for_x_bytes, measure_text,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalPosition;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::EventLoop;
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;
use winit::window::{ImePurpose, Window, WindowAttributes};

pub fn run_android_app(
    app: AndroidApp,
    mut root: impl FnMut(&mut Scheduler) -> View + 'static,
) -> anyhow::Result<()> {
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = winit::event_loop::EventLoopBuilder::new()
        .with_android_app(app)
        .build()?;

    struct App {
        root: Box<dyn FnMut(&mut Scheduler) -> View>,
        window: Option<Arc<Window>>,
        backend: Option<repose_render_wgpu::WgpuBackend>,
        sched: Scheduler,
        inspector: repose_devtools::Inspector,
        frame_cache: Option<Frame>,

        // Input state
        last_pos_px: (f32, f32),
        modifiers: Modifiers,
        hover_id: Option<u64>,
        capture_id: Option<u64>,
        pressed_ids: HashSet<u64>,
        key_pressed_active: Option<u64>,
        ime_preedit: bool,

        // TextFields
        prev_touch_px: Option<(f32, f32)>,
        textfield_states: HashMap<u64, Rc<RefCell<repose_ui::TextFieldState>>>,
        last_focus: Option<u64>,
    }

    impl App {
        fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>) -> Self {
            Self {
                root,
                window: None,
                backend: None,
                sched: Scheduler::new(),
                inspector: repose_devtools::Inspector::new(),
                frame_cache: None,

                last_pos_px: (0.0, 0.0),
                modifiers: Modifiers::default(),
                hover_id: None,
                capture_id: None,
                pressed_ids: HashSet::new(),
                key_pressed_active: None,
                ime_preedit: false,

                prev_touch_px: None,
                textfield_states: HashMap::new(),
                last_focus: None,
            }
        }
        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        fn announce_focus_change(&mut self) {
            if let Some(f) = &self.frame_cache {
                let focused_node = self
                    .sched
                    .focused
                    .and_then(|id| f.semantics_nodes.iter().find(|n| n.id == id));
                self.inspector.hud.metrics = self.inspector.hud.metrics.take(); // no-op to silence mut borrow
                // bridge to a11y stub (same as desktop)
            }
        }
        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.frame_cache {
                if let Some(h) = f.hit_regions.iter().find(|h| h.id == id) {
                    if let Some(cb) = &h.on_text_change {
                        cb(text);
                    }
                }
            }
        }
        fn scale(&self) -> f32 {
            self.window
                .as_ref()
                .map(|w| w.scale_factor() as f32)
                .unwrap_or(1.0)
        }
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            if self.window.is_none() {
                match el.create_window(WindowAttributes::default().with_title("Repose Android")) {
                    Ok(win) => {
                        let w = Arc::new(win);
                        let sz = w.inner_size();
                        self.sched.size = (sz.width, sz.height);
                        match repose_render_wgpu::WgpuBackend::new(w.clone()) {
                            Ok(b) => {
                                self.backend = Some(b);
                                self.window = Some(w);
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
                    self.sched.size = (size.width, size.height);
                    if let Some(b) = &mut self.backend {
                        b.configure_surface(size.width, size.height);
                    }
                    self.request_redraw();
                }

                // Touch → pointer (down/move/up/cancel) with capture and click-on-release
                WindowEvent::Touch(t) => {
                    // Map device-pixel position to logical f32 (we treat them as px in our scene)
                    let pos_px = (t.location.x as f32, t.location.y as f32);
                    self.last_pos_px = pos_px;

                    // Helper to deliver pointer events to a region id
                    let make_pe = |mods: Modifiers| crate::input::PointerEvent {
                        id: crate::input::PointerId(0),
                        kind: crate::input::PointerKind::Touch,
                        event: crate::input::PointerEventKind::Move, // will be replaced per call
                        position: crate::Vec2 {
                            x: pos_px.0,
                            y: pos_px.1,
                        },
                        pressure: 1.0,
                        modifiers: mods,
                    };

                    match t.phase {
                        winit::event::TouchPhase::Started => {
                            let (hit_id, rect, on_pd, is_textfield) =
                                if let Some(f) = &self.frame_cache {
                                    if let Some(hit) = f.hit_regions.iter().rev().find(|h| {
                                        h.rect.contains(crate::Vec2 {
                                            x: pos_px.0,
                                            y: pos_px.1,
                                        })
                                    }) {
                                        let is_tf = f
                                            .semantics_nodes
                                            .iter()
                                            .any(|n| n.id == hit.id && n.role == Role::TextField);
                                        (
                                            Some(hit.id),
                                            Some(hit.rect),
                                            hit.on_pointer_down.clone(),
                                            is_tf,
                                        )
                                    } else {
                                        (None, None, None, false)
                                    }
                                } else {
                                    (None, None, None, false)
                                };

                            if let Some(id) = hit_id {
                                self.capture_id = Some(id);
                                self.pressed_ids.insert(id);
                                if let Some(r) = rect {
                                    // focus & IME only for focusables
                                    if is_textfield {
                                        self.sched.focused = Some(id);
                                        self.textfield_states.entry(id).or_insert_with(|| {
                                            Rc::new(RefCell::new(repose_ui::TextFieldState::new()))
                                        });
                                        if let Some(win) = &self.window {
                                            let sf = win.scale_factor();
                                            win.set_ime_allowed(true);
                                            win.set_ime_purpose(ImePurpose::Normal);
                                            win.set_ime_cursor_area(
                                                PhysicalPosition::new(
                                                    (r.x * sf as f32) as i32,
                                                    (r.y * sf as f32) as i32,
                                                ),
                                                PhysicalSize::new(
                                                    (r.w * sf as f32) as u32,
                                                    (r.h * sf as f32) as u32,
                                                ),
                                            );
                                        }
                                    }
                                    // Pointer down callback
                                    if let Some(cb) = on_pd {
                                        let mut pe = make_pe(self.modifiers);
                                        pe.event = crate::input::PointerEventKind::Down(
                                            crate::input::PointerButton::Primary,
                                        );
                                        cb(pe);
                                    }
                                    // TextField caret placement
                                    if is_textfield {
                                        if let Some(state_rc) = self.textfield_states.get(&id) {
                                            let mut state = state_rc.borrow_mut();
                                            let inner_x_px = r.x + dp_to_px(TF_PADDING_X_DP);
                                            let content_x_px = self.last_pos_px.0 - inner_x_px
                                                + state.scroll_offset;
                                            let font_dp = TF_FONT_DP as u32;
                                            let idx = index_for_x_bytes(
                                                &state.text,
                                                font_dp,
                                                content_x_px.max(0.0),
                                            );
                                            state.begin_drag(idx, self.modifiers.shift);
                                            let m = measure_text(&state.text, font_dp);
                                            let caret_x_px = m
                                                .positions
                                                .get(state.caret_index())
                                                .copied()
                                                .unwrap_or(0.0);
                                            state.ensure_caret_visible(
                                                caret_x_px,
                                                r.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                            );
                                        }
                                    }
                                }
                                self.prev_touch_px = Some(pos_px);
                                self.request_redraw();
                            }
                        }
                        winit::event::TouchPhase::Moved => {
                            // Move to capture (if any), else to hover target
                            let (cid, on_pm, rect) = if let (Some(f), Some(cid)) =
                                (&self.frame_cache, self.capture_id)
                            {
                                let cb = f
                                    .hit_regions
                                    .iter()
                                    .find(|h| h.id == cid)
                                    .and_then(|h| h.on_pointer_move.clone());
                                let r = f.hit_regions.iter().find(|h| h.id == cid).map(|h| h.rect);
                                (Some(cid), cb, r)
                            } else {
                                (None, None, None)
                            };

                            if let Some(id) = cid {
                                // deliver move
                                if let Some(cb) = on_pm {
                                    let mut pe = make_pe(self.modifiers);
                                    pe.event = crate::input::PointerEventKind::Move;
                                    cb(pe);
                                }
                                // drag to scroll using dy
                                if let (Some(prev), Some(r)) = (self.prev_touch_px, rect) {
                                    let dy_px = pos_px.1 - prev.1;
                                    if dy_px.abs() > 0.0 {
                                        if let Some(f) = &self.frame_cache {
                                            if let Some(h) =
                                                f.hit_regions.iter().find(|h| h.id == id)
                                            {
                                                if let Some(cb) = &h.on_scroll {
                                                    let _ = cb(crate::Vec2 { x: 0.0, y: -dy_px }); // invert for natural scroll
                                                }
                                            }
                                        }
                                    }
                                }
                                self.prev_touch_px = Some(pos_px);
                                self.request_redraw();
                            }
                        }
                        winit::event::TouchPhase::Ended => {
                            // Release capture, click-on-release if still inside
                            let (cid, on_pu, on_click, rect, is_textfield) = if let (
                                Some(f),
                                Some(cid),
                            ) =
                                (&self.frame_cache, self.capture_id)
                            {
                                let pu = f
                                    .hit_regions
                                    .iter()
                                    .find(|h| h.id == cid)
                                    .and_then(|h| h.on_pointer_up.clone());
                                let clk = f
                                    .hit_regions
                                    .iter()
                                    .find(|h| h.id == cid)
                                    .and_then(|h| h.on_click.clone());
                                let r = f.hit_regions.iter().find(|h| h.id == cid).map(|h| h.rect);
                                let tf = f
                                    .semantics_nodes
                                    .iter()
                                    .any(|n| n.id == cid && n.role == Role::TextField);
                                (Some(cid), pu, clk, r, tf)
                            } else {
                                (None, None, None, None, false)
                            };

                            if let Some(id) = cid {
                                self.pressed_ids.remove(&id);
                                if let Some(cb) = on_pu {
                                    let mut pe = make_pe(self.modifiers);
                                    pe.event = crate::input::PointerEventKind::Up(
                                        crate::input::PointerButton::Primary,
                                    );
                                    cb(pe);
                                }
                                if let Some(r) = rect {
                                    if r.contains(crate::Vec2 {
                                        x: self.last_pos_px.0,
                                        y: self.last_pos_px.1,
                                    }) {
                                        if let Some(cb) = on_click {
                                            cb();
                                        }
                                    }
                                }
                                if is_textfield {
                                    if let Some(state_rc) = self.textfield_states.get(&id) {
                                        state_rc.borrow_mut().end_drag();
                                    }
                                }
                            }
                            self.capture_id = None;
                            self.prev_touch_px = None;
                            self.request_redraw();
                        }
                        winit::event::TouchPhase::Cancelled => {
                            // Cancel capture
                            if let Some(cid) = self.capture_id {
                                self.pressed_ids.remove(&cid);
                            }
                            self.capture_id = None;
                            self.prev_touch_px = None;
                            self.request_redraw();
                        }
                    }
                }

                // Keyboard (hardware keyboards)
                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    // Focus traversal with Tab
                    if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                        if key_event.state == ElementState::Pressed && !key_event.repeat {
                            if let Some(f) = &self.frame_cache {
                                let chain = &f.focus_chain;
                                if !chain.is_empty() {
                                    if let Some(active) = self.key_pressed_active.take() {
                                        self.pressed_ids.remove(&active);
                                    }
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
                                        if f.semantics_nodes
                                            .iter()
                                            .any(|n| n.id == next && n.role == Role::TextField)
                                        {
                                            win.set_ime_allowed(true);
                                            win.set_ime_purpose(ImePurpose::Normal);
                                        } else {
                                            win.set_ime_allowed(false);
                                        }
                                    }
                                    self.announce_focus_change();
                                    self.request_redraw();
                                }
                            }
                        }
                        return;
                    }

                    // Keyboard activation: Space/Enter on focus (not TextField)
                    if let Some(fid) = self.sched.focused {
                        let is_textfield = if let Some(f) = &self.frame_cache {
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
                                        self.pressed_ids.insert(fid);
                                        self.key_pressed_active = Some(fid);
                                        self.request_redraw();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Enter submits TextField
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        if let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key {
                            if let Some(focused_id) = self.sched.focused {
                                if let Some(f) = &self.frame_cache {
                                    if let Some(hit) =
                                        f.hit_regions.iter().find(|h| h.id == focused_id)
                                    {
                                        if let Some(on_submit) = &hit.on_text_submit {
                                            if let Some(state) =
                                                self.textfield_states.get(&focused_id)
                                            {
                                                let text = state.borrow().text.clone();
                                                on_submit(text);
                                                self.request_redraw();
                                                return;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // IME (Preedit/Commit)
                WindowEvent::Ime(ime) => {
                    if let Some(focused_id) = self.sched.focused {
                        if let Some(state_rc) = self.textfield_states.get(&focused_id) {
                            let mut state = state_rc.borrow_mut();
                            match ime {
                                winit::event::Ime::Enabled => {
                                    self.ime_preedit = false;
                                }
                                winit::event::Ime::Preedit(text, cursor) => {
                                    let cursor_usize =
                                        cursor.map(|(a, b)| (a as usize, b as usize));
                                    state.set_composition(text.clone(), cursor_usize);
                                    self.ime_preedit = !text.is_empty();

                                    // Ensure caret visible
                                    let font_dp = TF_FONT_DP as u32;
                                    let m = measure_text(&state.text, font_dp);
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    // Need viewport width: find rect
                                    if let Some(f) = &self.frame_cache {
                                        if let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == focused_id)
                                        {
                                            state.ensure_caret_visible(
                                                caret_x_px,
                                                hit.rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                            );
                                        }
                                    }

                                    // Notify change
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.request_redraw();
                                }
                                winit::event::Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.ime_preedit = false;

                                    let font_dp = TF_FONT_DP as u32;
                                    let m = measure_text(&state.text, font_dp);
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    if let Some(f) = &self.frame_cache {
                                        if let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == focused_id)
                                        {
                                            state.ensure_caret_visible(
                                                caret_x_px,
                                                hit.rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                            );
                                        }
                                    }
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.request_redraw();
                                }
                                winit::event::Ime::Disabled => {
                                    self.ime_preedit = false;
                                    if state.composition.is_some() {
                                        state.cancel_composition();

                                        let font_dp = TF_FONT_DP as u32;
                                        let m = measure_text(&state.text, font_dp);
                                        let caret_x_px = m
                                            .positions
                                            .get(state.caret_index())
                                            .copied()
                                            .unwrap_or(0.0);
                                        if let Some(f) = &self.frame_cache {
                                            if let Some(hit) =
                                                f.hit_regions.iter().find(|h| h.id == focused_id)
                                            {
                                                state.ensure_caret_visible(
                                                    caret_x_px,
                                                    hit.rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                                );
                                            }
                                        }

                                        self.notify_text_change(focused_id, state.text.clone());
                                    }
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    if let (Some(backend), Some(win)) =
                        (self.backend.as_mut(), self.window.as_ref())
                    {
                        let scale = win.scale_factor() as f32;
                        let t0 = std::time::Instant::now();

                        let focused = self.sched.focused;
                        let hover_id = self.hover_id;
                        let pressed_ids = self.pressed_ids.clone();
                        let tf_states = &self.textfield_states;

                        let root_fn = &mut self.root;

                        let frame = self.sched.repose(
                            {
                                let scale = scale;
                                move |sched: &mut Scheduler| {
                                    with_density(Density { scale }, || {
                                        with_text_scale(TextScale(1.0), || (root_fn)(sched))
                                    })
                                }
                            },
                            {
                                let hover_id = hover_id;
                                let pressed_ids = pressed_ids.clone();
                                move |view, size| {
                                    let interactions = repose_ui::Interactions {
                                        hover: hover_id,
                                        pressed: pressed_ids.clone(),
                                    };
                                    with_density(Density { scale }, || {
                                        with_text_scale(TextScale(1.0), || {
                                            layout_and_paint(
                                                view,
                                                size,
                                                tf_states,
                                                &interactions,
                                                focused,
                                            )
                                        })
                                    })
                                }
                            },
                        );

                        // self.a11y.publish_tree(&frame.semantics_nodes); // you can add an Android stub similar to desktop later

                        let mut scene = frame.scene.clone();
                        self.inspector.hud.metrics = Some(repose_devtools::Metrics {
                            build_layout_ms: (std::time::Instant::now() - t0).as_secs_f32()
                                * 1000.0,
                            scene_nodes: scene.nodes.len(),
                        });
                        self.inspector.frame(&mut scene);
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                        self.frame_cache = Some(frame);
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            self.request_redraw();
        }
    }

    let mut app_state = App::new(Box::new(root));
    event_loop.run_app(&mut app_state)?;
    Ok(())
}
