use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::{PullToRefresh, PullToRefreshState};
use repose_ui::scroll::{NestedScrollConnection, ScrollArea, remember_scroll_state};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Column(Modifier::new().fill_max_width().gap(16.0)).child((
        flow_row_demo(),
        overscroll_demo(),
        nested_scroll_demo(),
        pull_to_refresh_demo(),
    ))
}

fn flow_row_demo() -> View {
    const N_COLORS: usize = 6;
    let colors = [0x4285F4, 0xEA4335, 0x34A853, 0xFBBC05, 0x8E24AA, 0x00BCD4];
    Section(
        "FlowRow - auto-wrapping horizontal layout",
        FlowRow(Modifier::new().gap(8.0).fill_max_width()).child(
            (0..25)
                .map(|i| {
                    let hex = colors[i as usize % N_COLORS];
                    let c = Color::from_rgba(
                        ((hex >> 16) & 0xFF) as u8,
                        ((hex >> 8) & 0xFF) as u8,
                        (hex & 0xFF) as u8,
                        200,
                    );
                    Box(Modifier::new().background(c).clip_rounded(8.0).padding(8.0))
                        .child(Text(format!("Item {i}")).color(Color::WHITE).size(14.0))
                })
                .collect::<Vec<_>>(),
        ),
    )
}

fn overscroll_demo() -> View {
    Section(
        "OverscrollEffect - rubber-band at scroll boundaries",
        ScrollArea(
            Modifier::new()
                .height(200.0)
                .fill_max_width()
                .border(1.0, theme().outline, 12.0)
                .clip_rounded(12.0),
            remember_scroll_state("overscroll_demo"),
            Column(Modifier::new().fill_max_width().gap(4.0)).child(
                (0..20)
                    .map(|i| {
                        Box(Modifier::new()
                            .fill_max_width()
                            .padding(10.0)
                            .background(theme().surface)
                            .border(1.0, theme().outline, 10.0)
                            .clip_rounded(10.0))
                        .child(Text(format!(
                            "Scroll past boundary to see rubber-band - Row {i}"
                        )))
                    })
                    .collect::<Vec<_>>(),
            ),
        ),
    )
}

fn pull_to_refresh_demo() -> View {
    let scroll_state = remember_scroll_state("ptr_demo");
    let ptr_rc = remember(|| Rc::new(PullToRefreshState::new()));
    ptr_rc.set_scroll_state(scroll_state.clone());
    let count = remember(|| signal(0u32));
    let refreshing = remember(|| signal(false));
    let frame_counter = remember(|| std::cell::Cell::new(0u32));
    let ptr_inner: &Rc<PullToRefreshState> = &*ptr_rc;

    // Auto-complete refresh after ~120 frames (≈2s at 60fps)
    if ptr_inner.is_refreshing() {
        let c = frame_counter.get() + 1;
        frame_counter.set(c);
        if c > 120 {
            ptr_inner.set_refreshing(false);
            frame_counter.set(0);
            refreshing.set(true);
        } else {
            request_frame();
        }
    } else {
        frame_counter.set(0);
    }

    // Simulate new data arriving when refresh completes
    let items: Vec<View> = if refreshing.get() {
        refreshing.set(false);
        // Regenerate items with a timestamp
        let ts = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        (0..20)
            .map(|i| {
                let label = if i == 0 {
                    format!("✓ Refreshed at {}", ts % 100000)
                } else {
                    format!("List item {}", i)
                };
                Box(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(theme().surface)
                    .border(1.0, theme().outline, 8.0)
                    .clip_rounded(8.0))
                .child(Text(label).size(14.0).color(theme().on_surface))
            })
            .collect()
    } else {
        (0..20)
            .map(|i| {
                Box(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(theme().surface)
                    .border(1.0, theme().outline, 8.0)
                    .clip_rounded(8.0))
                .child(
                    Text(format!("List item {i}"))
                        .size(14.0)
                        .color(theme().on_surface),
                )
            })
            .collect()
    };

    Section(
        "PullToRefresh - pull down to refresh",
        Column(Modifier::new().padding(12.0)).child((
            Text(format!("Refreshed {} times", count.get()))
                .size(14.0)
                .color(theme().on_surface_variant),
            Box(Modifier::new().height(4.0).width(1.0)),
            ScrollArea(
                Modifier::new()
                    .height(250.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                scroll_state,
                PullToRefresh(
                    ptr_inner.clone(),
                    Modifier::new().fill_max_width(),
                    Rc::new({
                        let c = count.clone();
                        let p = ptr_inner.clone();
                        let f = frame_counter.clone();
                        move || {
                            c.update(|x| *x += 1);
                            p.set_refreshing(true);
                            f.set(0);
                            request_frame();
                        }
                    }),
                    Column(Modifier::new().fill_max_width().gap(4.0).padding(4.0)).child(items),
                ),
            ),
        )),
    )
}

fn nested_scroll_demo() -> View {
    // Create outer and inner scroll states with nested scroll connection
    let outer_state = remember_scroll_state("nested_outer");
    let inner_state = remember_scroll_state("nested_inner");

    // Wire up nested scroll: inner scrolls first, overscroll propagates to outer
    let conn = NestedScrollConnection::new();
    inner_state.set_nested_scroll_parent(conn);

    Section(
        "NestedScroll - coordinated parent-child scrolling",
        ScrollArea(
            Modifier::new()
                .height(300.0)
                .fill_max_width()
                .border(1.0, theme().outline, 12.0)
                .clip_rounded(12.0),
            outer_state,
            Column(Modifier::new().fill_max_width().gap(8.0)).child((
                // Outer content before the nested scrollable
                Box(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(theme().primary_container)
                    .clip_rounded(8.0))
                .child(
                    Text("Outer scroll (before nested area)")
                        .size(14.0)
                        .color(theme().on_primary_container),
                ),
                // Inner scroll area
                Box(Modifier::new()
                    .fill_max_width()
                    .height(160.0)
                    .border(1.0, theme().outline_variant, 8.0)
                    .clip_rounded(8.0))
                .child(ScrollArea(
                    Modifier::new().fill_max_size(),
                    inner_state,
                    Column(Modifier::new().fill_max_width().gap(4.0)).child(
                        (0..15)
                            .map(|i| {
                                Box(Modifier::new()
                                    .fill_max_width()
                                    .padding(8.0)
                                    .background(theme().secondary_container)
                                    .clip_rounded(6.0))
                                .child(
                                    Text(format!("Inner item {i}"))
                                        .size(13.0)
                                        .color(theme().on_secondary_container),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                )),
                // Outer content after the nested scrollable
                Box(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(theme().tertiary_container)
                    .clip_rounded(8.0))
                .child(
                    Text("Outer scroll (after nested area)")
                        .size(14.0)
                        .color(theme().on_tertiary_container),
                ),
                // Extra content to make outer scroll scrollable
                Box(Modifier::new()
                    .fill_max_width()
                    .padding(12.0)
                    .background(theme().surface_variant)
                    .clip_rounded(8.0))
                .child(
                    Text("More outer content - scroll here when inner reaches its boundary")
                        .size(14.0)
                        .color(theme().on_surface_variant),
                ),
                Box(Modifier::new().height(200.0).fill_max_width()).child(
                    Text(
                        "Bottom area - demonstrates outer scroll continuing past inner scrollable",
                    )
                    .size(13.0)
                    .color(theme().on_surface.with_alpha(100)),
                ),
            )),
        ),
    )
}
