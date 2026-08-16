#![allow(non_snake_case)]

mod util;

pub mod advbuttons;
mod app_bar;
mod badge;
mod bottom_sheet;
mod buttons;
mod card;
mod carousel;
mod chips;
mod date_picker;
pub mod defaults;
pub mod dialog;
mod divider;
mod dropdown_menu;
mod fab;
mod icon_button;
mod list_item;
mod nav_bar;
mod nav_drawer;
mod nav_rail;
mod progress;
mod pull_to_refresh;
mod scaffold;
mod search_bar;
mod segmented_button;
mod selection;
mod slider;
mod snackbar;
mod surface;
mod swipe;
mod tab_row;
mod text_field;
mod time_picker;
mod tooltip;

pub use advbuttons::*;
pub use app_bar::*;
pub use badge::*;
pub use bottom_sheet::*;
pub use buttons::*;
pub use card::*;
pub use carousel::*;
pub use chips::*;
pub use date_picker::*;
pub use defaults::*;
pub use dialog::*;
pub use divider::*;
pub use dropdown_menu::*;
pub use fab::*;
pub use icon_button::*;
pub use list_item::*;
pub use nav_bar::*;
pub use nav_drawer::*;
pub use nav_rail::*;
pub use progress::*;
pub use pull_to_refresh::*;
pub use scaffold::*;
pub use search_bar::*;
pub use segmented_button::*;
pub use selection::*;
pub use slider::*;
pub use snackbar::*;
pub use surface::*;
pub use swipe::*;
pub use tab_row::*;
pub use text_field::*;
pub use time_picker::*;
pub use tooltip::*;

use repose_core::{
    JustifyContent, Modifier, PaddingValues, Theme, View, with_local_indication, with_theme,
};
use repose_ui::{Box, Column, Row, ViewExt};

use crate::ripple::default_ripple;

/// Wrap a subtree with a `Theme` and install the default M3 ripple indication.
///
/// Mirrors Compose's Material theme: it provides `LocalIndication` so plain
/// `.clickable()` surfaces receive a ripple with no per-component wiring.
pub fn MaterialTheme(theme: Theme, content: impl FnOnce() -> View) -> View {
    with_theme(theme, || with_material_indication(content))
}

/// Install the default M3 ripple indication (`LocalIndication`) for a subtree.
pub fn with_material_indication<R>(f: impl FnOnce() -> R) -> R {
    with_local_indication(Some(default_ripple()), f)
}

/// Shared layout helper for alert dialog content.
pub(crate) fn alert_dialog_body(
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
) -> View {
    Column(
        Modifier::new()
            .padding_values(PaddingValues {
                left: 24.0,
                right: 24.0,
                top: 24.0,
                bottom: 24.0,
            })
            .fill_max_width(),
    )
    .child((
        title,
        Box(Modifier::new().fill_max_width().height(16.0)),
        text,
        Box(Modifier::new().fill_max_width().height(24.0)),
        Row(Modifier::new()
            .fill_max_width()
            .justify_content(JustifyContent::FLEX_END)
            .gap(8.0))
        .child((
            dismiss_button.unwrap_or(Box(Modifier::new())),
            confirm_button,
        )),
    ))
}
