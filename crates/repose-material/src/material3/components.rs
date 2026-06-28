#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::Duration;

use repose_core::animation::{AnimationSpec, CubicBezier, Easing, KeyframesSpec, RepeatableSpec};
use repose_core::*;
use repose_ui::anim::{animate_color, animate_f32};
use repose_ui::{Box, Column, Row, Stack, Text, TextStyle, ViewExt};

use super::*;

use crate::{Icon, Symbol};

/// Color slots for [`TopAppBar`].
#[derive(Clone, Copy, Debug)]
pub struct TopAppBarColors {
    pub container_color: Color,
    pub navigation_icon_content_color: Color,
    pub title_content_color: Color,
    pub action_icon_content_color: Color,
}

impl Default for TopAppBarColors {
    fn default() -> Self {
        Self {
            container_color: TopAppBarDefaults::container_color(),
            navigation_icon_content_color: TopAppBarDefaults::navigation_icon_content_color(),
            title_content_color: TopAppBarDefaults::title_content_color(),
            action_icon_content_color: TopAppBarDefaults::action_icon_content_color(),
        }
    }
}

/// Configuration for [`TopAppBar`].
#[derive(Clone, Debug)]
pub struct TopAppBarConfig {
    pub modifier: Modifier,
    pub colors: TopAppBarColors,
    pub height: f32,
    pub window_insets: WindowInsets,
    pub content_padding: PaddingValues,
}

/// System window insets for top app bar padding.
#[derive(Clone, Copy, Debug)]
pub struct WindowInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Default for WindowInsets {
    fn default() -> Self {
        Self {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        }
    }
}

impl Default for TopAppBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: TopAppBarColors::default(),
            height: TopAppBarDefaults::HEIGHT,
            window_insets: WindowInsets::default(),
            content_padding: PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            },
        }
    }
}

fn top_app_bar_layout(
    title: View,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
    centered: bool,
) -> View {
    let insets = config.window_insets;
    let mut m = Modifier::new()
        .min_width(200.0)
        .height(config.height + insets.top)
        .background(config.colors.container_color)
        .padding_values(PaddingValues {
            left: config.content_padding.left + insets.left,
            right: config.content_padding.right + insets.right,
            top: config.content_padding.top + insets.top,
            bottom: config.content_padding.bottom + insets.bottom,
        })
        .align_items(AlignItems::Center)
        .then(config.modifier);
    if centered {
        m = m.justify_content(JustifyContent::Center);
    }
    Row(m).child((
        navigation_icon.unwrap_or(Box(Modifier::new().width(16.0).fill_max_height())),
        Box(Modifier::new()
            .padding_values(PaddingValues {
                left: 16.0,
                right: if centered { 0.0 } else { 0.0 },
                top: 0.0,
                bottom: 0.0,
            })
            .flex_grow(1.0))
        .child(with_content_color(
            config.colors.title_content_color,
            || title,
        )),
        Row(Modifier::new()
            .align_items(AlignItems::Center)
            .clip_rounded(20.0))
        .child(
            actions
                .into_iter()
                .map(|a| {
                    with_content_color(config.colors.action_icon_content_color, move || a.clone())
                })
                .collect::<Vec<_>>(),
        ),
    ))
}

/// M3 Top App Bar (small). Displays a title with optional navigation icon and
/// trailing action buttons.
pub fn TopAppBar(
    title: View,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
) -> View {
    top_app_bar_layout(title, navigation_icon, actions, config, false)
}

/// M3 Center-Aligned Top App Bar - same as TopAppBar but title is centered.
pub fn CenterAlignedTopAppBar(
    title: View,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
) -> View {
    top_app_bar_layout(title, navigation_icon, actions, config, true)
}

/// Configuration for [`Surface`].
#[derive(Clone, Debug)]
pub struct SurfaceConfig {
    pub modifier: Modifier,
    pub color: Color,
    pub content_color: Color,
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub border: Option<(f32, Color)>,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: SurfaceDefaults::color(),
            content_color: SurfaceDefaults::content_color(),
            shape_radius: SurfaceDefaults::SHAPE_RADIUS,
            tonal_elevation: SurfaceDefaults::TONAL_ELEVATION,
            shadow_elevation: SurfaceDefaults::SHADOW_ELEVATION,
            border: None,
        }
    }
}

/// M3 Surface - a basic container with shape, color, elevation, and border.
/// Sets the ContentColor local for children based on the surface color.
pub fn Surface(config: SurfaceConfig, content: impl FnOnce() -> View) -> View {
    let mut m = Modifier::new()
        .background(config.color)
        .clip_rounded(config.shape_radius)
        .then(config.modifier);
    if config.tonal_elevation > 0.0 {
        m = m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            disabled: 0.0,
        });
    }
    if config.shadow_elevation > 0.0 {
        m = m.shadow(config.shadow_elevation, 0.0);
    }
    if let Some((w, c)) = config.border {
        m = m.border(w, c, config.shape_radius);
    }
    Box(m).color(config.content_color).child(content())
}

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
        }
    }
}

fn icon_button_render(
    icon: View,
    on_click: impl Fn() + 'static,
    config: &IconButtonConfig,
    sz: f32,
    bg: Option<Color>,
    bdr: Option<(f32, Color)>,
    state_colors: StateColors,
) -> View {
    let is_enabled = config.enabled;
    let content_color = config.colors.content(is_enabled);
    let mut m = Modifier::new()
        .size(sz, sz)
        .clip_rounded(sz * 0.5)
        .state_colors(state_colors)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .then(config.modifier.clone());

    if let Some(bg_color) = bg {
        m = m.background(bg_color);
    }
    if let Some((w, c)) = bdr {
        m = m.border(w, c, sz * 0.5);
    }
    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(icon)
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
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
    )
}

/// M3 Filled Icon Button - icon button with a filled container background.
pub fn FilledIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
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
            hovered: content_color.with_alpha_f32(0.08),
            pressed: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
    )
}

/// M3 Filled Tonal Icon Button - icon button with a secondary container background.
pub fn FilledTonalIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
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
            hovered: content_color.with_alpha_f32(0.08),
            pressed: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
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
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
    )
}

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
            hovered: colors.content_color.with_alpha_f32(0.08),
            pressed: colors.content_color.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        };
        let se = config.elevation.map(|e| StateElevation {
            default: e.default,
            hovered: e.hovered,
            pressed: e.pressed,
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
                pressed: Color::TRANSPARENT,
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
    padding_left: f32,
    padding_right: f32,
    height: f32,
    shape_radius: f32,
    enabled: bool,
) -> View {
    let mut m = Modifier::new().height(height).min_width(48.0);
    if let Some(bg) = container_color {
        m = m.background(bg);
    }
    m = m.state_colors(if enabled {
        state_colors
    } else {
        StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
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
        .padding_values(PaddingValues {
            left: padding_left,
            right: padding_right,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center);
    if enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }
    m = m.then(outer_modifier);
    let effective = if enabled {
        content_color
    } else {
        content_color.with_alpha_f32(0.38)
    };
    let content = with_content_color(effective, content);
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
        top: 0.0,
        bottom: 0.0,
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
        pad.left,
        pad.right,
        config.height,
        config.shape_radius,
        config.enabled,
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
        top: 0.0,
        bottom: 0.0,
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
        pad.left,
        pad.right,
        config.height,
        config.shape_radius,
        config.enabled,
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
        top: 0.0,
        bottom: 0.0,
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
        pad.left,
        pad.right,
        config.height,
        config.shape_radius,
        config.enabled,
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
        top: 0.0,
        bottom: 0.0,
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
        pad.left,
        pad.right,
        config.height,
        config.shape_radius,
        config.enabled,
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
        top: 0.0,
        bottom: 0.0,
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
        pad.left,
        pad.right,
        config.height,
        config.shape_radius,
        config.enabled,
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
        .height(height)
        .padding_values(PaddingValues {
            left: pad_left,
            right: pad_right,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape_radius)
        .state_colors(state_colors)
        .state_elevation(state_elevation);
    if let Some((w, c, r)) = border {
        m = m.border(w, c, r);
    }
    if enabled {
        m = m.clickable().on_pointer_down({
            let cb = on_checked_change;
            move |_| cb(!checked)
        });
    } else {
        m = m.alpha(0.38);
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
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        config.height,
        config.shape_radius,
        config.enabled,
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
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        config.height,
        config.shape_radius,
        config.enabled,
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
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        config.height,
        config.shape_radius,
        config.enabled,
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
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        ToggleButtonDefaults::HORIZONTAL_PADDING,
        config.height,
        config.shape_radius,
        config.enabled,
    )
}

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

    let mut m = Modifier::new()
        .size(size, size)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: config.content_color.with_alpha_f32(0.08),
            pressed: config.content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(config.state_elevation)
        .clip_rounded(shape_r)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .then(config.modifier);

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(icon)
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

    let mut m = Modifier::new()
        .height(56.0)
        .min_width(80.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: config.content_color.with_alpha_f32(0.08),
            pressed: config.content_color.with_alpha_f32(0.12),
            disabled: theme().on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(config.state_elevation)
        .clip_rounded(FABDefaults::SHAPE_RADIUS)
        .padding_values(PaddingValues {
            left: 16.0,
            right: 20.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center);

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }
    m = m.then(config.modifier);
    Row(m).child((
        icon.unwrap_or(Box(Modifier::new())),
        Box(Modifier::new()
            .width(if has_icon { 12.0 } else { 0.0 })
            .fill_max_height()),
        Text(label)
            .color(content_color)
            .size(th.typography.label_large)
            .single_line(),
    ))
}

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

/// Configuration for [`Badge`].
#[derive(Clone, Debug)]
pub struct BadgeConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: BadgeDefaults::container_color(),
            content_color: BadgeDefaults::content_color(),
        }
    }
}

/// M3 Badge - a small notification indicator. If `content` is `None`, shows a
/// small 6dp dot; otherwise shows the content inside a 16dp pill.
pub fn Badge(content: Option<View>, config: BadgeConfig) -> View {
    match content {
        None => Box(Modifier::new()
            .size(BadgeDefaults::DOT_SIZE, BadgeDefaults::DOT_SIZE)
            .background(config.container_color)
            .clip_rounded(BadgeDefaults::DOT_SIZE * 0.5)
            .then(config.modifier)),
        Some(view) => Box(Modifier::new()
            .min_width(BadgeDefaults::LABEL_MIN_WIDTH)
            .height(BadgeDefaults::LABEL_HEIGHT)
            .background(config.container_color)
            .clip_rounded(BadgeDefaults::LABEL_HEIGHT * 0.5)
            .padding_values(PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            })
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center)
            .then(config.modifier))
        .child(view),
    }
}

/// Configuration for [`BadgedBox`].
#[derive(Clone, Debug)]
pub struct BadgedBoxConfig {
    pub modifier: Modifier,
    /// Horizontal offset for the badge when it's a small dot.
    pub dot_offset_x: f32,
    /// Vertical offset for the badge when it's a small dot.
    pub dot_offset_y: f32,
    /// Horizontal offset for the badge when it has content.
    pub content_offset_x: f32,
    /// Vertical offset for the badge when it has content.
    pub content_offset_y: f32,
}

impl Default for BadgedBoxConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            dot_offset_x: BadgeDefaults::DOT_OFFSET_X,
            dot_offset_y: BadgeDefaults::DOT_OFFSET_Y,
            content_offset_x: BadgeDefaults::CONTENT_OFFSET_X,
            content_offset_y: BadgeDefaults::CONTENT_OFFSET_Y,
        }
    }
}

/// M3 BadgedBox - wraps `content` and shows a `badge` anchored to the top-end corner.
/// The badge is positioned at the top-end corner of the content.
pub fn BadgedBox(badge: View, content: View, config: BadgedBoxConfig) -> View {
    Stack(Modifier::new()).child((
        content,
        Box(Modifier::new().absolute().offset(
            None,
            Some(config.dot_offset_y),
            Some(config.dot_offset_x),
            None,
        ))
        .child(badge),
    ))
}

