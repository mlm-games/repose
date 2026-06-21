use repose_core::*;
use repose_material::material3::Button;
use repose_platform::{RenderContext, run_desktop_app};
use repose_ui::*;
use std::cell::RefCell;
use std::rc::Rc;

fn app(_s: &mut Scheduler, _rc: &RenderContext) -> View {
    let th = theme();
    let animated_color = remember_with_key("color", || {
        Rc::new(RefCell::new(repose_core::animation::AnimatedValue::new(
            th.primary,
            repose_core::animation::AnimationSpec::default(),
        )))
    });

    let animated_size = remember_with_key("size", || {
        Rc::new(RefCell::new(repose_core::animation::AnimatedValue::new(
            100.0f32,
            repose_core::animation::AnimationSpec::fast(),
        )))
    });

    // Update animations
    {
        let cont_color = animated_color.borrow_mut().update();
        let cont_size = animated_size.borrow_mut().update();

        // Keep ticking for anim
        if cont_color || cont_size {
            repose_core::request_frame();
        }
    }

    let current_color = *animated_color.borrow().get();
    let current_size = *animated_size.borrow().get();

    Box(
        Modifier::new().fill_max_size().background(th.background),
    )
    .child(
        Column(Modifier::new().padding(32.0)).child((
            Text("Animation Demo").modifier(Modifier::new().padding(12.0)),
            // Animated box
            Box(Modifier::new()
                .size(current_size, current_size)
                .background(current_color)
                .border(2.0, th.on_surface, 8.0)),
            // Controls
            Row(Modifier::new().padding(16.0)).child((
                Button(Modifier::new(), {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(th.primary);
                    }
                }, || Text("🔵 Blue").modifier(Modifier::new().padding(8.0).align_self_center())),
                Button(Modifier::new(), {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(th.secondary);
                    }
                }, || Text("🟢 Green").modifier(Modifier::new().padding(8.0).align_self_center())),
                Button(Modifier::new(), {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(th.error);
                    }
                }, || Text("🔴 Red").modifier(Modifier::new().padding(8.0).align_self_center())),
            )),
            Row(Modifier::new().padding(8.0)).child((
                Button(Modifier::new(), {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(80.0);
                    }
                }, || Text("Small").modifier(Modifier::new().padding(8.0).align_self_center())),
                Button(Modifier::new(), {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(150.0);
                    }
                }, || Text("Medium").modifier(Modifier::new().padding(8.0).align_self_center())),
                Button(Modifier::new(), {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(220.0);
                    }
                }, || Text("Large").modifier(Modifier::new().padding(8.0).align_self_center())),
            )),
            Text(
                if animated_color.borrow().is_animating() || animated_size.borrow().is_animating() {
                    "🎬 Animating..."
                } else {
                    "✓ Idle"
                },
            )
            .size(64.0)
            .color(th.on_surface_variant)
            .modifier(Modifier::new().padding(12.0)),
        )),
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    log::info!("Starting Animation Demo");
    run_desktop_app(app)
}
