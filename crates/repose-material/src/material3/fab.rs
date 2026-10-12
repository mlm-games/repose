#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Row, Text, TextStyle, ViewExt};

use super::util::{apply_m3_clickable, with_button_semantics};
use super::*;

/// Configuration for FAB components.
#[derive(Clone, Debug)]
pub struct FABConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub container_color: Color,
    pub content_color: Color,
    pub state_elevation: StateElevation,
    pub shape_radius: Dp,
    pub size: Dp,
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

fn fab_impl(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
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
            default: Dp::ZERO,
            hovered: Dp::ZERO,
            focused: Dp::ZERO,
            pressed: Dp::ZERO,
            dragged: Dp::ZERO,
            disabled: Dp::ZERO,
        }
    };

    let mut m = Modifier::new()
        .size(config.size, config.size)
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
        .clip_rounded(config.shape_radius)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    m = apply_m3_clickable(m, &source, content_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

    Box(m).child(super::util::icon_content_with_color(content_color, icon))
}

/// M3 Floating Action Button (regular, 56dp).
pub fn FAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(icon, on_click, config)
}

fn variant_config(mut config: FABConfig, default_size: Dp, default_shape_radius: Dp) -> FABConfig {
    if config.size == FABDefaults::SIZE {
        config.size = default_size;
    }
    if config.shape_radius == FABDefaults::SHAPE_RADIUS {
        config.shape_radius = default_shape_radius;
    }
    config
}

/// M3 Small FAB (40dp).
pub fn SmallFAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(
        icon,
        on_click,
        variant_config(
            config,
            FABDefaults::SMALL_SIZE,
            FABDefaults::SMALL_SHAPE_RADIUS,
        ),
    )
}

/// M3 Large FAB (96dp).
pub fn LargeFAB(icon: View, on_click: impl Fn() + 'static, config: FABConfig) -> View {
    fab_impl(
        icon,
        on_click,
        variant_config(
            config,
            FABDefaults::LARGE_SIZE,
            FABDefaults::LARGE_SHAPE_RADIUS,
        ),
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
            default: Dp::ZERO,
            hovered: Dp::ZERO,
            focused: Dp::ZERO,
            pressed: Dp::ZERO,
            dragged: Dp::ZERO,
            disabled: Dp::ZERO,
        }
    };

    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .height(config.size)
        .min_width(config.size.max(Dp(80.0)))
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
        .clip_rounded(config.shape_radius)
        .padding_values(PaddingValues {
            left: Dp(16.0),
            right: Dp(20.0),
            top: Dp(0.0),
            bottom: Dp(0.0),
        })
        .align_items(AlignItems::CENTER);

    m = apply_m3_clickable(m, &source, content_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);
    m = m.then(config.modifier);
    Row(m).child((
        icon.map(|v| super::util::icon_content_with_color(content_color, v))
            .unwrap_or(Box(Modifier::new())),
        Box(Modifier::new()
            .width(if has_icon { Dp(12.0) } else { Dp::ZERO })
            .fill_max_height()),
        Text(label)
            .color(content_color)
            .size(th.typography.label_large)
            .single_line(),
    ))
}

pub struct FABMenuItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

#[derive(Clone, Debug)]
pub struct FABMenuConfig {
    pub modifier: Modifier,
    pub fab_config: FABConfig,
    pub item_spacing: Dp,
}

impl Default for FABMenuConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            fab_config: FABConfig::default(),
            item_spacing: Dp(12.0),
        }
    }
}

pub fn FloatingActionButtonMenu(
    expanded: bool,
    on_expanded_change: impl Fn(bool) + 'static,
    toggle_icon: View,
    items: Vec<FABMenuItem>,
    config: FABMenuConfig,
) -> View {
    let th = theme();
    let id = remember(unique_component_id);
    let reveal = repose_ui::anim::animate_f32(
        format!("fabmenu:{id}:reveal"),
        if expanded { 1.0 } else { 0.0 },
        th.motion.overlay,
    );
    let on_expanded_change = Rc::new(on_expanded_change);
    if reveal <= 0.01 && !expanded {
        let opener = on_expanded_change.clone();
        return FAB(toggle_icon, move || opener(true), config.fab_config);
    }
    let mut sub_views: Vec<View> = items
        .into_iter()
        .map(|item| {
            let cb = item.on_click.clone();
            let closer = on_expanded_change.clone();
            repose_ui::Row(
                Modifier::new()
                    .align_items(AlignItems::CENTER)
                    .column_gap(Dp(12.0))
                    .justify_content(JustifyContent::END),
            )
            .child((
                Box(Modifier::new()
                    .background(th.surface_container_high)
                    .clip_rounded(Dp(8.0))
                    .padding_values(PaddingValues {
                        left: Dp(12.0),
                        right: Dp(12.0),
                        top: Dp(8.0),
                        bottom: Dp(8.0),
                    }))
                .child(Text(item.label).size(th.typography.label_large)),
                SmallFAB(
                    item.icon,
                    move || {
                        cb();
                        closer(false);
                    },
                    config.fab_config.clone(),
                ),
            ))
        })
        .collect::<Vec<_>>();
    sub_views.push(FAB(
        toggle_icon,
        move || on_expanded_change(false),
        config.fab_config.clone(),
    ));
    repose_ui::Column(
        Modifier::new()
            .alpha(reveal)
            .row_gap(config.item_spacing)
            .align_items(AlignItems::END)
            .then(config.modifier),
    )
    .with_children(sub_views)
}