/// Colors for [`ListItem`] — matches Compose Material3 `ListItemColors` with
/// 4 state groups (default, disabled, selected, dragged) × 6 slots each.
#[derive(Clone, Debug)]
pub struct ListItemColors {
    pub container_color: Color,
    pub headline_color: Color,
    pub supporting_color: Color,
    pub overline_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,

    pub disabled_container_color: Color,
    pub disabled_headline_color: Color,
    pub disabled_supporting_color: Color,
    pub disabled_overline_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,

    pub selected_container_color: Color,
    pub selected_headline_color: Color,
    pub selected_supporting_color: Color,
    pub selected_overline_color: Color,
    pub selected_leading_icon_color: Color,
    pub selected_trailing_icon_color: Color,

    pub dragged_container_color: Color,
    pub dragged_headline_color: Color,
    pub dragged_supporting_color: Color,
    pub dragged_overline_color: Color,
    pub dragged_leading_icon_color: Color,
    pub dragged_trailing_icon_color: Color,
}

impl ListItemColors {
    pub fn container(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_container_color
        } else if dragged {
            self.dragged_container_color
        } else if selected {
            self.selected_container_color
        } else {
            self.container_color
        }
    }
    pub fn headline(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_headline_color
        } else if dragged {
            self.dragged_headline_color
        } else if selected {
            self.selected_headline_color
        } else {
            self.headline_color
        }
    }
    pub fn supporting(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_supporting_color
        } else if dragged {
            self.dragged_supporting_color
        } else if selected {
            self.selected_supporting_color
        } else {
            self.supporting_color
        }
    }
    pub fn overline(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_overline_color
        } else if dragged {
            self.dragged_overline_color
        } else if selected {
            self.selected_overline_color
        } else {
            self.overline_color
        }
    }
    pub fn leading_icon(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if dragged {
            self.dragged_leading_icon_color
        } else if selected {
            self.selected_leading_icon_color
        } else {
            self.leading_icon_color
        }
    }
    pub fn trailing_icon(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if dragged {
            self.dragged_trailing_icon_color
        } else if selected {
            self.selected_trailing_icon_color
        } else {
            self.trailing_icon_color
        }
    }
}

impl Default for ListItemColors {
    fn default() -> Self {
        Self {
            container_color: Color::TRANSPARENT,
            headline_color: ListItemDefaults::headline_color(),
            supporting_color: ListItemDefaults::supporting_color(),
            overline_color: ListItemDefaults::overline_color(),
            leading_icon_color: ListItemDefaults::leading_icon_color(),
            trailing_icon_color: ListItemDefaults::trailing_icon_color(),
            disabled_container_color: ListItemDefaults::disabled_container_color(),
            disabled_headline_color: ListItemDefaults::disabled_headline_color(),
            disabled_supporting_color: ListItemDefaults::disabled_supporting_color(),
            disabled_overline_color: ListItemDefaults::disabled_overline_color(),
            disabled_leading_icon_color: ListItemDefaults::disabled_leading_icon_color(),
            disabled_trailing_icon_color: ListItemDefaults::disabled_trailing_icon_color(),
            selected_container_color: ListItemDefaults::selected_container_color(),
            selected_headline_color: ListItemDefaults::selected_headline_color(),
            selected_supporting_color: ListItemDefaults::selected_supporting_color(),
            selected_overline_color: ListItemDefaults::selected_overline_color(),
            selected_leading_icon_color: ListItemDefaults::selected_leading_icon_color(),
            selected_trailing_icon_color: ListItemDefaults::selected_trailing_icon_color(),
            dragged_container_color: ListItemDefaults::dragged_container_color(),
            dragged_headline_color: ListItemDefaults::dragged_headline_color(),
            dragged_supporting_color: ListItemDefaults::dragged_supporting_color(),
            dragged_overline_color: ListItemDefaults::dragged_overline_color(),
            dragged_leading_icon_color: ListItemDefaults::dragged_leading_icon_color(),
            dragged_trailing_icon_color: ListItemDefaults::dragged_trailing_icon_color(),
        }
    }
}

/// Configuration for [`ListItem`].
#[derive(Clone, Debug)]
pub struct ListItemConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and suppresses clicks.
    pub enabled: bool,
    pub colors: ListItemColors,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub shape_radius: f32,
    pub horizontal_padding: f32,
    pub trailing_padding: f32,
    pub one_line_height: f32,
    pub two_line_height: f32,
    pub three_line_height: f32,
}

impl Default for ListItemConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            colors: ListItemColors::default(),
            tonal_elevation: 0.0,
            shadow_elevation: 0.0,
            shape_radius: 0.0,
            horizontal_padding: ListItemDefaults::HORIZONTAL_PADDING,
            trailing_padding: ListItemDefaults::TRAILING_PADDING,
            one_line_height: ListItemDefaults::ONE_LINE_HEIGHT,
            two_line_height: ListItemDefaults::TWO_LINE_HEIGHT,
            three_line_height: ListItemDefaults::THREE_LINE_HEIGHT,
        }
    }
}

/// M3 List Item - a single row in a list with optional leading/trailing content.
pub fn ListItem(
    headline: impl Into<String>,
    supporting_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
    config: ListItemConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let c = &config.colors;

    let hd_col = c.headline(is_enabled, false, false);
    let sp_col = c.supporting(is_enabled, false, false);
    let ld_col = c.leading_icon(is_enabled, false, false);
    let tr_col = c.trailing_icon(is_enabled, false, false);
    let bg = c.container(is_enabled, false, false);

    let line_count = if supporting_text.is_some() { 2 } else { 1 };
    let min_h = match line_count {
        2 => config.two_line_height,
        3 => config.three_line_height,
        _ => config.one_line_height,
    };

    let mut modifier = Modifier::new()
        .min_width(200.0)
        .min_height(min_h)
        .background(bg)
        .clip_rounded(config.shape_radius)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.trailing_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .align_items(AlignItems::Center)
        .then(config.modifier);

    if config.tonal_elevation > 0.0 {
        modifier = modifier.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            disabled: 0.0,
        });
    }
    if config.shadow_elevation > 0.0 {
        modifier = modifier.shadow(config.shadow_elevation, 0.0);
    }

    if let Some(cb) = on_click {
        let cb = cb.clone();
        modifier = modifier.clickable().on_pointer_down(move |_| {
            if is_enabled {
                cb();
            }
        });
    }

    let wrap_icon = |color: Color, v: View| -> View { with_content_color(color, move || v) };

    Row(modifier).child((
        leading
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 16.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(wrap_icon(ld_col, v))
            })
            .unwrap_or(Box(Modifier::new())),
        Column(
            Modifier::new()
                .flex_grow(1.0)
                .justify_content(JustifyContent::Center),
        )
        .child((
            Text(headline)
                .color(hd_col)
                .size(th.typography.body_large)
                .single_line(),
            supporting_text
                .map(|st| {
                    Text(st)
                        .color(sp_col)
                        .size(th.typography.body_medium)
                        .max_lines(2)
                        .overflow_ellipsize()
                })
                .unwrap_or(Box(Modifier::new())),
        )),
        trailing
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 16.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(wrap_icon(tr_col, v))
            })
            .unwrap_or(Box(Modifier::new())),
    ))
}

/// A single tab definition for use with `TabRow`.
pub struct Tab {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
}

/// Configuration for [`TabRow`].
#[derive(Clone, Debug)]
pub struct TabRowConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub selected_content_color: Color,
    pub unselected_content_color: Color,
    pub indicator_color: Color,
    pub height: f32,
    pub indicator_height: f32,
}

impl Default for TabRowConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: TabDefaults::container_color(),
            selected_content_color: TabDefaults::selected_content_color(),
            unselected_content_color: TabDefaults::unselected_content_color(),
            indicator_color: TabDefaults::indicator_color(),
            height: TabDefaults::HEIGHT,
            indicator_height: TabDefaults::INDICATOR_HEIGHT,
        }
    }
}

static TABROW_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Tab Row - a horizontal row of tabs with an active indicator.
/// Text colors and indicator height animate with 150ms FastOutSlowIn.
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    Column(Modifier::new().fill_max_width())
        .child((
            Row(Modifier::new()
                .fill_max_width()
                .height(config.height)
                .background(config.container_color))
            .child(
                tabs.into_iter()
                    .enumerate()
                    .map(|(i, tab)| {
                        let selected = i == selected_index;
                        let color = animate_color(
                            format!("tab_clr_{}_{}", id, i),
                            if selected {
                                config.selected_content_color
                            } else {
                                config.unselected_content_color
                            },
                            spec,
                        );
                        let indicator_h = animate_f32(
                            format!("tab_ind_{}_{}", id, i),
                            if selected {
                                config.indicator_height
                            } else {
                                0.0
                            },
                            spec,
                        );
                        let cb = tab.on_click.clone();

                        let mut tab_m = Modifier::new()
                            .flex_grow(1.0)
                            .fill_max_height()
                            .align_items(AlignItems::Center)
                            .justify_content(JustifyContent::Center)
                            .state_colors(StateColors {
                                default: Color::TRANSPARENT,
                                hovered: th.on_surface.with_alpha_f32(0.08),
                                pressed: th.on_surface.with_alpha_f32(0.12),
                                disabled: Color::TRANSPARENT,
                            });
                        if tab.enabled {
                            tab_m = tab_m.clickable().on_pointer_down(move |_| cb());
                        }

                        Column(tab_m).child((
                            tab.icon.unwrap_or(Box(Modifier::new())),
                            Text(tab.label)
                                .color(color)
                                .size(th.typography.title_small)
                                .single_line(),
                            Box(Modifier::new()
                                .min_width(200.0)
                                .height(indicator_h)
                                .background(config.indicator_color)
                                .clip_rounded(TabDefaults::INDICATOR_CORNER)),
                        ))
                    })
                    .collect::<Vec<_>>(),
            ),
            Box(Modifier::new()
                .min_width(200.0)
                .height(1.0)
                .background(th.outline_variant)),
        ))
        .modifier(config.modifier)
}

/// A single segment definition for `SegmentedButton`.
pub struct Segment {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
}

/// Configuration for [`SegmentedButton`].
#[derive(Clone, Debug)]
pub struct SegmentedButtonConfig {
    pub modifier: Modifier,
    pub border_color: Color,
    pub selected_container_color: Color,
    pub selected_content_color: Color,
    pub unselected_content_color: Color,
    pub state_colors: StateColors,
    pub height: f32,
    pub shape_radius: f32,
}

impl Default for SegmentedButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            border_color: SegmentedButtonDefaults::border_color(),
            selected_container_color: SegmentedButtonDefaults::selected_container_color(),
            selected_content_color: SegmentedButtonDefaults::selected_content_color(),
            unselected_content_color: SegmentedButtonDefaults::unselected_content_color(),
            state_colors: SegmentedButtonDefaults::state_colors_default(),
            height: SegmentedButtonDefaults::HEIGHT,
            shape_radius: SegmentedButtonDefaults::SHAPE_RADIUS,
        }
    }
}

