use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::{anim::animate_f32, *};

pub fn screen() -> View {
    let spring_mode = remember(|| signal("gentle".to_string()));
    let visible = remember(|| signal(true));
    let cross = remember(|| signal(0u32));

    Column(Modifier::new().padding(8.0)).child((
        Section(
            "Springs (gentle / bouncy / crit)",
            Column(Modifier::new().padding(8.0)).child((
                Row(Modifier::new().padding(4.0)).child((
                    Button(Text("Gentle"), {
                        let m = spring_mode.clone();
                        move || m.set("gentle".into())
                    }),
                    Button(Text("Bouncy"), {
                        let m = spring_mode.clone();
                        move || m.set("bouncy".into())
                    }),
                    Button(Text("Crit"), {
                        let m = spring_mode.clone();
                        move || m.set("crit".into())
                    }),
                    Spacer(),
                    Button(Text("Toggle"), {
                        let v = visible.clone();
                        move || v.update(|x| *x = !*x)
                    }),
                )),
                {
                    let spec = match spring_mode.get().as_str() {
                        "bouncy" => repose_core::animation::AnimationSpec::spring_bouncy(),
                        "crit" => repose_core::animation::AnimationSpec::spring_crit(
                            8.0,
                            std::time::Duration::from_millis(500),
                        ),
                        _ => repose_core::animation::AnimationSpec::spring_gentle(),
                    };
                    let s = if visible.get() {
                        animate_f32("spring_demo_scale", 1.0, spec)
                    } else {
                        animate_f32("spring_demo_scale", 0.6, spec)
                    };
                    Box(Modifier::new().padding(12.0)).child(Box(Modifier::new()
                        .size(120.0, 80.0)
                        .scale(s)
                        .alpha(s)
                        .background(theme().primary)
                        .clip_rounded(12.0)))
                },
            )),
        ),
        Section(
            "AnimatedVisibility + Crossfade",
            Row(Modifier::new().padding(8.0)).child((
                {
                    let show = visible.get();
                    // key-safe AnimatedVisibility
                    repose_ui::anim_ext::AnimatedVisibility(
                        "demo_visibility",
                        show,
                        repose_ui::anim_ext::EnterTransition::FadeIn,
                        repose_ui::anim_ext::ExitTransition::FadeOut,
                        Text("Peek-a-boo!").size(20.0),
                    )
                },
                Spacer(),
                Button(Text("Crossfade"), {
                    let cross = cross.clone();
                    move || cross.update(|c| *c = (*c + 1) % 3)
                }),
                {
                    let idx = cross.get();
                    // key-safe Crossfade
                    repose_ui::anim_ext::Crossfade("cf_key", idx, |i| {
                        Text(match i {
                            0 => "One",
                            1 => "Two",
                            _ => "Three",
                        })
                        .size(24.0)
                    })
                },
            )),
        ),
    ))
}
