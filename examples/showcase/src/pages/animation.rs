use repose_core::prelude::*;
use repose_core::signal;
use repose_material::material3::{ElevatedButton, FilledButton, TextButton};
use repose_ui::anim::{animate_f32, animate_f32_from, animate_keyframes};
use repose_ui::anim_ext::{AnimatedContent, Crossfade, EnterTransition, ExitTransition};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;
use web_time::Duration;

use crate::ui::Section;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpringMode {
    Gentle,
    Bouncy,
    Crit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CrossfadeState {
    A,
    B,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContentState {
    First,
    Second,
    Third,
}

pub fn screen() -> View {
    let mode = remember(|| signal(SpringMode::Gentle));
    let visible = remember(|| signal(true));

    let cross = remember(|| signal(CrossfadeState::A));

    let content_state = remember(|| signal(ContentState::First));
    let transition_kind = remember(|| signal(0u8)); // 0=fade, 1=slide, 2=scale

    let long_text = remember(|| signal(false));
    let bounce_anim = animate_keyframes(
        "kf_bounce",
        KeyframesSpec::new(vec![
            (0.0, 20.0),
            (0.4, 140.0),
            (0.6, 80.0),
            (0.8, 120.0),
            (1.0, 100.0),
        ]),
        AnimationSpec::tween(Duration::from_millis(800), Easing::EaseOut)
            .repeated(RepeatableSpec::infinite()),
    );
    let repeated_anim = animate_f32_from(
        "rep_pulse",
        1.0,
        0.5,
        AnimationSpec::tween(Duration::from_millis(600), Easing::EaseInOut)
            .repeated(RepeatableSpec::infinite().reverse()),
    );
    let scroll = remember_scroll_state("animation_scroll");

    ScrollArea(Modifier::new().fill_max_size(), scroll, Column(Modifier::new().padding(8.0)).child((

        Section("Spring Animation", {
            let spec = match mode.get() {
                SpringMode::Gentle => AnimationSpec::spring_gentle(),
                SpringMode::Bouncy => AnimationSpec::spring_bouncy(),
                SpringMode::Crit => AnimationSpec::spring_crit(8.0),
            };

            let t = animate_f32("demo_scale", if visible.get() { 1.0 } else { 0.75 }, spec);
            Column(Modifier::new().padding(12.0)).child((
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    TextButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Gentle) }, || Text("Gentle")),
                    Box(Modifier::new().width(8.0).height(1.0)),
                    TextButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Bouncy) }, || Text("Bouncy")),
                    Box(Modifier::new().width(8.0).height(1.0)),
                    ElevatedButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Crit) }, || Text("Crit")),
                    Spacer(),
                    TextButton(Modifier::new(), { let v = visible.clone(); move || v.update(|x| *x = !*x) }, || Text("Toggle")),
                )),
                Box(Modifier::new().height(8.0).width(1.0)),
                Box(Modifier::new().padding(8.0)).child(Box(Modifier::new()
                    .size(220.0, 120.0)
                    .scale(t).alpha(t)
                    .background(theme().primary)
                    .clip_rounded(16.0))),
            ))
        }),


        Section("Crossfade", {
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), { let c = cross.clone(); move || c.update(|x| *x = match x { CrossfadeState::A => CrossfadeState::B, CrossfadeState::B => CrossfadeState::A }) }, || Text("Toggle")),
                Box(Modifier::new().height(12.0).width(1.0)),
                Box(Modifier::new().size(200.0, 80.0)).child(
                    Crossfade("cross_demo", cross.get(), AnimationSpec::tween(web_time::Duration::from_millis(400), Easing::EaseInOut), |s| {
                        match s {
                            CrossfadeState::A => Box(Modifier::new().fill_max_size().background(theme().primary).clip_rounded(12.0).align_items(AlignItems::Center).justify_content(JustifyContent::Center))
                                .child(Text("State A").color(theme().on_primary).size(18.0)),
                            CrossfadeState::B => Box(Modifier::new().fill_max_size().background(theme().tertiary).clip_rounded(12.0).align_items(AlignItems::Center).justify_content(JustifyContent::Center))
                                .child(Text("State B").color(theme().on_tertiary).size(18.0)),
                        }
                    }),
                ),
            ))
        }),

        Section("Animate Content Size", {
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), { let x = long_text.clone(); move || x.update(|v| *v = !*v) }, || Text("Toggle Long Text")),
                Box(Modifier::new().height(8.0).width(1.0)),
                Box(Modifier::new()
                    .animate_content_size(AnimationSpec::spring_gentle())
                    .background(theme().surface_container_highest)
                    .clip_rounded(12.0)
                    .padding(16.0)
                ).child(
                    Text(
                        if long_text.get() {
                            "This is a much longer text that demonstrates how animateContentSize smoothly transitions between different content sizes without any jarring jumps."
                        } else {
                            "Short text."
                        }
                    ).color(theme().on_surface).size(16.0),
                ),
            ))
        }),


        // Section("Keyframes (bounce)", {
        //     Column(Modifier::new().padding(12.0)).child((
        //         Text("A bar bouncing in width via 5 keyframe stops")
        //             .size(14.0).color(theme().on_surface_variant),
        //         Box(Modifier::new().height(8.0).width(1.0)),
        //         Box(Modifier::new()
        //             .size(bounce_anim + 20.0, 24.0)
        //             .background(theme().primary)
        //             .clip_rounded(4.0)),
        //     ))
        // }),

        Section("Repeated Pulse (infinite + reverse)", {
            Column(Modifier::new().padding(12.0)).child((
                Text("A pulsing box using repeated animation spec")
                    .size(14.0).color(theme().on_surface_variant),
                Box(Modifier::new().height(8.0).width(1.0)),
                Box(Modifier::new()
                    .size(120.0 * repeated_anim, 120.0 * repeated_anim)
                    .background(theme().tertiary)
                    .clip_rounded(16.0)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center))
                .child(Text("Pulse").color(theme().on_tertiary).size(16.0)),
            ))
        }),

        Section("AnimatedContent", {
            let enter = match transition_kind.get() {
                0 => EnterTransition::FadeIn,
                1 => EnterTransition::SlideIn { offset_x: 200.0, offset_y: 0.0 },
                _ => EnterTransition::ScaleIn { initial: 0.5 },
            };
            let exit = match transition_kind.get() {
                0 => ExitTransition::FadeOut,
                1 => ExitTransition::SlideOut { offset_x: -200.0, offset_y: 0.0 },
                _ => ExitTransition::ScaleOut { target: 0.5 },
            };

            Column(Modifier::new().padding(12.0)).child((
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(0) }, || Text("Fade")),
                    Box(Modifier::new().width(8.0).height(1.0)),
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(1) }, || Text("Slide")),
                    Box(Modifier::new().width(8.0).height(1.0)),
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(2) }, || Text("Scale")),
                    Spacer(),
                    FilledButton(Modifier::new(), { let s = content_state.clone(); move || {
                        s.update(|x| *x = match x {
                            ContentState::First => ContentState::Second,
                            ContentState::Second => ContentState::Third,
                            ContentState::Third => ContentState::First,
                        })
                    }}, || Text("Next")),
                )),
                Box(Modifier::new().height(12.0).width(1.0)),
                Box(Modifier::new().size(300.0, 100.0)).child(
                    AnimatedContent("content_demo", content_state.get(), AnimationSpec::tween(web_time::Duration::from_millis(350), Easing::EaseInOut), enter, exit, |s| {
                        match s {
                            ContentState::First => Box(Modifier::new().fill_max_size().background(theme().primary).clip_rounded(12.0).align_items(AlignItems::Center).justify_content(JustifyContent::Center))
                                .child(Text("First").color(theme().on_primary).size(18.0)),
                            ContentState::Second => Box(Modifier::new().fill_max_size().background(theme().tertiary).clip_rounded(12.0).align_items(AlignItems::Center).justify_content(JustifyContent::Center))
                                .child(Text("Second").color(theme().on_tertiary).size(18.0)),
                            ContentState::Third => Box(Modifier::new().fill_max_size().background(theme().error_container).clip_rounded(12.0).align_items(AlignItems::Center).justify_content(JustifyContent::Center))
                                .child(Text("Third").color(theme().on_error_container).size(18.0)),
                        }
                    }),
                ),
            ))
        }),



    )))
}
