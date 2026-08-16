#![allow(non_snake_case)]

use std::rc::Rc;

use crate::ripple::{RippleConfig, ripple};
use repose_core::*;
use repose_ui::{Box, ViewExt};

use super::*;

/// Color slots for buttons (matching Compose Material3 `ButtonColors`).
#[derive(Clone, Copy, Debug)]
pub struct ButtonColors {
    pub container_color: Color,
    pub content_color: Color,
    pub disabled_container_color: Color,
    pub disabled_content_color: Color,
}

impl ButtonColors {
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

/// Elevation levels for buttons (matching Compose Material3 `ButtonElevation`).
#[derive(Clone, Copy, Debug)]
pub struct ButtonElevation {
    pub default: f32,
    pub pressed: f32,
    pub focused: f32,
    pub hovered: f32,
    pub disabled: f32,
}

/// Configuration for button components.
#[derive(Clone, Debug)]
pub struct ButtonConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub content_color: Option<Color>,
    pub container_color: Option<Color>,
    pub state_colors: StateColors,
    pub state_elevation: Option<StateElevation>,
    pub border: Option<(f32, Color, f32)>,
    pub shape_radius: f32,
    pub content_padding: Option<PaddingValues>,
    pub height: f32,
    pub colors: Option<ButtonColors>,
    pub elevation: Option<ButtonElevation>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for ButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            content_color: None,
            container_color: None,
            state_colors: ButtonDefaults::state_colors_default(),
            state_elevation: None,
            border: None,
            shape_radius: ButtonDefaults::SHAPE_RADIUS,
            content_padding: None,
            height: ButtonDefaults::HEIGHT,
            colors: None,
            elevation: None,
            interaction_source: None,
        }
    }
}

/// Resolve effective button colors from config, given the variant's default colors.
/// When `config.colors` is set, it takes priority over individual fields.
fn resolve_button_colors(
    config: &ButtonConfig,
    def: ButtonColors,
) -> (Color, Option<Color>, StateColors, Option<StateElevation>) {
    if let Some(colors) = &config.colors {
        let bg = if config.enabled {
            colors.container_color
        } else {
            colors.disabled_container_color
        };
        let cc = if config.enabled {
            colors.content_color
        } else {
            colors.disabled_content_color
        };
        let sc = StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: colors.content_color.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        };
        let se = config.elevation.map(|e| StateElevation {
            default: e.default,
            hovered: e.hovered,
            focused: e.focused,
            pressed: e.pressed,
            dragged: e.pressed,
            disabled: e.disabled,
        });
        (cc, Some(bg), sc, se)
    } else {
        let cc = config.content_color.unwrap_or(def.content_color);
        let bg = Some(config.container_color.unwrap_or(def.container_color));
        let sc = if config.enabled {
            config.state_colors
        } else {
            StateColors {
                default: Color::TRANSPARENT,
                hovered: Color::TRANSPARENT,
                focused: Color::TRANSPARENT,
                pressed: Color::TRANSPARENT,
                dragged: Color::TRANSPARENT,
                disabled: config.state_colors.disabled,
            }
        };
        let se = config.state_elevation;
        (cc, bg, sc, se)
    }
}

fn button_impl(
    outer_modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
    content_color: Color,
    container_color: Option<Color>,
    state_colors: StateColors,
    state_elevation: Option<StateElevation>,
    border: Option<(f32, Color, f32)>,
    pad: PaddingValues,
    height: f32,
    shape_radius: f32,
    enabled: bool,
    interaction_source: Option<MutableInteractionSource>,
) -> View {
    let mut m = Modifier::new()
        .min_height(height)
        .min_width(58.0)
        .flex_shrink(0.0);
    if let Some(bg) = container_color {
        m = m.background(bg);
    }
    m = m.state_colors(if enabled {
        state_colors
    } else {
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: Color::TRANSPARENT,
            disabled: state_colors.disabled,
        }
    });
    if let Some(se) = state_elevation {
        m = m.state_elevation(se);
    }
    if let Some((w, c, r)) = border {
        m = m.border(w, c, r);
    }
    m = m
        .clip_rounded(shape_radius)
        .padding_values(pad)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER);

    // Interaction source + ripple indication
    let source: Rc<MutableInteractionSource> =
        interaction_source
            .map(Rc::new)
            .unwrap_or_else(|| match outer_modifier.key {
                Some(k) => {
                    remember_with_key(format!("m3_btn_src:{k}"), MutableInteractionSource::new)
                }
                None => remember(MutableInteractionSource::new),
            });
    m = m.interaction_source(&source);
    m = m.indication(ripple(RippleConfig {
        color: Some(content_color),
        bounded: true,
        ..Default::default()
    }));

    if enabled {
        m = m.clickable().on_click(on_click);
    } else {
        m = m.enabled(false);
    }
    m = m.then(outer_modifier);
    let content = with_content_color(content_color, content);
    Box(m).child(content)
}

