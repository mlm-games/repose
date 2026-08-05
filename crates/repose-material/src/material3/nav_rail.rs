#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Text, TextStyle,
    ViewExt,
    anim::animate_color,
};

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
    pub width: f32,
    pub item_radius: f32,
    pub indicator_opacity: f32,
    pub item_spacing: f32,
    pub indicator_width: f32,
    pub indicator_height: f32,
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
                    left: 12.0,
                    right: 12.0,
                    top: 12.0,
                    bottom: 12.0,
                })
                .align_self(AlignSelf::CENTER))
            .child(h),
        );
    }

    if let Some(f) = fab {
        top_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 8.0,
                    bottom: 8.0,
                })
                .align_self(AlignSelf::CENTER))
            .child(f),
        );
    }

    if has_header || has_fab {
        top_children.push(Box(Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(th.outline_variant)));
    }

    for (i, item) in items.into_iter().enumerate() {
        let selected = i == selected_index;
        let is_enabled = item.enabled;

        let fg = animate_color(
            format!("nr_fg_{}_{}", id, i),
            if selected {
                config.selected_icon_color
            } else {
                config.unselected_icon_color
            },
            default_effects,
        );
        let fg_label = animate_color(
            format!("nr_fl_{}_{}", id, i),
            if selected {
                config.selected_text_color
            } else {
                config.unselected_text_color
            },
            default_effects,
        );
        let bg = animate_color(
            format!("nr_bg_{}_{}", id, i),
            if selected {
                config.indicator_color
            } else {
                Color::TRANSPARENT
            },
            default_effects,
        );

        let cb = item.on_click.clone();
        let nr_source: Rc<MutableInteractionSource> = item
            .interaction_source
            .clone()
            .map(Rc::new)
            .unwrap_or_else(|| remember(MutableInteractionSource::new));

        let mut item_m = Modifier::new()
            .fill_max_width()
            .padding_values(PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 4.0,
                bottom: 4.0,
            })
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(bg)
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08),
                pressed: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .clip_rounded(config.item_radius)
            .interaction_source(&*nr_source)
            .semantics(Semantics::new(Role::Tab).with_label(&item.label));

        if is_enabled {
            item_m = item_m.clickable().on_click({
                let cb = cb.clone();
                move || cb()
            });
        }

        item_views.push(
            Column(item_m).child((
                Column(Modifier::new()).child((
                    Box(Modifier::new().size(24.0, 24.0))
                        .child(with_content_color(fg, move || item.icon)),
                    item.badge
                        .map(|b| {
                            Box(Modifier::new()
                                .absolute()
                                .offset(None, None, None, Some(0.0)))
                            .child(b)
                        })
                        .unwrap_or(Box(Modifier::new())),
                )),
                Box(Modifier::new().fill_max_width().height(4.0)),
                Text(item.label)
                    .color(fg_label)
                    .size(th.typography.label_medium)
                    .single_line(),
            )),
        );
    }

    Column(
        Modifier::new()
            .width(config.width)
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
                    .fill_max_size()
                    .justify_content(JustifyContent::SPACE_BETWEEN)
                    .align_items(AlignItems::CENTER),
            )
            .with_children(item_views),
        ),
    ))
}
