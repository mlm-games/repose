use repose_core::prelude::*;
use repose_core::signal;
use repose_material::material3::{ElevatedButton, FilledButton, TextButton};
use repose_ui::anim::{animate_f32, animate_f32_from};
use repose_ui::anim_ext::{AnimatedContent, Crossfade, EnterTransition, ExitTransition};
use repose_ui::lazy::{LazyColumn, LazyColumnState};
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

#[derive(Clone, Debug)]
struct ListItem {
    id: u64,
    label: String,
    color_idx: u8,
}

pub fn screen() -> View {
    let mode = remember(|| signal(SpringMode::Gentle));
    let visible = remember(|| signal(true));

    let cross = remember(|| signal(CrossfadeState::A));

    let content_state = remember(|| signal(ContentState::First));
    let transition_kind = remember(|| signal(0u8));

    let long_text = remember(|| signal(false));
    let repeated_anim = animate_f32_from(
        "rep_pulse",
        1.0,
        0.5,
        AnimationSpec::tween(Duration::from_millis(600), Easing::EaseInOut)
            .repeated(RepeatableSpec::infinite().reverse()),
    );

    // LazyColumnAnimated state
    let list_items = remember(|| {
        signal(
            (0u64..6)
                .map(|id| ListItem {
                    id,
                    label: format!("Item {id}"),
                    color_idx: (id % 4) as u8,
                })
                .collect::<Vec<_>>(),
        )
    });
    let list_state = remember(LazyColumnState::new);
    let next_id = remember(|| signal(6u64));
    let list_anim_spec = remember(|| signal(0u8));

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

        Section("LazyColumn Item Animations (fade-in/out)", {
            let items = list_items.get();
            let spec = match list_anim_spec.get() {
                0 => AnimationSpec::fast(),
                1 => AnimationSpec::tween(Duration::from_millis(350), Easing::EaseInOut),
                _ => AnimationSpec::spring_gentle(),
            };
            let colors = [theme().primary, theme().tertiary, theme().secondary, theme().error];

            Column(Modifier::new().padding(12.0)).child((
                // Controls
                Row(Modifier::new().align_items(AlignItems::Center).gap(6.0)).child((
                    FilledButton(Modifier::new(), {
                        let li = list_items.clone();
                        let nid = next_id.clone();
                        move || {
                            let mut v = li.get();
                            let id = nid.get();
                            v.push(ListItem { id, label: format!("Item {id}"), color_idx: (id % 4) as u8 });
                            li.set(v);
                            nid.set(id + 1);
                        }
                    }, || Text("Add")),
                    FilledButton(Modifier::new(), {
                        let li = list_items.clone();
                        move || {
                            let mut v = li.get();
                            if !v.is_empty() {
                                v.remove(0);
                                li.set(v);
                            }
                        }
                    }, || Text("Pop First")),
                    FilledButton(Modifier::new(), {
                        let li = list_items.clone();
                        move || {
                            let mut v = li.get();
                            if !v.is_empty() {
                                v.pop();
                                li.set(v);
                            }
                        }
                    }, || Text("Pop Last")),
                    Spacer(),
                )),
                Box(Modifier::new().height(6.0).width(1.0)),
                Row(Modifier::new().align_items(AlignItems::Center).gap(6.0)).child((
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(0) }, || Text("Fast")),
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(1) }, || Text("Tween")),
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(2) }, || Text("Spring")),
                    Spacer(),
                    Text("Count: ").size(13.0).color(theme().on_surface_variant),
                    Text(items.len().to_string()).size(13.0).color(theme().on_surface),
                )),
                Box(Modifier::new().height(8.0).width(1.0)),
                // Animated list
                Box(Modifier::new()
                    .max_width(600.0)
                    .max_height(220.0)
                    .border(1.0, theme().outline_variant, 8.0)
                    .clip_rounded(8.0)
                ).child(
                    LazyColumn(
                        items,
                        44.0,
                        list_state.clone(),
                        Modifier::new().fill_max_size(),
                        |item: &ListItem| item.id,
                        Some(spec),
                        move |item: ListItem, _idx| {
                            let c = colors[item.color_idx as usize % colors.len()];
                            Row(Modifier::new()
                                .padding(12.0)
                                .fill_max_width()
                                .height(44.0)
                                .align_items(AlignItems::Center)
                            ).child((
                                Box(Modifier::new()
                                    .size(24.0, 24.0)
                                    .background(c)
                                    .clip_rounded(12.0)
                                    .flex_shrink(0.0)
                                ),
                                Box(Modifier::new().width(10.0).height(1.0).flex_shrink(0.0)),
                                Column(Modifier::new()).child((
                                    Text(item.label).size(14.0).color(theme().on_surface),
                                    Text(format!("id={}", item.id))
                                        .size(11.0).color(theme().on_surface_variant),
                                )),
                                Spacer(),
                                TextButton(Modifier::new(), {
                                    let li = list_items.clone();
                                    let target_id = item.id;
                                    move || {
                                        let mut v = li.get();
                                        v.retain(|x| x.id != target_id);
                                        li.set(v);
                                    }
                                }, || Text("✕")),
                            ))
                        },
                    )
                ),
            ))
        }),

    )))
}
