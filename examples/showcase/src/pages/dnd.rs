use std::{any::Any, rc::Rc};

use repose_core::{prelude::*, signal};
use repose_ui::*;

use crate::ui::{Hint, Section, sp};

#[derive(Clone, Debug)]
struct DragItem {
    id: u32,
}

fn draggable(id: u32, label: &str) -> View {
    let th = theme();
    Box(Modifier::new()
        .padding(sp::SM)
        .background(th.surface)
        .border(1.0, th.outline, 10.0)
        .clip_rounded(10.0)
        .on_drag_start({
            let item = DragItem { id };
            move |_start| Some(Rc::new(item.clone()) as Rc<dyn Any>)
        }))
    .child(Text(label).color(th.on_surface))
}

pub fn screen() -> View {
    let dropped = remember_with_key("dnd:dropped", || signal("Nothing dropped yet".to_string()));

    let zone = {
        let th = theme();
        let sink = dropped.clone();
        Box(Modifier::new()
            .height(160.0)
            .fill_max_width()
            .background(th.surface)
            .border(2.0, th.outline, 12.0)
            .clip_rounded(12.0)
            .padding(sp::MD)
            .on_drop(move |ev| {
                // 1) Internal item drops
                if let Some(it) = ev.payload.as_ref().downcast_ref::<DragItem>() {
                    sink.set(format!("Dropped DragItem id={}", it.id));
                    return true;
                }
                // 2) File drops (if platform runner provides them)
                if let Some(files) = ev
                    .payload
                    .as_ref()
                    .downcast_ref::<repose_core::dnd::DroppedFiles>()
                {
                    let mut lines = vec![format!("Dropped {} file(s):", files.files.len())];
                    lines.extend(files.files.iter().map(|f| format!("• {} ({:?})", f.name, f.path)));
                    sink.set(lines.join("\n"));
                    return true;
                }
                false
            }))
        .child(Column(Modifier::new().fill_max_size().gap(sp::SM)).child((
            Text("Drop zone").size(16.0).color(th.on_surface),
            Hint("Drag an item here (internal), or drop files from the OS/browser (if supported)."),
            Text(dropped.get())
                .size(12.0)
                .color(th.on_surface_variant)
                .overflow_clip(),
        )))
    };

    Section(
        "Drag & Drop",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            Hint("Internal DnD works on all platforms; file drop depends on runner support."),
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::MD)).child((
                draggable(1, "Drag me (Item 1)"),
                draggable(2, "Drag me (Item 2)"),
            )),
            zone,
        )),
    )
}
