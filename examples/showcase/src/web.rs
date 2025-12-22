use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen_futures::spawn_local;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::EventLoop;
use winit::platform::web::{EventLoopExtWebSys, WindowAttributesExtWebSys};
use winit::window::{Window, WindowAttributes};

use repose_core::RenderBackend;
use repose_core::{GlyphRasterConfig, Scheduler, View};
use repose_platform::compose_frame;

pub fn start() {
    // Better panic + logs in browser console
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
    let _ = console_log::init_with_level(log::Level::Info);

    log::info!("showcase wasm_start() running");

    // Clock (optional but good for anim parity)
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));

    let event_loop = EventLoop::new().expect("EventLoop::new");

    struct App {
        window: Option<Arc<Window>>,
        backend: Rc<RefCell<Option<repose_render_wgpu::WgpuBackend>>>,

        root: Box<dyn FnMut(&mut Scheduler) -> View>,
        sched: Scheduler,

        frame_cache: Option<repose_core::runtime::Frame>,

        // minimal interaction state (kept empty for now; enough to draw UI)
        hover_id: Option<u64>,
        pressed_ids: HashSet<u64>,
        tf_states: HashMap<u64, Rc<RefCell<repose_ui::TextFieldState>>>,
    }

    impl App {
        fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>) -> Self {
            Self {
                window: None,
                backend: Rc::new(RefCell::new(None)),
                root,
                sched: Scheduler::new(),
                frame_cache: None,
                hover_id: None,
                pressed_ids: HashSet::new(),
                tf_states: HashMap::new(),
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
            if self.window.is_some() {
                return;
            }

            // On web: create + append the canvas automatically.
            let attrs: WindowAttributes = Window::default_attributes()
                .with_title("Repose Showcase (Web)")
                .with_append(true)
                .with_prevent_default(true);

            let window = Arc::new(el.create_window(attrs).expect("create_window"));
            let size = window.inner_size();
            self.sched.size = (size.width, size.height);
            self.window = Some(window.clone());

            // Async GPU init (required on web)
            let backend_cell = self.backend.clone();
            let window_for_async = window.clone();

            spawn_local(async move {
                match repose_render_wgpu::WgpuBackend::new_async(window_for_async.clone()).await {
                    Ok(mut b) => {
                        // Initial configure (safe even if already configured)
                        let s = window_for_async.inner_size();
                        b.configure_surface(s.width, s.height);

                        log::info!("WGPU backend initialized");
                        *backend_cell.borrow_mut() = Some(b);
                        window_for_async.request_redraw();
                    }
                    Err(e) => {
                        log::error!("Failed to init WGPU backend: {e:?}");
                    }
                }
            });

            self.request_redraw();
        }

        fn window_event(
            &mut self,
            _el: &winit::event_loop::ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            match event {
                WindowEvent::Resized(size) => {
                    self.sched.size = (size.width, size.height);

                    // Borrow backend for just this scope
                    let mut backend_ref = self.backend.borrow_mut();
                    if let Some(b) = backend_ref.as_mut() {
                        b.configure_surface(size.width, size.height);
                    }

                    self.request_redraw();
                }

                WindowEvent::RedrawRequested => {
                    let Some(window) = &self.window else {
                        return;
                    };

                    // If GPU not ready yet, just keep requesting frames
                    {
                        let backend_ready = self.backend.borrow().is_some();
                        if !backend_ready {
                            window.request_redraw();
                            return;
                        }
                    }

                    let scale = window.scale_factor() as f32;
                    let size_px_u32 = self.sched.size;
                    let focused = self.sched.focused;

                    let frame = compose_frame(
                        &mut self.sched,
                        &mut self.root,
                        scale,
                        size_px_u32,
                        self.hover_id,
                        &self.pressed_ids,
                        &self.tf_states,
                        focused,
                    );

                    // Now borrow backend mutably and render
                    let mut backend_ref = self.backend.borrow_mut();
                    if let Some(backend) = backend_ref.as_mut() {
                        backend.frame(&frame.scene, GlyphRasterConfig { px: 18.0 * scale });
                    }

                    self.frame_cache = Some(frame);

                    // keep animating (simple always-on loop)
                    window.request_redraw();
                }

                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            self.request_redraw();
        }
    }

    // IMPORTANT: spawn_app takes the app BY VALUE and requires 'static.
    // So do not create `let mut app` and pass `&mut app`.
    let app = App::new(Box::new(crate::app::app as fn(&mut Scheduler) -> View));
    event_loop.spawn_app(app);
}
