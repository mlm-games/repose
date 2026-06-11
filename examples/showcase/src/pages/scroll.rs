use repose_core::prelude::*;
use repose_ui::scroll::{
    HorizontalScrollArea, ScrollArea, ScrollAreaXY, remember_horizontal_scroll_state,
    remember_scroll_state, remember_scroll_state_xy,
};
use repose_ui::*;

use crate::ui::{Page, Section};

fn frame(height: f32) -> Modifier {
    Modifier::new()
        .height(height)
        .fill_max_width()
        .border(1.0, theme().outline, 12.0)
        .clip_rounded(12.0)
}

fn tile(label: String, m: Modifier) -> View {
    Box(m
        .padding(10.0)
        .background(theme().surface)
        .border(1.0, theme().outline, 10.0)
        .clip_rounded(10.0))
    .child(Text(label))
}

pub fn screen() -> View {
    Page(vec![
        Section(
            "Vertical ScrollArea",
            ScrollArea(
                frame(220.0),
                remember_scroll_state("scroll_v"),
                Column(Modifier::new().fill_max_width()).child(
                    (0..40)
                        .map(|i| tile(format!("Row {i}"), Modifier::new().fill_max_width()))
                        .collect::<Vec<_>>(),
                ),
            ),
        ),
        Section(
            "Horizontal ScrollArea",
            HorizontalScrollArea(
                frame(140.0),
                remember_horizontal_scroll_state("scroll_h"),
                Row(Modifier::new()).child(
                    (0..30)
                        .map(|i| {
                            tile(
                                format!("Tile {i}"),
                                Modifier::new().key(i as u64).size(140.0, 90.0),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
        ),
        Section(
            "2D ScrollAreaXY (responsive width)",
            ScrollAreaXY(
                frame(220.0),
                remember_scroll_state_xy("scroll_xy"),
                Grid(
                    10,
                    Modifier::new(),
                    (0..140)
                        .map(|i| {
                            tile(
                                format!("{i}"),
                                Modifier::new().key(i as u64).size(120.0, 60.0),
                            )
                        })
                        .collect(),
                    8.0,
                    8.0,
                ),
            ),
        ),
    ])
}
