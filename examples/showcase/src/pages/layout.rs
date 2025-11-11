use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
    Row(Modifier::new().align_self_center()).child(
        Column(Modifier::new().padding(8.0)).child((
            Section(
                "Grid layout (3 columns, gaps)",
                Grid(
                    3,
                    Modifier::new().padding(8.0),
                    (0..6)
                        .map(|i| {
                            Box(Modifier::new()
                                .padding(8.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 8.0))
                            .child(
                                Text(format!("Item {}", i + 1))
                                    .modifier(Modifier::new().padding(12.0)),
                            )
                        })
                        .collect(),
                ),
            ),
            Section(
                "Absolute positioning + aspect ratio",
                Stack(
                    Modifier::new()
                        .size(360.0, 180.0)
                        .background(theme().surface),
                )
                .child((
                    Box(Modifier::new()
                        .aspect_ratio(16.0 / 9.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 6.0)
                        .size(320.0, 0.0)),
                    // Badge in top-right
                    Box(Modifier::new()
                        .absolute()
                        .offset(None, Some(8.0), Some(8.0), None)
                        .background(theme().primary)
                        .clip_rounded(4.0)
                        .padding(6.0))
                    .child(Text("ABS")),
                )),
            ),
            Section(
                "Baseline alignment",
                Row(Modifier::new().padding(8.0)).child((
                    Text("Top")
                        .size(24.0)
                        .modifier(Modifier::new().padding(4.0)),
                    Text("Baseline aligned")
                        .size(16.0)
                        .modifier(Modifier::new().padding(4.0).align_self_baseline()),
                    Text("Big")
                        .size(32.0)
                        .modifier(Modifier::new().padding(4.0)),
                )),
            ),
        )),
    )
}
