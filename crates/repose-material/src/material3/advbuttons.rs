#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Row, Text, TextStyle, ViewExt};
use super::{ButtonGroupDefaults, SplitButtonDefaults};

/// Configuration for [`SplitButtonLayout`].
#[derive(Clone)]
pub struct SplitButtonConfig {
    pub modifier: Modifier,
    pub spacing: f32,
    pub shape_radius: Option<f32>,
    pub container_color: Option<Color>,
}

impl Default for SplitButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            spacing: SplitButtonDefaults::SPACING,
            shape_radius: None,
            container_color: None,
        }
    }
}

/// M3 Split Button Layout - arranges a leading action button and a trailing
/// toggle/menu button side by side. Equivalent to Compose's `SplitButtonLayout`.
///
/// For a "connected" appearance, set `config.shape_radius` and
/// `config.container_color` to wrap both buttons in a shared container.
pub fn SplitButtonLayout(
    leading_button: View,
    trailing_button: View,
    config: SplitButtonConfig,
) -> View {
    let inner = Row(Modifier::new()
        .gap(config.spacing)
        .align_items(AlignItems::Center))
    .child((leading_button, trailing_button));

    if let (Some(radius), Some(color)) = (config.shape_radius, config.container_color) {
        Box(Modifier::new()
            .clip_rounded(radius)
            .background(color)
            .then(config.modifier))
        .child(inner)
    } else {
        Box(config.modifier).child(inner)
    }
}

//
// ButtonGroup
//

/// A single item in a [`ButtonGroup`].
#[derive(Clone)]
pub struct ButtonGroupItem {
    pub label: String,
    pub selected: bool,
    pub on_click: Rc<dyn Fn()>,
}

/// Configuration for [`ButtonGroup`].
#[derive(Clone)]
pub struct ButtonGroupConfig {
    pub modifier: Modifier,
    pub shape_radius: f32,
    pub gap: f32,
    pub color: Color,
    pub selected_color: Color,
    pub text_color: Color,
    pub selected_text_color: Color,
}

impl Default for ButtonGroupConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            shape_radius: ButtonGroupDefaults::SHAPE_RADIUS,
            gap: ButtonGroupDefaults::GAP,
            color: ButtonGroupDefaults::color(),
            selected_color: ButtonGroupDefaults::selected_color(),
            text_color: ButtonGroupDefaults::text_color(),
            selected_text_color: ButtonGroupDefaults::selected_text_color(),
        }
    }
}

/// M3 Button Group - a horizontal row of toggle buttons where one or more
/// can be selected. Equivalent to Compose's `ButtonGroup`/`SingleChoiceButtonGroup`.
pub fn ButtonGroup(
    items: Vec<ButtonGroupItem>,
    config: ButtonGroupConfig,
) -> View {
    Row(config.modifier.gap(config.gap).align_items(AlignItems::Center)).with_children(
        items.into_iter().map(|item| {
            let bg = if item.selected {
                config.selected_color
            } else {
                config.color
            };
            let tc = if item.selected {
                config.selected_text_color
            } else {
                config.text_color
            };
            let cb = item.on_click;
            Box(Modifier::new()
                .clip_rounded(config.shape_radius)
                .background(bg)
                .clickable()
                .on_click(move || (cb)())
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 8.0,
                    bottom: 8.0,
                }))
            .child(Text(item.label).color(tc).single_line())
        }).collect::<Vec<View>>(),
    )
}
