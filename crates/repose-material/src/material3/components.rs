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
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub border: Option<(f32, Color)>,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: SurfaceDefaults::color(),
            shape_radius: SurfaceDefaults::SHAPE_RADIUS,
            tonal_elevation: SurfaceDefaults::TONAL_ELEVATION,
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
    if let Some((w, c)) = config.border {
        m = m.border(w, c, config.shape_radius);
    }
    Box(m).child(content())
}

/// Configuration for [`IconButton`] and [`FilledIconButton`].
#[derive(Clone, Debug)]
pub struct IconButtonConfig {
    pub modifier: Modifier,
    pub content_color: Option<Color>,
    pub container_size: Option<f32>,
    pub filler_container_color: Option<Color>,
    pub state_colors: StateColors,
}

impl Default for IconButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            content_color: None,
            container_size: None,
            filler_container_color: None,
            state_colors: IconButtonDefaults::state_colors_default(),
        }
    }
}

/// M3 Icon Button - a tappable circular container for an icon.
pub fn IconButton(icon: View, on_click: impl Fn() + 'static, config: IconButtonConfig) -> View {
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::CONTAINER_SIZE);
    Box(Modifier::new()
        .size(sz, sz)
        .clip_rounded(sz * 0.5)
        .state_colors(config.state_colors)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child(icon)
}

/// M3 Filled Icon Button - icon button with a filled container background.
pub fn FilledIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let content_color = config
        .content_color
        .unwrap_or_else(IconButtonDefaults::filled_content_color);
    let bg = config
        .filler_container_color
        .unwrap_or_else(IconButtonDefaults::filled_container_color);
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    Box(Modifier::new()
        .size(sz, sz)
        .clip_rounded(sz * 0.5)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: content_color.with_alpha_f32(0.08),
            pressed: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child(icon)
}

