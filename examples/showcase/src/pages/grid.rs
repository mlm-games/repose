use repose_core::{prelude::*, signal};
use repose_ui::lazy::{LazyGridState, LazyVerticalGrid};
use repose_ui::*;

use crate::ui::DemoTile;

#[derive(Clone)]
struct GridItem {
    id: usize, // label is derived at render time
}

pub fn screen() -> View {
    let items = remember_with_key("grid_items", || {
        signal((0..200).map(|id| GridItem { id }).collect::<Vec<_>>())
    });
    let state = remember_with_key("grid_state", LazyGridState::new);

    LazyVerticalGrid(
        4,
        items.get(),
        100.0,
        state,
        Modifier::new()
            .fill_max_width()
            .max_width(800.0)
            .fill_max_height()
            .max_height(500.0)
            .gap(8.0),
        move |item, _| {
            let th = theme();
            let bg = if item.id % 2 == 0 {
                th.primary_container
            } else {
                th.secondary_container
            };
            DemoTile(
                format!("#{}", item.id + 1),
                format!("id: {}", item.id),
                bg,
                th.on_primary_container,
                100.0,
            )
        },
    )
}
