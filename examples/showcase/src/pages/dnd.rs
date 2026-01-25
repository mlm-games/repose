use std::{any::Any, rc::Rc};

use repose_core::{prelude::*, signal};
use repose_ui::*;

use crate::ui::Section;

#[derive(Clone, Debug)]
struct DragItem {
    id: u32,
}

pub fn screen() -> View {
    let dropped = remember_with_key("dnd:dropped", || signal("Nothing dropped yet".to_string()));

    let mk_draggable = |id: u32, label: &str| {
        let th = theme();
        Box(Modifier::new()
            .padding(8.0)
            .background(th.surface)
            .border(1.0, th.outline, 10.0)
            .clip_rounded(10.0)
            .on_drag_start({
                let item = DragItem { id };
                move |_start| Some(Rc::new(item.clone()) as Rc<dyn Any>)
            }))
        .child(Text(label).color(th.on_surface))
    };

    let d2 = dropped.clone();
    let drop_zone = {
        let dropped = dropped.clone();
        let th = theme();

        Box(
            Modifier::new()
                .height(160.0)
                .fill_max_width()
                .background(th.surface)
                .border(2.0, th.outline, 12.0)
                .clip_rounded(12.0)
                .padding(12.0)
                .on_drop(move |ev| {
                    // 1) Internal item drops
                    if let Some(it) = ev.payload.as_ref().downcast_ref::<DragItem>() {
                        d2.set(format!("Dropped DragItem id={}", it.id));
                        return true;
                    }

                    // 2) File drops (if platform runner provides them)
                    if let Some(files) = ev.payload.as_ref().downcast_ref::<repose_core::dnd::DroppedFiles>() {
                        let mut lines = Vec::new();
                        lines.push(format!("Dropped {} file(s):", files.files.len()));
                        for f in &files.files {
                            lines.push(format!("• {} ({:?})", f.name, f.path));
                        }
                        d2.set(lines.join("\n"));
                        return true;
                    }

                    false
                }),
        )
        .child(
            Column(Modifier::new().fill_max_size()).child((
                Text("Drop zone")
                    .size(16.0)
                    .color(th.on_surface),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text("Drag an item here (internal), or drop files from the OS/browser (if supported).")
                    .size(14.0)
                    .color(th.on_surface_variant),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text(dropped.get())
                    .size(12.0)
                    .color(th.on_surface_variant)
                    .overflow_clip(),
            )),
        )
    };

    Section(
        "Drag & Drop",
        Column(Modifier::new().padding(12.0)).child((
            Text("Internal DnD works on all platforms; file drop depends on runner support.")
                .size(14.0)
                .color(theme().on_surface_variant),
            Box(Modifier::new().height(12.0).width(1.0)),
            Row(Modifier::new().align_items(AlignItems::Center)).child((
                mk_draggable(1, "Drag me (Item 1)"),
                Box(Modifier::new().width(12.0).height(1.0)),
                mk_draggable(2, "Drag me (Item 2)"),
            )),
            Box(Modifier::new().height(12.0).width(1.0)),
            drop_zone,
        )),
    )
}
