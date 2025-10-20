use compose_core::*;
use compose_ui::layout_and_paint;
use compose_ui::textfield::{
    byte_to_char_index, index_for_x_bytes, measure_text, TextFieldState, TF_FONT_PX, TF_PADDING_X,
};

#[cfg(feature = "desktop")]
pub fn run_desktop_app(root: impl FnMut(&mut Scheduler) -> View + 'static) -> anyhow::Result<()> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Arc;

    use compose_ui::TextFieldState;
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{ImePurpose, Window, WindowAttributes};

    enum ImeState {
        Disabled,
        Enabled,
        Preedit,
    }

    struct App {
        // App state
        root: Box<dyn FnMut(&mut Scheduler) -> View>,
        window: Option<Arc<Window>>,
        backend: Option<compose_render_wgpu::WgpuBackend>,
        sched: Scheduler,
        inspector: compose_devtools::Inspector,
        frame_cache: Option<Frame>,
        mouse_pos: (f32, f32),
        modifiers: Modifiers,
        textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
        ime_preedit: bool,
        hover_id: Option<u64>,
        capture_id: Option<u64>,
    }

    impl App {
        fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>) -> Self {
            Self {
                root,
                window: None,
                backend: None,
                sched: Scheduler::new(),
                inspector: compose_devtools::Inspector::new(),
                frame_cache: None,
                mouse_pos: (0.0, 0.0),
                modifiers: Modifiers::default(),
                textfield_states: HashMap::new(),
                ime_preedit: false,
                hover_id: None,
                capture_id: None,
            }
        }

        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }
        fn tf_ensure_caret_visible(st: &mut TextFieldState) {
            let px = TF_FONT_PX as u32;
            let m = measure_text(&st.text, px);
            let i0 = byte_to_char_index(&m, st.selection.start);
            let i1 = byte_to_char_index(&m, st.selection.end);
            let caret_x = m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
            st.ensure_caret_visible(caret_x, st.inner_width);
        }
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            // Create the window once when app resumes.
            if self.window.is_none() {
                match el.create_window(
                    WindowAttributes::default()
                        .with_title("Repose v0.2")
                        .with_inner_size(PhysicalSize::new(1280, 800)),
                ) {
                    Ok(win) => {
                        let w = Arc::new(win);
                        let size = w.inner_size();
                        self.sched.size = (size.width, size.height);
                        // Create WGPU backend
                        match compose_render_wgpu::WgpuBackend::new(w.clone()) {
                            Ok(b) => {
                                self.backend = Some(b);
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
            match event {
                WindowEvent::CloseRequested => {
                    log::info!("Window close requested");
                    el.exit();
                }
                WindowEvent::Resized(size) => {
                    self.sched.size = (size.width, size.height);
                    if let Some(b) = &mut self.backend {
                        b.configure_surface(size.width, size.height);
                    }
                    self.request_redraw();
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_pos = (position.x as f32, position.y as f32);

                    // Inspector hover
                    if self.inspector.hud.inspector_enabled {
                        if let Some(f) = &self.frame_cache {
                            let hover_rect = f
                                .hit_regions
                                .iter()
                                .find(|h| {
                                    h.rect.contains(Vec2 {
                                        x: self.mouse_pos.0,
                                        y: self.mouse_pos.1,
                                    })
                                })
                                .map(|h| h.rect);
                            self.inspector.hud.set_hovered(hover_rect);
                            self.request_redraw();
                        }
                    }

                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                        if let Some(_sem) = f
                            .semantics_nodes
                            .iter()
                            .find(|n| n.id == cid && n.role == Role::TextField)
                        {
                            if let Some(state_rc) = self.textfield_states.get(&cid) {
                                let mut state = state_rc.borrow_mut();
                                let inner_x = f
                                    .hit_regions
                                    .iter()
                                    .find(|h| h.id == cid)
                                    .map(|h| h.rect.x + TF_PADDING_X)
                                    .unwrap_or(0.0);
                                let content_x = self.mouse_pos.0 - inner_x + state.scroll_offset;
                                let px = TF_FONT_PX as u32;
                                let idx = index_for_x_bytes(&state.text, px, content_x.max(0.0));
                                state.drag_to(idx);

                                // Scroll caret into view
                                let px = TF_FONT_PX as u32;
                                let m = measure_text(&state.text, px);
                                let i0 = byte_to_char_index(&m, state.selection.start);
                                let i1 = byte_to_char_index(&m, state.selection.end);
                                let caret_x =
                                    m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
                                // We also need inner width; get rect
                                if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                                    state.ensure_caret_visible(
                                        caret_x,
                                        hit.rect.w - 2.0 * TF_PADDING_X,
                                    );
                                }
                                self.request_redraw();
                            }
                        }
                    }

                    // Pointer routing: hover + move/capture
                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos.0,
                            y: self.mouse_pos.1,
                        };
                        // Topmost hit (hits are z-sorted; take last that contains)
                        let top = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos));

                        // Hover target change
                        let new_hover = top.map(|h| h.id);
                        if new_hover != self.hover_id {
                            self.hover_id = new_hover;
                        }

                        // Build PointerEvent
                        let pe = compose_core::input::PointerEvent {
                            id: compose_core::input::PointerId(0),
                            kind: compose_core::input::PointerKind::Mouse,
                            event: compose_core::input::PointerEventKind::Move,
                            position: pos,
                            pressure: 1.0,
                            modifiers: self.modifiers,
                        };

                        // Deliver Move: captured first, else hover target
                        if let Some(cid) = self.capture_id {
                            if let Some(h) = f.hit_regions.iter().find(|h| h.id == cid) {
                                if let Some(cb) = &h.on_pointer_move {
                                    cb(pe.clone());
                                }
                            }
                        } else if let Some(h) = top {
                            if let Some(cb) = &h.on_pointer_move {
                                cb(pe);
                            }
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos.0,
                            y: self.mouse_pos.1,
                        };
                        if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                        {
                            // Capture starts on press
                            self.capture_id = Some(hit.id);

                            // Focus & IME first for focusables (so state exists)
                            if hit.focusable {
                                self.sched.focused = Some(hit.id);
                                self.textfield_states.entry(hit.id).or_insert_with(|| {
                                    Rc::new(RefCell::new(
                                        compose_ui::textfield::TextFieldState::new(),
                                    ))
                                });
                                if let Some(win) = &self.window {
                                    let sf = win.scale_factor();
                                    win.set_ime_allowed(true);
                                    win.set_ime_purpose(ImePurpose::Normal);
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

                            // PointerDown callback (legacy)
                            if let Some(cb) = &hit.on_pointer_down {
                                let pe = compose_core::input::PointerEvent {
                                    id: compose_core::input::PointerId(0),
                                    kind: compose_core::input::PointerKind::Mouse,
                                    event: compose_core::input::PointerEventKind::Down(
                                        compose_core::input::PointerButton::Primary,
                                    ),
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                };
                                cb(pe);
                            }

                            // Legacy click
                            if let Some(cb) = &hit.on_click {
                                cb();
                            }

                            // TextField: place caret and start drag selection
                            if let Some(_sem) = f
                                .semantics_nodes
                                .iter()
                                .find(|n| n.id == hit.id && n.role == Role::TextField)
                            {
                                if let Some(state_rc) = self.textfield_states.get(&hit.id) {
                                    let mut state = state_rc.borrow_mut();
                                    let inner_x = hit.rect.x + TF_PADDING_X;
                                    let content_x =
                                        self.mouse_pos.0 - inner_x + state.scroll_offset;
                                    let px = TF_FONT_PX as u32;
                                    let idx =
                                        index_for_x_bytes(&state.text, px, content_x.max(0.0));
                                    state.begin_drag(idx, self.modifiers.shift);

                                    // Scroll caret into view
                                    let px = TF_FONT_PX as u32;
                                    let m = measure_text(&state.text, px);
                                    let i0 = byte_to_char_index(&m, state.selection.start);
                                    let i1 = byte_to_char_index(&m, state.selection.end);
                                    let caret_x = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x,
                                        hit.rect.w - 2.0 * TF_PADDING_X,
                                    );
                                }
                            }

                            self.request_redraw();
                        } else {
                            // Click outside: drop focus/IME
                            if self.ime_preedit {
                                if let Some(win) = &self.window {
                                    win.set_ime_allowed(false);
                                }
                                self.ime_preedit = false;
                            }
                            self.sched.focused = None;
                            self.request_redraw();
                        }
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                        if let Some(_sem) = f
                            .semantics_nodes
                            .iter()
                            .find(|n| n.id == cid && n.role == Role::TextField)
                        {
                            if let Some(state_rc) = self.textfield_states.get(&cid) {
                                state_rc.borrow_mut().end_drag();
                            }
                        }
                    }
                    self.capture_id = None;
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let dy = match delta {
                        MouseScrollDelta::LineDelta(_x, y) => -y * 40.0,
                        MouseScrollDelta::PixelDelta(lp) => -(lp.y as f32),
                    };

                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos.0,
                            y: self.mouse_pos.1,
                        };
                        if let Some(hit) = f
                            .hit_regions
                            .iter()
                            .rev()
                            .find(|h| h.rect.contains(pos) && h.on_scroll.is_some())
                        {
                            if let Some(cb) = &hit.on_scroll {
                                cb(dy);
                                self.request_redraw();
                            }
                        }
                    }
                }
                WindowEvent::ModifiersChanged(new_mods) => {
                    self.modifiers.shift = new_mods.state().shift_key();
                    self.modifiers.ctrl = new_mods.state().control_key();
                    self.modifiers.alt = new_mods.state().alt_key();
                    self.modifiers.meta = new_mods.state().super_key();
                }
                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    if key_event.state == ElementState::Pressed {
                        // Inspector hotkey: Ctrl+Shift+I
                        if self.modifiers.ctrl && self.modifiers.shift {
                            if let PhysicalKey::Code(KeyCode::KeyI) = key_event.physical_key {
                                self.inspector.hud.toggle_inspector();
                                self.request_redraw();
                                return;
                            }
                        }

                        // TextField navigation/edit
                        if let Some(focused_id) = self.sched.focused {
                            if let Some(state) = self.textfield_states.get(&focused_id) {
                                let mut state = state.borrow_mut();
                                match key_event.physical_key {
                                    PhysicalKey::Code(KeyCode::Backspace) => {
                                        state.delete_backward();
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Delete) => {
                                        state.delete_forward();
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                        state.move_cursor(-1, self.modifiers.shift);
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                                        state.move_cursor(1, self.modifiers.shift);
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Home) => {
                                        state.selection = 0..0;
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::End) => {
                                        {
                                            let end = state.text.len();
                                            state.selection = end..end;
                                        }
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::KeyA) if self.modifiers.ctrl => {
                                        state.selection = 0..state.text.len();
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Plain text input when IME is not active
                        if !self.ime_preedit
                            && !self.modifiers.ctrl
                            && !self.modifiers.alt
                            && !self.modifiers.meta
                        {
                            if let Some(raw) = key_event.text.as_deref() {
                                // Drop control chars (e.g., backspace/delete), keep printable only
                                let text: String =
                                    raw.chars().filter(|c| !c.is_control()).collect();

                                if !text.is_empty() {
                                    if let Some(focused_id) = self.sched.focused {
                                        if let Some(state_rc) =
                                            self.textfield_states.get(&focused_id)
                                        {
                                            let mut st = state_rc.borrow_mut();
                                            st.insert_text(&text);
                                            App::tf_ensure_caret_visible(&mut st);
                                            self.request_redraw();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                WindowEvent::Ime(ime) => {
                    use winit::event::Ime;

                    if let Some(focused_id) = self.sched.focused {
                        if let Some(state) = self.textfield_states.get(&focused_id) {
                            let mut state = state.borrow_mut();
                            match ime {
                                Ime::Enabled => {
                                    // IME allowed, but not necessarily composing
                                    self.ime_preedit = false;
                                }
                                Ime::Preedit(text, cursor) => {
                                    {
                                        state.set_composition(text.clone(), cursor);
                                    }
                                    self.ime_preedit = !text.is_empty();
                                    App::tf_ensure_caret_visible(&mut state);
                                    self.request_redraw();
                                }
                                Ime::Commit(text) => {
                                    {
                                        state.commit_composition(text);
                                    }
                                    self.ime_preedit = false;
                                    App::tf_ensure_caret_visible(&mut state);
                                    self.request_redraw();
                                }
                                Ime::Disabled => {
                                    self.ime_preedit = false;
                                    if state.composition.is_some() {
                                        {
                                            state.cancel_composition();
                                        }
                                        App::tf_ensure_caret_visible(&mut state);
                                    }
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let (Some(backend), Some(_win)) =
                        (self.backend.as_mut(), self.window.as_ref())
                    {
                        // Compose
                        let frame = self.sched.compose(&mut self.root, |view, size| {
                            layout_and_paint(view, size, &self.textfield_states)
                        });

                        // Render
                        let mut scene = frame.scene.clone();
                        self.inspector.frame(&mut scene);
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 });
                        self.frame_cache = Some(frame);
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            self.request_redraw();
        }

        fn new_events(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::event::StartCause,
        ) {
        }
        fn user_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: ()) {}
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

    let event_loop = EventLoop::new()?;
    let mut app = App::new(Box::new(root));
    event_loop.run_app(&mut app)?;
    Ok(())
}
