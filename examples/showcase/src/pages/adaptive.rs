use repose_core::prelude::*;
use repose_ui::adaptive::{ListDetailPaneScaffold, ListDetailPaneValue, PaneScaffoldDirective};
use repose_ui::{Box, Column, Row, Text, TextStyle, ViewExt, subcompose_layout_with_slots};

use crate::ui::Section;

#[derive(Clone, Copy)]
struct ListEntry {
    id: u32,
    title: &'static str,
    subtitle: &'static str,
}

const ENTRIES: &[ListEntry] = &[
    ListEntry {
        id: 1,
        title: "Pinned item",
        subtitle: "Last opened yesterday",
    },
    ListEntry {
        id: 2,
        title: "Drafts",
        subtitle: "3 unread",
    },
    ListEntry {
        id: 3,
        title: "Archive",
        subtitle: "20 items",
    },
    ListEntry {
        id: 4,
        title: "Trash",
        subtitle: "Empty",
    },
    ListEntry {
        id: 5,
        title: "Spam",
        subtitle: "Empty",
    },
];

pub fn screen() -> View {
    let class = window_size_class();
    let selected = remember_with_key("adaptive:selected", || signal(0u32));
    let pane = remember_with_key("adaptive:pane", || signal(ListDetailPaneValue::List));
    let directive = PaneScaffoldDirective::from_window_size_class(class);

    let header = Section(
        "Window size class",
        Column(Modifier::new().padding(16.0).gap(4.0)).child((
            Text(format!("Width: {:?}", class.width)).size(14.0),
            Text(format!("Height: {:?}", class.height)).size(14.0),
            Text(if class.is_at_least_medium_width() {
                "Multi-pane layout active (list + detail side by side)."
            } else {
                "Single-pane layout (resize window wider to see the list+detail split)."
            })
            .size(13.0)
            .color(theme().on_surface_variant),
        )),
    );

    let list_pane = {
        let pane = pane.clone();
        Column(Modifier::new().fill_max_size().padding(8.0).gap(4.0)).child(
            ENTRIES
                .iter()
                .map(|e| {
                    let sel = selected.clone();
                    let pane = pane.clone();
                    let is_selected = sel.get() == e.id;
                    let th = theme();
                    let bg = if is_selected {
                        th.surface_container_high
                    } else {
                        th.surface
                    };
                    let on_click = {
                        let sel = sel.clone();
                        let pane = pane.clone();
                        move || {
                            sel.set(e.id);
                            pane.set(ListDetailPaneValue::Detail);
                        }
                    };
                    Box(Modifier::new()
                        .fill_max_width()
                        .padding(12.0)
                        .gap(2.0)
                        .background(bg)
                        .clip_rounded(8.0)
                        .clickable()
                        .on_pointer_down(move |_| on_click()))
                    .child((
                        Text(e.title).size(15.0).color(th.on_surface),
                        Text(e.subtitle).size(12.0).color(th.on_surface_variant),
                    ))
                })
                .collect::<Vec<_>>(),
        )
    };

    let detail_pane =
        {
            let cur = ENTRIES
                .iter()
                .find(|e| e.id == selected.get())
                .copied()
                .unwrap_or(ENTRIES[0]);
            let pane_back = pane.clone();
            let th = theme();
            let back = if directive.max_horizontal_partitions >= 2 {
                None
            } else {
                let pane_back = pane_back.clone();
                Some(
                    Box(Modifier::new()
                        .padding(8.0)
                        .background(th.surface_container)
                        .clip_rounded(6.0)
                        .clickable()
                        .on_pointer_down(move |_| pane_back.set(ListDetailPaneValue::List)))
                    .child(Text("<- List").size(13.0).color(th.primary)),
                )
            };
            let mut children: Vec<View> = Vec::new();
            if let Some(b) = back {
                children.push(b);
            }
            children.push(Text(cur.title).size(22.0).color(th.on_surface));
            children.push(Text(cur.subtitle).size(14.0).color(th.on_surface_variant));
            children.push(
            Text("Resize the window to see the pane split adapt between compact and medium widths.")
                .size(13.0)
                .color(th.on_surface_variant),
        );
            Column(Modifier::new().fill_max_size().padding(16.0).gap(8.0)).child(children)
        };

    let scaffold = ListDetailPaneScaffold(
        directive,
        pane.get(),
        move || list_pane.clone(),
        move || detail_pane.clone(),
    );

    let main_col = Column(Modifier::new().flex_grow(1.0).fill_max_height().gap(12.0)).child((
        Section("ListDetailPaneScaffold", scaffold),
        multi_slot_demo(),
    ));

    Row(Modifier::new().fill_max_size().padding(12.0).gap(12.0)).child((
        Box(Modifier::new().width(280.0).fill_max_height()).child(header),
        main_col,
    ))
}

/// A multi-slot `SubcomposeLayout` with a stable "header" slot and a "body"
/// slot that re-lays-out based on the available width. Slot identity survives
/// across width-driven slot re-runs.
fn multi_slot_demo() -> View {
    let th = theme();
    Section(
        "Multi-slot SubcomposeLayout",
        subcompose_layout_with_slots(
            Modifier::new().fill_max_width().padding(8.0),
            move |scope| {
                let wide = scope.max_width.is_finite() && scope.max_width >= 480.0;
                let header = Box(Modifier::new()
                    .fill_max_width()
                    .padding(8.0)
                    .background(th.surface_container)
                    .clip_rounded(6.0))
                .child(
                    Text("slot 0: header (stable)")
                        .size(12.0)
                        .color(th.on_surface),
                );
                let body = if wide {
                    Row(Modifier::new().fill_max_width().gap(8.0)).child((
                        Box(Modifier::new()
                            .flex_grow(1.0)
                            .padding(8.0)
                            .background(th.surface_container_high)
                            .clip_rounded(6.0))
                        .child(Text("slot 1: wide left").size(12.0).color(th.on_surface)),
                        Box(Modifier::new()
                            .flex_grow(1.0)
                            .padding(8.0)
                            .background(th.surface_container_high)
                            .clip_rounded(6.0))
                        .child(Text("slot 1: wide right").size(12.0).color(th.on_surface)),
                    ))
                } else {
                    Column(Modifier::new().fill_max_width().gap(8.0)).child((
                        Box(Modifier::new()
                            .fill_max_width()
                            .padding(8.0)
                            .background(th.surface_container_high)
                            .clip_rounded(6.0))
                        .child(Text("slot 1: stacked top").size(12.0).color(th.on_surface)),
                        Box(Modifier::new()
                            .fill_max_width()
                            .padding(8.0)
                            .background(th.surface_container_high)
                            .clip_rounded(6.0))
                        .child(
                            Text("slot 1: stacked bottom")
                                .size(12.0)
                                .color(th.on_surface),
                        ),
                    ))
                };
                vec![(0, header), (1, body)]
            },
        ),
    )
}
