use repose_core::{prelude::*, signal};
use repose_material::material3::{ButtonConfig, TextButton};
use repose_ui::*;

use crate::ui::{Section, sp};

pub fn screen() -> View {
    let boom = remember(|| signal(false));
    let boom_for_view = boom.clone();

    Section(
        "ErrorBoundary",
        ErrorBoundary(
            |info| {
                let th = theme();
                Box(Modifier::new()
                    .background(th.error)
                    .border(1.0, th.outline, 12.0)
                    .clip_rounded(12.0)
                    .padding(sp::MD))
                .child(Text(format!("Recovered from panic: {}", info.message)))
            },
            move || {
                Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                    if boom_for_view.get() {
                        panic!("Boom from demo component!");
                    } else {
                        Text("Press the button to throw.")
                    },
                    Row(Modifier::new().gap(sp::MD)).child((
                        TextButton(
                            Modifier::new(),
                            {
                                let b = boom.clone();
                                move || b.set(true)
                            },
                            ButtonConfig::default(),
                            || Text("Throw"),
                        ),
                        TextButton(
                            Modifier::new(),
                            {
                                let b = boom.clone();
                                move || b.set(false)
                            },
                            ButtonConfig::default(),
                            || Text("Reset"),
                        ),
                    )),
                ))
            },
        ),
    )
}
