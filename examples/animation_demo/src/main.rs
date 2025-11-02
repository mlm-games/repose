use compose_core::*;
use compose_platform::run_desktop_app;
use compose_ui::*;
use std::cell::RefCell;
use std::rc::Rc;

fn app(_s: &mut Scheduler) -> View {
    let animated_color = remember_with_key("color", || {
        Rc::new(RefCell::new(compose_core::animation::AnimatedValue::new(
            Color::from_hex("#2196F3"),
            compose_core::animation::AnimationSpec::default(),
        )))
    });

    let animated_size = remember_with_key("size", || {
        Rc::new(RefCell::new(compose_core::animation::AnimatedValue::new(
            100.0f32,
            compose_core::animation::AnimationSpec::fast(),
        )))
    });

    // Update animations
    {
        let mut color = animated_color.borrow_mut();
        let mut size = animated_size.borrow_mut();
        color.update();
        size.update();
    }

    let current_color = animated_color.borrow().get().clone();
    let current_size = *animated_size.borrow().get();

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new().padding(32.0)).child((
            Text("Animation Demo").modifier(Modifier::new().padding(12.0)),
            // Animated box
            Box(Modifier::new()
                .size(current_size, current_size)
                .background(current_color)
                .border(2.0, Color::WHITE, 8.0)),
            // Controls
            Row(Modifier::new().padding(16.0)).child((
                Button("🔵 Blue", {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(Color::from_hex("#2196F3"));
                    }
                }),
                Button("🟢 Green", {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(Color::from_hex("#4CAF50"));
                    }
                }),
                Button("🔴 Red", {
                    let anim = animated_color.clone();
                    move || {
                        anim.borrow_mut().set_target(Color::from_hex("#FF5252"));
                    }
                }),
            )),
            Row(Modifier::new().padding(8.0)).child((
                Button("Small", {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(80.0);
                    }
                }),
                Button("Medium", {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(150.0);
                    }
                }),
                Button("Large", {
                    let anim = animated_size.clone();
                    move || {
                        anim.borrow_mut().set_target(220.0);
                    }
                }),
            )),
            TextColor(
                TextSize(
                    Text(
                        if animated_color.borrow().is_animating()
                            || animated_size.borrow().is_animating()
                        {
                            "🎬 Animating..."
                        } else {
                            "✓ Idle"
                        },
                    ),
                    64.0,
                ),
                Color::from_hex("#888888"),
            )
            .modifier(Modifier::new().padding(12.0)),
        )),
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    log::info!("Starting Animation Demo");
    run_desktop_app(app)
}
