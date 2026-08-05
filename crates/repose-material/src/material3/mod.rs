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

use repose_core::{Modifier, View};
use repose_ui::{Box, Column, Row, Spacer, ViewExt};

/// Shared layout helper for alert dialog content.
pub(crate) fn alert_dialog_body(
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
) -> View {
    Column(Modifier::new()).child((
        title,
        Box(Modifier::new().fill_max_width().height(16.0)),
        text,
        Spacer(),
        Row(Modifier::new()).child((
            dismiss_button.unwrap_or(Box(Modifier::new())),
            Spacer(),
            confirm_button,
        )),
    ))
}
