#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{
    Box, Column, Row, Text, TextStyle, ViewExt,
    anim::{animate_color_directional, animate_f32_from},
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

#[derive(Clone, Copy)]
enum IndicatorWidth {
    Tab,
    Content,
    Fraction(f32),
}

fn label_width_px(label: &str, style_px: f32, has_icon: bool) -> f32 {
    let metrics = measure_text(label, style_px, TextMeasureConfig::default());
    let text = metrics.positions.last().copied().unwrap_or_default();
    let icon = if has_icon {
        Dp(24.0).to_px().0 + Dp(4.0).to_px().0
    } else {
        0.0
    };
    (text + icon).max(Dp(24.0).to_px().0)
}

fn tab_cell(
    row_id: u64,
    index: usize,
    tab: Tab,
    selected: bool,
    config: &TabRowConfig,
    fill: bool,
    min_width: Option<Dp>,
    width_state: Mutable<f32>,
) -> View {
    let th = theme();
    let color = animate_color_directional(
        format!("tab_clr_{}_{}", row_id, index),
        if selected {
            config.selected_content_color
        } else {
            config.unselected_content_color
        },
        selected,
        th.motion.color,
        th.motion.fast_effects,
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
        .on_size_changed({
            let width = width_state.clone();
            move |size| width.set_neq(size.x)
        })
        .semantics(Semantics::new(Role::Tab).with_label(&tab.label));
    if fill {
        tab_m = tab_m.flex_grow(1.0);
    }
    if let Some(w) = min_width {
        tab_m = tab_m.min_width(w);
    }

    tab_m = apply_m3_clickable(
        tab_m,
        &tab_source,
        config.selected_content_color,
        tab.enabled,
        move || cb(),
    );

    Column(tab_m.gap(Dp(4.0))).child((
        tab.icon.unwrap_or(Box(Modifier::new())),
        Text(tab.label)
            .color(color)
            .size(th.typography.title_small)
            .single_line(),
    ))
}

fn indicator_view(row_id: u64, left_px: f32, width_px: f32, height: Dp, color: Color) -> View {
    if width_px <= 1.0 {
        return Box(Modifier::new());
    }
    let spec = theme().motion.default_spatial;
    let left = animate_f32_from(format!("tab_ind_x_{row_id}"), left_px, left_px, spec);
    let width = animate_f32_from(format!("tab_ind_w_{row_id}"), width_px, width_px, spec);
    Box(Modifier::new()
        .absolute()
        .inset(Some(Px(left).to_dp()), None, None, Some(Dp::ZERO))
        .width(Px(width).to_dp())
        .height(height)
        .clip_rounded(TabDefaults::INDICATOR_CORNER)
        .background(color))
}

fn indicator_geometry(
    cells: &[f32],
    content_widths: &[f32],
    selected: usize,
    mode: IndicatorWidth,
) -> (f32, f32) {
    if cells.is_empty() {
        return (0.0, 0.0);
    }
    let i = selected.min(cells.len() - 1);
    let cell = cells[i].max(1.0);
    let width = match mode {
        IndicatorWidth::Tab => cell,
        IndicatorWidth::Fraction(f) => cell * f.clamp(0.0, 1.0),
        IndicatorWidth::Content => content_widths.get(i).copied().unwrap_or(cell).min(cell),
    };
    let left: f32 = cells.iter().take(i).sum::<f32>() + (cell - width) / 2.0;
    (left, width)
}

/// M3 Tab Row. A single indicator slides and resizes between tabs, matching Compose.
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Tab, false)
}

fn fixed_tab_row(
    selected_index: usize,
    tabs: Vec<Tab>,
    config: TabRowConfig,
    mode: IndicatorWidth,
    scrollable: bool,
) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let label_size = th.typography.title_small;
    let content_widths: Vec<f32> = tabs
        .iter()
        .map(|tab| label_width_px(&tab.label, label_size.to_px().0, tab.icon.is_some()))
        .collect();
    let row_height = if tabs
        .iter()
        .any(|tab| tab.icon.is_some() && !tab.label.is_empty())
    {
        Dp(72.0)
    } else {
        config.height
    };

    let widths: Vec<Mutable<f32>> = (0..tabs.len())
        .map(|i| remember_mutable_with_key(format!("tab_w_{}_{}", id, i), || 0.0f32))
        .collect();
    let cells: Vec<f32> = widths.iter().map(|w| *w.get()).collect();
    let (left, width) = indicator_geometry(&cells, &content_widths, selected_index, mode);

    let mut children: Vec<View> = tabs
        .into_iter()
        .enumerate()
        .map(|(i, tab)| {
            tab_cell(
                *id,
                i,
                tab,
                i == selected_index,
                &config,
                true,
                None,
                widths[i].clone(),
            )
        })
        .collect();
    children.push(indicator_view(
        *id,
        left,
        width,
        config.indicator_height,
        config.indicator_color,
    ));

    let row = Row(Modifier::new()
        .fill_max_width()
        .height(row_height)
        .background(config.container_color)
        .semantics(Semantics::new(Role::Container).with_selectable_group()))
    .with_children(children);

    if scrollable {
        return row;
    }
    Column(
        Modifier::new()
            .fill_max_width()
            .then(config.modifier.clone()),
    )
    .child((
        row,
        Box(Modifier::new()
            .fill_max_width()
            .height(Dp(1.0))
            .background(th.outline_variant)),
    ))
}

