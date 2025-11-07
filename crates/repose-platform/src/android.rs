use crate::*;
use repose_ui::layout_and_paint;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use winit::application::ApplicationHandler;
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
    // Same clock as desktop
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
        mouse_pos: (f32, f32),
        modifiers: Modifiers,
        textfield_states: HashMap<u64, Rc<RefCell<repose_ui::TextFieldState>>>,
        hover_id: Option<u64>,
        capture_id: Option<u64>,
        pressed_ids: HashSet<u64>,
        key_pressed_active: Option<u64>,
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
                mouse_pos: (0.0, 0.0),
                modifiers: Modifiers::default(),
                textfield_states: HashMap::new(),
                hover_id: None,
                capture_id: None,
                pressed_ids: HashSet::new(),
                key_pressed_active: None,
                last_focus: None,
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

                        let frame = self.sched.repose(&mut self.root, move |view, size| {
                            let interactions = repose_ui::Interactions {
                                hover: hover_id,
                                pressed: pressed_ids.clone(),
                            };
                            // Density + TextScale from device scale
                            with_density(Density { scale }, || {
                                with_text_scale(TextScale(1.0), || {
                                    layout_and_paint(view, size, tf_states, &interactions, focused)
                                })
                            })
                        });

                        let build_layout_ms =
                            (std::time::Instant::now() - t0).as_secs_f32() * 1000.0;
                        let mut scene = frame.scene.clone();
                        self.inspector.hud.metrics = Some(repose_devtools::Metrics {
                            build_layout_ms,
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