/// M3 Button - prominent action button with primary color fill.
/// (Equivalent to Compose Material3's `Button`.)
pub fn Button(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let def = ButtonColors {
        container_color: ButtonDefaults::container_color(),
        content_color: ButtonDefaults::content_color(),
        disabled_container_color: ButtonDefaults::container_color()
            .with_alpha_f32(0.12)
            .composite_over(theme().surface_container_low),
        disabled_content_color: ButtonDefaults::content_color().with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config.content_padding.unwrap_or(PaddingValues {
        left: 24.0,
        right: 24.0,
        top: 8.0,
        bottom: 8.0,
    });
    button_impl(
        modifier.then(config.modifier),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(ButtonDefaults::state_elevation_default())),
        config.border,
        pad,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Filled Tonal Button - uses secondary container colors.
pub fn FilledTonalButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let def = ButtonColors {
        container_color: ButtonDefaults::tonal_container_color(),
        content_color: ButtonDefaults::tonal_content_color(),
        disabled_container_color: th
            .on_surface
            .with_alpha_f32(0.12)
            .composite_over(th.surface_container_low),
        disabled_content_color: th.on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config.content_padding.unwrap_or(PaddingValues {
        left: 24.0,
        right: 24.0,
        top: 8.0,
        bottom: 8.0,
    });
    button_impl(
        modifier.then(config.modifier),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(ButtonDefaults::state_elevation_default())),
        config.border,
        pad,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Outlined Button - button with an outline border and no fill.
pub fn OutlinedButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let def = ButtonColors {
        container_color: Color::TRANSPARENT,
        content_color: ButtonDefaults::outlined_content_color(),
        disabled_container_color: Color::TRANSPARENT,
        disabled_content_color: th.on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let border = config
        .border
        .unwrap_or((1.0, ButtonDefaults::outlined_border_color(), 20.0));
    let pad = config.content_padding.unwrap_or(PaddingValues {
        left: 24.0,
        right: 24.0,
        top: 8.0,
        bottom: 8.0,
    });
    button_impl(
        modifier.then(config.modifier),
        on_click,
        content,
        cc,
        bg,
        sc,
        se,
        Some(border),
        pad,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Text Button - a low-emphasis button.
pub fn TextButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let def = ButtonColors {
        container_color: Color::TRANSPARENT,
        content_color: ButtonDefaults::text_content_color(),
        disabled_container_color: Color::TRANSPARENT,
        disabled_content_color: th.on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config.content_padding.unwrap_or(PaddingValues {
        left: 12.0,
        right: 12.0,
        top: 8.0,
        bottom: 8.0,
    });
    button_impl(
        modifier.then(config.modifier),
        on_click,
        content,
        cc,
        bg,
        sc,
        se,
        None,
        pad,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Elevated Button - uses `surface_container_low` background with elevation.
pub fn ElevatedButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let def = ButtonColors {
        container_color: ButtonDefaults::elevated_container_color(),
        content_color: ButtonDefaults::elevated_content_color(),
        disabled_container_color: th.on_surface.with_alpha_f32(0.04),
        disabled_content_color: th.on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config.content_padding.unwrap_or(PaddingValues {
        left: 24.0,
        right: 24.0,
        top: 8.0,
        bottom: 8.0,
    });
    button_impl(
        modifier.then(config.modifier),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(ButtonDefaults::elevated_state_elevation())),
        config.border,
        pad,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// Configuration for toggle button components.
#[derive(Clone, Debug)]
pub struct ToggleButtonConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub container_color: Option<Color>,
    pub content_color: Option<Color>,
    pub checked_container_color: Option<Color>,
    pub checked_content_color: Option<Color>,
    pub state_colors: StateColors,
    pub state_elevation: Option<StateElevation>,
    pub border: Option<(f32, Color, f32)>,
    pub shape_radius: f32,
    pub height: f32,
    pub content_padding: Option<PaddingValues>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for ToggleButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            container_color: None,
            content_color: None,
            checked_container_color: None,
            checked_content_color: None,
            state_colors: ToggleButtonDefaults::state_colors_default(),
            state_elevation: None,
            border: None,
            shape_radius: ToggleButtonDefaults::SHAPE_RADIUS,
            height: ToggleButtonDefaults::HEIGHT,
            content_padding: None,
            interaction_source: None,
        }
    }
}

fn toggle_button_impl(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    content: impl FnOnce(bool) -> View,
    content_color: Color,
    container_color: Option<Color>,
    checked_container_color: Option<Color>,
    checked_content_color: Option<Color>,
    state_colors: StateColors,
    state_elevation: StateElevation,
    border: Option<(f32, Color, f32)>,
    pad_left: f32,
    pad_right: f32,
    height: f32,
    shape_radius: f32,
    enabled: bool,
    interaction_source: Option<MutableInteractionSource>,
) -> View {
    let th = theme();
    let bg = if checked {
        checked_container_color.unwrap_or(th.primary)
    } else {
        container_color.unwrap_or(Color::TRANSPARENT)
    };
    let fg = if checked {
        checked_content_color.unwrap_or(th.on_primary)
    } else {
        content_color
    };
    let mut m = Modifier::new()
        .min_height(height)
        .padding_values(PaddingValues {
            left: pad_left,
            right: pad_right,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape_radius)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .state_colors(state_colors)
        .state_elevation(state_elevation);
    let tg_source: Rc<MutableInteractionSource> = interaction_source
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    m = m.interaction_source(&tg_source);
    m = m.indication(ripple(RippleConfig {
        color: Some(fg),
        bounded: true,
        ..Default::default()
    }));
    if let Some((w, c, r)) = border {
        m = m.border(w, c, r);
    }
    if enabled {
        let cb = on_checked_change;
        m = m.clickable().on_click(move || cb(!checked));
    } else {
        m = m.enabled(false);
    }
    with_content_color(fg, || Box(m).child(content(checked)))
}

/// M3 Toggle Button - a button that toggles between checked/unchecked states.
pub fn ToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    config: ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let cc = config
        .content_color
        .unwrap_or_else(ToggleButtonDefaults::content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(ToggleButtonDefaults::checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(ToggleButtonDefaults::checked_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ToggleButtonDefaults::state_elevation_default);
    let pad_l = config
        .content_padding
        .map(|p| p.left)
        .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING);
    let pad_r = config
        .content_padding
        .map(|p| p.right)
        .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING);
    toggle_button_impl(
        checked,
        on_checked_change,
        content,
        cc,
        None,
        Some(checked_bg),
        Some(checked_cc),
        config.state_colors,
        se,
        config.border,
        pad_l,
        pad_r,
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Tonal Toggle Button - uses secondary container colors.
pub fn TonalToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    config: ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let cc = config
        .content_color
        .unwrap_or_else(ToggleButtonDefaults::tonal_content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(ToggleButtonDefaults::tonal_checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(ToggleButtonDefaults::tonal_checked_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ToggleButtonDefaults::state_elevation_default);
    toggle_button_impl(
        checked,
        on_checked_change,
        content,
        cc,
        None,
        Some(checked_bg),
        Some(checked_cc),
        config.state_colors,
        se,
        config.border,
        config
            .content_padding
            .map(|p| p.left)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config
            .content_padding
            .map(|p| p.right)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Outlined Toggle Button - outlined button that toggles between states.
pub fn OutlinedToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    config: ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let cc = config
        .content_color
        .unwrap_or_else(ToggleButtonDefaults::outlined_content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(ToggleButtonDefaults::outlined_checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(ToggleButtonDefaults::outlined_checked_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ToggleButtonDefaults::state_elevation_default);
    let border = if !checked {
        Some(config.border.unwrap_or((
            1.0,
            ToggleButtonDefaults::outlined_border_color(),
            config.shape_radius,
        )))
    } else {
        config.border
    };
    toggle_button_impl(
        checked,
        on_checked_change,
        content,
        cc,
        None,
        Some(checked_bg),
        Some(checked_cc),
        config.state_colors,
        se,
        border,
        config
            .content_padding
            .map(|p| p.left)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config
            .content_padding
            .map(|p| p.right)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}

/// M3 Elevated Toggle Button - elevated button that toggles between states.
pub fn ElevatedToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    config: ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let cc = config
        .content_color
        .unwrap_or_else(ToggleButtonDefaults::elevated_content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(ToggleButtonDefaults::elevated_checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(ToggleButtonDefaults::elevated_checked_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ToggleButtonDefaults::elevated_state_elevation);
    toggle_button_impl(
        checked,
        on_checked_change,
        content,
        cc,
        None,
        Some(checked_bg),
        Some(checked_cc),
        config.state_colors,
        se,
        config.border,
        config
            .content_padding
            .map(|p| p.left)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config
            .content_padding
            .map(|p| p.right)
            .unwrap_or(ToggleButtonDefaults::HORIZONTAL_PADDING),
        config.height,
        config.shape_radius,
        config.enabled,
        config.interaction_source.clone(),
    )
}
