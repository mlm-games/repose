use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::{Hint, Page, Section, sp};

fn shadow_card(m: Modifier, label: &'static str, fg: Option<Color>) -> View {
    let mut t = Text(label).size(14.0);
    if let Some(c) = fg {
        t = t.color(c);
    }
    Box(m
        .size(160.0, 100.0)
        .graphics_layer(1.0)
        .clip_rounded(12.0)
        .padding(sp::MD))
    .child(t)
}

pub fn screen() -> View {
    Page(vec![
        Section("view! macro - declarative syntax", {
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Layout built via view! macro instead of nested function calls"),
                repose_core::View!(Row(Modifier::new().gap(8.0)).child((
                    Box(Modifier::new().size(32.0, 32.0).background(theme().primary).clip_rounded(6.0)),
                    Text("Macro").size(18.0).color(theme().on_surface),
                    Box(Modifier::new().size(32.0, 32.0).background(theme().tertiary).clip_rounded(6.0)),
                ))),
                Hint("Equivalent: Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child(("),
            ))
        }),
        Section(
            "Grid (3 columns)",
            Grid(
                3,
                Modifier::new().padding(sp::MD),
                (0..6)
                    .map(|i| {
                        Box(Modifier::new()
                            .padding(sp::SM)
                            .background(theme().surface)
                            .border(1.0, theme().outline, 10.0)
                            .clip_rounded(10.0))
                        .child(Text(format!("Item {}", i + 1)).modifier(Modifier::new().padding(sp::MD)))
                    })
                    .collect(),
                8.0,
                8.0,
            ),
        ),
        Section(
            "Graphics Layer (Modifier::graphics_layer)",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Render subtree to an offscreen texture, then composite with group alpha."),
                Column(Modifier::new().size(420.0, 160.0)).child((
                    Box(Modifier::new()
                        .size(420.0, 160.0)
                        .background(theme().primary.with_alpha(96))
                        .clip_rounded(12.0)),
                    Box(Modifier::new()
                        .size(360.0, 120.0)
                        .graphics_layer(0.7)
                        .absolute()
                        .offset(Some(20.0), Some(20.0), None, None)
                        .background(theme().secondary)
                        .border(1.0, theme().outline, 12.0)
                        .clip_rounded(12.0)
                        .padding(sp::LG))
                    .child((
                        Text("graphics_layer(0.7)").size(20.0).color(theme().on_secondary),
                        Text("Subtree is rendered to an offscreen texture\nand composited at 70% alpha.")
                            .size(12.0)
                            .color(theme().on_secondary),
                    )),
                    Box(Modifier::new()
                        .size(280.0, 60.0)
                        .graphics_layer(0.5)
                        .absolute()
                        .offset(Some(120.0), Some(80.0), None, None)
                        .background(theme().tertiary)
                        .border(1.0, theme().outline, 8.0)
                        .clip_rounded(8.0)
                        .padding(sp::SM))
                    .child(Text("graphics_layer(0.5) overlapping").color(theme().on_tertiary).size(14.0)),
                )),
            )),
        ),
        Section(
            "Drop Shadow (.elevation / .shadow)",
            Column(Modifier::new().padding(20.0).gap(20.0)).child((
                Hint("Combine graphics_layer with shadow/elevation to render an offscreen-pass Gaussian drop shadow."),
                FlowRow(Modifier::new().fill_max_width().gap(sp::XL), FlowRowConfig::default()).child((
                    shadow_card(
                        Modifier::new().elevation(4.0).background(theme().surface)
                            .border(1.0, theme().outline_variant, 12.0),
                        "elevation(4)", None,
                    ),
                    shadow_card(
                        Modifier::new().elevation(8.0).background(theme().surface)
                            .border(1.0, theme().outline_variant, 12.0),
                        "elevation(8)", None,
                    ),
                    shadow_card(
                        Modifier::new().shadow(16.0, 6.0).background(theme().primary),
                        "shadow(16, 6)", Some(theme().on_primary),
                    ),
                )),
            )),
        ),
        Section(
            "Stack (absolute positioning)",
            Column(
                Modifier::new()
                    .size(420.0, 180.0)
                    .background(theme().surface)
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
            )
            .child((
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(12.0), Some(12.0), None, None)
                    .background(theme().primary)
                    .clip_rounded(10.0)
                    .padding(10.0))
                .child(Text("Top-left").color(theme().on_primary)),
                Box(Modifier::new()
                    .absolute()
                    .offset(None, None, Some(12.0), Some(12.0))
                    .background(theme().surface)
                    .border(1.0, theme().outline, 10.0)
                    .clip_rounded(10.0)
                    .padding(10.0))
                .child(Text("Bottom-right")),
            )),
        ),
    ])
}
