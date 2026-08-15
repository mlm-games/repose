#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Row, Text, TextStyle, ViewExt,
    anim::{animate_color, animate_f32},
};

use super::*;

/// A single tab definition for use with `TabRow`.
pub struct Tab {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
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

/// M3 Tab Row -> a horizontal row of tabs with per-tab animated-height indicators.
/// Text colors animate with DefaultEffects (spring_crit 40.0).
/// Indicator height animates with DefaultEffects (spring_crit 40.0).
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let default_effects = AnimationSpec::spring_crit(40.0);
    Column(Modifier::new().fill_max_width().then(config.modifier)).child((
        Row(Modifier::new()
            .fill_max_width()
            .height(config.height)
            .background(config.container_color)
            .semantics(Semantics::new(Role::Container).with_selectable_group()))
        .child(
            tabs.into_iter()
                .enumerate()
                .map(|(i, tab)| {
                    let selected = i == selected_index;
                    let is_enabled = tab.enabled;
                    let color = animate_color(
                        format!("tab_clr_{}_{}", id, i),
                        if selected {
                            config.selected_content_color
                        } else {
                            config.unselected_content_color
                        },
                        default_effects,
                    );
                    let indicator_h = animate_f32(
                        format!("tab_ind_h_{}_{}", id, i),
                        if selected {
                            config.indicator_height
                        } else {
                            0.0
                        },
                        default_effects,
                    );
                    let cb = tab.on_click.clone();
                    let tab_source: Rc<MutableInteractionSource> = tab
                        .interaction_source
                        .clone()
                        .map(Rc::new)
                        .unwrap_or_else(|| remember(MutableInteractionSource::new));

                    let mut tab_m = Modifier::new()
                        .flex_grow(1.0)
                        .fill_max_height()
                        .interaction_source(&tab_source)
                        .align_items(AlignItems::CENTER)
                        .justify_content(JustifyContent::CENTER)
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            focused: th.on_surface.with_alpha_f32(0.12),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            dragged: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        })
                        .semantics(Semantics::new(Role::Tab).with_label(&tab.label));

                    if is_enabled {
                        tab_m = tab_m.clickable().on_click(move || cb());
                    }

                    Column(tab_m).child((
                        tab.icon.unwrap_or(Box(Modifier::new())),
                        Text(tab.label)
                            .color(color)
                            .size(th.typography.title_small)
                            .single_line(),
                        Box(Modifier::new()
                            .fill_max_width()
                            .height(indicator_h)
                            .background(config.indicator_color)
                            .clip_rounded(TabDefaults::INDICATOR_CORNER)),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
        // Divider
        Box(Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(th.outline_variant)),
    ))
}
