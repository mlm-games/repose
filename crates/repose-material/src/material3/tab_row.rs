#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Row, Text, TextStyle, ViewExt,
    anim::{animate_color, animate_f32},
    textfield::{TextMeasureConfig, measure_text},
};

use super::util::apply_m3_clickable;
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
    pub height: Dp,
    pub indicator_height: Dp,
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

enum TabCellWidth {
    Fill,
    Min(Dp),
}

#[derive(Clone, Copy)]
enum IndicatorWidth {
    Tab,
    Content,
    Fraction(f32),
}

fn tab_cell(
    row_id: u64,
    index: usize,
    tab: Tab,
    selected: bool,
    config: &TabRowConfig,
    width: TabCellWidth,
    indicator_width: IndicatorWidth,
) -> View {
    let th = theme();
    let default_effects = AnimationSpec::spring_crit(40.0);
    let color = animate_color(
        format!("tab_clr_{}_{}", row_id, index),
        if selected {
            config.selected_content_color
        } else {
            config.unselected_content_color
        },
        default_effects,
    );
    let indicator_h = animate_f32(
        format!("tab_ind_h_{}_{}", row_id, index),
        if selected {
            config.indicator_height.0
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
        .fill_max_height()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .semantics(Semantics::new(Role::Tab).with_label(&tab.label));
    match width {
        TabCellWidth::Fill => tab_m = tab_m.flex_grow(1.0),
        TabCellWidth::Min(w) => tab_m = tab_m.min_width(w),
    }

    tab_m = apply_m3_clickable(tab_m, &tab_source, th.on_surface, tab.enabled, move || cb());

    let label_size = th.typography.title_small;
    let width = match indicator_width {
        IndicatorWidth::Tab => Modifier::new().fill_max_width(),
        IndicatorWidth::Fraction(fraction) => Modifier::new().fill_max_width_frac(fraction),
        IndicatorWidth::Content => {
            let metrics = measure_text(&tab.label, label_size.to_px().0, TextMeasureConfig::default());
            let label_width = Px(metrics.positions.last().copied().unwrap_or_default()).to_dp();
            Modifier::new().width(Dp(label_width.0.max(24.0)))
        }
    };
    let indicator = Row(Modifier::new()
        .absolute()
        .inset(Some(Dp::ZERO), None, Some(Dp::ZERO), Some(Dp::ZERO))
        .justify_content(JustifyContent::CENTER))
        .child(Box(width.height(Dp(indicator_h))
            .clip_rounded(TabDefaults::INDICATOR_CORNER)
            .background(config.indicator_color)));

    Column(tab_m.gap(Dp(4.0))).child((
        tab.icon.unwrap_or(Box(Modifier::new())),
        Text(tab.label).color(color).size(label_size).single_line(),
        indicator,
    ))
}

/// M3 Tab Row -> a horizontal row of tabs with per-tab animated-height indicators.
/// Text colors animate with DefaultEffects (spring_crit 40.0).
/// Indicator height animates with DefaultEffects (spring_crit 40.0).
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Tab)
}

fn fixed_tab_row(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig, width: IndicatorWidth) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let config_modifier = config.modifier.clone();
    Column(Modifier::new().fill_max_width().then(config_modifier)).child((
        Row(Modifier::new()
            .fill_max_width()
            .height(config.height)
            .background(config.container_color)
            .semantics(Semantics::new(Role::Container).with_selectable_group()))
        .child(
            tabs.into_iter()
                .enumerate()
                .map(|(i, tab)| {
                    tab_cell(
                        *id,
                        i,
                        tab,
                        i == selected_index,
                        &config,
                        TabCellWidth::Fill,
                        width,
                    )
                })
                .collect::<Vec<_>>(),
        ),
        Box(Modifier::new()
            .fill_max_width()
            .height(Dp(1.0))
            .background(th.outline_variant)),
    ))
}

pub fn PrimaryTabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Content)
}

pub fn SecondaryTabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Fraction(0.6))
}

#[derive(Clone, Debug)]
pub struct ScrollableTabRowConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub selected_content_color: Color,
    pub unselected_content_color: Color,
    pub indicator_color: Color,
    pub height: Dp,
    pub indicator_height: Dp,
    pub min_tab_width: Dp,
    pub edge_padding: Dp,
}

impl Default for ScrollableTabRowConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: TabDefaults::container_color(),
            selected_content_color: TabDefaults::selected_content_color(),
            unselected_content_color: TabDefaults::unselected_content_color(),
            indicator_color: TabDefaults::indicator_color(),
            height: TabDefaults::HEIGHT,
            indicator_height: TabDefaults::INDICATOR_HEIGHT,
            min_tab_width: Dp(90.0),
            edge_padding: Dp(52.0),
        }
    }
}

pub fn ScrollableTabRow(
    selected_index: usize,
    tabs: Vec<Tab>,
    config: ScrollableTabRowConfig,
) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let base = TabRowConfig {
        modifier: Modifier::new(),
        container_color: config.container_color,
        selected_content_color: config.selected_content_color,
        unselected_content_color: config.unselected_content_color,
        indicator_color: config.indicator_color,
        height: config.height,
        indicator_height: config.indicator_height,
    };
    let scroll: Rc<HorizontalScrollState> =
        remember_with_key(format!("stab_scroll_{id}"), HorizontalScrollState::new);
    let row = Row(Modifier::new()
        .fill_max_height()
        .background(config.container_color)
        .padding_values(PaddingValues {
            left: config.edge_padding,
            right: config.edge_padding,
            top: Dp::ZERO,
            bottom: Dp::ZERO,
        })
        .semantics(Semantics::new(Role::Container).with_selectable_group()))
    .child(
        tabs.into_iter()
            .enumerate()
            .map(|(i, tab)| {
                tab_cell(
                    *id,
                    i,
                    tab,
                    i == selected_index,
                    &base,
                    TabCellWidth::Min(config.min_tab_width),
                    IndicatorWidth::Tab,
                )
            })
            .collect::<Vec<_>>(),
    );
    Column(Modifier::new().fill_max_width().then(config.modifier)).child((
        repose_ui::scroll::HorizontalScrollArea(
            Modifier::new().fill_max_width().height(config.height),
            scroll,
            row,
        ),
        Box(Modifier::new()
            .fill_max_width()
            .height(Dp(1.0))
            .background(th.outline_variant)),
    ))
}
