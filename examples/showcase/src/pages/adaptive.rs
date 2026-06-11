use repose_core::prelude::*;
use repose_ui::adaptive::{ListDetailPaneScaffold, ListDetailPaneValue, PaneScaffoldDirective};
use repose_ui::{Box, Column, Row, Text, TextStyle, ViewExt, subcompose_layout_with_slots};

use crate::ui::{Hint, Section, sp};

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
        Column(Modifier::new().padding(sp::LG).gap(sp::XS)).child((
            Text(format!("Width: {:?}", class.width)).size(14.0),
            Text(format!("Height: {:?}", class.height)).size(14.0),
            Hint(if class.is_at_least_medium_width() {
                "Multi-pane layout active (list + detail side by side)."
            } else {
                "Single-pane layout (resize window wider to see the list+detail split)."
            }),
        )),
    );

    let list_pane = {
        let pane = pane.clone();
        Column(Modifier::new().fill_max_size().padding(sp::SM).gap(sp::XS)).child(
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
                    let on_click = move || {
                        sel.set(e.id);
                        pane.set(ListDetailPaneValue::Detail);
                    };
                    Box(Modifier::new()
                        .fill_max_width()
                        .padding(sp::MD)
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

    let detail_pane = {
        let cur = ENTRIES
            .iter()
            .find(|e| e.id == selected.get())
            .copied()
            .unwrap_or(ENTRIES[0]);
        let th = theme();
        let mut children: Vec<View> = Vec::new();
        if directive.max_horizontal_partitions < 2 {
            let pane_back = pane.clone();
            children.push(
                Box(Modifier::new()
                    .padding(sp::SM)
                    .background(th.surface_container)
                    .clip_rounded(6.0)
                    .clickable()
                    .on_pointer_down(move |_| pane_back.set(ListDetailPaneValue::List)))
                .child(Text("<- List").size(13.0).color(th.primary)),
            );
        }
        children.push(Text(cur.title).size(22.0).color(th.on_surface));
        children.push(Text(cur.subtitle).size(14.0).color(th.on_surface_variant));
        children.push(Hint(
            "Resize the window to see the pane split adapt between compact and medium widths.",
        ));
        Column(Modifier::new().fill_max_size().padding(sp::LG).gap(sp::SM)).child(children)
    };

    let scaffold = ListDetailPaneScaffold(
        directive,
        pane.get(),
        move || list_pane.clone(),
        move || detail_pane.clone(),
    );

    let main_col = Column(Modifier::new().flex_grow(1.0).fill_max_height().gap(sp::MD)).child((
        Section("ListDetailPaneScaffold", scaffold),
        multi_slot_demo(),
    ));

    Row(Modifier::new().fill_max_size().padding(sp::MD).gap(sp::MD)).child((
        Box(Modifier::new().width(280.0).fill_max_height()).child(header),
        main_col,
    ))
}

/// Multi-slot `SubcomposeLayout`: stable "header" slot + width-dependent "body" slot.
fn multi_slot_demo() -> View {
    let th = theme();
    let slot = move |label: &'static str, grow: bool| {
        let mut m = Modifier::new()
            .padding(sp::SM)
            .background(th.surface_container_high)
            .clip_rounded(6.0);
        m = if grow {
            m.flex_grow(1.0)
        } else {
            m.fill_max_width()
        };
        Box(m).child(Text(label).size(12.0).color(th.on_surface))
    };

    Section(
        "Multi-slot SubcomposeLayout",
        subcompose_layout_with_slots(
            Modifier::new().fill_max_width().padding(sp::SM),
            move |scope| {
                let wide = scope.max_width.is_finite() && scope.max_width >= 480.0;
                let header = Box(Modifier::new()
                    .fill_max_width()
                    .padding(sp::SM)
                    .background(th.surface_container)
                    .clip_rounded(6.0))
                .child(
                    Text("slot 0: header (stable)")
                        .size(12.0)
                        .color(th.on_surface),
                );
                let body = if wide {
                    Row(Modifier::new().fill_max_width().gap(sp::SM)).child((
                        slot("slot 1: wide left", true),
                        slot("slot 1: wide right", true),
                    ))
                } else {
                    Column(Modifier::new().fill_max_width().gap(sp::SM)).child((
                        slot("slot 1: stacked top", false),
                        slot("slot 1: stacked bottom", false),
                    ))
                };
                vec![(0, header), (1, body)]
            },
        ),
    )
}
