#![allow(non_snake_case)]

use repose_core::*;
use repose_ui::Box;

use super::*;

/// Configuration for divider components.
#[derive(Clone, Debug)]
pub struct DividerConfig {
    pub modifier: Modifier,
    pub thickness: f32,
    pub color: Color,
}

impl Default for DividerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            thickness: DividerDefaults::THICKNESS,
            color: DividerDefaults::color(),
        }
    }
}

/// M3 Horizontal Divider - a thin 1dp line.
/// (Equivalent to Compose Material3's `HorizontalDivider`.)
pub fn HorizontalDivider(config: DividerConfig) -> View {
    Box(Modifier::new()
        .min_width(200.0)
        .height(config.thickness)
        .background(config.color)
        .then(config.modifier))
}

#[deprecated(since = "0.19.5", note = "renamed to HorizontalDivider")]
pub fn Divider(config: DividerConfig) -> View {
    HorizontalDivider(config)
}

/// M3 Vertical Divider - a thin 1dp vertical line.
pub fn VerticalDivider(config: DividerConfig) -> View {
    Box(Modifier::new()
        .width(config.thickness)
        .fill_max_height()
        .background(config.color)
        .then(config.modifier))
}
