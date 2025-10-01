use compose_core::*;
use compose_ui::layout_and_paint;

#[cfg(feature = "desktop")]
pub fn run_desktop_app(mut root: impl FnMut(&mut Scheduler) -> View + 'static) -> anyhow::Result<()> {
    use winit::event::{Event, WindowEvent, ElementState, MouseButton, KeyboardInput, VirtualKeyCode};
    use winit::event_loop::EventLoop;
    use winit::dpi::PhysicalSize;

    let event_loop = EventLoop::new()?;
    let window = winit::window::WindowBuilder::new()
        .with_title("Compose-RS v0.1")
        .with_inner_size(PhysicalSize::new(1280, 800))
        .build(&event_loop)?;

    let mut backend = compose_render_wgpu::WgpuBackend::new(&window)?;
    let mut sched = Scheduler::new();

    let mut frame_cache: Option<Frame> = None;
    let mut mouse_pos = (0.0f32, 0.0f32);

    event_loop.run(move |event, elwt| {
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => elwt.exit(),
                WindowEvent::Resized(size) => {
                    sched.size = (size.width, size.height);
                    backend.configure_surface(size.width, size.height);
                }
                WindowEvent::CursorMoved { position, .. } => {
                    mouse_pos = (position.x as f32, position.y as f32);
                }
                WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                    // Hit test
                    if let Some(f) = &frame_cache {
                        for hit in &f.hit_regions {
                            if hit.rect.contains(compose_core::Vec2 { x: mouse_pos.0, y: mouse_pos.1 }) {
                                if let Some(cb) = &hit.on_click { (cb)(); }
                                if hit.focusable { sched.focused = Some(hit.id); }
                                break;
                            }
                        }
                    }
                    elwt.set_control_flow(winit::event_loop::ControlFlow::Poll);
                }
                WindowEvent::ReceivedCharacter(ch) => {
                    if ch.is_control() { return; }
                    // Feed TextField
                    // TextField stores state by state_key; we keep a global store in remember_state + caller
                }
                WindowEvent::KeyboardInput { input: KeyboardInput { state: ElementState::Pressed, virtual_keycode: Some(VirtualKeyCode::Back), .. }, .. } => {
                    // Backspace would be handled by user state in examples
                }
                _ => {}
            }
            Event::AboutToWait => {
                // Compose and render
                let frame = sched.compose(&mut root, |view, size| layout_and_paint(view, size));
                backend.frame(&frame.scene, GlyphRasterConfig { px: 18.0 });
                frame_cache = Some(frame);
                window.request_redraw();
            }
            Event::LoopExiting => {}
            _ => {}
        }
    })?;
    // unreachable
}
