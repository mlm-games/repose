use repose_core::prelude::*;
use repose_ui::scroll::{
    HorizontalScrollArea, ScrollArea, ScrollAreaXY, remember_horizontal_scroll_state,
    remember_scroll_state, remember_scroll_state_xy,
};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    let v_state = remember_scroll_state("scroll_v");
    let h_state = remember_horizontal_scroll_state("scroll_h");
    let xy_state = remember_scroll_state_xy("scroll_xy");

    Column(Modifier::new().fill_max_width()).child((
        Section(
            "Vertical ScrollArea",
            ScrollArea(
                Modifier::new()
                    .height(220.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                v_state,
                Column(Modifier::new().fill_max_width()).child(
                    (0..40)
                        .map(|i| {
                            Box(Modifier::new()
                                .fill_max_width()
                                .padding(10.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 10.0)
                                .clip_rounded(10.0))
                            .child(Text(format!("Row {i}")))
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
        ),
        Section(
            "Horizontal ScrollArea",
            HorizontalScrollArea(
                Modifier::new()
                    .height(140.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                h_state,
                Row(Modifier::new()).child(
                    (0..30)
                        .map(|i| {
                            Box(Modifier::new()
                                .key(i as u64)
                                .padding(10.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 10.0)
                                .clip_rounded(10.0)
                                .size(140.0, 90.0))
                            .child(Text(format!("Tile {i}")))
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
        ),
        Section(
            "2D ScrollAreaXY (responsive width)", // Only works well with min size 0 for height, which breaks other containers...
            ScrollAreaXY(
                Modifier::new()
                    .height(220.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                xy_state,
                Grid(
                    10,
                    Modifier::new(),
                    (0..140)
                        .map(|i| {
                            Box(Modifier::new()
                                .key(i as u64)
                                .padding(8.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 10.0)
                                .clip_rounded(10.0)
                                .size(120.0, 60.0))
                            .child(Text(format!("{i}")))
                        })
                        .collect(),
                    8.0,
                    8.0,
                ),
            ),
        ),
    ))
}
