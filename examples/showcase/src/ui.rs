#![allow(non_snake_case)]

use repose_core::prelude::*;
use repose_material::material3::Card;
use repose_ui::*;

/// A titled section with consistent spacing.
pub fn Section(title: &str, body: View) -> View {
    Column(Modifier::new().padding(8.0)).child((
        Text(title)
            .color(theme().on_surface)
            .size(18.0)
            .modifier(Modifier::new().padding(4.0)),
        Card(Modifier::new().fill_max_width(), true, body),
    ))
}

/// Top bar with a subtle bottom divider.
pub fn TopBar() -> View {
    Row(Modifier::new()
        .padding(12.0)
        .background(theme().surface)
        .border(1.0, theme().outline, 0.0))
}

pub fn Page(body: View) -> View {
    Row(Modifier::new().fill_max_size()).child((
        Spacer(),
        Box(Modifier::new().fill_max_size().max_width(1200.0)).child(body),
        Spacer(),
    ))
}

pub fn LabeledCheckbox(
    checked: bool,
    label: impl Into<String>,
    on_change: impl Fn(bool) + 'static,
) -> View {
    Row(Modifier::new()
        .align_items(AlignItems::Center)
        .justify_content(AlignContent::Start))
    .child((
        Checkbox(checked, on_change),
        Box(Modifier::new().width(8.0).height(1.0)),
        Text(label).size(16.0),
    ))
}

pub fn LabeledRadioButton(
    selected: bool,
    label: impl Into<String>,
    on_select: impl Fn() + 'static,
) -> View {
    Row(Modifier::new()
        .align_items(AlignItems::Center)
        .justify_content(AlignContent::Start))
    .child((
        RadioButton(selected, on_select),
        Box(Modifier::new().width(8.0).height(1.0)),
        Text(label).size(16.0),
    ))
}

pub fn LabeledSwitch(
    checked: bool,
    label: impl Into<String>,
    on_change: impl Fn(bool) + 'static,
) -> View {
    Row(Modifier::new()
        .align_items(AlignItems::Center)
        .justify_content(AlignContent::Start))
    .child((
        Switch(checked, on_change),
        Box(Modifier::new().width(8.0).height(1.0)),
        Text(label).size(16.0),
    ))
}
pub fn LabeledSlider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    label: impl Into<String>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    Column(
        Modifier::new()
            .justify_content(AlignContent::Start)
            .align_items(AlignItems::Stretch),
    )
    .child((
        Text(label).size(14.0).color(Color::from_hex("#BBBBBB")),
        Box(Modifier::new().height(4.0).width(1.0)),
        Slider(value, range, step, on_change),
    ))
}

pub fn LabeledRangeSlider(
    start: f32,
    end: f32,
    range: (f32, f32),
    step: Option<f32>,
    label: impl Into<String>,
    on_change: impl Fn(f32, f32) + 'static,
) -> View {
    Column(
        Modifier::new()
            .justify_content(AlignContent::Start)
            .align_items(AlignItems::Stretch),
    )
    .child((
        Text(label).size(14.0).color(Color::from_hex("#BBBBBB")),
        Box(Modifier::new().height(4.0).width(1.0)),
        RangeSlider(start, end, range, step, on_change),
    ))
}

pub fn LabeledLinearProgress(value: Option<f32>, label: impl Into<String>) -> View {
    Column(
        Modifier::new()
            .justify_content(AlignContent::Start)
            .align_items(AlignItems::Stretch),
    )
    .child((
        Text(label).size(14.0).color(Color::from_hex("#BBBBBB")),
        Box(Modifier::new().height(4.0).width(1.0)),
        LinearProgress(value),
    ))
}
