use repose_core::{prelude::*, signal};
use repose_material::material3::{ButtonConfig, TextButton};
use repose_ui::*;

use crate::ui::{Section, sp};

pub fn screen() -> View {
    let boom = remember(|| signal(false));

    Section(
        "ErrorBoundary",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            // Controls always composed - never inside the failing leaf, so Reset
            // survives the trip.
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
                    || Text("Clear boom flag"),
                ),
            )),
            ErrorBoundary(
                |info, reset| {
                    let th = theme();
                    Column(
                        Modifier::new()
                            .background(th.error)
                            .border(1.0, th.outline, 12.0)
                            .clip_rounded(12.0)
                            .padding(sp::MD)
                            .gap(sp::SM),
                    )
                    .child((
                        Text(format!("Recovered: {}", info.message)).color(th.on_error),
                        TextButton(
                            Modifier::new(),
                            move || reset(),
                            ButtonConfig::default(),
                            || Text("Reset boundary"),
                        ),
                    ))
                },
                {
                    let boom = boom.clone();
                    move || {
                        if boom.get() {
                            // WASM-safe trip (panic unwinding is unreliable on web);
                            // native catch_unwind remains as a backup path.
                            throw_boundary("Boom from demo component!");
                            // Unreachable placeholder.
                            Text("Hi")
                        } else {
                            Text("Press Throw to trip the boundary.")
                        }
                    }
                },
            ),
        )),
    )
}
