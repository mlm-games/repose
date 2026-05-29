use repose_core::prelude::*;
use repose_ui::scroll::{
    NestedScrollConnection, ScrollArea, remember_scroll_state,
};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Column(
        Modifier::new()
            .fill_max_width()
            .gap(16.0),
    )
    .child((
        flow_row_demo(),
        overscroll_demo(),
        nested_scroll_demo(),
    ))
}

fn flow_row_demo() -> View {
    const N_COLORS: usize = 6;
    let colors = [0x4285F4, 0xEA4335, 0x34A853, 0xFBBC05, 0x8E24AA, 0x00BCD4];
    Section(
        "FlowRow — auto-wrapping horizontal layout",
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
                    Box(
                        Modifier::new()
                            .background(c)
                            .clip_rounded(8.0)
                            .padding(8.0),
                    )
                    .child(Text(format!("Item {i}")).color(Color::WHITE).size(14.0))
                })
                .collect::<Vec<_>>(),
        ),
    )
}

fn overscroll_demo() -> View {
    Section(
        "OverscrollEffect — rubber-band at scroll boundaries",
        ScrollArea(
            Modifier::new()
                .height(200.0)
                .fill_max_width()
                .border(1.0, theme().outline, 12.0)
                .clip_rounded(12.0),
            remember_scroll_state("overscroll_demo"),
            Column(Modifier::new().fill_max_width().gap(4.0))
                .child(
                    (0..20)
                        .map(|i| {
                            Box(
                                Modifier::new()
                                    .fill_max_width()
                                    .padding(10.0)
                                    .background(theme().surface)
                                    .border(1.0, theme().outline, 10.0)
                                    .clip_rounded(10.0),
                            )
                            .child(Text(format!("Scroll past boundary to see rubber-band — Row {i}")))
                        })
                        .collect::<Vec<_>>(),
                ),
        ),
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
        "NestedScroll — coordinated parent-child scrolling",
        ScrollArea(
            Modifier::new()
                .height(300.0)
                .fill_max_width()
                .border(1.0, theme().outline, 12.0)
                .clip_rounded(12.0),
            outer_state,
            Column(Modifier::new().fill_max_width().gap(8.0)).child((
                // Outer content before the nested scrollable
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .padding(12.0)
                        .background(theme().primary_container)
                        .clip_rounded(8.0),
                )
                .child(
                    Text("Outer scroll (before nested area)")
                        .size(14.0)
                        .color(theme().on_primary_container),
                ),
                // Inner scroll area
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(160.0)
                        .border(1.0, theme().outline_variant, 8.0)
                        .clip_rounded(8.0),
                )
                .child(
                    ScrollArea(
                        Modifier::new().fill_max_size(),
                        inner_state,
                        Column(Modifier::new().fill_max_width().gap(4.0))
                            .child(
                                (0..15)
                                    .map(|i| {
                                        Box(
                                            Modifier::new()
                                                .fill_max_width()
                                                .padding(8.0)
                                                .background(theme().secondary_container)
                                                .clip_rounded(6.0),
                                        )
                                        .child(
                                            Text(format!("Inner item {i}"))
                                                .size(13.0)
                                                .color(theme().on_secondary_container),
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                            ),
                    ),
                ),
                // Outer content after the nested scrollable
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .padding(12.0)
                        .background(theme().tertiary_container)
                        .clip_rounded(8.0),
                )
                .child(
                    Text("Outer scroll (after nested area)")
                        .size(14.0)
                        .color(theme().on_tertiary_container),
                ),
                // Extra content to make outer scroll scrollable
                Box(
                    Modifier::new()
                        .fill_max_width()
                        .padding(12.0)
                        .background(theme().surface_variant)
                        .clip_rounded(8.0),
                )
                .child(
                    Text("More outer content — scroll here when inner reaches its boundary")
                        .size(14.0)
                        .color(theme().on_surface_variant),
                ),
                Box(Modifier::new().height(200.0).fill_max_width())
                    .child(
                        Text("Bottom area — demonstrates outer scroll continuing past inner scrollable")
                            .size(13.0)
                            .color(theme().on_surface.with_alpha(100)),
                    ),
            )),
        ),
    )
}