static SEGBUTTON_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Segmented Button - a row of toggle segments. `selected` contains the
/// indices of selected segments (single-select: pass a single-element set).
/// Each segment is shaped independently: first has rounded left corners,
/// last has rounded right corners, middle segments are rectangular.
pub fn SegmentedButton(
    selected: &[usize],
    segments: Vec<Segment>,
    config: SegmentedButtonConfig,
) -> View {
    let th = theme();
    let count = segments.len();
    let id = remember(|| SEGBUTTON_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let shape_r = config.shape_radius;

    // corner order: [BL, BR, TR, TL]
    let segment_radii = |i: usize| -> [f32; 4] {
        if count == 1 {
            [shape_r, shape_r, shape_r, shape_r]
        } else if i == 0 {
            [shape_r, 0.0, 0.0, shape_r]
        } else if i == count - 1 {
            [0.0, shape_r, shape_r, 0.0]
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    };

    // Outer border wraps the entire group. Internal dividers are inside each segment Row.
    Row(Modifier::new()
        .height(config.height)
        .border(1.0, config.border_color, shape_r)
        .then(config.modifier))
    .child(
        segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| {
                let is_selected = selected.contains(&i);

                let bg = animate_color(
                    format!("sb_bg_{}_{}", id, i),
                    if is_selected {
                        config.selected_container_color
                    } else {
                        Color::TRANSPARENT
                    },
                    spec,
                );
                let fg = animate_color(
                    format!("sb_fg_{}_{}", id, i),
                    if is_selected {
                        config.selected_content_color
                    } else {
                        config.unselected_content_color
                    },
                    spec,
                );

                let cb = seg.on_click.clone();
                let radii = segment_radii(i);
                let is_enabled = seg.enabled;

                let state_colors = config.state_colors;
                let content_modifier = Modifier::new()
                    .flex_grow(1.0)
                    .fill_max_height()
                    .clip_rounded_radii(radii)
                    .background(bg)
                    .state_colors(state_colors)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center)
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    });

                let content_modifier = if is_enabled {
                    content_modifier.clickable().on_pointer_down(move |_| cb())
                } else {
                    content_modifier
                };

                Row(Modifier::new().flex_grow(1.0).fill_max_height()).child((
                    Row(content_modifier).child((
                        seg.icon.unwrap_or(Box(Modifier::new())),
                        Text(seg.label)
                            .color(fg)
                            .size(th.typography.label_large)
                            .single_line(),
                    )),
                    if i < count - 1 {
                        Box(Modifier::new()
                            .width(1.0)
                            .fill_max_height()
                            .background(th.outline))
                    } else {
                        Box(Modifier::new())
                    },
                ))
            })
            .collect::<Vec<_>>(),
    )
}

/// Configuration for [`CircularProgressIndicator`].
#[derive(Clone, Debug)]
pub struct CircularProgressIndicatorConfig {
    pub color: Color,
    pub track_color: Color,
    pub stroke_width: f32,
    pub stroke_cap: StrokeCap,
    pub gap_size: f32,
}

impl Default for CircularProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            color: ProgressIndicatorDefaults::circular_color(),
            track_color: ProgressIndicatorDefaults::circular_track_color(),
            stroke_width: ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH,
            stroke_cap: StrokeCap::Round,
            gap_size: 0.0,
        }
    }
}

/// M3 Circular Progress Indicator.
///
/// Determinate (`Some(0..1)`): draws arc from 12 o'clock clockwise.
/// Indeterminate (`None`): animates a spinning 270° arc.
pub fn CircularProgressIndicator(
    value: Option<f32>,
    config: CircularProgressIndicatorConfig,
) -> View {
    let sz = dp_to_px(ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE);
    let stroke_px = dp_to_px(config.stroke_width);
    let val = value.map(|v| v.clamp(0.0, 1.0));

    // Three concurrent animations matching Compose Material3 indeterminate spec:
    //   1. Global rotation — 1080° linear over 6000ms
    //   2. Additional rotation — 90° stepped jumps with EmphasizedDecelerate
    //   3. Sweep — oscillates 0.1 → 0.87 → 0.1 over 6000ms
    let (global_rotation, additional_rotation, sweep_val) = if value.is_none() {
        let shared = remember_state_with_key("circ_ind_shared", || {
            let mut a = AnimatedValue::new(
                0.0f32,
                AnimationSpec::tween(Duration::from_millis(6000), Easing::Linear)
                    .repeated(RepeatableSpec::infinite()),
            );
            a.set_target(1.0);
            a
        });
        let mut s = shared.borrow_mut();
        s.update();
        let t = *s.get();
        drop(s);

        let gv = t * 1080.0;

        let emph = Easing::Custom(CubicBezier::new(0.05, 0.7, 0.1, 1.0));
        let add_kf = remember_state_with_key("circ_ind_add_kf", || KeyframesSpec {
            keyframes: vec![
                (0.0, 0.0, None),
                (0.05, 90.0, Some(emph)),
                (0.25, 90.0, None),
                (0.30, 180.0, None),
                (0.50, 180.0, None),
                (0.55, 270.0, None),
                (0.75, 270.0, None),
                (0.80, 360.0, None),
                (1.0, 360.0, None),
            ],
        });
        let av = add_kf.borrow().evaluate(t);

        let std_dec = Easing::Custom(CubicBezier::new(0.2, 0.0, 0.0, 1.0));
        let sweep_kf = remember_state_with_key("circ_ind_sweep_kf", || KeyframesSpec {
            keyframes: vec![
                (0.0, 0.1, None),
                (0.5, 0.87, Some(std_dec)),
                (1.0, 0.1, None),
            ],
        });
        let sv = sweep_kf.borrow().evaluate(t);

        (gv, av, sv)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Pre-compute gap angular size in radians
    let indicator_size_dp = ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE;
    let adjusted_gap_dp = if config.stroke_cap == StrokeCap::Butt {
        config.gap_size
    } else {
        config.gap_size + config.stroke_width
    };
    let circle_dia_dp = indicator_size_dp - config.stroke_width;
    let gap_sweep_rad = 2.0 * adjusted_gap_dp / circle_dia_dp;

    Box(Modifier::new()
        .size(sz, sz)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let cx = rect.x + rect.w * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let r = (rect.w.min(rect.h)) * 0.5 - stroke_px * 0.5;
            let circle = Rect {
                x: cx - r,
                y: cy - r,
                w: r * 2.0,
                h: r * 2.0,
            };

            match val {
                Some(p) => {
                    let sweep_rad = p * std::f32::consts::TAU;
                    let start_angle = -std::f32::consts::FRAC_PI_2;
                    let effective_gap = gap_sweep_rad.min(sweep_rad);

                    // Indicator arc
                    if p > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle,
                            sweep_angle: sweep_rad,
                            stroke_width: stroke_px,
                            color: mul_c(config.color),
                            cap: config.stroke_cap,
                        });
                    }

                    // Track arc (with gap from indicator)
                    let track_start = start_angle + sweep_rad + effective_gap;
                    let track_sweep = std::f32::consts::TAU - sweep_rad - 2.0 * effective_gap;
                    if track_sweep > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle: track_start,
                            sweep_angle: track_sweep,
                            stroke_width: stroke_px,
                            color: mul_c(config.track_color),
                            cap: config.stroke_cap,
                        });
                    }
                }
                None => {
                    let radians =
                        (global_rotation + additional_rotation) * std::f32::consts::PI / 180.0;
                    let start_angle = -std::f32::consts::FRAC_PI_2 + radians;
                    let sweep_rad = sweep_val * std::f32::consts::TAU;
                    let effective_gap = gap_sweep_rad.min(sweep_rad);

                    // Indicator arc
                    scene.nodes.push(SceneNode::Arc {
                        rect: circle,
                        start_angle,
                        sweep_angle: sweep_rad,
                        stroke_width: stroke_px,
                        color: mul_c(config.color),
                        cap: config.stroke_cap,
                    });

                    // Track arc (with gap from indicator)
                    let track_start = start_angle + sweep_rad + effective_gap;
                    let track_sweep = std::f32::consts::TAU - sweep_rad - 2.0 * effective_gap;
                    if track_sweep > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle: track_start,
                            sweep_angle: track_sweep,
                            stroke_width: stroke_px,
                            color: mul_c(config.track_color),
                            cap: config.stroke_cap,
                        });
                    }
                }
            }
        }))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

/// Configuration for [`LinearProgressIndicator`].
#[derive(Clone, Debug)]
pub struct LinearProgressIndicatorConfig {
    pub color: Color,
    pub track_color: Color,
    /// Stroke cap style for the indicator ends. Default: `StrokeCap::Round`
    pub stroke_cap: StrokeCap,
    /// Gap between indicator and track, in dp.
    pub gap_size: f32,
    /// Diameter of the stop indicator dot, in dp.
    pub stop_size: f32,
}

impl Default for LinearProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            color: ProgressIndicatorDefaults::linear_color(),
            track_color: ProgressIndicatorDefaults::linear_track_color(),
            stroke_cap: StrokeCap::Round,
            gap_size: ProgressIndicatorDefaults::LINEAR_INDICATOR_GAP_SIZE,
            stop_size: ProgressIndicatorDefaults::LINEAR_TRACK_STOP_SIZE,
        }
    }
}

/// M3 Linear Progress Indicator.
///
/// Pass `LinearProgressIndicatorConfig::default()` for standard M3 appearance,
/// or override individual fields via struct-update syntax.
pub fn LinearProgressIndicator(value: Option<f32>, config: LinearProgressIndicatorConfig) -> View {
    Box(Modifier::new()
        .fill_max_width()
        .height(ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = rect.h;
            let corner = track_h * 0.5;
            let dot_r = dp_to_px(config.stop_size) * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let t = value.unwrap_or(0.0).clamp(0.0, 1.0);

            let cap_radius = if config.stroke_cap == StrokeCap::Butt {
                0.0
            } else {
                corner
            };

            let gap = dp_to_px(config.gap_size)
                - if config.stroke_cap == StrokeCap::Butt {
                    0.0
                } else {
                    cap_radius
                };

            let cap_ofs = cap_radius;
            let ind_end = (t * rect.w).clamp(cap_ofs, rect.w - cap_ofs);
            let ind_w = (ind_end - cap_ofs).max(0.0);

            // Indicator (active portion from left)
            if t > 0.0 && ind_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x + cap_ofs,
                        y: cy - corner,
                        w: ind_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.color)),
                    radius: [cap_radius; 4],
                });
            }

            // Track (inactive portion after gap)
            let track_start = (rect.x + ind_end + gap).min(rect.x + rect.w);
            let track_w = (rect.x + rect.w - track_start).max(0.0);
            if t < 1.0 && track_w > 0.0 {
                let track_left = track_start + cap_ofs;
                let track_right = rect.x + rect.w;
                if track_right > track_left {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: track_left,
                            y: cy - corner,
                            w: track_right - track_left,
                            h: track_h,
                        },
                        brush: Brush::Solid(mul_c(config.track_color)),
                        radius: [cap_radius; 4],
                    });
                }
            }

            // Stop indicator at right end circle
            {
                let sx = rect.x + rect.w - dot_r;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(config.color)),
                });
            }
        }))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

/// Color slots for text fields — matches Compose Material3 `TextFieldColors`.
/// All 42 color fields (focused/unfocused/disabled/error variants of each slot).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TextFieldColors {
    pub focused_text_color: Color,
    pub unfocused_text_color: Color,
    pub disabled_text_color: Color,
    pub error_text_color: Color,
    pub focused_container_color: Color,
    pub unfocused_container_color: Color,
    pub disabled_container_color: Color,
    pub error_container_color: Color,
    pub cursor_color: Color,
    pub error_cursor_color: Color,
    pub focused_indicator_color: Color,
    pub unfocused_indicator_color: Color,
    pub disabled_indicator_color: Color,
    pub error_indicator_color: Color,
    pub focused_leading_icon_color: Color,
    pub unfocused_leading_icon_color: Color,
    pub disabled_leading_icon_color: Color,
    pub error_leading_icon_color: Color,
    pub focused_trailing_icon_color: Color,
    pub unfocused_trailing_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub error_trailing_icon_color: Color,
    pub focused_label_color: Color,
    pub unfocused_label_color: Color,
    pub disabled_label_color: Color,
    pub error_label_color: Color,
    pub focused_placeholder_color: Color,
    pub unfocused_placeholder_color: Color,
    pub disabled_placeholder_color: Color,
    pub error_placeholder_color: Color,
    pub focused_supporting_text_color: Color,
    pub unfocused_supporting_text_color: Color,
    pub disabled_supporting_text_color: Color,
    pub error_supporting_text_color: Color,
    pub focused_prefix_color: Color,
    pub unfocused_prefix_color: Color,
    pub disabled_prefix_color: Color,
    pub error_prefix_color: Color,
    pub focused_suffix_color: Color,
    pub unfocused_suffix_color: Color,
    pub disabled_suffix_color: Color,
    pub error_suffix_color: Color,
}