/// M3 Filled Tonal Icon Button - icon button with a secondary container background.
pub fn FilledTonalIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
    config: IconButtonConfig,
) -> View {
    let th = theme();
    let content_color = config
        .content_color
        .unwrap_or_else(IconButtonDefaults::filled_tonal_content_color);
    let bg = config
        .filler_container_color
        .unwrap_or_else(IconButtonDefaults::filled_tonal_container_color);
    let sz = config
        .container_size
        .unwrap_or(IconButtonDefaults::FILLED_CONTAINER_SIZE);
    Box(Modifier::new()
        .size(sz, sz)
        .clip_rounded(sz * 0.5)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: content_color.with_alpha_f32(0.08),
            pressed: content_color.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child(icon)
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
    Box(Modifier::new()
        .size(sz, sz)
        .clip_rounded(sz * 0.5)
        .state_colors(config.state_colors)
        .border(1.0, th.outline, sz * 0.5)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child(icon)
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
        }
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
    m = m.state_colors(state_colors);
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
    let content = with_content_color(
        if enabled {
            content_color
        } else {
            content_color.with_alpha_f32(0.38)
        },
        content,
    );
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
    let cc = config
        .content_color
        .unwrap_or_else(ButtonDefaults::content_color);
    let bg = config
        .container_color
        .unwrap_or_else(ButtonDefaults::container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ButtonDefaults::state_elevation_default);
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
        Some(bg),
        config.state_colors,
        Some(se),
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
    let cc = config
        .content_color
        .unwrap_or_else(ButtonDefaults::tonal_content_color);
    let bg = config
        .container_color
        .unwrap_or_else(ButtonDefaults::tonal_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ButtonDefaults::state_elevation_default);
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
        Some(bg),
        config.state_colors,
        Some(se),
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
    let cc = config
        .content_color
        .unwrap_or_else(ButtonDefaults::outlined_content_color);
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
        None,
        config.state_colors,
        None,
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
    let cc = config
        .content_color
        .unwrap_or_else(ButtonDefaults::text_content_color);
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
        None,
        config.state_colors,
        None,
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
    let cc = config
        .content_color
        .unwrap_or_else(ButtonDefaults::elevated_content_color);
    let bg = config
        .container_color
        .unwrap_or_else(ButtonDefaults::elevated_container_color);
    let se = config
        .state_elevation
        .unwrap_or_else(ButtonDefaults::elevated_state_elevation);
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
        Some(bg),
        config.state_colors,
        Some(se),
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
    Box(Modifier::new()
        .size(size, size)
        .background(config.container_color)
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
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child(icon)
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
    Row(Modifier::new()
        .height(56.0)
        .min_width(80.0)
        .background(config.container_color)
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
        .align_items(AlignItems::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(config.modifier))
    .child((
        icon.unwrap_or(Box(Modifier::new())),
        Box(Modifier::new()
            .width(if has_icon { 12.0 } else { 0.0 })
            .fill_max_height()),
        Text(label)
            .color(config.content_color)
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
    pub color: Color,
    pub label_color: Color,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: BadgeDefaults::color(),
            label_color: BadgeDefaults::label_color(),
        }
    }
}

/// M3 Badge - a small notification indicator. If `label` is `None`, shows a
/// small 6dp dot; otherwise shows the label text inside a 16dp pill.
pub fn Badge(label: Option<impl Into<String>>, config: BadgeConfig) -> View {
    let th = theme();
    match label {
        None => Box(Modifier::new()
            .size(BadgeDefaults::DOT_SIZE, BadgeDefaults::DOT_SIZE)
            .background(config.color)
            .clip_rounded(BadgeDefaults::DOT_SIZE * 0.5)
            .then(config.modifier)),
        Some(text) => {
            let text = text.into();
            Box(Modifier::new()
                .min_width(BadgeDefaults::LABEL_MIN_WIDTH)
                .height(BadgeDefaults::LABEL_HEIGHT)
                .background(config.color)
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
            .child(
                Text(text)
                    .color(config.label_color)
                    .size(th.typography.label_small)
                    .single_line(),
            )
        }
    }
}

/// M3 BadgedBox - wraps `content` and shows a `badge` anchored to the top-end corner.
/// The badge is positioned at (top: 0, end: 0) relative to the content.
pub fn BadgedBox(badge: View, content: View) -> View {
    Stack(Modifier::new()).child((
        content,
        Box(Modifier::new()
            .absolute()
            .offset(None, Some(0.0), Some(0.0), None))
        .child(badge),
    ))
}

/// Configuration for [`ListItem`].
#[derive(Clone, Debug)]
pub struct ListItemConfig {
    pub modifier: Modifier,
    pub headline_color: Color,
    pub supporting_color: Color,
    pub horizontal_padding: f32,
    pub trailing_padding: f32,
    pub one_line_height: f32,
    pub two_line_height: f32,
}

impl Default for ListItemConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            headline_color: ListItemDefaults::headline_color(),
            supporting_color: ListItemDefaults::supporting_color(),
            horizontal_padding: ListItemDefaults::HORIZONTAL_PADDING,
            trailing_padding: ListItemDefaults::TRAILING_PADDING,
            one_line_height: ListItemDefaults::ONE_LINE_HEIGHT,
            two_line_height: ListItemDefaults::TWO_LINE_HEIGHT,
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
    let mut modifier = Modifier::new()
        .min_width(200.0)
        .min_height(if supporting_text.is_some() {
            config.two_line_height
        } else {
            config.one_line_height
        })
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

    if let Some(cb) = on_click {
        modifier = modifier.clickable().on_pointer_down(move |_| cb());
    }

    Row(modifier).child((
        leading
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 16.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(v)
            })
            .unwrap_or(Box(Modifier::new())),
        Column(
            Modifier::new()
                .flex_grow(1.0)
                .justify_content(JustifyContent::Center),
        )
        .child((
            Text(headline)
                .color(config.headline_color)
                .size(th.typography.body_large)
                .single_line(),
            supporting_text
                .map(|st| {
                    Text(st)
                        .color(config.supporting_color)
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
                .child(v)
            })
            .unwrap_or(Box(Modifier::new())),
    ))
}

/// A single tab definition for use with `TabRow`.
pub struct Tab {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
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

                        Column(
                            Modifier::new()
                                .flex_grow(1.0)
                                .fill_max_height()
                                .align_items(AlignItems::Center)
                                .justify_content(JustifyContent::Center)
                                .state_colors(StateColors {
                                    default: Color::TRANSPARENT,
                                    hovered: th.on_surface.with_alpha_f32(0.08),
                                    pressed: th.on_surface.with_alpha_f32(0.12),
                                    disabled: Color::TRANSPARENT,
                                })
                                .clickable()
                                .on_pointer_down(move |_| cb()),
                        )
                        .child((
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

    // outer border with per-segment corner radii requires iterating over segments
    // to build a Row whose border uses the overall shape_r uniformly
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

                    let state_colors = config.state_colors;
                let mut modifier = Modifier::new()
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
                    })
                    .clickable()
                    .on_pointer_down(move |_| cb());

                if i < count - 1 {
                    modifier = modifier.border_radii(1.0, th.outline, [0.0; 4]);
                }

                Row(modifier).child((
                    seg.icon.unwrap_or(Box(Modifier::new())),
                    Text(seg.label)
                        .color(fg)
                        .size(th.typography.label_large)
                        .single_line(),
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
}

impl Default for CircularProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            color: ProgressIndicatorDefaults::circular_color(),
            track_color: ProgressIndicatorDefaults::circular_track_color(),
        }
    }
}

fn draw_sweep_arc(
    scene: &mut Scene,
    cx: f32,
    cy: f32,
    r: f32,
    stroke: f32,
    progress: f32,
    color: Color,
    cap: StrokeCap,
) {
    if progress <= 0.0 {
        return;
    }
    let sweep_rad = progress * std::f32::consts::TAU;
    let start_angle = -std::f32::consts::FRAC_PI_2;
    let rect = Rect {
        x: cx - r,
        y: cy - r,
        w: r * 2.0,
        h: r * 2.0,
    };
    scene.nodes.push(SceneNode::Arc {
        rect,
        start_angle,
        sweep_angle: sweep_rad,
        stroke_width: stroke,
        color,
        cap,
    });
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
    let stroke_px = dp_to_px(ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH);
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

            // Track ring
            scene.nodes.push(SceneNode::EllipseBorder {
                rect: circle,
                color: mul_c(config.track_color),
                width: stroke_px,
            });

            match val {
                Some(p) => {
                    draw_sweep_arc(
                        scene,
                        cx,
                        cy,
                        r,
                        stroke_px,
                        p,
                        mul_c(config.color),
                        StrokeCap::Round,
                    );
                }
                None => {
                    let radians =
                        (global_rotation + additional_rotation) * std::f32::consts::PI / 180.0;
                    let start_angle = -std::f32::consts::FRAC_PI_2 + radians;
                    let sweep_rad = sweep_val * std::f32::consts::TAU;
                    scene.nodes.push(SceneNode::Arc {
                        rect: circle,
                        start_angle,
                        sweep_angle: sweep_rad,
                        stroke_width: stroke_px,
                        color: mul_c(config.color),
                        cap: StrokeCap::Round,
                    });
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
            let gap = dp_to_px(config.gap_size);
            let dot_r = dp_to_px(config.stop_size) * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let t = value.unwrap_or(0.0).clamp(0.0, 1.0);

            // Indicator (active portion from left)
            if t > 0.0 {
                let ind_w = t * rect.w;
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x,
                        y: cy - corner,
                        w: ind_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.color)),
                    radius: [corner; 4],
                });
            }

            // Track (inactive portion after gap)
            let track_start = rect.x + t * rect.w + gap;
            let track_w = (rect.x + rect.w - track_start).max(0.0);
            if t < 1.0 && track_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_start,
                        y: cy - corner,
                        w: track_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.track_color)),
                    radius: [corner; 4],
                });
            }

            // Stop indicator at right end (4dp circle, Primary)
            if t < 1.0 {
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

    // Border color: error > focused (float) > default
    let border_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.outline
    };

    // Label color: error > focused > default
    let label_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.on_surface_variant
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

    // Indicator: unfocused = on_surface_variant/1dp, focused = primary/2dp, error = error/2dp
    let indicator_is_active = config.is_error || float_t > 0.5;
    let indicator_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.on_surface_variant
    };
    let indicator_target_w = if config.enabled && indicator_is_active {
        2.0
    } else {
        1.0
    };
    let indicator_w = animate_f32(
        format!("tf_ind_w_{}", anim_key),
        indicator_target_w,
        th.motion.color,
    );

    let label_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.on_surface_variant
    };

    let label_size = 16.0 - 4.0 * float_t;
    let label_y = 16.0 - 20.0 * float_t;

    let tf_placeholder = if has_label {
        String::new()
    } else {
        config.placeholder.unwrap_or_default()
    };

    let container_bg = if config.enabled {
        th.surface_container_highest
    } else {
        th.on_surface
            .with_alpha_f32(0.04)
            .composite_over(th.surface)
    };

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
    pub checked_color: Color,
    pub unchecked_color: Color,
    pub checkmark_color: Color,
    pub state_colors: StateColors,
}

