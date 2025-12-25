use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Section(
        "Welcome",
        Column(Modifier::new().padding(12.0)).child((
            Text("This is the Home Showcase screen.")
                .size(16.0)
                .color(theme().on_surface),
            Box(Modifier::new().height(8.0).width(1.0)),
            Text("Use the navigation rail on the left to explore features.")
                .size(16.0)
                .color(theme().on_surface),
            Box(Modifier::new().height(16.0).width(1.0)),
            Text("Highlights:").size(16.0).color(theme().on_surface),
            Text("• Typed navigation (repose-navigation)")
                .size(16.0)
                .color(theme().on_surface),
            Text("• Stable identity via Modifier::key")
                .size(16.0)
                .color(theme().on_surface),
            Text("• Scroll, text, canvas, animations, and error boundaries")
                .size(16.0)
                .color(theme().on_surface),
        )),
    )
}