#[allow(dead_code)]
impl TextFieldColors {
    pub fn text_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_text_color
        } else if is_error {
            self.error_text_color
        } else if focused {
            self.focused_text_color
        } else {
            self.unfocused_text_color
        }
    }
    pub fn container_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_container_color
        } else if is_error {
            self.error_container_color
        } else if focused {
            self.focused_container_color
        } else {
            self.unfocused_container_color
        }
    }
    pub fn cursor_color(&self, is_error: bool) -> Color {
        if is_error {
            self.error_cursor_color
        } else {
            self.cursor_color
        }
    }
    pub fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_indicator_color
        } else if is_error {
            self.error_indicator_color
        } else if focused {
            self.focused_indicator_color
        } else {
            self.unfocused_indicator_color
        }
    }
    pub fn leading_icon_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if is_error {
            self.error_leading_icon_color
        } else if focused {
            self.focused_leading_icon_color
        } else {
            self.unfocused_leading_icon_color
        }
    }
    pub fn trailing_icon_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if is_error {
            self.error_trailing_icon_color
        } else if focused {
            self.focused_trailing_icon_color
        } else {
            self.unfocused_trailing_icon_color
        }
    }
    pub fn label_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_label_color
        } else if is_error {
            self.error_label_color
        } else if focused {
            self.focused_label_color
        } else {
            self.unfocused_label_color
        }
    }
    pub fn placeholder_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_placeholder_color
        } else if is_error {
            self.error_placeholder_color
        } else if focused {
            self.focused_placeholder_color
        } else {
            self.unfocused_placeholder_color
        }
    }
    pub fn supporting_text_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_supporting_text_color
        } else if is_error {
            self.error_supporting_text_color
        } else if focused {
            self.focused_supporting_text_color
        } else {
            self.unfocused_supporting_text_color
        }
    }
}

/// Default values for text field colors.
pub struct TextFieldDefaults;

impl TextFieldDefaults {
    pub fn colors() -> TextFieldColors {
        let th = theme();
        TextFieldColors {
            focused_text_color: th.on_surface,
            unfocused_text_color: th.on_surface,
            disabled_text_color: th.on_surface.with_alpha_f32(0.38),
            error_text_color: th.on_surface,
            focused_container_color: th.surface_container_highest,
            unfocused_container_color: th.surface_container_highest,
            disabled_container_color: th.on_surface.with_alpha_f32(0.04),
            error_container_color: th.surface_container_highest,
            cursor_color: th.primary,
            error_cursor_color: th.error,
            focused_indicator_color: th.primary,
            unfocused_indicator_color: th.on_surface_variant,
            disabled_indicator_color: th.on_surface.with_alpha_f32(0.12),
            error_indicator_color: th.error,
            focused_leading_icon_color: th.on_surface_variant,
            unfocused_leading_icon_color: th.on_surface_variant,
            disabled_leading_icon_color: th.on_surface.with_alpha_f32(0.38),
            error_leading_icon_color: th.error,
            focused_trailing_icon_color: th.on_surface_variant,
            unfocused_trailing_icon_color: th.on_surface_variant,
            disabled_trailing_icon_color: th.on_surface.with_alpha_f32(0.38),
            error_trailing_icon_color: th.error,
            focused_label_color: th.primary,
            unfocused_label_color: th.on_surface_variant,
            disabled_label_color: th.on_surface.with_alpha_f32(0.38),
            error_label_color: th.error,
            focused_placeholder_color: th.on_surface_variant,
            unfocused_placeholder_color: th.on_surface_variant,
            disabled_placeholder_color: th.on_surface.with_alpha_f32(0.38),
            error_placeholder_color: th.error,
            focused_supporting_text_color: th.on_surface_variant,
            unfocused_supporting_text_color: th.on_surface_variant,
            disabled_supporting_text_color: th.on_surface.with_alpha_f32(0.38),
            error_supporting_text_color: th.error,
            focused_prefix_color: th.on_surface,
            unfocused_prefix_color: th.on_surface,
            disabled_prefix_color: th.on_surface.with_alpha_f32(0.38),
            error_prefix_color: th.on_surface,
            focused_suffix_color: th.on_surface,
            unfocused_suffix_color: th.on_surface,
            disabled_suffix_color: th.on_surface.with_alpha_f32(0.38),
            error_suffix_color: th.on_surface,
        }
    }
}

/// Configuration for an `OutlinedTextField`.
#[derive(Clone)]
pub struct OutlinedTextFieldConfig {
    /// Floating label shown above the input when the field has text or is focused.
    /// When set, this acts as the visual placeholder (the TextField's own placeholder
    /// is suppressed). When the label floats, it animates to the top border.
    pub label: Option<String>,
    /// Placeholder text shown inside the TextField when empty and unfocused.
    /// Only shown when `label` is `None`; when a label is present the label
    /// itself serves as the visual placeholder.
    pub placeholder: Option<String>,
    /// Icon displayed at the start of the input.
    pub leading_icon: Option<View>,
    /// Icon displayed at the end of the input.
    pub trailing_icon: Option<View>,
    /// If true, Enter submits; if false, Enter inserts a newline.
    pub single_line: bool,
    /// If true, border and label color switch to error color.
    pub is_error: bool,
    /// If false, input is visually disabled and `on_value_change` won't fire.
    pub enabled: bool,
    /// Called when the user presses Enter on a single-line field.
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    /// Colors for all text field UI elements.
    pub colors: Option<TextFieldColors>,
}

impl Default for OutlinedTextFieldConfig {
    fn default() -> Self {
        Self {
            label: None,
            placeholder: None,
            leading_icon: None,
            trailing_icon: None,
            single_line: true,
            is_error: false,
            enabled: true,
            on_submit: None,
            colors: None,
        }
    }
}

/// M3 Outlined Text Field with floating label, leading/trailing icons, and error state.
///
/// The label floats up when `value` is non-empty or when the field is focused.
/// Note: focus-based floating is approximated via animated `float_t` - the label
/// begins floating once `on_value_change` fires (i.e. when the user types).
/// For strict focus-on-tap floating, pair with an external focus signal.
///
/// # Example
/// ```ignore
/// let text = remember(|| signal(String::new()));
/// OutlinedTextField(
///     Modifier::new().fill_max_width().padding(16.0),
///     text.get(),
///     { let t = text.clone(); move |v| t.set(v) },
///     OutlinedTextFieldConfig {
///         label: Some("Email".into()),
///         placeholder: Some("user@example.com".into()),
///         ..Default::default()
///     },
/// );
/// ```
pub fn OutlinedTextField(
    modifier: Modifier,
    value: String,
    on_value_change: impl Fn(String) + 'static,
    config: OutlinedTextFieldConfig,
) -> View {
    let th = theme();
    let label_str: Option<Rc<str>> = config.label.map(Rc::from);
    let has_label = label_str.is_some();

    // Unique animation key per label to avoid conflicts when multiple fields exist
    let anim_key = match &label_str {
        Some(l) => format!("otf_{}", &l[..l.len().min(32)]),
        None => "otf_nolabel".into(),
    };

    // Persistent focus tracker - set by layout/paint when this field is focused,
    // read here on the next frame. This gives a one-frame delay on tap-to-float,
    // which is negligible at 60fps.
    let focus_tracker: Rc<Cell<bool>> =
        remember_with_key(format!("otf_focus_{}", anim_key), || Cell::new(false));
    let is_focused = focus_tracker.get();
    let should_float = !value.is_empty() || is_focused;

    let float_t = animate_f32(
        anim_key.clone(),
        if should_float { 1.0 } else { 0.0 },
        th.motion.color,
    );

    // Resolve colors from config.colors if set, otherwise use legacy theme-based approach
    let (border_color, label_color) = if let Some(ref tc) = config.colors {
        let enf = config.enabled && is_focused;
        (
            tc.indicator_color(config.enabled, config.is_error, enf),
            tc.label_color(config.enabled, config.is_error, enf),
        )
    } else {
        (
            if config.is_error {
                th.error
            } else if float_t > 0.5 {
                th.primary
            } else {
                th.outline
            },
            if config.is_error {
                th.error
            } else if float_t > 0.5 {
                th.primary
            } else {
                th.on_surface_variant
            },
        )
    };

    // Label font size: 16dp at rest (placeholder position) → 12dp when floating
    let label_size = 16.0 - 4.0 * float_t;

    // Label Y offset: 16dp (same line as text) → -4dp (overlapping top border)
    let label_y = 16.0 - 20.0 * float_t;

    // The TextField inside uses no placeholder when a label is present -
    // the label itself serves as the visual placeholder.
    let tf_placeholder = if has_label {
        String::new()
    } else {
        config.placeholder.unwrap_or_default()
    };

    Box(modifier
        .clip_rounded(th.shapes.small)
        .border(1.0, border_color, th.shapes.small)
        .background(th.surface))
    .child(
        Stack(Modifier::new().fill_max_size()).child((
            // Input row - always at the same position, with room at the top
            // for the floating label to overlap.
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 16.0,
                    bottom: 8.0,
                })
                .align_items(AlignItems::Center))
            .child((
                config.leading_icon.unwrap_or(Box(Modifier::new())),
                View::new(0, ViewKind::Box)
                    .modifier(
                        Modifier::new()
                            .flex_grow(1.0)
                            .padding_values(PaddingValues {
                                left: 8.0,
                                right: 8.0,
                                top: 0.0,
                                bottom: 0.0,
                            })
                            .text_input(TextInputConfig {
                                hint: tf_placeholder,
                                multiline: false,
                                on_change: Some(Rc::new(on_value_change) as _),
                                on_submit: config.on_submit.clone().map(|f| {
                                    let f = f.clone();
                                    Rc::new(move |s| f(s)) as Rc<dyn Fn(String)>
                                }),
                                focus_tracker: Some(focus_tracker.clone()),
                                value: value.clone(),
                                visual_transformation: None,
                                keyboard_type: None,
                                ime_action: None,
                                enabled: config.enabled,
                                read_only: false,
                                max_lines: None,
                                min_lines: None,
                                cursor_color: config
                                    .colors
                                    .as_ref()
                                    .map(|c| c.cursor_color(config.is_error)),
                                on_text_layout: None,
                            }),
                    )
                    .semantics(Semantics {
                        role: Role::TextField,
                        label: None,
                        focused: false,
                        enabled: true,
                    }),
                config.trailing_icon.unwrap_or(Box(Modifier::new())),
            )),
            // Floating label - absolutely positioned, animates between text-line
            // and top-border positions as the field gains content / focus.
            // A surface-colored background box hides the border stroke behind the label.
            if let Some(lbl) = label_str {
                Box(Modifier::new()
                    .min_width(200.0)
                    .padding_values(PaddingValues {
                        left: 20.0,
                        right: 20.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .absolute()
                    .offset(Some(0.0), Some(label_y), None, None))
                .child(
                    Box(Modifier::new()
                        .background(th.surface)
                        .padding_values(PaddingValues {
                            left: 4.0,
                            right: 4.0,
                            top: 2.0,
                            bottom: 2.0,
                        }))
                    .child(
                        Text(lbl.as_ref().to_string())
                            .color(label_color)
                            .size(label_size),
                    ),
                )
            } else {
                Box(Modifier::new())
            },
        )),
    )
}

/// Configuration for a filled M3 [`TextField`].
#[derive(Clone)]
pub struct TextFieldConfig {
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub single_line: bool,
    pub is_error: bool,
    pub enabled: bool,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    pub colors: Option<TextFieldColors>,
}

