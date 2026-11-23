#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Text, TextStyle, ViewExt,
    anim::{animate_color, animate_f32, animate_f32_from},
};

use crate::ripple::{RippleConfig, ripple};

use super::util::apply_m3_clickable_without_indication;
use super::*;

/// Configuration for [`NavigationRail`].
#[derive(Clone, Debug)]
pub struct NavigationRailConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub selected_icon_color: Color,
    pub selected_text_color: Color,
    pub unselected_icon_color: Color,
    pub unselected_text_color: Color,
    pub indicator_color: Color,
    pub width: Dp,
    pub item_radius: Dp,
    pub indicator_opacity: f32,
    pub item_spacing: Dp,
    pub indicator_width: Dp,
    pub indicator_height: Dp,
    pub always_show_label: bool,
}

impl Default for NavigationRailConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationRailDefaults::container_color(),
            selected_icon_color: NavigationRailDefaults::selected_icon_color(),
            selected_text_color: NavigationRailDefaults::selected_text_color(),
            unselected_icon_color: NavigationRailDefaults::unselected_icon_color(),
            unselected_text_color: NavigationRailDefaults::unselected_text_color(),
            indicator_color: NavigationRailDefaults::indicator_color(),
            width: NavigationRailDefaults::WIDTH,
            item_radius: NavigationRailDefaults::ITEM_RADIUS,
            indicator_opacity: NavigationRailDefaults::ITEM_ACTIVE_INDICATOR_OPACITY,
            item_spacing: NavigationRailDefaults::ITEM_SPACING,
            indicator_width: NavigationRailDefaults::ACTIVE_INDICATOR_WIDTH,
            indicator_height: NavigationRailDefaults::ACTIVE_INDICATOR_HEIGHT,
            always_show_label: true,
        }
    }
}

/// A destination entry inside a NavigationRail.
pub struct NavRailItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub badge: Option<View>,
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
}

