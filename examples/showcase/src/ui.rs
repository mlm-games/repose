#![allow(non_snake_case)]

use repose_core::prelude::*;
use repose_ui::{material3::components::Card, *};

/// A titled section with consistent spacing.
pub fn Section(title: &str, body: View) -> View {
    Column(Modifier::new().padding(8.0)).child((
        Text(title)
            .color(theme().on_surface)
            .size(18.0)
            .modifier(Modifier::new().padding(4.0)),
        Card(Modifier::new().fill_max_width(), true, body),
    ))
}

/// Top bar with a subtle bottom divider.
pub fn TopBar() -> View {
    Row(Modifier::new()
        .padding(12.0)
        .background(theme().surface)
        .border(1.0, theme().outline, 0.0))
}

pub fn Page(body: View) -> View {
    Row(Modifier::new().fill_max_size()).child((
        Spacer(),
        Box(Modifier::new().fill_max_size().max_width(1200.0)).child(body),
        Spacer(),
    ))
}
