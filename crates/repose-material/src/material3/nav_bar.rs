#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Row, Text, TextStyle, ViewExt,
    anim::{animate_color, animate_f32},
};

use crate::ripple::{RippleConfig, ripple};

use super::util::apply_m3_clickable_without_indication;
use super::*;

static NAVBAR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Configuration for [`NavigationBar`].
#[derive(Clone, Debug)]
pub struct NavigationBarConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub selected_icon_color: Color,
    pub selected_text_color: Color,
    pub unselected_icon_color: Color,
    pub unselected_text_color: Color,
    pub indicator_color: Color,
    pub height: f32,
    pub tonal_elevation: f32,
    pub indicator_opacity: f32,
    pub indicator_radius: f32,
    pub item_spacing: f32,
    pub indicator_width: f32,
    pub indicator_height: f32,
}

impl Default for NavigationBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationBarDefaults::container_color(),
            content_color: NavigationBarDefaults::content_color(),
            selected_icon_color: NavigationBarDefaults::selected_icon_color(),
            selected_text_color: NavigationBarDefaults::selected_text_color(),
            unselected_icon_color: NavigationBarDefaults::unselected_icon_color(),
            unselected_text_color: NavigationBarDefaults::unselected_text_color(),
            indicator_color: NavigationBarDefaults::indicator_color(),
            height: NavigationBarDefaults::HEIGHT,
            tonal_elevation: NavigationBarDefaults::TONAL_ELEVATION,
            indicator_opacity: NavigationBarDefaults::ITEM_ACTIVE_INDICATOR_OPACITY,
            indicator_radius: NavigationBarDefaults::INDICATOR_RADIUS,
            item_spacing: NavigationBarDefaults::ITEM_SPACING,
            indicator_width: NavigationBarDefaults::ACTIVE_INDICATOR_WIDTH,
            indicator_height: NavigationBarDefaults::ACTIVE_INDICATOR_HEIGHT,
        }
    }
}

/// M3 Navigation Bar - a bottom navigation bar with animated selection.
/// Colors and indicator background transition with 200ms FastOutSlowIn.
pub fn NavigationBar(
    selected_index: usize,
    items: Vec<NavItem>,
    config: NavigationBarConfig,
) -> View {
    let th = theme();
    let id = remember(|| NAVBAR_COUNTER.fetch_add(1, Ordering::Relaxed));

    let mut bar_m = Modifier::new()
        .fill_max_size()
        .min_height(config.height)
        .background(config.container_color)
        .then(config.modifier);

    if config.tonal_elevation > 0.0 {
        bar_m = bar_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            focused: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER)
            .column_gap(config.item_spacing)
            .semantics(Semantics::new(Role::Container).with_selectable_group()))
        .child(
            items
                .into_iter()
                .enumerate()
                .map(|(i, item)| {
                    let selected = i == selected_index;
                    let is_enabled = item.enabled;
                    let default_effects = AnimationSpec::spring_crit(40.0);
                    let fg_icon = animate_color(
                        format!("nb_fi_{}_{}", id, i),
                        if selected {
                            config.selected_icon_color
                        } else {
                            config.unselected_icon_color
                        },
                        default_effects,
                    );
                    let fg_label = animate_color(
                        format!("nb_fl_{}_{}", id, i),
                        if selected {
                            config.selected_text_color
                        } else {
                            config.unselected_text_color
                        },
                        default_effects,
                    );
                    let bg_alpha = animate_f32(
                        format!("nb_bg_{}_{}", id, i),
                        if selected { 1.0 } else { 0.0 },
                        default_effects,
                    );
                    let indicator_bg = config
                        .indicator_color
                        .with_alpha_f32(bg_alpha * config.indicator_opacity);
                    let cb = item.on_click.clone();
                    let nb_source: Rc<MutableInteractionSource> = item
                        .interaction_source
                        .clone()
                        .map(Rc::new)
                        .unwrap_or_else(|| remember(MutableInteractionSource::new));

                    let item_width =
                        remember_state_with_key(format!("nb_w_{}_{}", id, i), || 0.0f32);
                    let mut item_m = Modifier::new()
                        .flex_grow(1.0)
                        .fill_max_height()
                        .propagate_min_constraints(true)
                        .on_size_changed({
                            let w = item_width.clone();
                            move |size| *w.borrow_mut() = size.x
                        })
                        .semantics(Semantics::new(Role::Tab).with_label(&item.label));

                    item_m =
                        apply_m3_clickable_without_indication(item_m, &nb_source, is_enabled, {
                            let cb = cb.clone();
                            move || cb()
                        });

                    // Pill-sized ripple host - hover/focus now pill-bounded, not full item.
                    // Map press pos from outer item (full width) to pill local via MappedInteractionSource offset.
                    let pill_dx = (*item_width.borrow() - config.indicator_width) / 2.0;
                    let pill_dy = 14.0;
                    let pill_m = Modifier::new()
                        .absolute()
                        .offset(
                            Some((24.0 - config.indicator_width) / 2.0),
                            Some((24.0 - config.indicator_height) / 2.0),
                            None,
                            None,
                        )
                        .width(config.indicator_width)
                        .height(config.indicator_height)
                        .clip_rounded(config.indicator_radius)
                        .interaction_source(&nb_source)
                        .indication(ripple(RippleConfig {
                            color: Some(theme().on_surface_variant),
                            bounded: true,
                            press_offset: if *item_width.borrow() > 0.0 {
                                Some(Vec2 {
                                    x: pill_dx,
                                    y: pill_dy,
                                })
                            } else {
                                None
                            },
                            ..Default::default()
                        }));
                    let bg_m = Modifier::new()
                        .absolute()
                        .offset(
                            Some((24.0 - config.indicator_width) / 2.0),
                            Some((24.0 - config.indicator_height) / 2.0),
                            None,
                            None,
                        )
                        .width(config.indicator_width)
                        .height(config.indicator_height)
                        .background(indicator_bg)
                        .clip_rounded(config.indicator_radius)
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: Color::TRANSPARENT,
                            focused: Color::TRANSPARENT,
                            pressed: Color::TRANSPARENT,
                            dragged: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        });

                    Box(item_m).child(
                        Column(
                            Modifier::new()
                                .fill_max_size()
                                .align_items(AlignItems::CENTER)
                                .justify_content(JustifyContent::CENTER),
                        )
                        .child((
                            Column(
                                Modifier::new()
                                    .align_items(AlignItems::CENTER)
                                    .justify_content(JustifyContent::CENTER),
                            )
                            .child((
                                Box(bg_m),
                                with_content_color(fg_icon, move || item.icon),
                                Box(pill_m),
                            )),
                            // 8dp gap: 4dp IndicatorVerticalPadding + 4dp IndicatorToLabelPadding
                            Box(Modifier::new().height(8.0)),
                            Text(item.label)
                                .color(fg_label)
                                .size(th.typography.label_medium)
                                .single_line(),
                        )),
                    )
                })
                .collect::<Vec<_>>(),
        ),
    )
}

pub struct NavItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
}
