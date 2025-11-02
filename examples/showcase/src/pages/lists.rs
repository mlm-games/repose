use compose_core::prelude::*;
use compose_ui::*;

#[derive(Clone)]
struct Item {
    id: usize,
    title: String,
    done: bool,
}

pub fn screen() -> View {
    let items = remember_with_key("items", || {
        signal(
            (0..500)
                .map(|i| Item {
                    id: i,
                    title: format!("Task #{}", i + 1),
                    done: i % 3 == 0,
                })
                .collect::<Vec<_>>(),
        )
    });
    let scroll = remember_with_key("lazy", || {
        std::cell::RefCell::new(compose_ui::lazy::LazyColumnState::new())
    });

    compose_ui::lazy::LazyColumn(
        items.get(),
        48.0,
        scroll,
        Modifier::new().fill_max_size(),
        |it, _| {
            Row(Modifier::new()
                .padding(12.0)
                .background(if it.done {
                    Color::from_hex("#1A3A1A")
                } else {
                    Color::from_hex("#1E1E1E")
                })
                .border(1.0, Color::from_hex("#333333"), 0.0))
            .child((
                Text(if it.done { "✓" } else { "○" }).modifier(Modifier::new().padding(8.0)),
                Text(it.title).modifier(Modifier::new().padding(4.0)),
            ))
        },
    )
}
