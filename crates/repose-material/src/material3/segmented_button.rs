#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{Box, Row, Text, TextStyle, ViewExt, anim::animate_color};

use super::*;

/// Configuration for a single segment in [`SegmentedButton`].
#[derive(Clone)]
pub struct SegmentConfig {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            label: String::new(),
            icon: None,
            on_click: Rc::new(|| {}),
            enabled: true,
            interaction_source: None,
        }
    }
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
    pub content_padding: PaddingValues,
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
            content_padding: SegmentedButtonDefaults::CONTENT_PADDING,
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
    segments: Vec<SegmentConfig>,
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
                let seg_source: Rc<MutableInteractionSource> = seg
                    .interaction_source
                    .clone()
                    .map(Rc::new)
                    .unwrap_or_else(|| remember(MutableInteractionSource::new));

                let state_colors = config.state_colors;
                let content_modifier = Modifier::new()
                    .flex_grow(1.0)
                    .fill_max_height()
                    .clip_rounded_radii(radii)
                    .background(bg)
                    .state_colors(state_colors)
                    .interaction_source(&seg_source)
                    .align_items(AlignItems::CENTER)
                    .justify_content(JustifyContent::CENTER)
                    .padding_values(config.content_padding);

                let content_modifier = if is_enabled {
                    content_modifier.clickable().on_click(move || cb())
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