impl Default for TextFieldConfig {
    fn default() -> Self {
        Self {
            label: None,
            placeholder: None,
            leading_icon: None,
            trailing_icon: None,
            single_line: true,
            is_error: false,
            enabled: true,
            on_submit: None,
            colors: None,
        }
    }
}

/// M3 Filled Text Field with floating label, leading/trailing icons, error state,
/// and a bottom indicator line. (Equivalent to Compose Material3's `TextField`.)
///
/// The label floats up when `value` is non-empty or when the field is focused.
/// Container: `SurfaceContainerHighest` bg, top-rounded corners (4dp), flat bottom.
/// Indicator: always visible, 1dp (unfocused) / 2dp (focused/error), animated color+thickness.
pub fn TextField(
    modifier: Modifier,
    value: String,
    on_value_change: impl Fn(String) + 'static,
    config: TextFieldConfig,
) -> View {
    let th = theme();
    let label_str: Option<Rc<str>> = config.label.map(Rc::from);
    let has_label = label_str.is_some();

    let anim_key = match &label_str {
        Some(l) => format!("tf_{}", &l[..l.len().min(32)]),
        None => "tf_nolabel".into(),
    };

    let focus_tracker: Rc<Cell<bool>> =
        remember_with_key(format!("tf_focus_{}", anim_key), || Cell::new(false));
    let is_focused = focus_tracker.get();
    let should_float = !value.is_empty() || is_focused;

    let float_t = animate_f32(
        anim_key.clone(),
        if should_float { 1.0 } else { 0.0 },
        th.motion.color,
    );

    // Resolve colors from config.colors if set, otherwise use legacy theme-based approach
    let (indicator_color, label_color, container_bg) = if let Some(ref tc) = config.colors {
        let enf = config.enabled && is_focused;
        let ind = tc.indicator_color(config.enabled, config.is_error, enf);
        let lb = tc.label_color(config.enabled, config.is_error, enf);
        let bg = tc.container_color(config.enabled, config.is_error, enf);
        (ind, lb, bg)
    } else {
        let ind = if config.is_error {
            th.error
        } else if float_t > 0.5 {
            th.primary
        } else {
            th.on_surface_variant
        };
        let lb = if config.is_error {
            th.error
        } else if float_t > 0.5 {
            th.primary
        } else {
            th.on_surface_variant
        };
        let bg = if config.enabled {
            th.surface_container_highest
        } else {
            th.on_surface
                .with_alpha_f32(0.04)
                .composite_over(th.surface)
        };
        (ind, lb, bg)
    };

    let label_size = 16.0 - 4.0 * float_t;
    let label_y = 16.0 - 20.0 * float_t;

    let tf_placeholder = if has_label {
        String::new()
    } else {
        config.placeholder.unwrap_or_default()
    };

    let indicator_active = config.is_error || (config.enabled && is_focused);
    let indicator_target_w = if indicator_active { 2.0 } else { 1.0 };
    let indicator_w = animate_f32(
        format!("tf_ind_w_{}", anim_key),
        indicator_target_w,
        th.motion.color,
    );

    Box(modifier
        .clip_rounded(th.shapes.extra_small)
        .background(container_bg))
    .child(
        Stack(Modifier::new().fill_max_size()).child((
            // Bottom indicator line — full width, clipped by container shape
            Box(Modifier::new()
                .fill_max_size()
                .align_items(AlignItems::FlexEnd))
            .child(Box(Modifier::new()
                .fill_max_width()
                .height(indicator_w)
                .background(indicator_color))),
            // Input row
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 16.0,
                    bottom: 10.0,
                })
                .align_items(AlignItems::Center))
            .child((
                config.leading_icon.unwrap_or(Box(Modifier::new())),
                View::new(0, ViewKind::Box)
                    .modifier(
                        Modifier::new()
                            .flex_grow(1.0)
                            .padding_values(PaddingValues {
                                left: 8.0,
                                right: 8.0,
                                top: 0.0,
                                bottom: 0.0,
                            })
                            .text_input(TextInputConfig {
                                hint: tf_placeholder,
                                multiline: !config.single_line,
                                on_change: Some(Rc::new(on_value_change) as _),
                                on_submit: config.on_submit.clone().map(|f| {
                                    let f = f.clone();
                                    Rc::new(move |s| f(s)) as Rc<dyn Fn(String)>
                                }),
                                focus_tracker: Some(focus_tracker.clone()),
                                value: value.clone(),
                                visual_transformation: None,
                                keyboard_type: None,
                                ime_action: None,
                                enabled: config.enabled,
                                read_only: false,
                                max_lines: None,
                                min_lines: None,
                                cursor_color: config
                                    .colors
                                    .as_ref()
                                    .map(|c| c.cursor_color(config.is_error)),
                                on_text_layout: None,
                            }),
                    )
                    .semantics(Semantics {
                        role: Role::TextField,
                        label: None,
                        focused: false,
                        enabled: true,
                    }),
                config.trailing_icon.unwrap_or(Box(Modifier::new())),
            )),
            // Floating label
            if let Some(lbl) = label_str {
                Box(Modifier::new()
                    .min_width(200.0)
                    .padding_values(PaddingValues {
                        left: 20.0,
                        right: 20.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .absolute()
                    .offset(Some(0.0), Some(label_y), None, None))
                .child(
                    Box(Modifier::new()
                        .background(th.surface_container_highest)
                        .padding_values(PaddingValues {
                            left: 4.0,
                            right: 4.0,
                            top: 2.0,
                            bottom: 2.0,
                        }))
                    .child(
                        Text(lbl.as_ref().to_string())
                            .color(label_color)
                            .size(label_size),
                    ),
                )
            } else {
                Box(Modifier::new())
            },
        )),
    )
}

/// Configuration for [`Checkbox`].
#[derive(Clone, Debug)]
pub struct CheckboxConfig {
    pub modifier: Modifier,
    /// When false, the checkbox renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub checked_color: Color,
    pub unchecked_color: Color,
    pub checkmark_color: Color,
    /// Border color when checked. Default: same as `checked_color`.
    pub checked_border_color: Color,
    /// Border color when unchecked. Default: same as `unchecked_color`.
    pub unchecked_border_color: Color,
    pub disabled_checked_box_color: Color,
    pub disabled_unchecked_box_color: Color,
    pub disabled_indeterminate_box_color: Color,
    pub disabled_checkmark_color: Color,
    pub disabled_checked_border_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_indeterminate_border_color: Color,
    pub state_colors: StateColors,
}

impl Default for CheckboxConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            checked_color: CheckboxDefaults::checked_color(),
            unchecked_color: CheckboxDefaults::unchecked_color(),
            checkmark_color: CheckboxDefaults::checkmark_color(),
            checked_border_color: CheckboxDefaults::checked_color(),
            unchecked_border_color: CheckboxDefaults::unchecked_color(),
            disabled_checked_box_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_unchecked_box_color: Color::TRANSPARENT,
            disabled_indeterminate_box_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_checkmark_color: CheckboxDefaults::disabled_checkmark_color(),
            disabled_checked_border_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_unchecked_border_color: CheckboxDefaults::disabled_unchecked_border_color(),
            disabled_indeterminate_border_color: CheckboxDefaults::disabled_checked_box_color(),
            state_colors: CheckboxDefaults::state_colors_default(),
        }
    }
}

