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
