use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Column(Modifier::new().fill_max_width()).child((

        Section("view! macro - declarative syntax", {
            Column(Modifier::new().padding(12.0)).child((
                Text("Layout built via view! macro instead of nested function calls")
                    .size(14.0).color(theme().on_surface_variant),
                Box(Modifier::new().height(8.0).width(1.0)),
                repose_core::View!(Row(Modifier::new().gap(8.0)).child((
                    Box(Modifier::new().size(32.0, 32.0).background(theme().primary).clip_rounded(6.0)),
                    Text("Macro").size(18.0).color(theme().on_surface),
                    Box(Modifier::new().size(32.0, 32.0).background(theme().tertiary).clip_rounded(6.0)),
                ))),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text("Equivalent: Row(Modifier::new().gap(8.0).align_items(AlignItems::Center)).child((")
                    .size(12.0).color(theme().on_surface_variant),
            ))
        }),

        Section(
            "Grid (3 columns)",
            Grid(
                3,
                Modifier::new().padding(12.0),
                (0..6)
                    .map(|i| {
                        Box(Modifier::new()
                            .padding(8.0)
                            .background(theme().surface)
                            .border(1.0, theme().outline, 10.0)
                            .clip_rounded(10.0))
                        .child(
                            Text(format!("Item {}", i + 1)).modifier(Modifier::new().padding(12.0)),
                        )
                    })
                    .collect(),
                8.0,
                8.0,
            ),
        ),
        Section(
            "Graphics Layer (Modifier::graphics_layer)",
            Column(Modifier::new().padding(12.0).gap(12.0)).child((
                Text("Render subtree to an offscreen texture, then composite with group alpha.")
                    .size(14.0).color(theme().on_surface_variant),
                Stack(Modifier::new().size(420.0, 160.0))
                    .child((
                        // Background rect (rendered directly)
                        Box(Modifier::new()
                            .size(420.0, 160.0)
                            .background(theme().primary.with_alpha(96))
                            .clip_rounded(12.0)),
                        // Layered card with 70% alpha
                        Box(Modifier::new()
                            .size(360.0, 120.0)
                            .graphics_layer(0.7)
                            .absolute()
                            .offset(Some(20.0), Some(20.0), None, None)
                            .background(theme().secondary)
                            .border(1.0, theme().outline, 12.0)
                            .clip_rounded(12.0)
                            .padding(16.0))
                        .child((
                            Text("graphics_layer(0.7)")
                                .size(20.0)
                                .color(theme().on_secondary),
                            Text("Subtree is rendered to an offscreen texture\nand composited at 70% alpha.")
                                .size(12.0)
                                .color(theme().on_secondary),
                        )),
                        // Another layer with 50% alpha to demonstrate stacking
                        Box(Modifier::new()
                            .size(280.0, 60.0)
                            .graphics_layer(0.5)
                            .absolute()
                            .offset(Some(120.0), Some(80.0), None, None)
                            .background(theme().tertiary)
                            .border(1.0, theme().outline, 8.0)
                            .clip_rounded(8.0)
                            .padding(8.0))
                        .child(Text("graphics_layer(0.5) overlapping").color(theme().on_tertiary).size(14.0)),
                    )),
            )),
        ),
        Section(
            "Drop Shadow (.elevation / .shadow)",
            Column(Modifier::new().padding(20.0).gap(20.0)).child((
                Text("Combine graphics_layer with shadow/elevation to render an offscreen-pass Gaussian drop shadow.")
                    .size(14.0).color(theme().on_surface_variant),
                Row(Modifier::new().fill_max_width().gap(24.0)).child((
                    // Card 1: small elevation
                    Box(Modifier::new()
                        .size(160.0, 100.0)
                        .graphics_layer(1.0)
                        .elevation(4.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline_variant, 12.0)
                        .clip_rounded(12.0)
                        .padding(12.0))
                    .child(Text("elevation(4)").size(14.0)),
                    // Card 2: medium elevation
                    Box(Modifier::new()
                        .size(160.0, 100.0)
                        .graphics_layer(1.0)
                        .elevation(8.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline_variant, 12.0)
                        .clip_rounded(12.0)
                        .padding(12.0))
                    .child(Text("elevation(8)").size(14.0)),
                    // Card 3: large elevation + custom shadow
                    Box(Modifier::new()
                        .size(160.0, 100.0)
                        .graphics_layer(1.0)
                        .shadow(16.0, 6.0)
                        .background(theme().primary)
                        .clip_rounded(12.0)
                        .padding(12.0))
                    .child(Text("shadow(16, 6)").color(theme().on_primary).size(14.0)),
                )),
            )),
        ),
        Section(
            "Stack (absolute positioning)",
            Stack(
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
    ))
}
