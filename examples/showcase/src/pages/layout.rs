use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Column(Modifier::new().fill_max_width()).child((
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