pub fn PrimaryTabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Content, false)
}

pub fn SecondaryTabRow(selected_index: usize, tabs: Vec<Tab>, config: TabRowConfig) -> View {
    fixed_tab_row(selected_index, tabs, config, IndicatorWidth::Tab, false)
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
    let scroll: Rc<HorizontalScrollState> =
        remember_with_key(format!("stab_scroll_{id}"), HorizontalScrollState::new);
    let viewport = remember_mutable_with_key(format!("stab_vp_{id}"), || 0.0f32);

    let label_size = th.typography.title_small;
    let content_widths: Vec<f32> = tabs
        .iter()
        .map(|tab| label_width_px(&tab.label, label_size.to_px().0, tab.icon.is_some()))
        .collect();
    let widths: Vec<Mutable<f32>> = (0..tabs.len())
        .map(|i| remember_mutable_with_key(format!("stab_w_{}_{}", id, i), || 0.0f32))
        .collect();
    let cells: Vec<f32> = widths.iter().map(|w| *w.get()).collect();
    let (left, width) = indicator_geometry(
        &cells,
        &content_widths,
        selected_index,
        IndicatorWidth::Content,
    );
    let edge_px = config.edge_padding.to_px().0;

    let total: f32 = cells.iter().sum();
    let vp = *viewport.get();
    if vp > 0.0 {
        let center: f32 = cells.iter().take(selected_index).sum::<f32>()
            + cells.get(selected_index).copied().unwrap_or(0.0) / 2.0;
        let target = (center - vp / 2.0).clamp(0.0, (total - vp).max(0.0));
        let animated = animate_f32_from(
            format!("stab_off_{id}"),
            0.0,
            target,
            th.motion.default_spatial,
        );
        scroll.set_offset(animated);
    }

    let base = TabRowConfig {
        modifier: Modifier::new(),
        container_color: config.container_color,
        selected_content_color: config.selected_content_color,
        unselected_content_color: config.unselected_content_color,
        indicator_color: config.indicator_color,
        height: config.height,
        indicator_height: config.indicator_height,
    };
    let mut children: Vec<View> = tabs
        .into_iter()
        .enumerate()
        .map(|(i, tab)| {
            tab_cell(
                *id,
                i,
                tab,
                i == selected_index,
                &base,
                false,
                Some(config.min_tab_width),
                widths[i].clone(),
            )
        })
        .collect();
    if width <= 1.0 {
        children.push(Box(Modifier::new()));
    } else {
        let spec = theme().motion.default_spatial;
        let ind_left = animate_f32_from(
            format!("stab_ind_x_{id}"),
            left + edge_px,
            left + edge_px,
            spec,
        );
        let ind_width = animate_f32_from(format!("stab_ind_w_{id}"), width, width, spec);
        children.push(Box(Modifier::new()
            .absolute()
            .inset(Some(Px(ind_left).to_dp()), None, None, Some(Dp::ZERO))
            .width(Px(ind_width).to_dp())
            .height(config.indicator_height)
            .clip_rounded(TabDefaults::INDICATOR_CORNER)
            .background(config.indicator_color)));
    }

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
    .with_children(children);

    Column(Modifier::new().fill_max_width().then(config.modifier)).child((
        repose_ui::scroll::HorizontalScrollArea(
            Modifier::new()
                .fill_max_width()
                .height(config.height)
                .on_size_changed({
                    let vp = viewport.clone();
                    move |size| vp.set_neq(size.x)
                }),
            scroll,
            row,
        ),
        Box(Modifier::new()
            .fill_max_width()
            .height(Dp(1.0))
            .background(th.outline_variant)),
    ))
}