impl Default for CheckboxConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            checked_color: CheckboxDefaults::checked_color(),
            unchecked_color: CheckboxDefaults::unchecked_color(),
            checkmark_color: CheckboxDefaults::checkmark_color(),
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

    let fill = animate_color(
        format!("cb_fill_{}", id),
        if checked {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("cb_bw_{}", id),
        if checked {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("cb_bd_{}", id),
        if checked {
            Color::TRANSPARENT
        } else {
            config.unchecked_color
        },
        spec,
    );
    let check_alpha = animate_f32(
        format!("cb_ca_{}", id),
        if checked { 1.0 } else { 0.0 },
        spec,
    );

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
        .on_pointer_down(move |_| on_change(!checked))
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
                    .color(config.checkmark_color)
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

    let fill = animate_color(
        format!("tc_fill_{}", id),
        if has_fill {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("tc_bw_{}", id),
        if has_fill {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("tc_bd_{}", id),
        if has_fill {
            Color::TRANSPARENT
        } else {
            config.unchecked_color
        },
        spec,
    );
    let symbol_alpha = animate_f32(
        format!("tc_sa_{}", id),
        if has_fill { 1.0 } else { 0.0 },
        spec,
    );

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
            on_change(match state {
                TriState::Checked => TriState::Unchecked,
                TriState::Indeterminate => TriState::Checked,
                TriState::Unchecked => TriState::Checked,
            })
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
                    .background(config.checkmark_color)
                    .clip_rounded(1.0))
            } else {
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(config.checkmark_color)
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
    pub selected_color: Color,
    pub unselected_color: Color,
    pub state_colors: StateColors,
}

impl Default for RadioButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            selected_color: RadioButtonDefaults::selected_color(),
            unselected_color: RadioButtonDefaults::unselected_color(),
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
        if selected {
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
        .on_pointer_down(move |_| on_select())
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
                .background(config.selected_color)
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
    pub checked_track_color: Color,
    pub unchecked_track_color: Color,
    pub checked_thumb_color: Color,
    pub unchecked_thumb_color: Color,
    pub unchecked_border_color: Color,
    pub state_colors: StateColors,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            checked_track_color: SwitchDefaults::checked_track_color(),
            unchecked_track_color: SwitchDefaults::unchecked_track_color(),
            checked_thumb_color: SwitchDefaults::checked_thumb_color(),
            unchecked_thumb_color: SwitchDefaults::unchecked_thumb_color(),
            unchecked_border_color: SwitchDefaults::unchecked_border_color(),
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
    let track_bg = animate_color(
        format!("sw_tbg_{}", id),
        if checked {
            config.checked_track_color
        } else {
            config.unchecked_track_color
        },
        color_spec,
    );
    let thumb_bg = animate_color(
        format!("sw_tmbg_{}", id),
        if checked {
            config.checked_thumb_color
        } else {
            config.unchecked_thumb_color
        },
        color_spec,
    );
    let track_border = animate_f32(
        format!("sw_tb_{}", id),
        if checked { 0.0 } else { 2.0 },
        color_spec,
    );
    let border_color = animate_color(
        format!("sw_bc_{}", id),
        if checked {
            Color::TRANSPARENT
        } else {
            config.unchecked_border_color
        },
        color_spec,
    );

    Box(Modifier::new()
        .size(track_w, track_h)
        .padding(0.0)
        .clip_rounded(track_h * 0.5)
        .background(track_bg)
        .state_colors(config.state_colors)
        .border(track_border, border_color, track_h * 0.5)
        .clickable()
        .on_pointer_down(move |_| on_change(!checked))
        .then(config.modifier))
    .child(Box(Modifier::new()
        .size(thumb_d, thumb_d)
        .background(thumb_bg)
        .clip_rounded(thumb_d * 0.5)
        .absolute()
        .offset(Some(thumb_left), Some(thumb_top), None, None)))
}

/// Configuration for [`Slider`] and [`RangeSlider`].
#[derive(Clone, Debug)]
pub struct SliderConfig {
    pub modifier: Modifier,
    pub active_track_color: Color,
    pub inactive_track_color: Color,
    pub thumb_color: Color,
    pub state_colors: StateColors,
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            active_track_color: SliderDefaults::active_track_color(),
            inactive_track_color: SliderDefaults::inactive_track_color(),
            thumb_color: SliderDefaults::thumb_color(),
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
    let hovered = remember_state_with_key(format!("ms_hv_{}", id), || false);

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let hovered_p = hovered.clone();

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t = ((value - min) / range_size).clamp(0.0, 1.0);
    let sc = config.state_colors;

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
            let gap = dp_to_px(8.0);
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
            for &tf in &tick_frac {
                let tx = tick_start + tf * (tick_end - tick_start);
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
                        config.inactive_track_color
                    } else {
                        config.active_track_color
                    })),
                });
            }
            if inactive_w > 0.0 {
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(config.active_track_color)),
                });
            }
            let da = *drag_active_p.borrow();
            let hv = *hovered_p.borrow();
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
            move |_pe: PointerEvent| *h.borrow_mut() = true
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| *h.borrow_mut() = false
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
    let hovered = remember_state_with_key(format!("mrs_hv_{}", id), || false);

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t0 = ((start - min) / range_size).clamp(0.0, 1.0);
    let t1 = ((end - min) / range_size).clamp(0.0, 1.0);
    let sc = config.state_colors;

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let active_thumb_p = active_thumb.clone();
    let hovered_p = hovered.clone();

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
            let gap = dp_to_px(8.0);
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
                    brush: Brush::Solid(mul_c(config.inactive_track_color)),
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
                    brush: Brush::Solid(mul_c(config.inactive_track_color)),
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
                    brush: Brush::Solid(mul_c(config.active_track_color)),
                    radius: [corner; 4],
                });
            }
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for &tf in &tick_frac {
                let tx = tick_start + tf * (tick_end - tick_start);
                if tx >= active_l - gap && tx <= active_r + gap {
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
                    brush: Brush::Solid(mul_c(if on_active {
                        config.inactive_track_color
                    } else {
                        config.active_track_color
                    })),
                });
            }
            if linactive_w > 0.0 {
                let sx0 = track_x + corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx0 - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(config.active_track_color)),
                });
            }
            if rinactive_w > 0.0 {
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(config.active_track_color)),
                });
            }
            let da = *drag_active_p.borrow();
            let at = *active_thumb_p.borrow();
            let hv = *hovered_p.borrow();
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
                    brush: Brush::Solid(mul_c(config.thumb_color)),
                    radius: [tw * 0.5; 4],
                });
                let sc_target = if is_active {
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
            move |_pe: PointerEvent| *h.borrow_mut() = true
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| *h.borrow_mut() = false
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |pe: PointerEvent| {
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
        enabled: true,
    })
}

