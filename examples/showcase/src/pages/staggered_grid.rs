use repose_core::{prelude::*, signal};
use repose_ui::lazy::LazyVerticalStaggeredGrid;
use repose_ui::LazyVerticalStaggeredGridState;
use repose_ui::*;

use crate::ui::DemoTile;

#[derive(Clone)]
struct StaggeredItem {
    id: usize,
    height: f32,
}

pub fn screen() -> View {
    let items = remember_with_key("stagg_items", || {
        signal(
            (0..50)
                .map(|i| StaggeredItem {
                    id: i,
                    height: 60.0 + (i as f32 * 17.0) % 140.0,
                })
                .collect::<Vec<_>>(),
        )
    });
    let state = remember_with_key("stagg_state", LazyVerticalStaggeredGridState::new);

    LazyVerticalStaggeredGrid(
        3,
        items.get(),
        |item| item.height,
        state,
        Modifier::new()
            .fill_max_width()
            .max_width(800.0)
            .fill_max_height()
            .max_height(500.0)
            .gap(8.0),
        move |item, _| {
            let th = theme();
            let bg = match item.id % 3 {
                0 => th.primary_container,
                1 => th.secondary_container,
                _ => th.tertiary_container,
            };
            DemoTile(
                format!("#{}", item.id + 1),
                format!("h: {:.0}", item.height),
                bg,
                th.on_primary_container,
                item.height,
            )
        },
    )
}
