use compose_core::*;
use compose_ui::layout_and_paint;

#[cfg(feature = "desktop")]
pub fn run_desktop_app(root: impl FnMut(&mut Scheduler) -> View + 'static) -> anyhow::Result<()> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    use std::sync::Arc;

    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{ImePurpose, Window, WindowAttributes};

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
        textfield_states: HashMap<u64, Rc<RefCell<compose_ui::textfield::TextFieldState>>>,
        ime_active: bool,
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
                ime_active: false,
            }
        }

        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
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

                    // Keep IME candidate box aligned with focused TextField
                    if self.ime_active {
                        if let (Some(win), Some(focused_id), Some(frame)) = (
                            self.window.as_ref(),
                            self.sched.focused,
                            self.frame_cache.as_ref(),
                        ) {
                            if let Some(h) = frame.hit_regions.iter().find(|h| h.id == focused_id) {
                                let sf = win.scale_factor();
                                win.set_ime_cursor_area(
                                    LogicalPosition::new(
                                        h.rect.x as f64 / sf,
                                        h.rect.y as f64 / sf,
                                    ),
                                    LogicalSize::new(h.rect.w as f64 / sf, h.rect.h as f64 / sf),
                                );
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
                        let mut clicked_focusable = false;

                        for hit in &f.hit_regions {
                            if hit.rect.contains(pos) {
                                if let Some(cb) = &hit.on_click {
                                    cb();
                                }

                                if hit.focusable {
                                    clicked_focusable = true;
                                    self.sched.focused = Some(hit.id);

                                    // Ensure state
                                    self.textfield_states.entry(hit.id).or_insert_with(|| {
                                        Rc::new(RefCell::new(
                                            compose_ui::textfield::TextFieldState::new(),
                                        ))
                                    });

                                    // Enable IME and set candidate box
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
                                        self.ime_active = true;
                                    }
                                }

                                self.request_redraw();
                                break;
                            }
                        }

                        // Click outside any focusable -> disable IME and clear focus.
                        if !clicked_focusable {
                            if self.ime_active {
                                if let Some(win) = &self.window {
                                    win.set_ime_allowed(false);
                                }
                                self.ime_active = false;
                            }
                            self.sched.focused = None;
                            self.request_redraw();
                        }
                    }
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
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Delete) => {
                                        state.delete_forward();
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                        state.move_cursor(-1, self.modifiers.shift);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                                        state.move_cursor(1, self.modifiers.shift);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Home) => {
                                        state.selection = 0..0;
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::End) => {
                                        let end = state.text.len();
                                        state.selection = end..end;
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::KeyA) if self.modifiers.ctrl => {
                                        state.selection = 0..state.text.len();
                                        self.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Plain text input when IME is not active
                        if !self.ime_active {
                            if let Some(text) = key_event.text.as_deref() {
                                if !text.is_empty() {
                                    if let Some(focused_id) = self.sched.focused {
                                        if let Some(state) = self.textfield_states.get(&focused_id)
                                        {
                                            state.borrow_mut().insert_text(text);
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
                                    self.ime_active = true;
                                }
                                Ime::Preedit(text, cursor) => {
                                    state.set_composition(text, cursor);
                                    self.request_redraw();
                                }
                                Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.request_redraw();
                                }
                                Ime::Disabled => {
                                    self.ime_active = false;
                                    if state.composition.is_some() {
                                        state.cancel_composition();
                                        self.request_redraw();
                                    }
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