/// M3 Checkbox.
/// Renders a 40dp touch-target with an 18dp check box inside.
/// Fill, border, and check mark animate with 100ms FastOutSlowIn.
static CHECKBOX_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Checkbox(checked: bool, on_change: impl Fn(bool) + 'static, config: CheckboxConfig) -> View {
    let th = theme();
    let sz = CheckboxDefaults::BOX_SIZE;

    let id = remember(|| CHECKBOX_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color_fast;

    let is_enabled = config.enabled;

    let fill = animate_color(
        format!("cb_fill_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_box_color
            } else {
                config.disabled_unchecked_box_color
            }
        } else if checked {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("cb_bw_{}", id),
        if !is_enabled && checked {
            0.0
        } else if !is_enabled {
            CheckboxDefaults::STROKE_WIDTH
        } else if checked {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("cb_bd_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if checked {
            Color::TRANSPARENT
        } else {
            config.unchecked_border_color
        },
        spec,
    );
    let check_alpha = animate_f32(
        format!("cb_ca_{}", id),
        if checked { 1.0 } else { 0.0 },
        spec,
    );
    let check_col = if !is_enabled {
        config.disabled_checkmark_color
    } else {
        config.checkmark_color
    };

    let cb = move |_| {
        if config.enabled {
            on_change(!checked)
        }
    };

    Box(Modifier::new()
        .width(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .height(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .state_colors(config.state_colors)
        .clickable()
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .on_pointer_down(cb)
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(sz, sz)
            .background(fill)
            .border(bd_w, bd, CheckboxDefaults::CORNER_RADIUS)
            .clip_rounded(CheckboxDefaults::CORNER_RADIUS)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center))
        .child(if check_alpha > 0.01 {
            Box(Modifier::new().alpha(check_alpha)).child(
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(check_col)
                    .size(CheckboxDefaults::CHECK_ICON_SIZE),
            )
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Three-state value for [`TriStateCheckbox`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriState {
    Checked,
    Unchecked,
    Indeterminate,
}

/// M3 Tri-State Checkbox - cycles through Checked → Indeterminate → Unchecked.
/// Indeterminate shows a dash instead of a checkmark.
pub fn TriStateCheckbox(
    state: TriState,
    on_change: impl Fn(TriState) + 'static,
    config: CheckboxConfig,
) -> View {
    let th = theme();
    let sz = CheckboxDefaults::BOX_SIZE;

    let id = remember(|| CHECKBOX_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color_fast;

    let is_checked = state == TriState::Checked;
    let is_indeterminate = state == TriState::Indeterminate;
    let has_fill = is_checked || is_indeterminate;
    let is_enabled = config.enabled;

    let fill = animate_color(
        format!("tc_fill_{}", id),
        if !is_enabled {
            if has_fill {
                config.disabled_indeterminate_box_color
            } else {
                config.disabled_unchecked_box_color
            }
        } else if has_fill {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("tc_bw_{}", id),
        if !is_enabled {
            if has_fill {
                0.0
            } else {
                CheckboxDefaults::STROKE_WIDTH
            }
        } else if has_fill {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("tc_bd_{}", id),
        if !is_enabled {
            if has_fill {
                config.disabled_indeterminate_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if has_fill {
            Color::TRANSPARENT
        } else {
            config.unchecked_border_color
        },
        spec,
    );
    let symbol_alpha = animate_f32(
        format!("tc_sa_{}", id),
        if has_fill { 1.0 } else { 0.0 },
        spec,
    );
    let symbol_col = if !is_enabled {
        config.disabled_checkmark_color
    } else {
        config.checkmark_color
    };

    Box(Modifier::new()
        .width(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .height(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .clickable()
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .on_pointer_down(move |_| {
            if is_enabled {
                on_change(match state {
                    TriState::Checked => TriState::Unchecked,
                    TriState::Indeterminate => TriState::Checked,
                    TriState::Unchecked => TriState::Checked,
                })
            }
        })
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(sz, sz)
            .background(fill)
            .border(bd_w, bd, CheckboxDefaults::CORNER_RADIUS)
            .clip_rounded(CheckboxDefaults::CORNER_RADIUS)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center))
        .child(if symbol_alpha > 0.01 {
            Box(Modifier::new().alpha(symbol_alpha)).child(if is_indeterminate {
                // Dash for indeterminate
                Box(Modifier::new()
                    .width(10.0)
                    .height(2.0)
                    .background(symbol_col)
                    .clip_rounded(1.0))
            } else {
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(symbol_col)
                    .size(CheckboxDefaults::CHECK_ICON_SIZE)
            })
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Configuration for [`RadioButton`].
#[derive(Clone, Debug)]
pub struct RadioButtonConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub selected_color: Color,
    pub unselected_color: Color,
    pub disabled_selected_color: Color,
    pub disabled_unselected_color: Color,
    pub state_colors: StateColors,
}

impl Default for RadioButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            selected_color: RadioButtonDefaults::selected_color(),
            unselected_color: RadioButtonDefaults::unselected_color(),
            disabled_selected_color: RadioButtonDefaults::disabled_selected_color(),
            disabled_unselected_color: RadioButtonDefaults::disabled_unselected_color(),
            state_colors: RadioButtonDefaults::state_colors_default(),
        }
    }
}

/// M3 RadioButton.
/// Renders a 40dp touch-target with a 20dp outer circle + inner dot.
/// Ring color animates with 100ms FastOutSlowIn; dot size animates with spring.
static RADIO_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn RadioButton(
    selected: bool,
    on_select: impl Fn() + 'static,
    config: RadioButtonConfig,
) -> View {
    let th = theme();
    let d = RadioButtonDefaults::OUTER_RADIUS * 2.0;

    let id = remember(|| RADIO_COUNTER.fetch_add(1, Ordering::Relaxed));
    let color_spec = th.motion.color_fast;
    let spring = th.motion.spring;

    let ring_col = animate_color(
        format!("rb_ring_{}", id),
        if !config.enabled {
            if selected {
                config.disabled_selected_color
            } else {
                config.disabled_unselected_color
            }
        } else if selected {
            config.selected_color
        } else {
            config.unselected_color
        },
        color_spec,
    );
    let dot_size = animate_f32(
        format!("rb_dot_{}", id),
        if selected {
            RadioButtonDefaults::DOT_RADIUS * 2.0
        } else {
            0.0
        },
        spring,
    );
    let dot_col = if !config.enabled {
        config.disabled_selected_color
    } else {
        config.selected_color
    };

    let cb = move |_| {
        if config.enabled {
            on_select()
        }
    };

    Box(Modifier::new()
        .width(RadioButtonDefaults::TOUCH_TARGET_SIZE)
        .height(RadioButtonDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .state_colors(config.state_colors)
        .clickable()
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .on_pointer_down(cb)
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(d, d)
            .border(RadioButtonDefaults::STROKE_WIDTH, ring_col, d * 0.5)
            .clip_rounded(d * 0.5)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center))
        .child(if dot_size > 0.5 {
            Box(Modifier::new()
                .size(dot_size, dot_size)
                .background(dot_col)
                .clip_rounded(dot_size * 0.5))
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Configuration for [`Switch`].
#[derive(Clone, Debug)]
pub struct SwitchConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub checked_track_color: Color,
    pub unchecked_track_color: Color,
    pub checked_thumb_color: Color,
    pub unchecked_thumb_color: Color,
    /// Icon color for the thumb content when checked. Default: `on_primary`.
    pub checked_icon_color: Color,
    /// Icon color for the thumb content when unchecked. Default: `outline`.
    pub unchecked_icon_color: Color,
    /// Border color when checked. Default: transparent.
    pub checked_border_color: Color,
    /// Border color when unchecked.
    pub unchecked_border_color: Color,
    pub disabled_checked_thumb_color: Color,
    pub disabled_checked_track_color: Color,
    pub disabled_checked_border_color: Color,
    pub disabled_checked_icon_color: Color,
    pub disabled_unchecked_thumb_color: Color,
    pub disabled_unchecked_track_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_unchecked_icon_color: Color,
    pub state_colors: StateColors,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            checked_track_color: SwitchDefaults::checked_track_color(),
            unchecked_track_color: SwitchDefaults::unchecked_track_color(),
            checked_thumb_color: SwitchDefaults::checked_thumb_color(),
            unchecked_thumb_color: SwitchDefaults::unchecked_thumb_color(),
            checked_icon_color: SwitchDefaults::checked_icon_color(),
            unchecked_icon_color: SwitchDefaults::unchecked_icon_color(),
            checked_border_color: Color::TRANSPARENT,
            unchecked_border_color: SwitchDefaults::unchecked_border_color(),
            disabled_checked_thumb_color: SwitchDefaults::disabled_checked_thumb_color(),
            disabled_checked_track_color: SwitchDefaults::disabled_checked_track_color(),
            disabled_checked_border_color: Color::TRANSPARENT,
            disabled_checked_icon_color: SwitchDefaults::disabled_checked_icon_color(),
            disabled_unchecked_thumb_color: SwitchDefaults::disabled_unchecked_thumb_color(),
            disabled_unchecked_track_color: SwitchDefaults::disabled_unchecked_track_color(),
            disabled_unchecked_border_color: SwitchDefaults::disabled_unchecked_border_color(),
            disabled_unchecked_icon_color: SwitchDefaults::disabled_unchecked_icon_color(),
            state_colors: SwitchDefaults::state_colors_default(),
        }
    }
}

/// M3 Switch.
/// Renders a pill track with an animated thumb knob.
/// Thumb position, size, and colors animate with spring/tween physics.
static SWITCH_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Switch(checked: bool, on_change: impl Fn(bool) + 'static, config: SwitchConfig) -> View {
    let th = theme();
    let track_w = SwitchDefaults::TRACK_WIDTH;
    let track_h = SwitchDefaults::TRACK_HEIGHT;

    let id = remember(|| SWITCH_COUNTER.fetch_add(1, Ordering::Relaxed));

    let hovered = remember(|| Signal::new(false));
    let pressed = remember(|| Signal::new(false));

    // Thumb: spring-animated position and size
    let thumb_target_pos = if checked {
        track_w - SwitchDefaults::THUMB_CHECKED_SIZE - 4.0
    } else {
        8.0
    };
    let thumb_target_d = if checked {
        SwitchDefaults::THUMB_CHECKED_SIZE
    } else {
        SwitchDefaults::THUMB_UNCHECKED_SIZE
    };
    let spring = th.motion.spring;

    let thumb_left = animate_f32(format!("sw_pos_{}", id), thumb_target_pos, spring);
    let thumb_d = animate_f32(format!("sw_d_{}", id), thumb_target_d, spring);
    let thumb_top = (track_h - thumb_d) * 0.5;

    let color_spec = th.motion.color_fast;
    let is_enabled = config.enabled;

    let track_bg = animate_color(
        format!("sw_tbg_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_track_color
            } else {
                config.disabled_unchecked_track_color
            }
        } else if checked {
            config.checked_track_color
        } else {
            config.unchecked_track_color
        },
        color_spec,
    );
    let thumb_bg = animate_color(
        format!("sw_tmbg_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_thumb_color
            } else {
                config.disabled_unchecked_thumb_color
            }
        } else if checked {
            config.checked_thumb_color
        } else {
            config.unchecked_thumb_color
        },
        color_spec,
    );
    let track_border = animate_f32(
        format!("sw_tb_{}", id),
        if !is_enabled {
            if checked { 0.0 } else { 2.0 }
        } else if checked {
            0.0
        } else {
            2.0
        },
        color_spec,
    );
    let border_color = animate_color(
        format!("sw_bc_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if checked {
            config.checked_border_color
        } else {
            config.unchecked_border_color
        },
        color_spec,
    );

    let state_overlay = animate_color(
        format!("sw_ol_{}", id),
        if !is_enabled {
            Color::TRANSPARENT
        } else if pressed.get() {
            config.state_colors.pressed
        } else if hovered.get() {
            config.state_colors.hovered
        } else {
            config.state_colors.default
        },
        color_spec,
    );

    Box(Modifier::new()
        .size(track_w, track_h)
        .padding(0.0)
        .clip_rounded(track_h * 0.5)
        .background(track_bg)
        .border(track_border, border_color, track_h * 0.5)
        .clickable()
        .on_pointer_enter({
            let h = hovered.clone();
            move |_| h.set(true)
        })
        .on_pointer_leave({
            let h = hovered.clone();
            let p = pressed.clone();
            move |_| {
                h.set(false);
                p.set(false);
            }
        })
        .on_pointer_down({
            let p = pressed.clone();
            let cb = on_change;
            let en = is_enabled;
            move |_| {
                p.set(true);
                if en {
                    cb(!checked);
                }
            }
        })
        .on_pointer_up({
            let p = pressed.clone();
            move |_| p.set(false)
        })
        .then(config.modifier))
    .child((
        Box(Modifier::new()
            .size(thumb_d, thumb_d)
            .background(thumb_bg)
            .clip_rounded(thumb_d * 0.5)
            .absolute()
            .offset(Some(thumb_left), Some(thumb_top), None, None)),
        Box(Modifier::new()
            .size(40.0, 40.0)
            .clip_rounded(20.0)
            .background(state_overlay)
            .absolute()
            .offset(
                Some(thumb_left + thumb_d * 0.5 - 20.0),
                Some(track_h * 0.5 - 20.0),
                None,
                None,
            )),
    ))
}

/// Configuration for [`Slider`] and [`RangeSlider`].
#[derive(Clone, Debug)]
pub struct SliderConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to input.
    pub enabled: bool,
    pub active_track_color: Color,
    pub inactive_track_color: Color,
    pub thumb_color: Color,
    pub active_tick_color: Color,
    pub inactive_tick_color: Color,
    pub disabled_thumb_color: Color,
    pub disabled_active_track_color: Color,
    pub disabled_inactive_track_color: Color,
    pub disabled_active_tick_color: Color,
    pub disabled_inactive_tick_color: Color,
    pub state_colors: StateColors,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            active_track_color: SliderDefaults::active_track_color(),
            inactive_track_color: SliderDefaults::inactive_track_color(),
            thumb_color: SliderDefaults::thumb_color(),
            active_tick_color: SliderDefaults::active_tick_color(),
            inactive_tick_color: SliderDefaults::inactive_tick_color(),
            disabled_thumb_color: SliderDefaults::disabled_thumb_color(),
            disabled_active_track_color: SliderDefaults::disabled_active_track_color(),
            disabled_inactive_track_color: SliderDefaults::disabled_inactive_track_color(),
            disabled_active_tick_color: SliderDefaults::disabled_active_tick_color(),
            disabled_inactive_tick_color: SliderDefaults::disabled_inactive_tick_color(),
            state_colors: SliderDefaults::state_colors_default(),
        }
    }
}

static SLIDER_COUNTER: AtomicU64 = AtomicU64::new(0);

fn snap_step(v: f32, min: f32, max: f32, step: Option<f32>) -> f32 {
    let v = v.clamp(min, max);
    if let Some(s) = step.filter(|s| *s > 0.0) {
        let t = ((v - min) / s).round();
        (min + t * s).clamp(min, max)
    } else {
        v
    }
}

fn value_from_x(x: f32, rect: Rect, min: f32, max: f32, step: Option<f32>) -> f32 {
    let w = rect.w.max(1.0);
    let t = ((x - rect.x) / w).clamp(0.0, 1.0);
    let v = min + t * (max - min);
    snap_step(v, min, max, step)
}

pub fn Slider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
    config: SliderConfig,
) -> View {
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("ms_rect_{}", id), || Rect::default());
    let drag_active = remember_state_with_key(format!("ms_da_{}", id), || false);
    let hovered = remember(|| Signal::new(false));

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let hovered_sig = hovered.clone();
    let sc = config.state_colors;

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t = ((value - min) / range_size).clamp(0.0, 1.0);

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    Box(Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(16.0);
            let thumb_w = dp_to_px(4.0);
            let thumb_h = dp_to_px(44.0);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = thumb_w * 0.5 + dp_to_px(ProgressIndicatorDefaults::SLIDER_THUMB_TRACK_GAP);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            let kx = if step.is_some() && !tick_frac.is_empty() {
                let is_first = (t - tick_frac[0]).abs() < 1e-6;
                let is_last = (t - tick_frac[tick_frac.len() - 1]).abs() < 1e-6;
                if is_first || is_last {
                    track_x + t * track_w
                } else {
                    track_x + (track_w - track_h) * t + corner
                }
            } else {
                track_x + t * track_w
            };

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            let inactive_x = track_x.max(kx + gap);
            let inactive_w = (track_x + track_w - inactive_x).max(0.0);
            if inactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: inactive_x,
                        y: cy - track_h * 0.5,
                        w: inactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.inactive_track_color)),
                    radius: [corner; 4],
                });
            }
            let fill_w = (kx - gap - track_x).max(0.0);
            if fill_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: fill_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.active_track_color)),
                    radius: [corner; 4],
                });
            }
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for (i, &tf) in tick_frac.iter().enumerate() {
                let tx = tick_start + tf * (tick_end - tick_start);
                // skip ticks that fall on the stop indicator (last)
                if i == tick_frac.len() - 1 {
                    continue;
                }
                if tx >= kx - gap && tx <= kx + gap {
                    continue;
                }
                let on_active = tx <= kx - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active {
                        config.active_tick_color
                    } else {
                        config.inactive_tick_color
                    })),
                });
            }
            let sx = track_x + track_w - corner;
            scene.nodes.push(SceneNode::Ellipse {
                rect: Rect {
                    x: sx - dot_r,
                    y: cy - dot_r,
                    w: dot_r * 2.0,
                    h: dot_r * 2.0,
                },
                brush: Brush::Solid(mul_c(config.inactive_tick_color)),
            });
            let da = *drag_active_p.borrow();
            let hv = hovered_sig.get();
            let tw = if da { thumb_w * 0.5 } else { thumb_w };
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: kx - tw * 0.5,
                    y: cy - thumb_h * 0.5,
                    w: tw,
                    h: thumb_h,
                },
                brush: Brush::Solid(mul_c(config.thumb_color)),
                radius: [tw * 0.5; 4],
            });
            let sc_target = if da {
                sc.pressed
            } else if hv {
                sc.hovered
            } else {
                sc.default
            };
            if sc_target.3 > 0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: kx - tw * 0.5,
                        y: cy - thumb_h * 0.5,
                        w: tw,
                        h: thumb_h,
                    },
                    brush: Brush::Solid(mul_c(sc_target)),
                    radius: [tw * 0.5; 4],
                });
            }
        })
        .on_pointer_enter({
            let h = hovered.clone();
            move |_pe: PointerEvent| h.set(true)
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| h.set(false)
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            move |pe: PointerEvent| {
                *drag_active.borrow_mut() = true;
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position.x, r, min, max, step));
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            move |pe: PointerEvent| {
                if !*drag_active.borrow() {
                    return;
                }
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position.x, r, min, max, step));
            }
        })
        .on_pointer_up(move |_pe: PointerEvent| {
            *drag_active.borrow_mut() = false;
        })
        .on_scroll({
            let oc = oc.clone();
            move |d: Vec2| -> Vec2 {
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let new_val = snap_step(value + (dir as f32) * step_val, min, max, step);
                if (new_val - value).abs() > 1e-6 {
                    (oc)(new_val);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        })
        .then(config.modifier))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn RangeSlider(
    start: f32,
    end: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32, f32) + 'static,
    config: SliderConfig,
) -> View {
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("mrs_rect_{}", id), || Rect::default());
    let drag_active = remember_state_with_key(format!("mrs_da_{}", id), || false);
    let active_thumb = remember_state_with_key(format!("mrs_at_{}", id), || false);
    let hovered = remember(|| Signal::new(false));

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t0 = ((start - min) / range_size).clamp(0.0, 1.0);
    let t1 = ((end - min) / range_size).clamp(0.0, 1.0);
    let sc = config.state_colors;
    let is_enabled = config.enabled;

    let act_trk = if !is_enabled {
        config.disabled_active_track_color
    } else {
        config.active_track_color
    };
    let inact_trk = if !is_enabled {
        config.disabled_inactive_track_color
    } else {
        config.inactive_track_color
    };
    let act_tick = if !is_enabled {
        config.disabled_active_tick_color
    } else {
        config.active_tick_color
    };
    let inact_tick = if !is_enabled {
        config.disabled_inactive_tick_color
    } else {
        config.inactive_tick_color
    };
    let thumb_col = if !is_enabled {
        config.disabled_thumb_color
    } else {
        config.thumb_color
    };

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let active_thumb_p = active_thumb.clone();
    let hovered_sig = hovered.clone();

    Box(Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(16.0);
            let thumb_w = dp_to_px(4.0);
            let thumb_h = dp_to_px(44.0);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = thumb_w * 0.5 + dp_to_px(ProgressIndicatorDefaults::SLIDER_THUMB_TRACK_GAP);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            let thumb_pos = |tf: f32, fracs: &[f32]| {
                if step.is_some() && !fracs.is_empty() {
                    let is_first = (tf - fracs[0]).abs() < 1e-6;
                    let is_last = (tf - fracs[fracs.len() - 1]).abs() < 1e-6;
                    if is_first || is_last {
                        track_x + tf * track_w
                    } else {
                        track_x + (track_w - track_h) * tf + corner
                    }
                } else {
                    track_x + tf * track_w
                }
            };
            let k0 = thumb_pos(t0, &tick_frac);
            let k1 = thumb_pos(t1, &tick_frac);
            let active_l = k0.min(k1);
            let active_r = k0.max(k1);

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            let linactive_w = (active_l - gap - track_x).max(0.0);
            if linactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: linactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(inact_trk)),
                    radius: [corner; 4],
                });
            }
            let rinactive_x = (active_r + gap).min(track_x + track_w);
            let rinactive_w = (track_x + track_w - rinactive_x).max(0.0);
            if rinactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rinactive_x,
                        y: cy - track_h * 0.5,
                        w: rinactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(inact_trk)),
                    radius: [corner; 4],
                });
            }
            let active_w = (active_r - gap - (active_l + gap)).max(0.0);
            if active_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: active_l + gap,
                        y: cy - track_h * 0.5,
                        w: active_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(act_trk)),
                    radius: [corner; 4],
                });
            }
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for (i, &tf) in tick_frac.iter().enumerate() {
                let tx = tick_start + tf * (tick_end - tick_start);
                // skip ticks that fall on the stop indicators (first and last)
                if i == 0 || i == tick_frac.len() - 1 {
                    continue;
                }
                let in_lgap = tx >= active_l - gap && tx <= active_l + gap;
                let in_rgap = tx >= active_r - gap && tx <= active_r + gap;
                if in_lgap || in_rgap {
                    continue;
                }
                let on_active = tx >= active_l + gap && tx <= active_r - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active { act_tick } else { inact_tick })),
                });
            }
            // Stop indicators at both track ends (always drawn, matching Compose)
            let sx0 = track_x + corner;
            scene.nodes.push(SceneNode::Ellipse {
                rect: Rect {
                    x: sx0 - dot_r,
                    y: cy - dot_r,
                    w: dot_r * 2.0,
                    h: dot_r * 2.0,
                },
                brush: Brush::Solid(mul_c(inact_tick)),
            });
            let sx = track_x + track_w - corner;
            scene.nodes.push(SceneNode::Ellipse {
                rect: Rect {
                    x: sx - dot_r,
                    y: cy - dot_r,
                    w: dot_r * 2.0,
                    h: dot_r * 2.0,
                },
                brush: Brush::Solid(mul_c(inact_tick)),
            });
            let da = *drag_active_p.borrow();
            let at = *active_thumb_p.borrow();
            let hv = hovered_sig.get();
            let thumbs = [k0, k1];
            for (idx, &kx) in thumbs.iter().enumerate() {
                let is_active = da && (if idx == 0 { !at } else { at });
                let tw = if is_active { thumb_w * 0.5 } else { thumb_w };
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: kx - tw * 0.5,
                        y: cy - thumb_h * 0.5,
                        w: tw,
                        h: thumb_h,
                    },
                    brush: Brush::Solid(mul_c(thumb_col)),
                    radius: [tw * 0.5; 4],
                });
                let sc_target = if !is_enabled {
                    Color::TRANSPARENT
                } else if is_active {
                    sc.pressed
                } else if hv {
                    sc.hovered
                } else {
                    sc.default
                };
                if sc_target.3 > 0 {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: kx - tw * 0.5,
                            y: cy - thumb_h * 0.5,
                            w: tw,
                            h: thumb_h,
                        },
                        brush: Brush::Solid(mul_c(sc_target)),
                        radius: [tw * 0.5; 4],
                    });
                }
            }
        })
        .on_pointer_enter({
            let h = hovered.clone();
            let en = is_enabled;
            move |_pe: PointerEvent| {
                if en {
                    h.set(true);
                }
            }
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| h.set(false)
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |pe: PointerEvent| {
                if !en {
                    return;
                }
                *drag_active.borrow_mut() = true;
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position.x, r, min, max, step);
                let use_end = (v - end).abs() < (v - start).abs();
                *active_thumb.borrow_mut() = use_end;
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |pe: PointerEvent| {
                if !*drag_active.borrow() {
                    return;
                }
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position.x, r, min, max, step);
                let use_end = *active_thumb.borrow();
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
            }
        })
        .on_pointer_up({
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |_pe: PointerEvent| {
                *drag_active.borrow_mut() = false;
                *active_thumb.borrow_mut() = false;
            }
        })
        .on_scroll({
            let oc = oc.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |d: Vec2| -> Vec2 {
                if !en {
                    return d;
                }
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let use_end = *active_thumb.borrow();
                let (mut a, mut b) = (start, end);
                if use_end {
                    b = snap_step(end + (dir as f32) * step_val, min, max, step).max(a);
                } else {
                    a = snap_step(start + (dir as f32) * step_val, min, max, step).min(b);
                }
                if (a - start).abs() > 1e-6 || (b - end).abs() > 1e-6 {
                    (oc)(a, b);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        })
        .then(config.modifier))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: is_enabled,
    })
}

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
    let mut m = Modifier::new()
        .background(bg)
        .clip_rounded(config.shape_radius)
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
            disabled: 0.0,
        });
    }
    Box(m).color(fg).child(content())
}

