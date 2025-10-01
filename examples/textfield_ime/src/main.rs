use compose_core::{*, prelude::*};
use compose_ui::*;
use compose_platform::run_desktop_app;

fn app(s: &mut Scheduler) -> View {
    // Keep input text in a remembered state (not persistent across process restarts)
    let text = remember(|| signal(String::new()));

    let tf = TextField(0, "Type here")
        .modifier(Modifier::new()
            .size(300.0, 36.0)
            .background(Color::from_hex("#1E1E1E"))
            .border(1.0, Color::from_hex("#444444"), 6.0)
            .semantics("input"));

    Surface(
        Modifier::new().fill_max_size().background(Color::from_hex("#121212")),
        Column(Modifier::new().padding(24.0))
            .child((
                Text("TextField demo").modifier(Modifier::new().padding(4.0)),
                tf,
                Text(format!("You typed: {}", text.get())).modifier(Modifier::new().padding(12.0)),
                Row(Modifier::new()).child((
                    Button("Clear", {
                        let text = text.clone();
                        move || text.set(String::new())
                    }),
                )),
            ))
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app)
}
