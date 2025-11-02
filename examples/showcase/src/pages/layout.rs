use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
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
                            .background(Color::from_hex("#2A2A2A"))
                            .border(1.0, Color::from_hex("#3A3A3A"), 8.0))
                        .child(
                            Text(format!("Item {}", i + 1)).modifier(Modifier::new().padding(12.0)),
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
                    .background(Color::from_hex("#222")),
            )
            .child((
                Box(Modifier::new()
                    .aspect_ratio(16.0 / 9.0)
                    .background(Color::from_hex("#444"))
                    .border(1.0, Color::from_hex("#666"), 6.0)
                    .size(320.0, 0.0)),
                // Absolute badge in top-right
                Box(Modifier::new()
                    .absolute()
                    .offset(None, Some(8.0), Some(8.0), None)
                    .background(Color::from_hex("#34AF82"))
                    .clip_rounded(4.0)
                    .padding(6.0))
                .child(Text("ABS").modifier(Modifier::new())),
            )),
        ),
        Section(
            "Baseline alignment (Row)",
            Row(Modifier::new().padding(8.0)).child((
                TextSize(Text("Top"), 24.0).modifier(Modifier::new().padding(4.0)),
                TextSize(Text("Baseline aligned"), 16.0)
                    .modifier(Modifier::new().padding(4.0).align_self_baseline()),
                TextSize(Text("Big"), 32.0).modifier(Modifier::new().padding(4.0)),
            )),
        ),
    ))
}
