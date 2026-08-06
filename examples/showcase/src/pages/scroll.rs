use repose_core::prelude::*;
use repose_ui::scroll::{
    HorizontalScrollArea, ScrollArea, ScrollAreaXY, remember_horizontal_scroll_state,
    remember_scroll_state, remember_scroll_state_xy,
};
use repose_ui::*;

use crate::ui::{DemoTile, Hint, Page, Section, sp};

fn frame(height: f32) -> Modifier {
    Modifier::new()
        .height(height)
        .fill_max_width()
        .border(1.0, theme().outline, 16.0)
        .clip_rounded(16.0)
}

fn colors(i: usize) -> (Color, Color) {
    let th = theme();
    match i % 3 {
        0 => (th.primary_container, th.on_primary_container),
        1 => (th.secondary_container, th.on_secondary_container),
        _ => (th.tertiary_container, th.on_tertiary_container),
    }
}

pub fn screen() -> View {
    Page(vec![
        Section(
            "Vertical ScrollArea",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("A standard scroll container with an internal scrollbar."),
                ScrollArea(
                    frame(260.0),
                    remember_scroll_state("scroll_v"),
                    Column(Modifier::new().fill_max_width().gap(sp::SM).padding(sp::SM)).child(
                        (0..40)
                            .map(|i| {
                                let (bg, fg) = colors(i);
                                DemoTile(format!("Row {i}"), "scroll me", bg, fg, 64.0)
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
            )),
        ),
        Section(
            "Horizontal ScrollArea",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Keyed children keep identity as you scroll sideways."),
                HorizontalScrollArea(
                    frame(150.0),
                    remember_horizontal_scroll_state("scroll_h"),
                    Row(Modifier::new().gap(sp::SM).padding(sp::SM)).child(
                        (0..30)
                            .map(|i| {
                                let (bg, fg) = colors(i);
                                Box(Modifier::new().key(i as u64).width(130.0)).child(DemoTile(
                                    format!("Tile {i}"),
                                    "\u{2192}",
                                    bg,
                                    fg,
                                    110.0,
                                ))
                            })
                            .collect::<Vec<_>>(),
                    ),
                ),
            )),
        ),
        Section(
            "2D ScrollAreaXY",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Free scrolling on both axes over a wide, tall grid."),
                ScrollAreaXY(
                    frame(280.0),
                    remember_scroll_state_xy("scroll_xy"),
                    Grid(
                        10,
                        Modifier::new().padding(sp::SM),
                        (0..140)
                            .map(|i| {
                                let (bg, fg) = colors(i);
                                Box(Modifier::new().key(i as u64).size(120.0, 64.0))
                                    .child(DemoTile(format!("{i}"), "", bg, fg, 64.0))
                            })
                            .collect(),
                        sp::SM,
                        sp::SM,
                    ),
                ),
            )),
        ),
    ])
}
