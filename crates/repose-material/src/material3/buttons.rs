#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, ViewExt};

use super::util::apply_m3_clickable;
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
    let colors = config.colors.unwrap_or(ButtonColors {
        container_color: config.container_color.unwrap_or(def.container_color),
        content_color: config.content_color.unwrap_or(def.content_color),
        disabled_container_color: def.disabled_container_color,
        disabled_content_color: def.disabled_content_color,
    });

    let bg = colors.container(config.enabled);
    let cc = colors.content(config.enabled);

    let sc = if config.enabled {
        let mut sc = config.state_colors;
        if sc.dragged.3 == 0 {
            sc.dragged = colors.content_color.with_alpha_f32(0.12);
        }
        sc
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

    let se = config
        .elevation
        .map(|e| StateElevation {
            default: e.default,
            hovered: e.hovered,
            focused: e.focused,
            pressed: e.pressed,
            dragged: e.pressed,
            disabled: e.disabled,
        })
        .or(config.state_elevation);

    (cc, Some(bg), sc, se)
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
        .min_width(ButtonDefaults::MIN_WIDTH)
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

    // Interaction source + ripple indication (content-tinted, bounded — Compose Button)
    let source: Rc<MutableInteractionSource> =
        interaction_source
            .map(Rc::new)
            .unwrap_or_else(|| match outer_modifier.key {
                Some(k) => {
                    remember_with_key(format!("m3_btn_src:{k}"), MutableInteractionSource::new)
                }
                None => remember(MutableInteractionSource::new),
            });

    m = apply_m3_clickable(m, &source, content_color, enabled, on_click);
    m = m.then(outer_modifier);
    let content = with_content_color(content_color, || {
        with_text_size(theme().typography.label_large, content)
    });
    Box(m).child(content).semantics(Semantics {
        role: Role::Button,
        label: None,
        focused: false,
        enabled,
        selectable_group: false,
    })
}

/// M3 Button - prominent action button with primary color fill.
/// (Equivalent to Compose Material3's `Button`.)
pub fn Button(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let def = ButtonColors {
        container_color: ButtonDefaults::container_color(),
        content_color: ButtonDefaults::content_color(),
        disabled_container_color: th
            .on_surface
            .with_alpha_f32(0.12)
            .composite_over(th.surface),
        disabled_content_color: th.on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config
        .content_padding
        .unwrap_or(ButtonDefaults::CONTENT_PADDING);
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
    let pad = config
        .content_padding
        .unwrap_or(ButtonDefaults::CONTENT_PADDING);
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
    let border = config.border.unwrap_or_else(|| {
        let c = if config.enabled {
            ButtonDefaults::outlined_border_color()
        } else {
            th.on_surface.with_alpha_f32(0.12)
        };
        (1.0, c, config.shape_radius)
    });
    let pad = config
        .content_padding
        .unwrap_or(ButtonDefaults::CONTENT_PADDING);
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
    let pad = config
        .content_padding
        .unwrap_or(ButtonDefaults::TEXT_CONTENT_PADDING);
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
    let pad = config
        .content_padding
        .unwrap_or(ButtonDefaults::CONTENT_PADDING);
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
    let disabled_bg = {
        let enabled_bg = if checked {
            checked_container_color.unwrap_or(th.primary)
        } else {
            container_color.unwrap_or(Color::TRANSPARENT)
        };
        if enabled_bg.3 == 0 {
            Color::TRANSPARENT
        } else {
            th.on_surface
                .with_alpha_f32(0.12)
                .composite_over(th.surface)
        }
    };
    let bg = if !enabled {
        disabled_bg
    } else if checked {
        checked_container_color.unwrap_or(th.primary)
    } else {
        container_color.unwrap_or(Color::TRANSPARENT)
    };
    let fg = if !enabled {
        th.on_surface.with_alpha_f32(0.38)
    } else if checked {
        checked_content_color.unwrap_or(th.on_primary)
    } else {
        content_color
    };
    let border = border.map(|(w, c, r)| {
        if !enabled {
            (w, th.on_surface.with_alpha_f32(0.12), r)
        } else {
            (w, c, r)
        }
    });
    let se = if enabled {
        state_elevation
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
        .state_elevation(se);
    let tg_source: Rc<MutableInteractionSource> = interaction_source
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    if let Some((w, c, r)) = border {
        m = m.border(w, c, r);
    }
    let cb = on_checked_change;
    m = apply_m3_clickable(m, &tg_source, fg, enabled, move || cb(!checked));
    with_content_color(fg, || {
        with_text_size(theme().typography.label_large, || {
            Box(m).child(content(checked)).semantics(Semantics {
                role: Role::Button,
                label: None,
                focused: false,
                enabled,
                selectable_group: false,
            })
        })
    })
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
