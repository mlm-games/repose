use compose_core::*;
use compose_platform::run_desktop_app;
use compose_ui::*;

#[derive(Clone)]
struct Todo {
    id: usize,
    title: String,
    completed: bool,
}

fn app(_s: &mut Scheduler) -> View {
    // Generate large list
    let todos = remember_with_key("todos", || {
        signal(
            (0..1000)
                .map(|i| Todo {
                    id: i,
                    title: format!("Task #{}", i + 1),
                    completed: i % 3 == 0,
                })
                .collect::<Vec<_>>(),
        )
    });

    let scroll_state = remember_with_key("scroll", || {
        std::cell::RefCell::new(compose_ui::lazy::LazyColumnState::new())
    });

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new()).child((
            // Header
            Box(Modifier::new()
                .padding(16.0)
                .background(Color::from_hex("#1E1E1E")))
            .child(Text(format!(
                "📋 {} Tasks (Virtualized)",
                todos.get().len()
            ))),
            // Virtualized list
            compose_ui::lazy::LazyColumn(
                todos.get(),
                50.0, // Item height
                scroll_state,
                Modifier::new().fill_max_size(),
                |todo, _idx| {
                    Row(Modifier::new()
                        .padding(12.0)
                        .background(if todo.completed {
                            Color::from_hex("#1A3A1A")
                        } else {
                            Color::from_hex("#1E1E1E")
                        })
                        .border(1.0, Color::from_hex("#333333"), 0.0))
                    .child((
                        Text(if todo.completed { "✓" } else { "○" })
                            .modifier(Modifier::new().padding(8.0)),
                        Text(todo.title.clone()).modifier(Modifier::new().padding(4.0)),
                    ))
                },
            ),
        )),
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    log::info!("Starting LazyColumn Example v0.2");
    run_desktop_app(app)
}
