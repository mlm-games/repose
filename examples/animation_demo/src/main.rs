use repose_core::*;
use repose_material::material3::{Button, ButtonConfig};
use repose_platform::{RenderContext, run_desktop_app};
use repose_ui::*;
use std::cell::RefCell;
use std::rc::Rc;

fn app(_s: &mut Scheduler, _rc: &RenderContext) -> View {
    let th = theme();

    let registered = remember_state_with_key("driver_reg", || false);
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

    // Register with AnimationDriver
    if !(*registered.borrow()) {
        let reg_color = animated_color.clone();
        repose_core::animation_driver::register(
            "demo_color".into(),
            Rc::new(RefCell::new(move || reg_color.borrow_mut().update())),
        );
        let reg_size = animated_size.clone();
        repose_core::animation_driver::register(
            "demo_size".into(),
            Rc::new(RefCell::new(move || reg_size.borrow_mut().update())),
        );
        *registered.borrow_mut() = true;
    }

    let current_color = *animated_color.borrow().get();
    let current_size = *animated_size.borrow().get();

    let on_color = |target: Color| {
        let anim = animated_color.clone();
        move || {
            let mut a = anim.borrow_mut();
            a.set_target(target);
            drop(a);
            request_frame();
        }
    };

    let on_size = |target: f32| {
        let anim = animated_size.clone();
        move || {
            anim.borrow_mut().set_target(target);
            request_frame();
        }
    };

    Box(Modifier::new().fill_max_size().background(th.background)).child(
        Column(Modifier::new().padding(32.0)).child((
            Text("Animation Demo").modifier(Modifier::new().padding(12.0)),
            Box(Modifier::new()
                .size(current_size, current_size)
                .background(current_color)
                .border(2.0, th.on_surface, 8.0)),
            Row(Modifier::new().padding(16.0)).child((
                Button(
                    Modifier::new(),
                    on_color(th.primary),
                    ButtonConfig::default(),
                    || Text("🔵 Blue").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
                Button(
                    Modifier::new(),
                    on_color(th.secondary),
                    ButtonConfig::default(),
                    || Text("🟢 Green").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
                Button(
                    Modifier::new(),
                    on_color(th.error),
                    ButtonConfig::default(),
                    || Text("🔴 Red").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
            )),
            Row(Modifier::new().padding(8.0)).child((
                Button(
                    Modifier::new(),
                    on_size(80.0),
                    ButtonConfig::default(),
                    || Text("Small").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
                Button(
                    Modifier::new(),
                    on_size(150.0),
                    ButtonConfig::default(),
                    || Text("Medium").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
                Button(
                    Modifier::new(),
                    on_size(220.0),
                    ButtonConfig::default(),
                    || Text("Large").modifier(Modifier::new().padding(8.0).align_self_center()),
                ),
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
