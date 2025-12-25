use repose_core::{prelude::*, signal};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    let boom = remember(|| signal(false));
    let boom_for_view = boom.clone();

    Section(
        "ErrorBoundary",
        ErrorBoundary(
            |info| {
                Box(Modifier::new()
                    .background(Color::from_hex("#331111"))
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0)
                    .padding(12.0))
                .child(Text(format!("Recovered from panic: {}", info.message)))
            },
            move || {
                Column(Modifier::new().padding(12.0)).child((
                    if boom_for_view.get() {
                        panic!("Boom from demo component!");
                    } else {
                        Text("Press the button to throw.")
                    },
                    Box(Modifier::new().height(12.0).width(1.0)),
                    Row(Modifier::new()).child((
                        Button(Text("Throw"), {
                            let b = boom.clone();
                            move || b.set(true)
                        }),
                        Box(Modifier::new().width(12.0).height(1.0)),
                        Button(Text("Reset"), {
                            let b = boom.clone();
                            move || b.set(false)
                        }),
                    )),
                ))
            },
        ),
    )
}
