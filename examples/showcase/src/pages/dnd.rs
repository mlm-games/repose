use std::{any::Any, rc::Rc};

use repose_core::{prelude::*, signal};
use repose_material::{Icon, material_symbols};
use repose_ui::*;

use crate::ui::{Caption, Hint, Page, Section, sp};

material_symbols! {
    drag_indicator : '\u{E945}',
    inbox          : '\u{E156}',
    task_alt       : '\u{E2E6}',
    upload_file    : '\u{EAF3}',
}

#[derive(Clone, Debug)]
struct Task {
    id: u32,
    title: &'static str,
    tag: &'static str,
}

fn task_card(task: Task) -> View {
    let th = theme();
    let accent = match task.id % 3 {
        0 => th.primary,
        1 => th.tertiary,
        _ => th.secondary,
    };

    Box(Modifier::new()
        .width(220.0)
        .padding(sp::MD)
        .background(th.surface_container)
        .border(1.0, th.outline_variant, sp::MD)
        .clip_rounded(sp::MD)
        .on_drag_start({
            let t = task.clone();
            move |_| Some(Rc::new(t.clone()) as Rc<dyn Any>)
        }))
    .child(
        Column(Modifier::new().gap(sp::SM)).child((
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                Icon(Symbols::drag_indicator)
                    .size(16.0)
                    .color(th.on_surface_variant),
                Text(task.title).size(14.0).color(th.on_surface),
            )),
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                Box(Modifier::new()
                    .padding(4.0)
                    .background(accent.with_alpha(36))
                    .clip_rounded(999.0))
                .child(Text(task.tag).size(10.0).color(accent)),
                Spacer(),
                Caption(format!("#{}", task.id)),
            )),
        )),
    )
}

pub fn screen() -> View {
    let dropped = remember_with_key("dnd:dropped", || signal("Nothing dropped yet.".to_string()));

    let drop_zone = {
        let th = theme();
        let sink = dropped.clone();
        Box(Modifier::new()
            .fill_max_width()
            .height(180.0)
            .background(th.primary.with_alpha(18))
            .border(2.0, th.primary.with_alpha(140), sp::LG)
            .clip_rounded(sp::LG)
            .padding(sp::LG)
            .on_drop(move |ev| {
                if let Some(task) = ev.payload.as_ref().downcast_ref::<Task>() {
                    sink.set(format!(
                        "Received task \"{}\" (#{}, tag {})",
                        task.title, task.id, task.tag
                    ));
                    return true;
                }
                if let Some(files) = ev
                    .payload
                    .as_ref()
                    .downcast_ref::<repose_core::dnd::DroppedFiles>()
                {
                    let mut lines = vec![format!("Dropped {} file(s):", files.files.len())];
                    lines.extend(
                        files
                            .files
                            .iter()
                            .map(|f| format!("• {} ({:?})", f.name, f.path)),
                    );
                    sink.set(lines.join("\n"));
                    return true;
                }
                false
            }))
        .child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER)
                    .gap(sp::SM),
            )
            .child((
                Icon(Symbols::inbox).size(28.0).color(th.primary),
                Text("Drop here").size(16.0).color(th.on_surface),
                Caption("Drop a task card or an OS file"),
            )),
        )
    };

    let status = {
        let th = theme();
        Box(Modifier::new()
            .fill_max_width()
            .padding(sp::MD)
            .background(th.surface_container)
            .border(1.0, th.outline_variant, sp::MD)
            .clip_rounded(sp::MD))
        .child(
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                Icon(Symbols::task_alt).size(16.0).color(th.primary),
                Text(dropped.get()).size(13.0).color(th.on_surface),
            )),
        )
    };

    Page(vec![
        Section(
            "Drag tasks",
            Column(Modifier::new().gap(sp::MD)).child((
                Hint("Grab a card from Backlog and drop it into the target zone below."),
                Row(Modifier::new().gap(sp::MD)).child((
                    task_card(Task {
                        id: 1,
                        title: "Refine hero layout",
                        tag: "design",
                    }),
                    task_card(Task {
                        id: 2,
                        title: "Wire nav rail",
                        tag: "shell",
                    }),
                    task_card(Task {
                        id: 3,
                        title: "Snackbar polish",
                        tag: "ux",
                    }),
                )),
            )),
        ),
        Section(
            "Drop target",
            Column(Modifier::new().gap(sp::MD)).child((
                Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                    Icon(Symbols::upload_file)
                        .size(16.0)
                        .color(theme().on_surface_variant),
                    Hint("Internal cards and OS file drops both land here."),
                )),
                drop_zone,
                status,
            )),
        ),
    ])
}
