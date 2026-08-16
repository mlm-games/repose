#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, ViewExt};

use super::util::apply_m3_clickable_ex;
use super::*;

/// Color slots for icon buttons.
#[derive(Clone, Copy, Debug)]
pub struct IconButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
}

impl IconButtonColors {
    pub fn container(&self, enabled: bool) -> Color {
        if enabled {
            self.container_color
        } else {
            self.disabled_container_color
        }
    }
    pub fn content(&self, enabled: bool) -> Color {
        if enabled {
            self.content_color
        } else {
            self.disabled_content_color
        }
    }
}

/// Configuration for [`IconButton`], [`FilledIconButton`], [`FilledTonalIconButton`], and [`OutlinedIconButton`].
#[derive(Clone, Debug)]
pub struct IconButtonConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub colors: IconButtonColors,
    pub container_size: Option<f32>,
    pub interaction_source: Option<MutableInteractionSource>,
    pub shape_radius: Option<f32>,
}

impl Default for IconButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            colors: IconButtonColors {
                container_color: Color::TRANSPARENT,
                content_color: IconButtonDefaults::content_color(),
                disabled_container_color: Color::TRANSPARENT,
                disabled_content_color: IconButtonDefaults::disabled_content_color(),
            },
            container_size: None,
            interaction_source: None,
            shape_radius: None,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn icon_button_render(
    icon: View,
    on_click: impl Fn() + 'static,
    config: &IconButtonConfig,
    sz: f32,
    bg: Option<Color>,
    bdr: Option<(f32, Color)>,
    state_colors: StateColors,
    ripple_bounded: bool,
) -> View {
    let is_enabled = config.enabled;
    let content_color = config.colors.content(is_enabled);
    let radius = config.shape_radius.unwrap_or(sz * 0.5);

    let touch = IconButtonDefaults::MIN_INTERACTIVE_SIZE.max(sz);
    let mut outer = Modifier::new()
        .size(touch, touch)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier.clone());

    let mut inner = Modifier::new()
        .size(sz, sz)
        .clip_rounded(radius)
        .state_colors(state_colors)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER);

    if let Some(bg_color) = bg {
        inner = inner.background(bg_color);
    }
    if let Some((w, c)) = bdr {
        inner = inner.border(w, c, radius);
    }

    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    outer = apply_m3_clickable_ex(
        outer,
        &source,
        content_color,
        is_enabled,
        on_click,
        ripple_bounded,
        Some(if ripple_bounded {
            radius
        } else {
            IconButtonDefaults::STATE_LAYER_RADIUS
        }),
    );

    Box(outer).child(Box(inner).child(with_content_color(content_color, move || icon)))
}

/// M3 Icon Button - a tappable circular container for an icon.
pub fn IconButton(icon: View, on_click: impl Fn() + 'static, config: IconButtonConfig) -> View {
    let th = theme();
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::CONTAINER_SIZE);
    icon_button_render(
        icon,
        on_click,
        &config,
        sz,
        None,
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        false,
    )
}

/// M3 Filled Icon Button - icon button with a filled container background.
pub fn FilledIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    mut config: IconButtonConfig,
) -> View {
    let th = theme();
    if config.colors.container_color == Color::TRANSPARENT
        && config.colors.disabled_container_color == Color::TRANSPARENT
    {
        config.colors = IconButtonColors {
            container_color: IconButtonDefaults::filled_container_color(),
            content_color: IconButtonDefaults::filled_content_color(),
            disabled_container_color: th
                .on_surface
                .with_alpha_f32(0.12)
                .composite_over(th.surface),
            disabled_content_color: th.on_surface.with_alpha_f32(0.38),
        };
    }
    let is_enabled = config.enabled;
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    let bg = config.colors.container(is_enabled);
    let content_color = config.colors.content(is_enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        sz,
        Some(bg),
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
        true,
    )
}

/// M3 Filled Tonal Icon Button - icon button with a secondary container background.
pub fn FilledTonalIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    mut config: IconButtonConfig,
) -> View {
    let th = theme();
    if config.colors.container_color == Color::TRANSPARENT
        && config.colors.disabled_container_color == Color::TRANSPARENT
    {
        config.colors = IconButtonColors {
            container_color: IconButtonDefaults::filled_tonal_container_color(),
            content_color: IconButtonDefaults::filled_tonal_content_color(),
            disabled_container_color: th
                .on_surface
                .with_alpha_f32(0.12)
                .composite_over(th.surface),
            disabled_content_color: th.on_surface.with_alpha_f32(0.38),
        };
    }
    let is_enabled = config.enabled;
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    let bg = config.colors.container(is_enabled);
    let content_color = config.colors.content(is_enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        sz,
        Some(bg),
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
        true,
    )
}

/// M3 Outlined Icon Button - icon button with a transparent background and border.
pub fn OutlinedIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::CONTAINER_SIZE);
    let border_color = if config.enabled {
        th.outline
    } else {
        th.on_surface.with_alpha_f32(0.12)
    };
    icon_button_render(
        icon,
        on_click,
        &config,
        sz,
        None,
        Some((1.0, border_color)),
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        true,
    )
}
