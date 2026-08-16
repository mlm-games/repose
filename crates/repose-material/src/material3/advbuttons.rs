#![allow(non_snake_case)]

use std::rc::Rc;

use super::SplitButtonDefaults;
use super::util::apply_m3_clickable;
use repose_core::{locals::with_content_color, *};
use repose_ui::{Box, Row, Text, TextStyle, ViewExt};

/// Configuration for [`SplitButtonLayout`].
#[derive(Clone)]
pub struct SplitButtonConfig {
    pub modifier: Modifier,
    pub spacing: f32,
}

impl Default for SplitButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            spacing: SplitButtonDefaults::SPACING,
        }
    }
}

/// M3 SplitButtonLayout -> places leading and trailing buttons side by side.
///
/// Pass styled buttons (via [`Button`], [`FilledTonalButton`], etc.) or use
/// the factory helpers below for properly-shaped split button parts.
pub fn SplitButtonLayout(
    leading_button: View,
    trailing_button: View,
    config: SplitButtonConfig,
) -> View {
    Row(config
        .modifier
        .gap(config.spacing)
        .align_items(AlignItems::CENTER))
    .child((leading_button, trailing_button))
}

fn split_leading_shape_radii() -> [f32; 4] {
    [
        SplitButtonDefaults::OUTER_CORNER_SIZE,
        SplitButtonDefaults::SMALL_INNER_CORNER_SIZE,
        SplitButtonDefaults::SMALL_INNER_CORNER_SIZE,
        SplitButtonDefaults::OUTER_CORNER_SIZE,
    ]
}

fn split_trailing_shape_radii() -> [f32; 4] {
    [
        SplitButtonDefaults::SMALL_INNER_CORNER_SIZE,
        SplitButtonDefaults::OUTER_CORNER_SIZE,
        SplitButtonDefaults::OUTER_CORNER_SIZE,
        SplitButtonDefaults::SMALL_INNER_CORNER_SIZE,
    ]
}

fn split_button_impl(
    outer_modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
    content_color: Color,
    container_color: Option<Color>,
    state_colors: StateColors,
    state_elevation: Option<StateElevation>,
    border: Option<(f32, Color, f32)>,
    pad_left: f32,
    pad_right: f32,
    height: f32,
    enabled: bool,
    interaction_source: Option<MutableInteractionSource>,
) -> View {
    let mut m = Modifier::new()
        .height(height)
        .min_width(48.0)
        .padding_values(PaddingValues {
            left: pad_left,
            right: pad_right,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER);
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
    let source: Rc<MutableInteractionSource> = interaction_source
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    m = apply_m3_clickable(m, &source, content_color, enabled, on_click);
    m = m.then(outer_modifier);
    let effective = if enabled {
        content_color
    } else {
        content_color.with_alpha_f32(0.38)
    };
    with_content_color(effective, || Box(m).child(content()))
}

pub fn SplitButtonLeadingButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: super::ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let def = super::ButtonColors {
        container_color: super::ButtonDefaults::container_color(),
        content_color: super::ButtonDefaults::content_color(),
        disabled_container_color: super::ButtonDefaults::container_color()
            .with_alpha_f32(0.12)
            .composite_over(theme().surface_container_low),
        disabled_content_color: super::ButtonDefaults::content_color().with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config
        .content_padding
        .unwrap_or(SplitButtonDefaults::small_leading_content_padding());
    split_button_impl(
        modifier.clip_rounded_radii(split_leading_shape_radii()),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(super::ButtonDefaults::state_elevation_default())),
        config.border,
        pad.left,
        pad.right,
        config.height,
        config.enabled,
        config.interaction_source.clone(),
    )
}

pub fn SplitButtonTrailingButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: super::ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let def = super::ButtonColors {
        container_color: super::ButtonDefaults::container_color(),
        content_color: super::ButtonDefaults::content_color(),
        disabled_container_color: super::ButtonDefaults::container_color()
            .with_alpha_f32(0.12)
            .composite_over(theme().surface_container_low),
        disabled_content_color: super::ButtonDefaults::content_color().with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config
        .content_padding
        .unwrap_or(SplitButtonDefaults::small_trailing_content_padding());
    split_button_impl(
        modifier.clip_rounded_radii(split_trailing_shape_radii()),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(super::ButtonDefaults::state_elevation_default())),
        config.border,
        pad.left,
        pad.right,
        config.height,
        config.enabled,
        config.interaction_source.clone(),
    )
}

