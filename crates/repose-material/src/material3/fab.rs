#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Row, Text, TextStyle, ViewExt};

use super::util::apply_enabled_click;
use super::*;

/// Configuration for FAB components.
#[derive(Clone, Debug)]
pub struct FABConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub container_color: Color,
    pub content_color: Color,
    pub state_elevation: StateElevation,
    pub shape_radius: f32,
    pub size: f32,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for FABConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            container_color: FABDefaults::container_color(),
            content_color: FABDefaults::content_color(),
            state_elevation: FABDefaults::state_elevation(),
            shape_radius: FABDefaults::SHAPE_RADIUS,
            size: FABDefaults::SIZE,
            interaction_source: None,
        }
    }
}

fn fab_impl(
    icon: View,
    on_click: impl Fn() + 'static,
    size: f32,
    shape_r: f32,
    config: FABConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let bg = if is_enabled {
        config.container_color
    } else {
        th.on_surface
            .with_alpha_f32(0.12)
            .composite_over(th.surface_container_low)
    };
    let content_color = if is_enabled {
        config.content_color
    } else {
        th.on_surface.with_alpha_f32(0.38)
    };

    let elev = if is_enabled {
        config.state_elevation
    } else {
        StateElevation {
            default: 0.0,
            hovered: 0.0,
            focused: 0.0,
            pressed: 0.0,
            dragged: 0.0,
            disabled: 0.0,
        }
    };

    let mut m = Modifier::new()
        .size(size, size)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: config.content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(elev)
        .clip_rounded(shape_r)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    m = m.interaction_source(&source);
    m = m.indication(crate::ripple::ripple(crate::ripple::RippleConfig {
        color: Some(content_color),
        bounded: true,
        ..Default::default()
    }));
    m = apply_enabled_click(m, is_enabled, on_click);

    Box(m).child(with_content_color(content_color, move || icon))
}

/// M3 Floating Action Button (regular, 56dp).
pub fn FAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(
        icon,
        on_click,
        FABDefaults::SIZE,
        FABDefaults::SHAPE_RADIUS,
        config,
    )
}

/// M3 Small FAB (40dp).
pub fn SmallFAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(
        icon,
        on_click,
        FABDefaults::SMALL_SIZE,
        FABDefaults::SMALL_SHAPE_RADIUS,
        config,
    )
}

/// M3 Large FAB (96dp).
pub fn LargeFAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(
        icon,
        on_click,
        FABDefaults::LARGE_SIZE,
        FABDefaults::LARGE_SHAPE_RADIUS,
        config,
    )
}

/// M3 Extended FAB - FAB with icon + label.
pub fn ExtendedFAB(
    icon: Option<View>,
    label: impl Into<String>,
    on_click: impl Fn() + 'static,
    config: FABConfig,
) -> View {
    let th = theme();
    let has_icon = icon.is_some();
    let is_enabled = config.enabled;
    let bg = if is_enabled {
        config.container_color
    } else {
        th.on_surface
            .with_alpha_f32(0.12)
            .composite_over(th.surface_container_low)
    };
    let content_color = if is_enabled {
        config.content_color
    } else {
        th.on_surface.with_alpha_f32(0.38)
    };

    let elev = if is_enabled {
        config.state_elevation
    } else {
        StateElevation {
            default: 0.0,
            hovered: 0.0,
            focused: 0.0,
            pressed: 0.0,
            dragged: 0.0,
            disabled: 0.0,
        }
    };

    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .height(56.0)
        .min_width(80.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: config.content_color.with_alpha_f32(0.12),
            disabled: theme().on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(elev)
        .clip_rounded(FABDefaults::SHAPE_RADIUS)
        .padding_values(PaddingValues {
            left: 16.0,
            right: 20.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::CENTER);

    m = m.interaction_source(&source);
    m = m.indication(crate::ripple::ripple(crate::ripple::RippleConfig {
        color: Some(content_color),
        bounded: true,
        ..Default::default()
    }));
    m = apply_enabled_click(m, is_enabled, on_click);
    m = m.then(config.modifier);
    Row(m).child((
        icon.map(|v| with_content_color(content_color, move || v))
            .unwrap_or(Box(Modifier::new())),
        Box(Modifier::new()
            .width(if has_icon { 12.0 } else { 0.0 })
            .fill_max_height()),
        Text(label)
            .color(content_color)
            .size(th.typography.label_large)
            .single_line(),
    ))
}