/// Configuration for [`Card`].
#[derive(Clone, Debug)]
pub struct CardConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub state_elevation: Option<StateElevation>,
    pub border: Option<(f32, Color)>,
}

impl Default for CardConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: CardDefaults::filled_container_color(),
            shape_radius: CardDefaults::SHAPE_RADIUS,
            tonal_elevation: CardDefaults::ELEVATION,
            state_elevation: None,
            border: None,
        }
    }
}

/// M3 Card - a configurable container surface.
pub fn Card(config: CardConfig, content: impl FnOnce() -> View) -> View {
    let mut m = Modifier::new()
        .background(config.container_color)
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
    Box(m).child(content())
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
        .on_pointer_down(move |_| on_click());
    Card(
        CardConfig {
            modifier: m,
            container_color: bg,
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
            min_height: SnackbarDefaults::MIN_HEIGHT,
            min_width: SnackbarDefaults::MIN_WIDTH,
            max_width: SnackbarDefaults::MAX_WIDTH,
        }
    }
}

/// Configuration for chips.
#[derive(Clone, Debug)]
pub struct ChipConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub selected: bool,
    pub container_color: Color,
    pub selected_container_color: Color,
    pub content_color: Color,
    pub selected_content_color: Color,
    pub border_color: Color,
    pub shape_radius: f32,
    pub horizontal_padding: f32,
}