pub fn SplitButtonTrailingToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    modifier: Modifier,
    config: super::ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let _th = theme();
    let cc = config
        .content_color
        .unwrap_or_else(super::ToggleButtonDefaults::content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(super::ToggleButtonDefaults::checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(super::ToggleButtonDefaults::checked_container_color);
    let bg = if checked {
        checked_bg
    } else {
        config.container_color.unwrap_or(Color::TRANSPARENT)
    };
    let fg = if checked { checked_cc } else { cc };
    let se = config
        .state_elevation
        .unwrap_or_else(super::ToggleButtonDefaults::state_elevation_default);
    let pad_l = config
        .content_padding
        .map(|p| p.left)
        .unwrap_or(super::ToggleButtonDefaults::HORIZONTAL_PADDING);
    let pad_r = config
        .content_padding
        .map(|p| p.right)
        .unwrap_or(super::ToggleButtonDefaults::HORIZONTAL_PADDING);
    let mut m = Modifier::new()
        .height(config.height)
        .min_width(48.0)
        .padding_values(PaddingValues {
            left: pad_l,
            right: pad_r,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .state_colors(config.state_colors)
        .state_elevation(se);
    let tg_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    if let Some((w, c, r)) = config.border {
        m = m.border(w, c, r);
    }
    let cb = on_checked_change;
    m = apply_m3_clickable(m, &tg_source, fg, config.enabled, move || cb(!checked));
    if !config.enabled {
        m = m.alpha(0.38);
    }
    m = m.then(modifier.clip_rounded_radii(split_trailing_shape_radii()));
    with_content_color(fg, || Box(m).child(content(checked)))
}

pub fn SplitButtonTonalLeadingButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    config: super::ButtonConfig,
    content: impl FnOnce() -> View,
) -> View {
    let def = super::ButtonColors {
        container_color: super::ButtonDefaults::tonal_container_color(),
        content_color: super::ButtonDefaults::tonal_content_color(),
        disabled_container_color: theme()
            .on_surface
            .with_alpha_f32(0.12)
            .composite_over(theme().surface_container_low),
        disabled_content_color: theme().on_surface.with_alpha_f32(0.38),
    };
    let (cc, bg, sc, se) = resolve_button_colors(&config, def);
    let pad = config
        .content_padding
        .unwrap_or(SplitButtonDefaults::small_leading_content_padding());
    split_button_impl(
        modifier.clip_rounded_radii(split_leading_shape_radii()),
        on_click,
        content,
        cc,
        bg,
        sc,
        se.or(Some(super::ButtonDefaults::elevated_state_elevation())),
        config.border,
        pad.left,
        pad.right,
        config.height,
        config.enabled,
        config.interaction_source.clone(),
    )
}

pub fn SplitButtonTonalTrailingToggleButton(
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    modifier: Modifier,
    config: super::ToggleButtonConfig,
    content: impl FnOnce(bool) -> View,
) -> View {
    let _th = theme();
    let cc = config
        .content_color
        .unwrap_or_else(super::ToggleButtonDefaults::tonal_content_color);
    let checked_cc = config
        .checked_content_color
        .unwrap_or_else(super::ToggleButtonDefaults::tonal_checked_content_color);
    let checked_bg = config
        .checked_container_color
        .unwrap_or_else(super::ToggleButtonDefaults::tonal_checked_container_color);
    let bg = if checked {
        checked_bg
    } else {
        config.container_color.unwrap_or(Color::TRANSPARENT)
    };
    let fg = if checked { checked_cc } else { cc };
    let se = config
        .state_elevation
        .unwrap_or_else(super::ToggleButtonDefaults::state_elevation_default);
    let pad_l = config
        .content_padding
        .map(|p| p.left)
        .unwrap_or(super::ToggleButtonDefaults::HORIZONTAL_PADDING);
    let pad_r = config
        .content_padding
        .map(|p| p.right)
        .unwrap_or(super::ToggleButtonDefaults::HORIZONTAL_PADDING);
    let mut m = Modifier::new()
        .height(config.height)
        .min_width(48.0)
        .padding_values(PaddingValues {
            left: pad_l,
            right: pad_r,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .state_colors(config.state_colors)
        .state_elevation(se);
    let tg_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    if let Some((w, c, r)) = config.border {
        m = m.border(w, c, r);
    }
    let cb = on_checked_change;
    m = apply_m3_clickable(m, &tg_source, fg, config.enabled, move || cb(!checked));
    if !config.enabled {
        m = m.alpha(0.38);
    }
    m = m.then(modifier.clip_rounded_radii(split_trailing_shape_radii()));
    with_content_color(fg, || Box(m).child(content(checked)))
}

/// State for the overflow menu in [`ButtonGroup`].
pub struct ButtonGroupMenuState {
    pub is_showing: bool,
}

impl ButtonGroupMenuState {
    pub fn dismiss(&mut self) {
        self.is_showing = false;
    }
    pub fn show(&mut self) {
        self.is_showing = true;
    }
}

/// Scope passed to [`ButtonGroup`]'s content closure.
pub struct ButtonGroupScope {
    items: Vec<ButtonGroupItem>,
}

/// Internal item held by `ButtonGroupScope`.
#[allow(dead_code)] // `menu_content` is populated via the public API (WIP overflow menus).
struct ButtonGroupItem {
    button_group_content: Box<dyn FnOnce() -> View>,
    menu_content: Option<Box<dyn FnOnce(&mut ButtonGroupMenuState) -> View>>,
}

impl ButtonGroupScope {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Add a clickable item (rendered as a [`Button`] internally).
    pub fn clickable_item(
        &mut self,
        on_click: impl Fn() + 'static,
        label: String,
        icon: Option<View>,
    ) {
        let cb = Rc::new(on_click);
        let cb2 = cb.clone();
        self.items.push(ButtonGroupItem {
            button_group_content: Box::new(move || {
                let cb = cb2.clone();
                let config = super::ButtonConfig {
                    shape_radius: 0.0,
                    ..Default::default()
                };
                super::Button(
                    Modifier::new().flex_grow(1.0),
                    move || (cb)(),
                    config,
                    move || {
                        let label = label.clone();
                        let t = Text(label).single_line();
                        match icon.clone() {
                            Some(ic) => Box(Modifier::new()).child((ic, t)),
                            None => t,
                        }
                    },
                )
            }),
            menu_content: None,
        });
    }

    /// Add a toggleable item (rendered as a [`ToggleButton`] internally).
    pub fn toggleable_item(
        &mut self,
        checked: bool,
        on_checked_change: impl Fn(bool) + 'static,
        label: String,
        icon: Option<View>,
    ) {
        let cb = Rc::new(on_checked_change);
        let cb2 = cb.clone();
        self.items.push(ButtonGroupItem {
            button_group_content: Box::new(move || {
                let cb = cb2.clone();
                let config = super::ToggleButtonConfig {
                    shape_radius: 0.0,
                    ..Default::default()
                };
                super::ToggleButton(
                    checked,
                    move |b| (cb)(b),
                    config,
                    move |_| {
                        let label = label.clone();
                        let t = Text(label).single_line();
                        match icon.clone() {
                            Some(ic) => Box(Modifier::new()).child((ic, t)),
                            None => t,
                        }
                    },
                )
            }),
            menu_content: None,
        });
    }

    /// Add a custom item with a button group composable and an optional overflow menu content.
    pub fn custom_item(
        &mut self,
        button_group_content: impl FnOnce() -> View + 'static,
        menu_content: Option<impl FnOnce(&mut ButtonGroupMenuState) -> View + 'static>,
    ) {
        self.items.push(ButtonGroupItem {
            button_group_content: Box::new(button_group_content),
            menu_content: menu_content.map(|f| {
                let b: Box<dyn FnOnce(&mut ButtonGroupMenuState) -> View> = Box::new(f);
                b
            }),
        });
    }
}

/// M3 ButtonGroup -> a horizontal sequence of related action items.
///
/// Items are added via [`ButtonGroupScope::clickable_item`] and
/// [`ButtonGroupScope::toggleable_item`].
pub fn ButtonGroup(
    modifier: Modifier,
    gap: f32,
    content: impl FnOnce(&mut ButtonGroupScope),
) -> View {
    let mut scope = ButtonGroupScope::new();
    content(&mut scope);
    Row(modifier.gap(gap).align_items(AlignItems::CENTER)).with_children(
        scope
            .items
            .into_iter()
            .map(|item| (item.button_group_content)())
            .collect::<Vec<View>>(),
    )
}

fn resolve_button_colors(
    config: &super::ButtonConfig,
    def: super::ButtonColors,
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
