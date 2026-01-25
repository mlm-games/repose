use repose_core::{prelude::*, signal};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    let last_submit_single = remember_with_key("text_last_submit_single", || signal(String::new()));
    let last_change_single = remember_with_key("text_last_change_single", || signal(String::new()));
    let last_submit_multi = remember_with_key("text_last_submit_multi", || signal(String::new()));
    let last_change_multi = remember_with_key("text_last_change_multi", || signal(String::new()));

    Column(Modifier::new().fill_max_width()).child((
        Section(
            "TextField (single-line)",
            Column(Modifier::new().padding(12.0)).child((
                TextField(
                    "Type here",
                    Modifier::new()
                        .height(40.0)
                        .fill_max_width()
                        .background(theme().surface)
                        .border(1.0, theme().outline, 10.0)
                        .clip_rounded(10.0),
                    Some({
                        let last_change = last_change_single.clone();
                        move |s| last_change.set(s)
                    }),
                    Some({
                        let last_submit = last_submit_single.clone();
                        move |s| last_submit.set(s)
                    }),
                ),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text("Single-line: Enter submits.")
                    .size(14.0)
                    .color(theme().on_surface_variant),
                Text(format!("last change: {}", last_change_single.get()))
                    .size(12.0)
                    .color(theme().on_surface_variant),
                Text(format!("last submit: {}", last_submit_single.get()))
                    .size(12.0)
                    .color(theme().on_surface_variant),
            )),
        ),
        Section(
            "TextArea (multi-line)",
            Column(Modifier::new().padding(12.0)).child((
                TextArea(
                    "Write notes…",
                    Modifier::new()
                        .height(180.0)
                        .fill_max_width()
                        .background(theme().surface)
                        .border(1.0, theme().outline, 10.0)
                        .clip_rounded(10.0),
                    Some({
                        let last_change = last_change_multi.clone();
                        move |s| last_change.set(s)
                    }),
                    Some({
                        let last_submit = last_submit_multi.clone();
                        move |s| last_submit.set(s)
                    }),
                ),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text("Multi-line: Enter inserts newline. Cmd/Ctrl+Enter submits (if wired).")
                    .size(14.0)
                    .color(theme().on_surface_variant),
                Text(format!("last change: {}", last_change_multi.get()))
                    .size(12.0)
                    .color(theme().on_surface_variant),
                Text(format!("last submit: {}", last_submit_multi.get()))
                    .size(12.0)
                    .color(theme().on_surface_variant),
            )),
        ),
        Section(
            "Wrapping + Ellipsis",
            Column(Modifier::new().padding(12.0)).child((
                Text("Single-line label that ellipsizes when it runs out of space.")
                    .single_line()
                    .overflow_ellipsize()
                    .modifier(Modifier::new().fill_max_width()),
                Box(Modifier::new().height(12.0).width(1.0)),
                Text("This paragraph demonstrates wrapping in a constrained box. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Vestibulum at arcu sed justo viverra posuere.")
                    .size(16.0)
                    .modifier(Modifier::new().width(420.0)),
            )),
        ),
    ))
}
