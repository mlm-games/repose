use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::scroll::{
    HorizontalScrollArea, ScrollArea, ScrollAreaXY, remember_horizontal_scroll_state,
    remember_scroll_state, remember_scroll_state_xy,
};
use repose_ui::*;

pub fn screen() -> View {
    let v_state = remember_scroll_state("scroll_v");
    let h_state = remember_horizontal_scroll_state("scroll_h");
    let xy_state = remember_scroll_state_xy("scroll_xy");

    Column(Modifier::new().fill_max_width().padding(8.0)).child((
        Section(
            "Vertical ScrollArea",
            ScrollArea(
                Modifier::new()
                    .height(180.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 6.0),
                v_state,
                Column(Modifier::new()).child(
                    (0..40)
                        .map(|i| {
                            Box(Modifier::new()
                                .padding(8.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 6.0))
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
                    .size(360.0, 120.0)
                    .border(1.0, theme().outline, 6.0),
                h_state,
                Row(Modifier::new()).child(
                    (0..30)
                        .map(|i| {
                            Box(Modifier::new()
                                .padding(8.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 6.0)
                                .size(120.0, 80.0))
                            .child(Text(format!("Tile {i}")))
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
        ),
        Section(
            "XY ScrollArea",
            ScrollAreaXY(
                Modifier::new()
                    .size(360.0, 160.0)
                    .border(1.0, theme().outline, 6.0),
                xy_state,
                Grid(
                    8,
                    Modifier::new(),
                    (0..120)
                        .map(|i| {
                            Box(Modifier::new()
                                .padding(6.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 6.0)
                                .size(120.0, 60.0))
                            .child(Text(format!("{i}")))
                        })
                        .collect(),
                ),
            ),
        ),
    ))
}
