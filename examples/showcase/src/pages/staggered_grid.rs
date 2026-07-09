use repose_core::{prelude::*, signal};
use repose_material::material3::{Button, ButtonConfig, TextButton};
use repose_ui::LazyVerticalStaggeredGridState;
use repose_ui::lazy::LazyVerticalStaggeredGrid;
use repose_ui::*;

use crate::ui::{DemoTile, Hint, Page, Section, sp};

#[derive(Clone)]
struct StaggeredItem {
    id: usize,
    height: f32,
}

fn tile_colors(id: usize) -> (Color, Color) {
    let th = theme();
    match id % 3 {
        0 => (th.primary_container, th.on_primary_container),
        1 => (th.secondary_container, th.on_secondary_container),
        _ => (th.tertiary_container, th.on_tertiary_container),
    }
}

fn make(n: usize) -> Vec<StaggeredItem> {
    (0..n)
        .map(|i| StaggeredItem {
            id: i,
            height: 80.0 + (i as f32 * 37.0) % 160.0,
        })
        .collect()
}

pub fn screen() -> View {
    let items = remember_with_key("stagg_items", || signal(make(50)));
    let state = remember_with_key("stagg_state", LazyVerticalStaggeredGridState::new);

    Page(vec![Section(
        "LazyVerticalStaggeredGrid",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            Hint("Masonry-style layout: each cell keeps its natural height, columns fill independently."),
            Row(Modifier::new().gap(sp::SM).align_items(AlignItems::CENTER)).child((
                Button(Modifier::new(), {
                    let items = items.clone();
                    move || items.update(|v| {
                        let id = v.len();
                        v.push(StaggeredItem { id, height: 80.0 + (id as f32 * 37.0) % 160.0 });
                    })
                }, ButtonConfig::default(), || Text("Add tile")),
                TextButton(Modifier::new(), {
                    let items = items.clone();
                    move || items.update(|v| { v.pop(); })
                }, ButtonConfig::default(), || Text("Remove")),
                Spacer(),
                Text(format!("{} tiles", items.get().len()))
                    .size(13.0)
                    .color(theme().on_surface_variant),
            )),
            LazyVerticalStaggeredGrid(
                3,
                items.get(),
                |item| item.height,
                |item, _| {
                    let (bg, fg) = tile_colors(item.id);
                    DemoTile(format!("#{}", item.id + 1), format!("{:.0}dp", item.height), bg, fg, item.height)
                },
                LazyVerticalStaggeredGridConfig {
                    state,
                    modifier: Modifier::new()
                        .fill_max_width()
                        .max_width(820.0)
                        .fill_max_height()
                        .max_height(520.0)
                        .gap(sp::SM),
                    ..Default::default()
                },
            ),
        )),
    )])
}