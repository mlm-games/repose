#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, TextStyle, ViewExt};

use super::*;

/// Configuration for [`Card`].
#[derive(Clone, Debug)]
pub struct CardConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub state_elevation: Option<StateElevation>,
    pub border: Option<(f32, Color)>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for CardConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            container_color: CardDefaults::filled_container_color(),
            content_color: CardDefaults::filled_content_color(),
            disabled_container_color: CardDefaults::disabled_container_color(),
            disabled_content_color: CardDefaults::disabled_content_color(),
            shape_radius: CardDefaults::SHAPE_RADIUS,
            tonal_elevation: CardDefaults::ELEVATION,
            state_elevation: None,
            border: None,
            interaction_source: None,
        }
    }
}

/// M3 Card - a configurable container surface.
pub fn Card(config: CardConfig, content: impl FnOnce() -> View) -> View {
    let bg = if !config.enabled {
        config.disabled_container_color
    } else {
        config.container_color
    };
    let fg = if !config.enabled {
        config.disabled_content_color
    } else {
        config.content_color
    };
    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    let mut m = Modifier::new()
        .background(bg)
        .clip_rounded(config.shape_radius)
        .interaction_source(&source)
        .then(config.modifier);
    if let Some((w, c)) = config.border {
        m = m.border(w, c, config.shape_radius);
    }
    if let Some(se) = config.state_elevation {
        m = m.state_elevation(se);
    } else if config.tonal_elevation > 0.0 {
        m = m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: 0.0,
        });
    }
    Box(m).color(fg).child(content())
}

/// M3 Elevated Card - card with elevation.
pub fn ElevatedCard(config: CardConfig, content: impl FnOnce() -> View) -> View {
    let th = theme();
    Card(
        CardConfig {
            container_color: CardDefaults::elevated_container_color(),
            state_elevation: Some(StateElevation {
                default: th.elevation.level1,
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                dragged: th.elevation.level3,
                disabled: 0.0,
            }),
            ..config
        },
        content,
    )
}

/// M3 Outlined Card - card with border outline.
pub fn OutlinedCard(config: CardConfig, content: impl FnOnce() -> View) -> View {
    Card(
        CardConfig {
            container_color: CardDefaults::outlined_container_color(),
            border: Some((1.0, CardDefaults::outlined_border_color())),
            ..config
        },
        content,
    )
}

fn card_state_colors(bg: Color) -> StateColors {
    let th = theme();
    StateColors {
        default: Color::TRANSPARENT,
        hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
        pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
        dragged: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
        disabled: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
    }
}

fn clickable_card_impl(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    bg: Color,
    shape_radius: f32,
    config: CardConfig,
    content: impl FnOnce() -> View,
) -> View {
    let m = modifier
        .state_colors(card_state_colors(bg))
        .clickable()
        .on_pointer_down({
            let cb = on_click;
            let en = config.enabled;
            move |_| {
                if en {
                    cb();
                }
            }
        });
    Card(
        CardConfig {
            modifier: m,
            enabled: config.enabled,
            container_color: bg,
            content_color: config.content_color,
            disabled_container_color: config.disabled_container_color,
            disabled_content_color: config.disabled_content_color,
            shape_radius,
            border: config.border,
            state_elevation: config.state_elevation,
            tonal_elevation: config.tonal_elevation,
            interaction_source: config.interaction_source.clone(),
        },
        || Column(Modifier::new().fill_max_size()).child(content()),
    )
}

/// M3 Clickable Filled Card - interactive card with state coloring.
pub fn ClickableCard(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    config: CardConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    clickable_card_impl(
        on_click,
        modifier,
        th.surface_container_highest,
        th.shapes.medium,
        config,
        content,
    )
}

/// M3 Clickable Elevated Card - interactive card with elevation.
pub fn ClickableElevatedCard(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    config: CardConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let cfg = CardConfig {
        state_elevation: Some(StateElevation {
            default: th.elevation.level1,
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            dragged: th.elevation.level3,
            disabled: 0.0,
        }),
        ..config
    };
    clickable_card_impl(
        on_click,
        modifier,
        th.surface,
        th.shapes.medium,
        cfg,
        content,
    )
}

/// M3 Clickable Outlined Card - interactive card with border.
pub fn ClickableOutlinedCard(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    config: CardConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let cfg = CardConfig {
        border: Some((1.0, th.outline_variant)),
        ..config
    };
    clickable_card_impl(
        on_click,
        modifier,
        th.surface,
        th.shapes.medium,
        cfg,
        content,
    )
}
