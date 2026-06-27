use repose_core::prelude::*;
use repose_core::signal;
use repose_material::material3::{Button, ButtonConfig, ElevatedButton, TextButton};
use repose_ui::LazyColumnState;
use repose_ui::anim::{animate_f32, animate_f32_from};
use repose_ui::anim_ext::{AnimatedContent, Crossfade, EnterTransition, ExitTransition};
use repose_ui::lazy::LazyColumn;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;
use web_time::Duration;

use crate::ui::{Hint, Section, sp};

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

/// The centered colored "state card" used by Crossfade + AnimatedContent.
fn state_face(label: &'static str, bg: Color, fg: Color) -> View {
    Box(Modifier::new()
        .fill_max_size()
        .background(bg)
        .clip_rounded(12.0)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center))
    .child(Text(label).color(fg).size(18.0))
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

    ScrollArea(Modifier::new().fill_max_size(), scroll, Column(Modifier::new().padding(sp::SM).gap(sp::LG)).child((

        Section("Spring Animation", {
            let spec = match mode.get() {
                SpringMode::Gentle => AnimationSpec::spring_gentle(),
                SpringMode::Bouncy => AnimationSpec::spring_bouncy(),
                SpringMode::Crit => AnimationSpec::spring_crit(8.0),
            };
            let t = animate_f32("demo_scale", if visible.get() { 1.0 } else { 0.75 }, spec);
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Row(Modifier::new().align_items(AlignItems::Center).gap(sp::SM)).child((
                    TextButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Gentle) }, ButtonConfig::default(), || Text("Gentle")),
                    TextButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Bouncy) }, ButtonConfig::default(), || Text("Bouncy")),
                    ElevatedButton(Modifier::new(), { let m = mode.clone(); move || m.set(SpringMode::Crit) }, ButtonConfig::default(), || Text("Crit")),
                    Spacer(),
                    TextButton(Modifier::new(), { let v = visible.clone(); move || v.update(|x| *x = !*x) }, ButtonConfig::default(), || Text("Toggle")),
                )),
                Box(Modifier::new().padding(sp::SM)).child(Box(Modifier::new()
                    .size(220.0, 120.0)
                    .scale(t).alpha(t)
                    .background(theme().primary)
                    .clip_rounded(16.0))),
            ))
        }),

        Section("Crossfade", {
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Button(Modifier::new(), {
                    let c = cross.clone();
                    move || c.update(|x| *x = match x { CrossfadeState::A => CrossfadeState::B, CrossfadeState::B => CrossfadeState::A })
                }, ButtonConfig::default(), || Text("Toggle")),
                Box(Modifier::new().size(200.0, 80.0)).child(
                    Crossfade("cross_demo", cross.get(),
                        AnimationSpec::tween(Duration::from_millis(400), Easing::EaseInOut),
                        |s| match s {
                            CrossfadeState::A => state_face("State A", theme().primary, theme().on_primary),
                            CrossfadeState::B => state_face("State B", theme().tertiary, theme().on_tertiary),
                        }),
                ),
            ))
        }),

        Section("Animate Content Size", {
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Button(Modifier::new(), { let x = long_text.clone(); move || x.update(|v| *v = !*v) }, ButtonConfig::default(), || Text("Toggle Long Text")),
                Box(Modifier::new()
                    .animate_content_size(AnimationSpec::spring_gentle())
                    .background(theme().surface_container_highest)
                    .clip_rounded(12.0)
                    .padding(sp::LG))
                .child(
                    Text(if long_text.get() {
                        "This is a much longer text that demonstrates how animateContentSize smoothly transitions between different content sizes without any jarring jumps."
                    } else {
                        "Short text."
                    }).color(theme().on_surface).size(16.0),
                ),
            ))
        }),

        Section("Repeated Pulse (infinite + reverse)", {
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("A pulsing box using repeated animation spec"),
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

            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Row(Modifier::new().align_items(AlignItems::Center).gap(sp::SM)).child((
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(0) }, ButtonConfig::default(), || Text("Fade")),
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(1) }, ButtonConfig::default(), || Text("Slide")),
                    TextButton(Modifier::new(), { let t = transition_kind.clone(); move || t.set(2) }, ButtonConfig::default(), || Text("Scale")),
                    Spacer(),
                    Button(Modifier::new(), { let s = content_state.clone(); move || {
                        s.update(|x| *x = match x {
                            ContentState::First => ContentState::Second,
                            ContentState::Second => ContentState::Third,
                            ContentState::Third => ContentState::First,
                        })
                    }}, ButtonConfig::default(), || Text("Next")),
                )),
                Box(Modifier::new().size(300.0, 100.0)).child(
                    AnimatedContent("content_demo", content_state.get(),
                        AnimationSpec::tween(Duration::from_millis(350), Easing::EaseInOut),
                        enter, exit,
                        |s| match s {
                            ContentState::First => state_face("First", theme().primary, theme().on_primary),
                            ContentState::Second => state_face("Second", theme().tertiary, theme().on_tertiary),
                            ContentState::Third => state_face("Third", theme().error_container, theme().on_error_container),
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

            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Row(Modifier::new().align_items(AlignItems::Center).gap(6.0)).child((
                    Button(Modifier::new(), {
                        let li = list_items.clone();
                        let nid = next_id.clone();
                        move || {
                            let mut v = li.get();
                            let id = nid.get();
                            v.push(ListItem { id, label: format!("Item {id}"), color_idx: (id % 4) as u8 });
                            li.set(v);
                            nid.set(id + 1);
                        }
                    }, ButtonConfig::default(), || Text("Add")),
                    Button(Modifier::new(), {
                        let li = list_items.clone();
                        move || li.update(|v| { if !v.is_empty() { v.remove(0); } })
                    }, ButtonConfig::default(), || Text("Pop First")),
                    Button(Modifier::new(), {
                        let li = list_items.clone();
                        move || li.update(|v| { v.pop(); })
                    }, ButtonConfig::default(), || Text("Pop Last")),
                    Spacer(),
                )),
                Row(Modifier::new().align_items(AlignItems::Center).gap(6.0)).child((
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(0) }, ButtonConfig::default(), || Text("Fast")),
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(1) }, ButtonConfig::default(), || Text("Tween")),
                    TextButton(Modifier::new(), { let s = list_anim_spec.clone(); move || s.set(2) }, ButtonConfig::default(), || Text("Spring")),
                    Spacer(),
                    Text("Count: ").size(13.0).color(theme().on_surface_variant),
                    Text(items.len().to_string()).size(13.0).color(theme().on_surface),
                )),
                Box(Modifier::new()
                    .max_width(600.0)
                    .max_height(220.0)
                    .border(1.0, theme().outline_variant, 8.0)
                    .clip_rounded(8.0))
                .child(LazyColumn(
                    items,
                    44.0,
                    list_state.clone(),
                    Modifier::new().fill_max_size(),
                    |item: &ListItem| item.id,
                    Some(spec),
                    move |item: ListItem, _idx| {
                        let c = colors[item.color_idx as usize % colors.len()];
                        Row(Modifier::new()
                            .padding(sp::MD)
                            .fill_max_width()
                            .height(44.0)
                            .align_items(AlignItems::Center)
                            .gap(10.0))
                        .child((
                            Box(Modifier::new().size(24.0, 24.0).background(c).clip_rounded(12.0).flex_shrink(0.0)),
                            Column(Modifier::new()).child((
                                Text(item.label).size(14.0).color(theme().on_surface),
                                Text(format!("id={}", item.id)).size(11.0).color(theme().on_surface_variant),
                            )),
                            Spacer(),
                            TextButton(Modifier::new(), {
                                let li = list_items.clone();
                                let target_id = item.id;
                                move || li.update(|v| v.retain(|x| x.id != target_id))
                            }, ButtonConfig::default(), || Text("✕")),
                        ))
                    },
                )),
            ))
        }),
    )))
}
