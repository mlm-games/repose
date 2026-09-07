use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::{Hint, Page, Section, sp};

fn shadow_card(m: Modifier, label: &'static str, fg: Option<Color>) -> View {
    let mut t = Text(label).size(Sp(14.0));
    if let Some(c) = fg {
        t = t.color(c);
    }
    Box(m
        .size(Dp(160.0), Dp(100.0))
        .graphics_layer(1.0)
        .clip_rounded(Dp(12.0))
        .padding(sp::MD))
    .child(t)
}

pub fn screen() -> View {
    Page(vec![
        Section("view! macro - declarative syntax", {
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Layout built via view! macro instead of nested function calls"),
                repose_core::View!(Row(Modifier::new().gap(Dp(8.0))).child((
                    Box(Modifier::new().size(Dp(32.0), Dp(32.0)).background(theme().primary).clip_rounded(Dp(6.0))),
                    Text("Macro").size(Sp(18.0)).color(theme().on_surface),
                    Box(Modifier::new().size(Dp(32.0), Dp(32.0)).background(theme().tertiary).clip_rounded(Dp(6.0))),
                ))),
                Hint("Equivalent: Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER)).child(("),
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
                            .border(Dp(1.0), theme().outline, Dp(10.0))
                            .clip_rounded(Dp(10.0)))
                        .child(Text(format!("Item {}", i + 1)).modifier(Modifier::new().padding(sp::MD)))
                    })
                    .collect(),
                Dp(8.0),
                Dp(8.0),
            ),
        ),
        Section(
            "Graphics Layer (Modifier::graphics_layer)",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Render subtree to an offscreen texture, then composite with group alpha."),
                Column(Modifier::new().size(Dp(420.0), Dp(160.0))).child((
                    Box(Modifier::new()
                        .size(Dp(420.0), Dp(160.0))
                        .background(theme().primary.with_alpha(96))
                        .clip_rounded(Dp(12.0))),
                    Box(Modifier::new()
                        .size(Dp(360.0), Dp(120.0))
                        .graphics_layer(0.7)
                        .absolute()
                        .offset(Some(Dp(20.0)), Some(Dp(20.0)), None, None)
                        .background(theme().secondary)
                        .border(Dp(1.0), theme().outline, Dp(12.0))
                        .clip_rounded(Dp(12.0))
                        .padding(sp::LG))
                    .child((
                        Text("graphics_layer(0.7)").size(Sp(20.0)).color(theme().on_secondary),
                        Text("Subtree is rendered to an offscreen texture\nand composited at 70% alpha.")
                            .size(Sp(12.0))
                            .color(theme().on_secondary),
                    )),
                    Box(Modifier::new()
                        .size(Dp(280.0), Dp(60.0))
                        .graphics_layer(0.5)
                        .absolute()
                        .offset(Some(Dp(120.0)), Some(Dp(80.0)), None, None)
                        .background(theme().tertiary)
                        .border(Dp(1.0), theme().outline, Dp(8.0))
                        .clip_rounded(Dp(8.0))
                        .padding(sp::SM))
                    .child(Text("graphics_layer(0.5) overlapping").color(theme().on_tertiary).size(Sp(14.0))),
                )),
            )),
        ),
        Section(
            "Drop Shadow (.elevation / .shadow)",
            Column(Modifier::new().padding(Dp(20.0)).gap(Dp(20.0))).child((
                Hint("Combine graphics_layer with shadow/elevation to render an offscreen-pass Gaussian drop shadow."),
                FlowRow(Modifier::new().fill_max_width().gap(sp::XL), FlowRowConfig::default()).child((
                    shadow_card(
                        Modifier::new().elevation(Dp(4.0)).background(theme().surface)
                            .border(Dp(1.0), theme().outline_variant, Dp(12.0)),
                        "elevation(4)", None,
                    ),
                    shadow_card(
                        Modifier::new().elevation(Dp(8.0)).background(theme().surface)
                            .border(Dp(1.0), theme().outline_variant, Dp(12.0)),
                        "elevation(8)", None,
                    ),
                    shadow_card(
                        Modifier::new().shadow(Dp(16.0), Dp(6.0)).background(theme().primary),
                        "shadow(16, 6)", Some(theme().on_primary),
                    ),
                )),
            )),
        ),
        Section(
            "Stack (absolute positioning)",
            Column(
                Modifier::new()
                    .size(Dp(420.0), Dp(180.0))
                    .background(theme().surface)
                    .border(Dp(1.0), theme().outline, Dp(12.0))
                    .clip_rounded(Dp(12.0)),
            )
            .child((
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(Dp(12.0)), Some(Dp(12.0)), None, None)
                    .background(theme().primary)
                    .clip_rounded(Dp(10.0))
                    .padding(Dp(10.0)))
                .child(Text("Top-left").color(theme().on_primary)),
                Box(Modifier::new()
                    .absolute()
                    .offset(None, None, Some(Dp(12.0)), Some(Dp(12.0)))
                    .background(theme().surface)
                    .border(Dp(1.0), theme().outline, Dp(10.0))
                    .clip_rounded(Dp(10.0))
                    .padding(Dp(10.0)))
                .child(Text("Bottom-right")),
            )),
        ),
    ])
}
