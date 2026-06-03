use repose_core::{prelude::*, signal};
use repose_ui::lazy::{LazyVerticalStaggeredGrid, LazyVerticalStaggeredGridState};
use repose_ui::*;

#[derive(Clone)]
struct StaggeredItem {
    id: usize,
    label: String,
    height: f32,
}

pub fn screen() -> View {
    let items = remember_with_key("stagg_items", || {
        signal(
            (0..50)
                .map(|i| StaggeredItem {
                    id: i,
                    label: format!("#{}", i + 1),
                    height: 60.0 + (i as f32 * 17.0) % 140.0,
                })
                .collect::<Vec<_>>(),
        )
    });
    let state = remember_with_key("stagg_state", LazyVerticalStaggeredGridState::new);

    let it = items.get();
    LazyVerticalStaggeredGrid(
        3,
        it,
        |item| item.height,
        state,
        Modifier::new().fill_max_width().max_width(800.0).fill_max_height().max_height(500.0).gap(8.0),
        move |item, _| {
            let th = theme();
            let bg = if item.id % 3 == 0 {
                th.primary_container
            } else if item.id % 3 == 1 {
                th.secondary_container
            } else {
                th.tertiary_container
            };
            Surface(
                Modifier::new()
                    .fill_max_width()
                    .height(item.height)
                    .background(bg)
                    .clip_rounded(12.0),
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::Center),
                )
                .child((
                    Text(item.label)
                        .size(20.0)
                        .color(th.on_primary_container),
                    Text(format!("h: {:.0}", item.height))
                        .size(12.0)
                        .color(th.on_primary_container.with_alpha(180)),
                )),
            )
        },
    )
}
