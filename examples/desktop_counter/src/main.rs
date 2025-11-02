use compose_core::{prelude::*, signal};
use compose_platform::run_desktop_app;
use compose_ui::*;

fn app(s: &mut Scheduler) -> View {
    let count = remember(|| signal(0i32));
    compose_core::with_theme(
        compose_core::Theme {
            background: Color::from_hex("#FFFFFF"),
            surface: Color::from_hex("#F4F4F4"),
            on_surface: Color::from_hex("#222222"),
            primary: Color::from_hex("#3B82F6"),
            on_primary: Color::WHITE,
        },
        || {
            Surface(
                Modifier::new()
                    .fill_max_size()
                    .background(compose_core::theme().background),
                Column(Modifier::new().padding(24.0).size(300.0, 200.0)).with_children(vec![
                    Text(format!("Count: {}", count.get())).modifier(Modifier::new().padding(12.0)),
                    Button("Increment", {
                        let count = count.clone();
                        move || count.update(|c| *c += 1)
                    })
                    .modifier(Modifier::new().padding(4.0)),
                    Button("Decrement", {
                        let count = count.clone();
                        move || count.update(|c| *c -= 1)
                    })
                    .modifier(Modifier::new().padding(4.0)),
                ]),
            )
        },
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app)
}
