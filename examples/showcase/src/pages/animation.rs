use repose_core::{prelude::*, signal};
use repose_material::material3::TextButton;
use repose_ui::{anim::animate_f32, *};

use crate::ui::Section;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpringMode {
    Gentle,
    Bouncy,
    Crit,
}

pub fn screen() -> View {
    let mode = remember(|| signal(SpringMode::Gentle));
    let visible = remember(|| signal(true));

    Section(
        "Animations",
        Column(Modifier::new().padding(12.0)).child((
            Row(Modifier::new().align_items(AlignItems::Center)).child((
                TextButton(
                    {
                        let m = mode.clone();
                        move || m.set(SpringMode::Gentle)
                    },
                    || Text("Gentle"),
                ),
                Box(Modifier::new().width(8.0).height(1.0)),
                TextButton(
                    {
                        let m = mode.clone();
                        move || m.set(SpringMode::Bouncy)
                    },
                    || Text("Bouncy"),
                ),
                Box(Modifier::new().width(8.0).height(1.0)),
                TextButton(
                    {
                        let m = mode.clone();
                        move || m.set(SpringMode::Crit)
                    },
                    || Text("Crit"),
                ),
                Spacer(),
                TextButton(
                    {
                        let v = visible.clone();
                        move || v.update(|x| *x = !*x)
                    },
                    || Text("Toggle"),
                ),
            )),
            Box(Modifier::new().height(16.0).width(1.0)),
            {
                let spec = match mode.get() {
                    SpringMode::Gentle => AnimationSpec::spring_gentle(),
                    SpringMode::Bouncy => AnimationSpec::spring_bouncy(),
                    SpringMode::Crit => {
                        AnimationSpec::spring_crit(8.0, web_time::Duration::from_millis(500))
                    }
                };

                let t = animate_f32("demo_scale", if visible.get() { 1.0 } else { 0.75 }, spec);
                Box(Modifier::new().padding(8.0)).child(Box(Modifier::new()
                    .size(220.0, 120.0)
                    .scale(t)
                    .alpha(t)
                    .background(theme().primary)
                    .clip_rounded(16.0)))
            },
        )),
    )
}