impl Default for ChipConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            selected: false,
            container_color: ChipDefaults::surface_color(),
            selected_container_color: ChipDefaults::selected_container_color(),
            content_color: ChipDefaults::unselected_content_color(),
            selected_content_color: ChipDefaults::selected_content_color(),
            border_color: ChipDefaults::unselected_border_color(),
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
    let shape = config.shape_radius;
    Box(Modifier::new()
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
        .clickable()
        .on_pointer_down(move |_| on_click())
        .background(Color::TRANSPARENT)
        .clip_rounded(shape)
        .border(1.0, config.border_color, shape)
        .then(config.modifier))
    .child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(config.content_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(config.content_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(config.content_color, move || v))
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
    let shape = config.shape_radius;
    Box(Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(StateElevation {
            default: th.elevation.level1,
            hovered: th.elevation.level2,
            pressed: th.elevation.level1,
            disabled: 0.0,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .clickable()
        .on_pointer_down(move |_| on_click())
        .background(th.surface_container_low)
        .clip_rounded(shape)
        .then(config.modifier))
    .child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(config.content_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(config.content_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(config.content_color, move || v))
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
    pub selected_icon_color: Color,
    pub unselected_icon_color: Color,
    pub indicator_color: Color,
    pub height: f32,
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
            selected_icon_color: NavigationBarDefaults::selected_icon_color(),
            unselected_icon_color: NavigationBarDefaults::unselected_icon_color(),
            indicator_color: NavigationBarDefaults::indicator_color(),
            height: NavigationBarDefaults::HEIGHT,
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
    pub scrim_color: Color,
    pub width: f32,
    pub shape_radius: f32,
}

impl Default for NavigationDrawerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationDrawerDefaults::container_color(),
            scrim_color: NavigationDrawerDefaults::scrim_color(),
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
    pub scrim_color: Color,
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
            scrim_color: BottomSheetDefaults::scrim_color(),
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
