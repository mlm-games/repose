use repose_core::prelude::*;
use repose_ui::{
    lazy::{LazyColumn, LazyColumnState},
    *,
};

#[derive(Clone)]
struct Item {
    id: usize,
    title: String,
    done: bool,
}

pub fn screen() -> View {
    let items = remember_with_key("items", || {
        signal(
            (0..1_000)
                .map(|i| Item {
                    id: i,
                    title: format!("Task #{}", i + 1),
                    done: i % 3 == 0,
                })
                .collect::<Vec<_>>(),
        )
    });
    let scroll = remember_with_key("lazy", || LazyColumnState::new());

    LazyColumn(
        items.get(),
        48.0,
        scroll,
        Modifier::new().max_width(1200.0).max_height(500.0),
        |it, _| {
            let th = theme();
            let done_tint = Color(th.primary.0, th.primary.1, th.primary.2, 48);
            Row(Modifier::new()
                .padding(12.0)
                .background(if it.done { done_tint } else { th.surface })
                .border(1.0, th.outline, 0.0))
            .child((
                Text(if it.done { "✓" } else { "○" }).modifier(Modifier::new().padding(8.0)),
                Text(it.title).modifier(Modifier::new().padding(4.0)),
            ))
        },
    )
}