/// M3 Elevated Card - card with elevation.
pub fn ElevatedCard(modifier: Modifier, content: View) -> View {
    let th = theme();
    Card(
        CardConfig {
            modifier,
            container_color: CardDefaults::elevated_container_color(),
            state_elevation: Some(StateElevation {
                default: th.elevation.level1,
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                disabled: 0.0,
            }),
            ..Default::default()
        },
        || Column(Modifier::new().fill_max_size()).child(content),
    )
}

/// M3 Outlined Card - card with border outline.
pub fn OutlinedCard(modifier: Modifier, content: View) -> View {
    Card(
        CardConfig {
            modifier,
            container_color: CardDefaults::outlined_container_color(),
            border: Some((1.0, CardDefaults::outlined_border_color())),
            ..Default::default()
        },
        || Column(Modifier::new().fill_max_size()).child(content),
    )
}

fn card_state_colors(bg: Color) -> StateColors {
    let th = theme();
    StateColors {
        default: Color::TRANSPARENT,
        hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
        pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
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

/// Configuration for [`Snackbar`].
#[derive(Clone, Debug)]
pub struct SnackbarConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub action_color: Color,
    pub dismiss_action_content_color: Color,
    pub action_on_new_line: bool,
    pub shape_radius: f32,
    pub min_height: f32,
    pub min_width: f32,
    pub max_width: f32,
}

impl Default for SnackbarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: SnackbarDefaults::container_color(),
            content_color: SnackbarDefaults::content_color(),
            action_color: SnackbarDefaults::action_color(),
            dismiss_action_content_color: SnackbarDefaults::dismiss_action_content_color(),
            action_on_new_line: false,
            shape_radius: SnackbarDefaults::SHAPE_RADIUS,
            min_height: SnackbarDefaults::MIN_HEIGHT,
            min_width: SnackbarDefaults::MIN_WIDTH,
            max_width: SnackbarDefaults::MAX_WIDTH,
        }
    }
}

