use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::{PullToRefresh, PullToRefreshState};
use repose_ui::scroll::{NestedScrollConnection, ScrollArea, remember_scroll_state};
use repose_ui::*;

use crate::ui::{Hint, Page, Section, sp};

fn list_card(label: String) -> View {
    Box(Modifier::new()
        .fill_max_width()
        .padding(10.0)
        .background(theme().surface)
        .border(1.0, theme().outline, 10.0)
        .clip_rounded(10.0))
    .child(Text(label))
}

pub fn screen() -> View {
    Page(vec![
        flow_row_demo(),
        overscroll_demo(),
        nested_scroll_demo(),
        pull_to_refresh_demo(),
    ])
}

fn flow_row_demo() -> View {
    const COLORS: [u32; 6] = [0x4285F4, 0xEA4335, 0x34A853, 0xFBBC05, 0x8E24AA, 0x00BCD4];
    Section(
        "FlowRow - auto-wrapping horizontal layout",
        FlowRow(Modifier::new().gap(sp::SM).fill_max_width()).child(
            (0..25)
                .map(|i| {
                    let hex = COLORS[i as usize % COLORS.len()];
                    let c = Color::from_rgba(
                        ((hex >> 16) & 0xFF) as u8,
                        ((hex >> 8) & 0xFF) as u8,
                        (hex & 0xFF) as u8,
                        200,
                    );
                    Box(Modifier::new()
                        .background(c)
                        .clip_rounded(8.0)
                        .padding(sp::SM))
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
            Column(Modifier::new().fill_max_width().gap(sp::XS)).child(
                (0..20)
                    .map(|i| {
                        list_card(format!("Scroll past boundary to see rubber-band - Row {i}"))
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
    let ptr: &Rc<PullToRefreshState> = &ptr_rc;

    // Auto-complete refresh after ~120 frames (≈2s at 60fps)
    if ptr.is_refreshing() {
        let c = frame_counter.get() + 1;
        frame_counter.set(c);
        if c > 120 {
            ptr.set_refreshing(false);
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
                    .padding(sp::MD)
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
                    .padding(sp::MD)
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
        Column(Modifier::new().padding(sp::MD).gap(sp::XS)).child((
            Hint(format!("Refreshed {} times", count.get())),
            ScrollArea(
                Modifier::new()
                    .height(250.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                scroll_state,
                PullToRefresh(
                    ptr.clone(),
                    Modifier::new().fill_max_width(),
                    Rc::new({
                        let c = count.clone();
                        let p = ptr.clone();
                        let f = frame_counter.clone();
                        move || {
                            c.update(|x| *x += 1);
                            p.set_refreshing(true);
                            f.set(0);
                            request_frame();
                        }
                    }),
                    Column(Modifier::new().fill_max_width().gap(sp::XS).padding(sp::XS))
                        .child(items),
                ),
            ),
        )),
    )
}

fn nested_scroll_demo() -> View {
    let outer_state = remember_scroll_state("nested_outer");
    let inner_state = remember_scroll_state("nested_inner");
    inner_state.set_nested_scroll_parent(NestedScrollConnection::new());

    let banner = |bg: Color, fg: Color, text: &'static str| {
        Box(Modifier::new()
            .fill_max_width()
            .padding(sp::MD)
            .background(bg)
            .clip_rounded(8.0))
        .child(Text(text).size(14.0).color(fg))
    };

    let th = theme();
    Section(
        "NestedScroll - coordinated parent-child scrolling",
        ScrollArea(
            Modifier::new()
                .height(300.0)
                .fill_max_width()
                .border(1.0, th.outline, 12.0)
                .clip_rounded(12.0),
            outer_state,
            Column(Modifier::new().fill_max_width().gap(sp::SM)).child((
                banner(
                    th.primary_container,
                    th.on_primary_container,
                    "Outer scroll (before nested area)",
                ),
                Box(Modifier::new()
                    .fill_max_width()
                    .height(160.0)
                    .border(1.0, th.outline_variant, 8.0)
                    .clip_rounded(8.0))
                .child(ScrollArea(
                    Modifier::new().fill_max_size(),
                    inner_state,
                    Column(Modifier::new().fill_max_width().gap(sp::XS)).child(
                        (0..15)
                            .map(|i| {
                                Box(Modifier::new()
                                    .fill_max_width()
                                    .padding(sp::SM)
                                    .background(th.secondary_container)
                                    .clip_rounded(6.0))
                                .child(
                                    Text(format!("Inner item {i}"))
                                        .size(13.0)
                                        .color(th.on_secondary_container),
                                )
                            })
                            .collect::<Vec<_>>(),
                    ),
                )),
                banner(
                    th.tertiary_container,
                    th.on_tertiary_container,
                    "Outer scroll (after nested area)",
                ),
                banner(
                    th.surface_variant,
                    th.on_surface_variant,
                    "More outer content - scroll here when inner reaches its boundary",
                ),
                Box(Modifier::new().height(200.0).fill_max_width()).child(
                    Text(
                        "Bottom area - demonstrates outer scroll continuing past inner scrollable",
                    )
                    .size(13.0)
                    .color(th.on_surface.with_alpha(100)),
                ),
            )),
        ),
    )
}