static NAVRAIL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Navigation Rail - a compact vertical navigation sidebar.
///
/// Typically placed on the left side of the screen. Contains navigation items
/// (icon + label) with animated selection indicator.
pub fn NavigationRail(
    selected_index: usize,
    items: Vec<NavRailItem>,
    header: Option<View>,
    fab: Option<View>,
    config: NavigationRailConfig,
) -> View {
    let th = theme();
    let id = remember(|| NAVRAIL_COUNTER.fetch_add(1, Ordering::Relaxed));
    let default_effects = AnimationSpec::spring_crit(40.0);

    let mut top_children: Vec<View> = Vec::new();
    let mut item_views: Vec<View> = Vec::new();

    let has_header = header.is_some();
    let has_fab = fab.is_some();

    if let Some(h) = header {
        top_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: Dp(12.0),
                    right: Dp(12.0),
                    top: Dp(12.0),
                    bottom: Dp(12.0),
                })
                .align_self(AlignSelf::CENTER))
            .child(h),
        );
    }

    if let Some(f) = fab {
        top_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: Dp(12.0),
                    right: Dp(12.0),
                    top: Dp(8.0),
                    bottom: Dp(8.0),
                })
                .align_self(AlignSelf::CENTER))
            .child(f),
        );
    }

    if has_header || has_fab {
        top_children.push(Box(Modifier::new()
            .fill_max_width()
            .height(Dp(1.0))
            .background(th.outline_variant)));
    }

    for (i, item) in items.into_iter().enumerate() {
        let selected = i == selected_index;
        let is_enabled = item.enabled;

        let disabled_icon = config.unselected_icon_color.with_alpha_f32(0.38);
        let disabled_text = config.unselected_text_color.with_alpha_f32(0.38);
        let icon_target = if !is_enabled {
            disabled_icon
        } else if selected {
            config.selected_icon_color
        } else {
            config.unselected_icon_color
        };
        let label_target = if !is_enabled {
            disabled_text
        } else if selected {
            config.selected_text_color
        } else {
            config.unselected_text_color
        };
        let fg = animate_color(format!("nr_fg_{}_{}", id, i), icon_target, default_effects);
        let fg_label = animate_color(format!("nr_fl_{}_{}", id, i), label_target, default_effects);
        let bg = animate_color(
            format!("nr_bg_{}_{}", id, i),
            if selected {
                config.indicator_color
            } else {
                Color::TRANSPARENT
            },
            default_effects,
        );
        let label_alpha = animate_f32(
            format!("nr_la_{}_{}", id, i),
            if config.always_show_label || selected {
                1.0
            } else {
                0.0
            },
            default_effects,
        );
        let indicator_w = Dp(animate_f32_from(
            format!("nr_iw_{}_{}", id, i),
            0.0,
            if selected {
                config.indicator_width.0
            } else {
                0.0
            },
            default_effects,
        ));

        let cb = item.on_click.clone();
        let nr_source: Rc<MutableInteractionSource> = item
            .interaction_source
            .clone()
            .map(Rc::new)
            .unwrap_or_else(|| remember(MutableInteractionSource::new));

        let item_width = remember_mutable_with_key(format!("rail_w_{}_{}", id, i), || 0.0f32);
        let mut item_m = Modifier::new()
            .fill_max_width()
            .min_height(NavigationRailDefaults::ITEM_MIN_HEIGHT)
            .propagate_min_constraints(true)
            .padding_values(PaddingValues {
                left: Dp(4.0),
                right: Dp(4.0),
                top: Dp(4.0),
                bottom: Dp(4.0),
            })
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .on_size_changed({
                let w = item_width.clone();
                move |size| w.set_neq(size.x)
            })
            .semantics(Semantics::new(Role::Tab).with_label(&item.label));

        item_m = apply_m3_clickable_without_indication(item_m, &nr_source, is_enabled, {
            let cb = cb.clone();
            move || cb()
        });

        let measured_width = *item_width.get();
        let bg_m = Modifier::new()
            .absolute()
            .inset(
                Some((Dp(24.0) - indicator_w) / 2.0),
                Some((Dp(24.0) - config.indicator_height) / 2.0),
                None,
                None,
            )
            .width(indicator_w)
            .height(config.indicator_height)
            .background(bg)
            .clip_rounded(config.item_radius);

        let dx = (Dp(measured_width) - config.indicator_width) / 2.0;
        let pill_m = Modifier::new()
            .absolute()
            .inset(
                Some((Dp(24.0) - config.indicator_width) / 2.0),
                Some((Dp(24.0) - config.indicator_height) / 2.0),
                None,
                None,
            )
            .width(config.indicator_width)
            .height(config.indicator_height)
            .clip_rounded(config.item_radius)
            .interaction_source(&nr_source)
            .indication(ripple(RippleConfig {
                color: Some(th.on_surface),
                bounded: true,
                press_offset: if measured_width > 0.0 {
                    Some(Vec2 {
                        x: dx.to_px().0,
                        y: 0.0,
                    })
                } else {
                    None
                },
                ..Default::default()
            }));

        item_views.push(
            Column(item_m).child((
                Column(Modifier::new()).child((
                    Box(Modifier::new().size(Dp(24.0), Dp(24.0))).child((
                        Box(bg_m),
                        with_content_color(fg, move || item.icon),
                        Box(pill_m),
                    )),
                    item.badge
                        .map(|b| {
                            Box(Modifier::new()
                                .absolute()
                                .inset(None, None, None, Some(Dp(0.0))))
                            .child(b)
                        })
                        .unwrap_or(Box(Modifier::new())),
                )),
                Box(Modifier::new().fill_max_width().height(Dp(4.0))),
                Text(item.label)
                    .color(fg_label)
                    .size(th.typography.label_medium)
                    .single_line()
                    .modifier(Modifier::new().alpha(label_alpha)),
            )),
        );
    }

    Column(
        Modifier::new()
            .min_width(config.width)
            .fill_max_height()
            .background(config.container_color)
            .align_items(AlignItems::CENTER)
            .semantics(Semantics::new(Role::Container).with_selectable_group())
            .then(config.modifier),
    )
    .child((
        Column(Modifier::new()).with_children(top_children),
        Box(Modifier::new().flex_grow(1.0)).child(
            Column(
                Modifier::new()
                    .fill_max_width()
                    .gap(NavigationRailDefaults::ITEM_SPACING)
                    .align_items(AlignItems::CENTER),
            )
            .with_children(item_views),
        ),
    ))
}

pub fn WideNavigationRail(
    selected_index: usize,
    items: Vec<NavRailItem>,
    header: Option<View>,
    fab: Option<View>,
    config: NavigationRailConfig,
) -> View {
    let mut config = config;
    if config.width.0 < 120.0 {
        config.width = Dp(120.0);
    }
    NavigationRail(selected_index, items, header, fab, config)
}
