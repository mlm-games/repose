use std::rc::Rc;

use repose_core::{prelude::*, signal};
use repose_material::material3::{
    Carousel, SwipeToDismiss, SwipeToDismissConfig, SwipeToDismissState,
};
use repose_material::{Icon, material_symbols};
use repose_ui::{
    lazy::{LazyColumn, LazyRow},
    *,
};

material_symbols! {
    check_circle    : '\u{F0BE}',
    circle          : '\u{EF4A}',
    notifications   : '\u{E7F5}',
}

use crate::ui::{Page, Section};

#[derive(Clone)]
struct Item {
    id: usize,
    title: String,
    done: bool,
}

fn make_items(count: usize) -> Vec<Item> {
    (0..count)
        .map(|i| Item {
            id: i,
            title: format!("Task #{}", i + 1),
            done: i % 3 == 0,
        })
        .collect()
}

/// icon over title card
fn cell_card(m: Modifier, icon: View, title: String, pad: f32) -> View {
    Box(m).child(
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::Center)
                .align_items(AlignItems::Center)
                .padding(pad),
        )
        .child((
            icon,
            Text(title).size(14.0).single_line().overflow_ellipsize(),
        )),
    )
}

pub fn screen() -> View {
    let items = remember_with_key("items", || signal(make_items(1_000)));
    let scroll = remember_with_key("lazy", LazyColumnState::new);
    let row_items = remember_with_key("row_items", || signal(make_items(50)));
    let row_scroll = remember_with_key("lazy_row", LazyRowState::new);
    let carousel_items = remember_with_key("carousel_items", || signal(make_items(20)));
    let carousel_scroll = remember_with_key("carousel_scroll", LazyRowState::new);

    let dismiss_items = remember_with_key("dismiss_items", || {
        signal(
            (0..10)
                .map(|i| format!("Notification #{}", i + 1))
                .collect::<Vec<_>>(),
        )
    });
    let dismiss_states = remember_with_key("dismiss_states", || {
        (0..10)
            .map(|_| Rc::new(SwipeToDismissState::new()))
            .collect::<Vec<_>>()
    });
    let dismissed = remember_with_key("dismissed", || signal(vec![false; 10]));

    let th = theme();

    Page(vec![
        Section("LazyColumn (Vertical)", {
            LazyColumn(
                items.get(),
                48.0,
                scroll,
                Modifier::new().fill_max_width().max_height(400.0),
                |it: &Item| it.id as u64,
                None,
                move |it, _| {
                    let th = theme();
                    let done_tint = th.primary.with_alpha(48);
                    Row(Modifier::new()
                        .fill_max_width()
                        .padding(12.0)
                        .background(if it.done { done_tint } else { th.surface })
                        .border(1.0, th.outline, 0.0))
                    .child((
                        (if it.done {
                            Icon(Symbols::check_circle)
                        } else {
                            Icon(Symbols::circle)
                        })
                        .size(16.0)
                        .modifier(Modifier::new().padding(8.0)),
                        Text(it.title).modifier(Modifier::new().padding(4.0)),
                    ))
                },
            )
        }),
        Section("LazyColumn (Heterogeneous heights)", {
            let hetero_items: Vec<Item> = (0..200)
                .map(|i| Item {
                    id: i + 10_000,
                    title: format!("Row #{}", i + 1),
                    done: i % 2 == 0,
                })
                .collect();
            let hetero_scroll = remember_with_key("lazy_hetero", LazyColumnState::new);
            LazyColumn(
                hetero_items,
                |it: &Item| 48.0 + (it.id % 5) as f32 * 16.0,
                hetero_scroll,
                Modifier::new().fill_max_width().max_height(400.0),
                |it: &Item| it.id as u64,
                None,
                move |it, _| {
                    let th = theme();
                    let bg = if it.done {
                        th.primary.with_alpha(48)
                    } else {
                        th.surface_container
                    };
                    let h = 48.0 + (it.id % 5) as f32 * 16.0;
                    Box(Modifier::new()
                        .fill_max_width()
                        .height(h)
                        .background(bg)
                        .border(1.0, th.outline_variant, 0.0)
                        .padding(12.0)
                        .justify_content(JustifyContent::Center))
                    .child(Text(format!("{} (height = {}dp)", it.title, h as i32)))
                },
            )
        }),
        Section("LazyRow (Horizontal)", {
            LazyRow(
                row_items.get(),
                120.0,
                row_scroll,
                Modifier::new().fill_max_width().height(160.0),
                move |it, _| {
                    let th = theme();
                    let bg = if it.done {
                        th.primary.with_alpha(48)
                    } else {
                        th.surface_container
                    };
                    cell_card(
                        Modifier::new()
                            .width(120.0)
                            .height(140.0)
                            .background(bg)
                            .border(1.0, th.outline, 0.0)
                            .clip_rounded(12.0),
                        if it.done {
                            Icon(Symbols::check_circle).size(24.0)
                        } else {
                            Icon(Symbols::circle).size(24.0)
                        },
                        it.title,
                        8.0,
                    )
                },
            )
        }),
        Section("Carousel (LazyRow + Peek)", {
            Carousel(
                carousel_items.get(),
                160.0,
                24.0,
                Modifier::new().fill_max_width().height(180.0),
                carousel_scroll,
                move |it, _| {
                    let th = theme();
                    cell_card(
                        Modifier::new()
                            .fill_max_width()
                            .height(160.0)
                            .background(th.primary.with_alpha(32))
                            .clip_rounded(16.0),
                        Text(if it.done { "★" } else { "☆" }).size(32.0),
                        it.title,
                        12.0,
                    )
                },
            )
        }),
        Section("SwipeToDismiss", {
            let vis: Vec<View> = dismiss_items
                .get()
                .into_iter()
                .enumerate()
                .filter(|(i, _)| !dismissed.get()[*i])
                .map(|(i, msg)| {
                    let state = dismiss_states[i].clone();
                    let is_dismissed = dismissed.clone();
                    SwipeToDismiss(
                        state,
                        Some(Rc::new(move || {
                            let mut d = is_dismissed.get();
                            d[i] = true;
                            is_dismissed.set(d);
                        })),
                        // Background (revealed behind content when swiping)
                        Box(Modifier::new()
                            .fill_max_width()
                            .fill_max_height()
                            .background(th.error)
                            .padding(16.0)
                            .justify_content(JustifyContent::End)
                            .align_items(AlignItems::Center))
                        .child(Text("Delete").color(th.on_error).size(16.0)),
                        // Foreground content (draggable)
                        Box(Modifier::new()
                            .fill_max_width()
                            .background(th.surface_container)
                            .border(1.0, th.outline_variant, 0.0)
                            .padding(16.0))
                        .child(
                            Row(Modifier::new().align_items(AlignItems::Center)).child((
                                Icon(Symbols::notifications).size(20.0),
                                Box(Modifier::new().width(12.0).height(1.0)),
                                Column(Modifier::new()).child((
                                    Text(msg)
                                        .size(th.typography.body_large)
                                        .color(th.on_surface),
                                    Text("Swipe left to dismiss")
                                        .size(th.typography.body_small)
                                        .color(th.on_surface_variant),
                                )),
                            )),
                        ),
                        Modifier::new().fill_max_width(),
                        SwipeToDismissConfig::default(),
                    )
                })
                .collect();

            if vis.is_empty() {
                Column(
                    Modifier::new()
                        .padding(24.0)
                        .align_items(AlignItems::Center),
                )
                .child(
                    Text("All dismissed! 🎉")
                        .size(18.0)
                        .color(th.on_surface_variant),
                )
            } else {
                Column(Modifier::new().fill_max_width()).with_children(vis)
            }
        }),
    ])
}
