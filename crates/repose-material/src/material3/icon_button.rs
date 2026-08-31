#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, ViewExt};

use super::util::{apply_m3_clickable_ex, icon_content_with_color, with_button_semantics};
use super::*;

/// Color slots for icon buttons (Compose `IconButtonColors`).
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

    /// When caller only sets container, derive a contrasting content color
    /// (Compose `contentColorFor` + local fallback). Prevents white-on-white.
    pub fn ensuring_contrast(mut self) -> Self {
        // Transparent container: content is drawn on parent; keep content as-is.
        if self.container_color.3 == 0 {
            return self;
        }
        let paired = content_color_for(self.container_color);
        // If content is missing contrast vs container, replace with paired/fallback.
        let cl = self.content_color.relative_luminance();
        let bl = self.container_color.relative_luminance();
        if (cl - bl).abs() < 0.25 {
            self.content_color = paired;
        }
        let dcl = self.disabled_content_color.relative_luminance();
        let dbl = self.disabled_container_color.relative_luminance();
        if self.disabled_container_color.3 != 0 && (dcl - dbl).abs() < 0.25 {
            self.disabled_content_color = theme().on_surface.with_alpha_f32(0.38);
        }
        self
    }
}

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
                content_color: Color::TRANSPARENT,
                disabled_container_color: Color::TRANSPARENT,
                disabled_content_color: Color::TRANSPARENT,
            },
            container_size: None,
            interaction_source: None,
            shape_radius: None,
        }
    }
}

fn is_default_colors(c: &IconButtonColors) -> bool {
    c.container_color == Color::TRANSPARENT
        && c.disabled_container_color == Color::TRANSPARENT
        && c.content_color == Color::TRANSPARENT
        && c.disabled_content_color == Color::TRANSPARENT
}

fn resolve_colors(
    config: &IconButtonConfig,
    variant_defaults: IconButtonColors,
) -> IconButtonColors {
    if is_default_colors(&config.colors) {
        return variant_defaults.ensuring_contrast();
    }
    let mut c = config.colors;
    if c.content_color == Color::TRANSPARENT {
        c.content_color = if c.container_color.3 == 0 {
            IconButtonDefaults::content_color()
        } else {
            content_color_for(c.container_color)
        };
    }
    if c.disabled_content_color == Color::TRANSPARENT {
        c.disabled_content_color = theme().on_surface.with_alpha_f32(0.38);
    }
    c.ensuring_contrast()
}

#[allow(clippy::too_many_arguments)]
fn icon_button_render(
    icon: View,
    on_click: impl Fn() + 'static,
    config: &IconButtonConfig,
    colors: IconButtonColors,
    sz: f32,
    bg: Option<Color>,
    bdr: Option<(f32, Color)>,
    state_colors: StateColors,
    ripple_bounded: bool,
) -> View {
    let is_enabled = config.enabled;
    let content_color = colors.content(is_enabled);
    let radius = config.shape_radius.unwrap_or(sz * 0.5);
    let touch = IconButtonDefaults::MIN_INTERACTIVE_SIZE.max(sz);

    let outer = Modifier::new()
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

    let (bounded, r) = if ripple_bounded {
        (true, None)
    } else {
        (false, Some(IconButtonDefaults::STATE_LAYER_RADIUS))
    };

    inner = apply_m3_clickable_ex(
        inner,
        &source,
        content_color,
        is_enabled,
        on_click,
        bounded,
        r,
    );
    inner = with_button_semantics(inner, is_enabled);

    let icon = icon_content_with_color(content_color, icon);

    Box(outer).child(Box(inner).child(icon))
}

/// M3 standard Icon Button (transparent container).
pub fn IconButton(icon: View, on_click: impl Fn() + 'static, config: IconButtonConfig) -> View {
    let colors = resolve_colors(&config, IconButtonDefaults::colors());
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::CONTAINER_SIZE);
    let cc = colors.content(config.enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        colors,
        sz,
        None, // transparent container - no fill
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: cc.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        false,
    )
}

pub fn FilledIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let colors = resolve_colors(&config, IconButtonDefaults::filled_colors());
    let is_enabled = config.enabled;
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    let bg = colors.container(is_enabled);
    let content_color = colors.content(is_enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        colors,
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

pub fn FilledTonalIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let colors = resolve_colors(&config, IconButtonDefaults::filled_tonal_colors());
    let is_enabled = config.enabled;
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    let bg = colors.container(is_enabled);
    let content_color = colors.content(is_enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        colors,
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

pub fn OutlinedIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let colors = resolve_colors(&config, IconButtonDefaults::outlined_colors());
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::CONTAINER_SIZE);
    let border_color = if config.enabled {
        th.outline
    } else {
        th.on_surface.with_alpha_f32(0.12)
    };
    let cc = colors.content(config.enabled);
    icon_button_render(
        icon,
        on_click,
        &config,
        colors,
        sz,
        None,
        Some((1.0, border_color)),
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: cc.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        true,
    )
}
