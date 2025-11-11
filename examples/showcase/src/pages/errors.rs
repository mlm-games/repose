use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
    let boom = remember(|| signal(false));
    let boom_for_view = boom.clone();
    let err_view = move || {
        if boom_for_view.get() {
            panic!("Boom from demo component!");
        }
        Text("Everything is fine. Press the button to throw.")
    };

    Section(
        "ErrorBoundary",
        ErrorBoundary(
            |info| {
                Box(Modifier::new()
                    .background(Color::from_hex("#331111"))
                    .border(1.0, theme().outline, 6.0)
                    .padding(12.0))
                .child(Text(format!("Recovered from panic: {}", info.message)))
            },
            move || {
                Column(Modifier::new()).child((
                    err_view(),
                    Button("Throw", {
                        let boom = boom.clone();
                        move || boom.set(true)
                    })
                    .modifier(Modifier::new().padding(8.0)),
                    Button("Reset", {
                        let boom = boom.clone();
                        move || boom.set(false)
                    }),
                ))
            },
        ),
    )
}
