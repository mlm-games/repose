use repose_core::{prelude::*, signal};
use repose_ui::lazy::{LazyHorizontalGrid, LazyVerticalGrid};
use repose_ui::*;

use crate::ui::{DemoTile, Hint, Page, Section, compact_layout, sp};

#[derive(Clone)]
struct GridItem {
    id: usize,
}

fn tile_colors(id: usize) -> (Color, Color) {
    let th = theme();
    match id % 3 {
        0 => (th.primary_container, th.on_primary_container),
        1 => (th.secondary_container, th.on_secondary_container),
        _ => (th.tertiary_container, th.on_tertiary_container),
    }
}

pub fn screen() -> View {
    let items = remember_with_key("grid_items", || {
        signal((0..200).map(|id| GridItem { id }).collect::<Vec<_>>())
    });
    let vert_state = remember_with_key("grid_state", LazyGridState::new);
    let horiz_state = remember_with_key("grid_h_state", LazyGridState::new);
    let compact = compact_layout();
    let columns = if compact { 1 } else { 4 };
    let horizontal_columns = if compact { 1 } else { 3 };

    Page(vec![
        Section(
            "LazyVerticalGrid",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    if compact {
                        "Virtualized single-column grid over 200 items -> only visible cells are composed."
                    } else {
                        "Virtualized 4-column grid over 200 items -> only visible cells are composed."
                    },
                ),
                LazyVerticalGrid(
                    columns,
                    items.get(),
                    100.0,
                    |item, _| {
                        let (bg, fg) = tile_colors(item.id);
                        DemoTile(
                            format!("#{}", item.id + 1),
                            format!("id {}", item.id),
                            bg,
                            fg,
                            Dp(100.0),
                        )
                    },
                    LazyGridConfig {
                        state: vert_state,
                        modifier: Modifier::new()
                            .fill_max_width()
                            .max_width(Dp(820.0))
                            .fill_max_height()
                            .max_height(Dp(460.0))
                            .gap(sp::SM),
                        ..Default::default()
                    },
                ),
            )),
        ),
        Section(
            "LazyHorizontalGrid",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(if compact {
                    "Same data laid out in one compact column, scrolling horizontally."
                } else {
                    "Same data laid out in 3 fixed rows, scrolling horizontally."
                }),
                LazyHorizontalGrid(
                    horizontal_columns,
                    items.get(),
                    120.0,
                    |item, _| {
                        let (bg, fg) = tile_colors(item.id);
                        DemoTile(
                            format!("#{}", item.id + 1),
                            format!("id {}", item.id),
                            bg,
                            fg,
                            Dp(60.0),
                        )
                    },
                    LazyGridConfig {
                        state: horiz_state,
                        modifier: Modifier::new()
                            .fill_max_height()
                            .max_height(Dp(320.0))
                            .fill_max_width()
                            .max_width(Dp(820.0))
                            .gap(sp::SM),
                        ..Default::default()
                    },
                ),
            )),
        ),
    ])
}
