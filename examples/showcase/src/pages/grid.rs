use repose_core::{prelude::*, signal};
use repose_ui::lazy::{LazyGridState, LazyVerticalGrid};
use repose_ui::*;

#[derive(Clone)]
struct GridItem {
    id: usize,
    label: String,
}

pub fn screen() -> View {
    let items = remember_with_key("grid_items", || {
        signal(
            (0..200)
                .map(|i| GridItem {
                    id: i,
                    label: format!("#{}", i + 1),
                })
                .collect::<Vec<_>>(),
        )
    });
    let scroll = remember_with_key("grid_state", LazyGridState::new);

    let it = items.get();
    LazyVerticalGrid(
        4,
        it,
        100.0,
        scroll,
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
            Surface(
                Modifier::new()
                    .fill_max_width()
                    .height(100.0)
                    .background(bg)
                    .clip_rounded(12.0),
                Column(
                    Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::Center),
                )
                .child((
                    Text(item.label).size(20.0).color(th.on_primary_container),
                    Text(format!("id: {}", item.id))
                        .size(12.0)
                        .color(th.on_primary_container.with_alpha(180)),
                )),
            )
        },
    )
}
