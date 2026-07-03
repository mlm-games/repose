use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::{Hint, Section, sp};

pub fn screen() -> View {
    let th = theme();
    let bullet = move |text: &'static str| {
        Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
            Box(Modifier::new()
                .size(6.0, 6.0)
                .background(th.primary)
                .clip_rounded(3.0)),
            Text(text).size(15.0).color(th.on_surface),
        ))
    };

    Section(
        "Welcome",
        Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
            Text("This is the Home Showcase screen.")
                .size(16.0)
                .color(th.on_surface),
            Hint("Use the navigation rail on the left to explore features."),
            Text("Highlights").size(16.0).color(th.on_surface),
            bullet("Typed navigation (repose-navigation)"),
            bullet("Stable identity via Modifier::key"),
            bullet("Scroll, text, canvas, animations, and error boundaries"),
        )),
    )
}
