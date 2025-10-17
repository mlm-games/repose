use compose_core::{prelude::*, signal};
use compose_platform::run_desktop_app;
use compose_ui::*;

fn app(s: &mut Scheduler) -> View {
    let count = remember(|| signal(0i32));

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#221628")),
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
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app)
}