/// Color slots for chips (both non-selectable and selectable).
#[derive(Clone, Copy, Debug)]
pub struct ChipColors {
    pub container_color: Color,
    pub label_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,
    pub disabled_container_color: Color,
    pub disabled_label_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub selected_container_color: Color,
    pub selected_label_color: Color,
    pub selected_leading_icon_color: Color,
    pub selected_trailing_icon_color: Color,
    pub disabled_selected_container_color: Color,
}

impl ChipColors {
    pub fn container(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (true, true) => self.selected_container_color,
            (true, false) => self.container_color,
            (false, true) => self.disabled_selected_container_color,
            (false, false) => self.disabled_container_color,
        }
    }
    pub fn label(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_label_color
        } else if selected {
            self.selected_label_color
        } else {
            self.label_color
        }
    }
    pub fn leading_icon(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if selected {
            self.selected_leading_icon_color
        } else {
            self.leading_icon_color
        }
    }
    pub fn trailing_icon(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if selected {
            self.selected_trailing_icon_color
        } else {
            self.trailing_icon_color
        }
    }
}

/// Elevation levels for chips.
#[derive(Clone, Copy, Debug)]
pub struct ChipElevation {
    pub default: f32,
    pub hovered: f32,
    pub focused: f32,
    pub pressed: f32,
    pub dragged: f32,
    pub disabled: f32,
}

impl ChipElevation {
    pub fn to_state_elevation(&self) -> StateElevation {
        StateElevation {
            default: self.default,
            hovered: self.hovered,
            pressed: self.pressed,
            disabled: self.disabled,
        }
    }
}

impl Default for ChipElevation {
    fn default() -> Self {
        Self {
            default: ChipDefaults::elevation_default(),
            hovered: ChipDefaults::elevation_hovered(),
            focused: ChipDefaults::elevation_focused(),
            pressed: ChipDefaults::elevation_pressed(),
            dragged: ChipDefaults::elevation_dragged(),
            disabled: ChipDefaults::elevation_disabled(),
        }
    }
}

/// Configuration for chips.
#[derive(Clone, Debug)]
pub struct ChipConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub colors: ChipColors,
    pub elevation: ChipElevation,
    pub border_width: f32,
    pub border_color: Color,
    pub selected_border_color: Color,
    pub disabled_border_color: Color,
    pub disabled_selected_border_color: Color,
    pub shape_radius: f32,
    pub horizontal_padding: f32,
}

impl Default for ChipConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            colors: ChipColors {
                container_color: ChipDefaults::container_color(),
                label_color: ChipDefaults::label_color(),
                leading_icon_color: ChipDefaults::leading_icon_color(),
                trailing_icon_color: ChipDefaults::trailing_icon_color(),
                disabled_container_color: ChipDefaults::disabled_container_color(),
                disabled_label_color: ChipDefaults::disabled_label_color(),
                disabled_leading_icon_color: ChipDefaults::disabled_leading_icon_color(),
                disabled_trailing_icon_color: ChipDefaults::disabled_trailing_icon_color(),
                selected_container_color: ChipDefaults::selected_container_color(),
                selected_label_color: ChipDefaults::selected_label_color(),
                selected_leading_icon_color: ChipDefaults::selected_leading_icon_color(),
                selected_trailing_icon_color: ChipDefaults::selected_trailing_icon_color(),
                disabled_selected_container_color: ChipDefaults::disabled_selected_container_color(
                ),
            },
            elevation: ChipElevation::default(),
            border_width: ChipDefaults::BORDER_WIDTH,
            border_color: ChipDefaults::border_color(),
            selected_border_color: ChipDefaults::selected_border_color(),
            disabled_border_color: ChipDefaults::disabled_border_color(),
            disabled_selected_border_color: ChipDefaults::disabled_selected_border_color(),
            shape_radius: ChipDefaults::SHAPE_RADIUS,
            horizontal_padding: ChipDefaults::HORIZONTAL_PADDING,
        }
    }
}

/// M3 Assist Chip - a chip for triggering actions.
pub fn AssistChip(
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let trailing_color = colors.trailing_icon(is_enabled, false);
    let border = if is_enabled {
        config.border_color
    } else {
        config.disabled_border_color
    };
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

/// M3 Elevated Assist Chip - like [`AssistChip`] but with elevated container.
pub fn ElevatedAssistChip(
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let trailing_color = colors.trailing_icon(is_enabled, false);
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

/// Configuration for [`NavigationBar`].
#[derive(Clone, Debug)]
pub struct NavigationBarConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub selected_icon_color: Color,
    pub unselected_icon_color: Color,
    pub indicator_color: Color,
    pub height: f32,
    pub tonal_elevation: f32,
    pub indicator_opacity: f32,
    pub item_horizontal_padding: f32,
    pub item_vertical_padding: f32,
    pub indicator_radius: f32,
}

impl Default for NavigationBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationBarDefaults::container_color(),
            content_color: NavigationBarDefaults::content_color(),
            selected_icon_color: NavigationBarDefaults::selected_icon_color(),
            unselected_icon_color: NavigationBarDefaults::unselected_icon_color(),
            indicator_color: NavigationBarDefaults::indicator_color(),
            height: NavigationBarDefaults::HEIGHT,
            tonal_elevation: NavigationBarDefaults::TONAL_ELEVATION,
            indicator_opacity: NavigationBarDefaults::ITEM_ACTIVE_INDICATOR_OPACITY,
            item_horizontal_padding: NavigationBarDefaults::ITEM_HORIZONTAL_PADDING,
            item_vertical_padding: NavigationBarDefaults::ITEM_VERTICAL_PADDING,
            indicator_radius: NavigationBarDefaults::INDICATOR_RADIUS,
        }
    }
}

/// Configuration for [`NavigationRail`].
#[derive(Clone, Debug)]
pub struct NavigationRailConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub selected_icon_color: Color,
    pub unselected_icon_color: Color,
    pub selected_container_color: Color,
    pub width: f32,
    pub item_radius: f32,
}

impl Default for NavigationRailConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationRailDefaults::container_color(),
            selected_icon_color: NavigationRailDefaults::selected_icon_color(),
            unselected_icon_color: NavigationRailDefaults::unselected_icon_color(),
            selected_container_color: NavigationRailDefaults::selected_container_color(),
            width: NavigationRailDefaults::WIDTH,
            item_radius: NavigationRailDefaults::ITEM_RADIUS,
        }
    }
}

/// Configuration for [`Scaffold`].
#[derive(Clone, Debug)]
pub struct ScaffoldConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub top_bar_height: f32,
    pub bottom_bar_height: f32,
    pub fab_margin: f32,
}

impl Default for ScaffoldConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: ScaffoldDefaults::container_color(),
            top_bar_height: ScaffoldDefaults::TOP_BAR_HEIGHT,
            bottom_bar_height: ScaffoldDefaults::BOTTOM_BAR_HEIGHT,
            fab_margin: ScaffoldDefaults::FAB_MARGIN,
        }
    }
}

/// Configuration for [`NavigationDrawer`].
#[derive(Clone, Debug)]
pub struct NavigationDrawerConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub scrim_color: Color,
    pub tonal_elevation: f32,
    pub width: f32,
    pub shape_radius: f32,
}

impl Default for NavigationDrawerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationDrawerDefaults::container_color(),
            content_color: NavigationDrawerDefaults::content_color(),
            scrim_color: NavigationDrawerDefaults::scrim_color(),
            tonal_elevation: NavigationDrawerDefaults::TONAL_ELEVATION,
            width: NavigationDrawerDefaults::WIDTH,
            shape_radius: NavigationDrawerDefaults::SHAPE_RADIUS,
        }
    }
}

/// Configuration for [`BottomSheet`] / `ModalBottomSheet`.
#[derive(Clone, Debug)]
pub struct BottomSheetConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub scrim_color: Color,
    pub tonal_elevation: f32,
    pub drag_handle_color: Color,
    pub shape_radius: f32,
    pub max_width: f32,
    pub drag_handle_width: f32,
    pub drag_handle_height: f32,
    pub peek_height: f32,
}

impl Default for BottomSheetConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: BottomSheetDefaults::container_color(),
            content_color: BottomSheetDefaults::content_color(),
            scrim_color: BottomSheetDefaults::scrim_color(),
            tonal_elevation: BottomSheetDefaults::TONAL_ELEVATION,
            drag_handle_color: BottomSheetDefaults::drag_handle_color(),
            shape_radius: BottomSheetDefaults::SHAPE_RADIUS,
            max_width: BottomSheetDefaults::MAX_WIDTH,
            drag_handle_width: BottomSheetDefaults::DRAG_HANDLE_WIDTH,
            drag_handle_height: BottomSheetDefaults::DRAG_HANDLE_HEIGHT,
            peek_height: BottomSheetDefaults::PEEK_HEIGHT,
        }
    }
}

/// Configuration for [`SearchBar`].
#[derive(Clone, Debug)]
pub struct SearchBarConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub active_container_color: Color,
    pub content_color: Color,
    pub placeholder_color: Color,
    pub height: f32,
    pub expanded_width: f32,
    pub collapsed_width: f32,
}

impl Default for SearchBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: SearchBarDefaults::container_color(),
            active_container_color: SearchBarDefaults::active_container_color(),
            content_color: SearchBarDefaults::content_color(),
            placeholder_color: SearchBarDefaults::placeholder_color(),
            height: SearchBarDefaults::HEIGHT,
            expanded_width: SearchBarDefaults::EXPANDED_WIDTH,
            collapsed_width: SearchBarDefaults::COLLAPSED_WIDTH,
        }
    }
}

/// Configuration for [`DropdownMenu`].
#[derive(Clone, Debug)]
pub struct DropdownMenuConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub item_text_color: Color,
    pub disabled_item_text_color: Color,
    pub divider_color: Color,
    pub min_width: f32,
    pub item_height: f32,
}

impl Default for DropdownMenuConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: DropdownMenuDefaults::container_color(),
            item_text_color: DropdownMenuDefaults::item_text_color(),
            disabled_item_text_color: DropdownMenuDefaults::disabled_item_text_color(),
            divider_color: DropdownMenuDefaults::divider_color(),
            min_width: DropdownMenuDefaults::MIN_WIDTH,
            item_height: DropdownMenuDefaults::ITEM_HEIGHT,
        }
    }
}

/// Configuration for tooltip.
#[derive(Clone, Debug)]
pub struct TooltipConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub offset_y: f32,
    pub horizontal_padding: f32,
    pub vertical_padding: f32,
}

impl Default for TooltipConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: TooltipDefaults::container_color(),
            content_color: TooltipDefaults::content_color(),
            offset_y: TooltipDefaults::OFFSET_Y,
            horizontal_padding: TooltipDefaults::HORIZONTAL_PADDING,
            vertical_padding: TooltipDefaults::VERTICAL_PADDING,
        }
    }
}

/// Configuration for swipe-to-dismiss.
#[derive(Clone, Debug)]
pub struct SwipeToDismissConfig {
    pub modifier: Modifier,
    pub dismiss_threshold: f32,
    pub dismissed_offset: f32,
    pub animation_spec: AnimationSpec,
}

impl Default for SwipeToDismissConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            dismiss_threshold: SwipeToDismissDefaults::DISMISS_THRESHOLD,
            dismissed_offset: SwipeToDismissDefaults::DISMISSED_OFFSET,
            animation_spec: AnimationSpec::spring_gentle(),
        }
    }
}

/// Configuration for pull-to-refresh.
#[derive(Clone, Debug)]
pub struct PullToRefreshConfig {
    pub modifier: Modifier,
    pub indicator_color: Color,
    pub threshold: f32,
}

impl Default for PullToRefreshConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            indicator_color: PullToRefreshDefaults::indicator_color(),
            threshold: PullToRefreshDefaults::THRESHOLD,
        }
    }
}
