use compose_core::prelude::*;
use compose_platform::run_desktop_app;
use compose_ui::*;

fn app(_s: &mut Scheduler) -> View {
    let tf = TextField(
        "Type here",
        Modifier::new()
            .size(300.0, 36.0)
            .background(Color::from_hex("#1E1E1E"))
            .border(1.0, Color::from_hex("#444444"), 6.0)
            .semantics("input"),
    );

    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new().padding(24.0)).child((
            Text("TextField demo").modifier(Modifier::new().padding(4.0)),
            tf,
            Row(Modifier::new()).child(Button("Clear (no-op in this demo)", || {})),
        )),
    )
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app)
}
