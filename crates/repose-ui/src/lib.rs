#![allow(non_snake_case)]
//! Widgets, layout and text fields.

pub mod anim;
pub mod anim_ext;
pub mod gestures;
pub mod lazy;
pub mod navigation;
pub mod scroll;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::{cell::RefCell, cmp::Ordering};

use repose_core::*;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Style};
use taffy::{Overflow, Point, ResolveOrZero};

use taffy::prelude::{Position, Size, auto, length, percent};

pub mod textfield;
pub use textfield::{TextField, TextFieldState};

use crate::textfield::{TF_FONT_DP, TF_PADDING_X_DP, byte_to_char_index, measure_text};
use repose_core::locals;

#[derive(Default)]
pub struct Interactions {
    pub hover: Option<u64>,
    pub pressed: HashSet<u64>,
}

pub fn Surface(modifier: Modifier, child: View) -> View {
    let mut v = View::new(0, ViewKind::Surface).modifier(modifier);
    v.children = vec![child];
    v
}

pub fn Box(modifier: Modifier) -> View {
    View::new(0, ViewKind::Box).modifier(modifier)
}

pub fn Row(modifier: Modifier) -> View {
    View::new(0, ViewKind::Row).modifier(modifier)
}

pub fn Column(modifier: Modifier) -> View {
    View::new(0, ViewKind::Column).modifier(modifier)
}

pub fn Stack(modifier: Modifier) -> View {
    View::new(0, ViewKind::Stack).modifier(modifier)
}

#[deprecated = "Use ScollArea instead"]
pub fn Scroll(modifier: Modifier) -> View {
    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll: None,
            set_viewport_height: None,
            set_content_height: None,
            get_scroll_offset: None,
            set_scroll_offset: None,
        },
    )
    .modifier(modifier)
}

pub fn Text(text: impl Into<String>) -> View {
    View::new(
        0,
        ViewKind::Text {
            text: text.into(),
            color: Color::WHITE,
            font_size: 16.0, // dp (converted to px in layout/paint)
            soft_wrap: false,
            max_lines: None,
            overflow: TextOverflow::Visible,
        },
    )
}

pub fn Spacer() -> View {
    Box(Modifier::new().flex_grow(1.0))
}

pub fn Grid(
    columns: usize,
    modifier: Modifier,
    children: Vec<View>,
    row_gap: f32,
    column_gap: f32,
) -> View {
    Column(modifier.grid(columns, row_gap, column_gap)).with_children(children)
}

pub fn Button(text: impl Into<String>, on_click: impl Fn() + 'static) -> View {
    View::new(
        0,
        ViewKind::Button {
            text: text.into(),
            on_click: Some(Rc::new(on_click)),
        },
    )
    .semantics(Semantics {
        role: Role::Button,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn Checkbox(
    checked: bool,
    label: impl Into<String>,
    on_change: impl Fn(bool) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::Checkbox {
            checked,
            label: label.into(),
            on_change: Some(Rc::new(on_change)),
        },
    )
    .semantics(Semantics {
        role: Role::Checkbox,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn RadioButton(
    selected: bool,
    label: impl Into<String>,
    on_select: impl Fn() + 'static,
) -> View {
    View::new(
        0,
        ViewKind::RadioButton {
            selected,
            label: label.into(),
            on_select: Some(Rc::new(on_select)),
        },
    )
    .semantics(Semantics {
        role: Role::RadioButton,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn Switch(checked: bool, label: impl Into<String>, on_change: impl Fn(bool) + 'static) -> View {
    View::new(
        0,
        ViewKind::Switch {
            checked,
            label: label.into(),
            on_change: Some(Rc::new(on_change)),
        },
    )
    .semantics(Semantics {
        role: Role::Switch,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn Slider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    label: impl Into<String>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::Slider {
            value,
            min: range.0,
            max: range.1,
            step,
            label: label.into(),
            on_change: Some(Rc::new(on_change)),
        },
    )
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
    label: impl Into<String>,
    on_change: impl Fn(f32, f32) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::RangeSlider {
            start,
            end,
            min: range.0,
            max: range.1,
            step,
            label: label.into(),
            on_change: Some(Rc::new(on_change)),
        },
    )
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn ProgressBar(value: f32, range: (f32, f32), label: impl Into<String>) -> View {
    View::new(
        0,
        ViewKind::ProgressBar {
            value,
            min: range.0,
            max: range.1,
            label: label.into(),
            circular: false,
        },
    )
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn Image(modifier: Modifier, handle: ImageHandle) -> View {
    View::new(
        0,
        ViewKind::Image {
            handle,
            tint: Color::WHITE,
            fit: ImageFit::Contain,
        },
    )
    .modifier(modifier)
}

pub trait ImageExt {
    fn image_tint(self, c: Color) -> View;
    fn image_fit(self, fit: ImageFit) -> View;
}
impl ImageExt for View {
    fn image_tint(mut self, c: Color) -> View {
        if let ViewKind::Image { tint, .. } = &mut self.kind {
            *tint = c;
        }
        self
    }
    fn image_fit(mut self, fit: ImageFit) -> View {
        if let ViewKind::Image { fit: f, .. } = &mut self.kind {
            *f = fit;
        }
        self
    }
}

fn flex_dir_for(kind: &ViewKind) -> Option<FlexDirection> {
    match kind {
        ViewKind::Row => {
            if repose_core::locals::text_direction() == repose_core::locals::TextDirection::Rtl {
                Some(FlexDirection::RowReverse)
            } else {
                Some(FlexDirection::Row)
            }
        }
        ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV { .. } => {
            Some(FlexDirection::Column)
        }
        _ => None,
    }
}

/// Extension trait for child building
pub trait ViewExt: Sized {
    fn child(self, children: impl IntoChildren) -> Self;
}

impl ViewExt for View {
    fn child(self, children: impl IntoChildren) -> Self {
        self.with_children(children.into_children())
    }
}

pub trait IntoChildren {
    fn into_children(self) -> Vec<View>;
}

impl IntoChildren for View {
    fn into_children(self) -> Vec<View> {
        vec![self]
    }
}

impl IntoChildren for Vec<View> {
    fn into_children(self) -> Vec<View> {
        self
    }
}

impl<const N: usize> IntoChildren for [View; N] {
    fn into_children(self) -> Vec<View> {
        self.into()
    }
}

// Tuple implementations
macro_rules! impl_into_children_tuple {
    ($($idx:tt $t:ident),+) => {
        impl<$($t: IntoChildren),+> IntoChildren for ($($t,)+) {
            fn into_children(self) -> Vec<View> {
                let mut v = Vec::new();
                $(v.extend(self.$idx.into_children());)+
                v
            }
        }
    };
}

impl_into_children_tuple!(0 A, 1 B);
impl_into_children_tuple!(0 A, 1 B, 2 C);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H);

/// Layout and paint with TextField state injection (Taffy 0.9 API)
pub fn layout_and_paint(
    root: &View,
    size_px_u32: (u32, u32),
    textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
    interactions: &Interactions,
    focused: Option<u64>,
) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
    // Unit helpers
    // dp -> px using current Density
    let px = |dp_val: f32| dp_to_px(dp_val);
    // font dp -> px with TextScale applied
    let font_px = |dp_font: f32| dp_to_px(dp_font) * locals::text_scale().0;

    // Assign ids
    let mut id = 1u64;
    fn stamp(mut v: View, id: &mut u64) -> View {
        v.id = *id;
        *id += 1;
        v.children = v.children.into_iter().map(|c| stamp(c, id)).collect();
        v
    }
    let root = stamp(root.clone(), &mut id);

    // Build Taffy tree (with per-node contexts for measurement)
    use taffy::prelude::*;
    #[derive(Clone)]
    enum NodeCtx {
        Text {
            text: String,
            font_dp: f32, // logical size (dp)
            soft_wrap: bool,
            max_lines: Option<usize>,
            overflow: TextOverflow,
        },
        Button {
            label: String,
        },
        TextField,
        Container,
        ScrollContainer,
        Checkbox {
            label: String,
        },
        Radio {
            label: String,
        },
        Switch {
            label: String,
        },
        Slider {
            label: String,
        },
        Range {
            label: String,
        },
        Progress {
            label: String,
        },
    }

    let mut taffy: TaffyTree<NodeCtx> = TaffyTree::new();
    let mut nodes_map = HashMap::new();

    #[derive(Clone)]
    struct TextLayout {
        lines: Vec<String>,
        size_px: f32,
        line_h_px: f32,
    }
    use std::collections::HashMap as StdHashMap;
    let mut text_cache: StdHashMap<taffy::NodeId, TextLayout> = StdHashMap::new();

    fn style_from_modifier(m: &Modifier, kind: &ViewKind, px: &dyn Fn(f32) -> f32) -> Style {
        use taffy::prelude::*;
        let mut s = Style::default();

        // Display role
        s.display = match kind {
            ViewKind::Row => Display::Flex,
            ViewKind::Column
            | ViewKind::Surface
            | ViewKind::ScrollV { .. }
            | ViewKind::ScrollXY { .. } => Display::Flex,
            ViewKind::Stack => Display::Grid,
            _ => Display::Flex,
        };

        // Flex direction
        if matches!(kind, ViewKind::Row) {
            s.flex_direction =
                if crate::locals::text_direction() == crate::locals::TextDirection::Rtl {
                    FlexDirection::RowReverse
                } else {
                    FlexDirection::Row
                };
        }
        if matches!(
            kind,
            ViewKind::Column
                | ViewKind::Surface
                | ViewKind::ScrollV { .. }
                | ViewKind::ScrollXY { .. }
        ) {
            s.flex_direction = FlexDirection::Column;
        }

        // Defaults
        s.align_items = if matches!(
            kind,
            ViewKind::Row
                | ViewKind::Column
                | ViewKind::Stack
                | ViewKind::Surface
                | ViewKind::ScrollV { .. }
                | ViewKind::ScrollXY { .. }
        ) {
            Some(AlignItems::Stretch)
        } else {
            Some(AlignItems::FlexStart)
        };
        s.justify_content = Some(JustifyContent::FlexStart);

        // Aspect ratio
        if let Some(r) = m.aspect_ratio {
            s.aspect_ratio = Some(r.max(0.0));
        }

        // Flex props
        if let Some(g) = m.flex_grow {
            s.flex_grow = g;
        }
        if let Some(sh) = m.flex_shrink {
            s.flex_shrink = sh;
        }
        if let Some(b_dp) = m.flex_basis {
            s.flex_basis = length(px(b_dp.max(0.0)));
        }

        // Align self
        if let Some(a) = m.align_self {
            s.align_self = Some(a);
        }

        // Absolute positioning (convert insets from dp to px)
        if let Some(crate::modifier::PositionType::Absolute) = m.position_type {
            s.position = Position::Absolute;
            s.inset = taffy::geometry::Rect {
                left: m.offset_left.map(|v| length(px(v))).unwrap_or_else(auto),
                right: m.offset_right.map(|v| length(px(v))).unwrap_or_else(auto),
                top: m.offset_top.map(|v| length(px(v))).unwrap_or_else(auto),
                bottom: m.offset_bottom.map(|v| length(px(v))).unwrap_or_else(auto),
            };
        }

        // Grid config
        if let Some(cfg) = &m.grid {
            s.display = Display::Grid;
            s.grid_template_columns = (0..cfg.columns.max(1))
                .map(|_| GridTemplateComponent::Single(flex(1.0)))
                .collect();
            s.gap = Size {
                width: length(px(cfg.column_gap)),
                height: length(px(cfg.row_gap)),
            };
        }

        // Scrollables clip; sizing is decided by explicit/fill logic below
        if matches!(kind, ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. }) {
            s.overflow = Point {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            };
        }

        // Padding (content box). With axis-aware fill below, padding stays inside the allocated box.
        if let Some(pv_dp) = m.padding_values {
            s.padding = taffy::geometry::Rect {
                left: length(px(pv_dp.left)),
                right: length(px(pv_dp.right)),
                top: length(px(pv_dp.top)),
                bottom: length(px(pv_dp.bottom)),
            };
        } else if let Some(p_dp) = m.padding {
            let v = length(px(p_dp));
            s.padding = taffy::geometry::Rect {
                left: v,
                right: v,
                top: v,
                bottom: v,
            };
        }

        // Explicit size — highest priority
        let mut width_set = false;
        let mut height_set = false;
        if let Some(sz_dp) = m.size {
            if sz_dp.width.is_finite() {
                s.size.width = length(px(sz_dp.width.max(0.0)));
                width_set = true;
            }
            if sz_dp.height.is_finite() {
                s.size.height = length(px(sz_dp.height.max(0.0)));
                height_set = true;
            }
        }
        if let Some(w_dp) = m.width {
            s.size.width = length(px(w_dp.max(0.0)));
            width_set = true;
        }
        if let Some(h_dp) = m.height {
            s.size.height = length(px(h_dp.max(0.0)));
            height_set = true;
        }

        // Axis-aware fill
        let is_row = matches!(kind, ViewKind::Row);
        let is_column = matches!(
            kind,
            ViewKind::Column
                | ViewKind::Surface
                | ViewKind::ScrollV { .. }
                | ViewKind::ScrollXY { .. }
        );

        let want_fill_w = m.fill_max || m.fill_max_w;
        let want_fill_h = m.fill_max || m.fill_max_h;

        // Main axis fill -> weight (flex: 1 1 0%), Cross axis fill -> tight (min==max==100%)
        if is_column {
            // main axis = vertical
            if want_fill_h && !height_set {
                s.flex_grow = s.flex_grow.max(1.0);
                s.flex_shrink = s.flex_shrink.max(1.0);
                s.flex_basis = length(0.0);
                s.min_size.height = length(0.0); // allow shrinking, avoid min-content expansion
            }
            if want_fill_w && !width_set {
                s.min_size.width = percent(1.0);
                s.max_size.width = percent(1.0);
            }
        } else if is_row {
            // main axis = horizontal
            if want_fill_w && !width_set {
                s.flex_grow = s.flex_grow.max(1.0);
                s.flex_shrink = s.flex_shrink.max(1.0);
                s.flex_basis = length(0.0);
                s.min_size.width = length(0.0);
            }
            if want_fill_h && !height_set {
                s.min_size.height = percent(1.0);
                s.max_size.height = percent(1.0);
            }
        } else {
            // Fallback: treat like Column
            if want_fill_h && !height_set {
                s.flex_grow = s.flex_grow.max(1.0);
                s.flex_shrink = s.flex_shrink.max(1.0);
                s.flex_basis = length(0.0);
                s.min_size.height = length(0.0);
            }
            if want_fill_w && !width_set {
                s.min_size.width = percent(1.0);
                s.max_size.width = percent(1.0);
            }
        }

        if matches!(kind, ViewKind::Surface) {
            if (m.fill_max || m.fill_max_w) && s.min_size.width.is_auto() && !width_set {
                s.min_size.width = percent(1.0);
                s.max_size.width = percent(1.0);
            }
            if (m.fill_max || m.fill_max_h) && s.min_size.height.is_auto() && !height_set {
                s.min_size.height = percent(1.0);
                s.max_size.height = percent(1.0);
            }
        }

        // user min/max clamps
        if let Some(v_dp) = m.min_width {
            s.min_size.width = length(px(v_dp.max(0.0)));
        }
        if let Some(v_dp) = m.min_height {
            s.min_size.height = length(px(v_dp.max(0.0)));
        }
        if let Some(v_dp) = m.max_width {
            s.max_size.width = length(px(v_dp.max(0.0)));
        }
        if let Some(v_dp) = m.max_height {
            s.max_size.height = length(px(v_dp.max(0.0)));
        }

        s
    }

    fn build_node(
        v: &View,
        t: &mut TaffyTree<NodeCtx>,
        nodes_map: &mut HashMap<ViewId, taffy::NodeId>,
    ) -> taffy::NodeId {
        // We'll inject px() at call-site (need locals access); this function
        // is called from a scope that has the helper closure.
        let px_helper = |dp_val: f32| dp_to_px(dp_val);

        let mut style = style_from_modifier(&v.modifier, &v.kind, &px_helper);

        if v.modifier.grid_col_span.is_some() || v.modifier.grid_row_span.is_some() {
            use taffy::prelude::{GridPlacement, Line};

            let col_span = v.modifier.grid_col_span.unwrap_or(1).max(1);
            let row_span = v.modifier.grid_row_span.unwrap_or(1).max(1);

            style.grid_column = Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Span(col_span),
            };
            style.grid_row = Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Span(row_span),
            };
        }

        let children: Vec<_> = v
            .children
            .iter()
            .map(|c| build_node(c, t, nodes_map))
            .collect();

        let node = match &v.kind {
            ViewKind::Text {
                text,
                font_size: font_dp,
                soft_wrap,
                max_lines,
                overflow,
                ..
            } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Text {
                        text: text.clone(),
                        font_dp: *font_dp,
                        soft_wrap: *soft_wrap,
                        max_lines: *max_lines,
                        overflow: *overflow,
                    },
                )
                .unwrap(),
            ViewKind::Button { text, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Button {
                        label: text.clone(),
                    },
                )
                .unwrap(),
            ViewKind::TextField { .. } => {
                t.new_leaf_with_context(style, NodeCtx::TextField).unwrap()
            }
            ViewKind::Image { .. } => t.new_leaf_with_context(style, NodeCtx::Container).unwrap(),
            ViewKind::Checkbox { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Checkbox {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::RadioButton { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Radio {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::Switch { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Switch {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::Slider { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Slider {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::RangeSlider { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Range {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::ProgressBar { label, .. } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Progress {
                        label: label.clone(),
                    },
                )
                .unwrap(),
            ViewKind::ScrollV { .. } => {
                let children: Vec<_> = v
                    .children
                    .iter()
                    .map(|c| build_node(c, t, nodes_map))
                    .collect();

                let n = t.new_with_children(style, &children).unwrap();
                t.set_node_context(n, Some(NodeCtx::ScrollContainer)).ok();
                n
            }
            _ => {
                let n = t.new_with_children(style, &children).unwrap();
                t.set_node_context(n, Some(NodeCtx::Container)).ok();
                n
            }
        };

        nodes_map.insert(v.id, node);
        node
    }

    let root_node = build_node(&root, &mut taffy, &mut nodes_map);

    {
        let mut rs = taffy.style(root_node).unwrap().clone();
        rs.size.width = length(size_px_u32.0 as f32);
        rs.size.height = length(size_px_u32.1 as f32);
        taffy.set_style(root_node, rs).unwrap();
    }

    let available = taffy::geometry::Size {
        width: AvailableSpace::Definite(size_px_u32.0 as f32),
        height: AvailableSpace::Definite(size_px_u32.1 as f32),
    };

    // Measure function for intrinsic content
    taffy
        .compute_layout_with_measure(root_node, available, |known, avail, node, ctx, _style| {
            match ctx {
                Some(NodeCtx::Text {
                    text,
                    font_dp,
                    soft_wrap,
                    max_lines,
                    overflow,
                }) => {
                    // Apply density + text scale in measure so paint matches exactly
                    let size_px_val = font_px(*font_dp);
                    let line_h_px_val = size_px_val * 1.3;

                    // Content-hugging width by default (unless caller set known.width).
                    let approx_w_px = text.len() as f32 * size_px_val * 0.6; // rough estimate (glyph-width-ish)
                    let measured_w_px = known.width.unwrap_or(approx_w_px);

                    // Wrap width in px if soft wrap enabled
                    let wrap_w_px = if *soft_wrap {
                        match avail.width {
                            AvailableSpace::Definite(w) => w,
                            _ => measured_w_px,
                        }
                    } else {
                        measured_w_px
                    };

                    // Produce final lines once and cache
                    let lines_vec: Vec<String> = if *soft_wrap {
                        let (ls, _trunc) =
                            repose_text::wrap_lines(text, size_px_val, wrap_w_px, *max_lines, true);
                        ls
                    } else {
                        match overflow {
                            TextOverflow::Ellipsis => {
                                vec![repose_text::ellipsize_line(text, size_px_val, wrap_w_px)]
                            }
                            _ => vec![text.clone()],
                        }
                    };
                    text_cache.insert(
                        node,
                        TextLayout {
                            lines: lines_vec.clone(),
                            size_px: size_px_val,
                            line_h_px: line_h_px_val,
                        },
                    );
                    let n_lines = lines_vec.len().max(1);

                    taffy::geometry::Size {
                        width: measured_w_px,
                        height: line_h_px_val * n_lines as f32,
                    }
                }
                Some(NodeCtx::Button { label }) => taffy::geometry::Size {
                    width: (label.len() as f32 * font_px(16.0) * 0.6) + px(24.0),
                    height: px(36.0),
                },
                Some(NodeCtx::TextField) => taffy::geometry::Size {
                    width: known.width.unwrap_or(px(220.0)),
                    height: px(36.0),
                },
                Some(NodeCtx::Checkbox { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px = px(24.0) + px(8.0) + label_w_px; // box + gap + text estimate
                    taffy::geometry::Size {
                        width: known.width.unwrap_or(w_px),
                        height: px(24.0),
                    }
                }
                Some(NodeCtx::Radio { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px = px(24.0) + px(8.0) + label_w_px; // circle + gap + text estimate
                    taffy::geometry::Size {
                        width: known.width.unwrap_or(w_px),
                        height: px(24.0),
                    }
                }
                Some(NodeCtx::Switch { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px = (known.width)
                        .unwrap_or(px(46.0) + px(8.0) + label_w_px)
                        .max(px(80.0));
                    taffy::geometry::Size {
                        width: w_px,
                        height: px(28.0),
                    }
                }
                Some(NodeCtx::Slider { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px =
                        (known.width).unwrap_or(px(200.0).max(px(46.0) + px(8.0) + label_w_px));
                    taffy::geometry::Size {
                        width: w_px,
                        height: px(28.0),
                    }
                }
                Some(NodeCtx::Range { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px =
                        (known.width).unwrap_or(px(220.0).max(px(46.0) + px(8.0) + label_w_px));
                    taffy::geometry::Size {
                        width: w_px,
                        height: px(28.0),
                    }
                }
                Some(NodeCtx::Progress { label }) => {
                    let label_w_px = (label.len() as f32) * font_px(16.0) * 0.6;
                    let w_px =
                        (known.width).unwrap_or(px(200.0).max(px(100.0) + px(8.0) + label_w_px));
                    taffy::geometry::Size {
                        width: w_px,
                        height: px(12.0) + px(8.0),
                    }
                }
                Some(NodeCtx::ScrollContainer) | Some(NodeCtx::Container) | None => {
                    taffy::geometry::Size::ZERO
                }
            }
        })
        .unwrap();

    // eprintln!(
    //     "win {:?}x{:?} root {:?}",
    //     size_px_u32.0,
    //     size_px_u32.1,
    //     taffy.layout(root_node).unwrap().size
    // );

    fn layout_of(node: taffy::NodeId, t: &TaffyTree<impl Clone>) -> repose_core::Rect {
        let l = t.layout(node).unwrap();
        repose_core::Rect {
            x: l.location.x,
            y: l.location.y,
            w: l.size.width,
            h: l.size.height,
        }
    }

    fn add_offset(mut r: repose_core::Rect, off: (f32, f32)) -> repose_core::Rect {
        r.x += off.0;
        r.y += off.1;
        r
    }

    // Rect intersection helper for hit clipping
    fn intersect(a: repose_core::Rect, b: repose_core::Rect) -> Option<repose_core::Rect> {
        let x0 = a.x.max(b.x);
        let y0 = a.y.max(b.y);
        let x1 = (a.x + a.w).min(b.x + b.w);
        let y1 = (a.y + a.h).min(b.y + b.h);
        let w = (x1 - x0).max(0.0);
        let h = (y1 - y0).max(0.0);
        if w <= 0.0 || h <= 0.0 {
            None
        } else {
            Some(repose_core::Rect { x: x0, y: y0, w, h })
        }
    }

    fn clamp01(x: f32) -> f32 {
        x.max(0.0).min(1.0)
    }
    fn norm(value: f32, min: f32, max: f32) -> f32 {
        if max > min {
            (value - min) / (max - min)
        } else {
            0.0
        }
    }
    fn denorm(t: f32, min: f32, max: f32) -> f32 {
        min + t * (max - min)
    }
    fn snap_step(v: f32, step: Option<f32>, min: f32, max: f32) -> f32 {
        match step {
            Some(s) if s > 0.0 => {
                let k = ((v - min) / s).round();
                (min + k * s).clamp(min, max)
            }
            _ => v.clamp(min, max),
        }
    }
    fn mul_alpha(c: Color, a: f32) -> Color {
        let mut out = c;
        let na = ((c.3 as f32) * a).clamp(0.0, 255.0) as u8;
        out.3 = na;
        out
    }
    // draws scrollbar and registers their drag hit regions (both)
    fn push_scrollbar_v(
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        interactions: &Interactions,
        view_id: u64,
        vp: crate::Rect,
        content_h_px: f32,
        off_y_px: f32,
        z: f32,
        set_scroll_offset: Option<Rc<dyn Fn(f32)>>,
    ) {
        if content_h_px <= vp.h + 0.5 {
            return;
        }
        let thickness_px = dp_to_px(6.0);
        let margin_px = dp_to_px(2.0);
        let min_thumb_px = dp_to_px(24.0);
        let th = locals::theme();

        // Track geometry (inset inside viewport)
        let track_x = vp.x + vp.w - margin_px - thickness_px;
        let track_y = vp.y + margin_px;
        let track_h = (vp.h - 2.0 * margin_px).max(0.0);

        // Thumb geometry from content ratio
        let ratio = (vp.h / content_h_px).clamp(0.0, 1.0);
        let thumb_h = (track_h * ratio).clamp(min_thumb_px, track_h);
        let denom = (content_h_px - vp.h).max(1.0);
        let tpos = (off_y_px / denom).clamp(0.0, 1.0);
        let max_pos = (track_h - thumb_h).max(0.0);
        let thumb_y = track_y + tpos * max_pos;

        scene.nodes.push(SceneNode::Rect {
            rect: crate::Rect {
                x: track_x,
                y: track_y,
                w: thickness_px,
                h: track_h,
            },
            color: th.scrollbar_track,
            radius: thickness_px * 0.5,
        });
        scene.nodes.push(SceneNode::Rect {
            rect: crate::Rect {
                x: track_x,
                y: thumb_y,
                w: thickness_px,
                h: thumb_h,
            },
            color: th.scrollbar_thumb,
            radius: thickness_px * 0.5,
        });
        if let Some(setter) = set_scroll_offset {
            let thumb_id: u64 = view_id ^ 0x8000_0001;
            let map_to_off = Rc::new(move |py_px: f32| -> f32 {
                let denom = (content_h_px - vp.h).max(1.0);
                let max_pos = (track_h - thumb_h).max(0.0);
                let pos = ((py_px - track_y) - thumb_h * 0.5).clamp(0.0, max_pos);
                let t = if max_pos > 0.0 { pos / max_pos } else { 0.0 };
                t * denom
            });
            let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                let setter = setter.clone();
                let map = map_to_off.clone();
                Rc::new(move |pe| setter(map(pe.position.y)))
            };
            let on_pm: Option<Rc<dyn Fn(repose_core::input::PointerEvent)>> =
                if interactions.pressed.contains(&thumb_id) {
                    let setter = setter.clone();
                    let map = map_to_off.clone();
                    Some(Rc::new(move |pe| setter(map(pe.position.y))))
                } else {
                    None
                };
            let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> = Rc::new(move |_pe| {});
            hits.push(HitRegion {
                id: thumb_id,
                rect: crate::Rect {
                    x: track_x,
                    y: thumb_y,
                    w: thickness_px,
                    h: thumb_h,
                },
                on_click: None,
                on_scroll: None,
                focusable: false,
                on_pointer_down: Some(on_pd),
                on_pointer_move: on_pm,
                on_pointer_up: Some(on_pu),
                on_pointer_enter: None,
                on_pointer_leave: None,
                z_index: z + 1000.0,
                on_text_change: None,
                on_text_submit: None,
                tf_state_key: None,
            });
        }
    }

    fn push_scrollbar_h(
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        interactions: &Interactions,
        view_id: u64,
        vp: crate::Rect,
        content_w_px: f32,
        off_x_px: f32,
        z: f32,
        set_scroll_offset_xy: Option<Rc<dyn Fn(f32, f32)>>,
        keep_y: f32,
    ) {
        if content_w_px <= vp.w + 0.5 {
            return;
        }
        let thickness_px = dp_to_px(6.0);
        let margin_px = dp_to_px(2.0);
        let min_thumb_px = dp_to_px(24.0);
        let th = locals::theme();

        let track_x = vp.x + margin_px;
        let track_y = vp.y + vp.h - margin_px - thickness_px;
        let track_w = (vp.w - 2.0 * margin_px).max(0.0);

        let ratio = (vp.w / content_w_px).clamp(0.0, 1.0);
        let thumb_w = (track_w * ratio).clamp(min_thumb_px, track_w);
        let denom = (content_w_px - vp.w).max(1.0);
        let tpos = (off_x_px / denom).clamp(0.0, 1.0);
        let max_pos = (track_w - thumb_w).max(0.0);
        let thumb_x = track_x + tpos * max_pos;

        scene.nodes.push(SceneNode::Rect {
            rect: crate::Rect {
                x: track_x,
                y: track_y,
                w: track_w,
                h: thickness_px,
            },
            color: th.scrollbar_track,
            radius: thickness_px * 0.5,
        });
        scene.nodes.push(SceneNode::Rect {
            rect: crate::Rect {
                x: thumb_x,
                y: track_y,
                w: thumb_w,
                h: thickness_px,
            },
            color: th.scrollbar_thumb,
            radius: thickness_px * 0.5,
        });
        if let Some(set_xy) = set_scroll_offset_xy {
            let hthumb_id: u64 = view_id ^ 0x8000_0012;
            let map_to_off_x = Rc::new(move |px_pos: f32| -> f32 {
                let denom = (content_w_px - vp.w).max(1.0);
                let max_pos = (track_w - thumb_w).max(0.0);
                let pos = ((px_pos - track_x) - thumb_w * 0.5).clamp(0.0, max_pos);
                let t = if max_pos > 0.0 { pos / max_pos } else { 0.0 };
                t * denom
            });
            let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                let set_xy = set_xy.clone();
                let map = map_to_off_x.clone();
                Rc::new(move |pe| set_xy(map(pe.position.x), keep_y))
            };
            let on_pm: Option<Rc<dyn Fn(repose_core::input::PointerEvent)>> =
                if interactions.pressed.contains(&hthumb_id) {
                    let set_xy = set_xy.clone();
                    let map = map_to_off_x.clone();
                    Some(Rc::new(move |pe| set_xy(map(pe.position.x), keep_y)))
                } else {
                    None
                };
            let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> = Rc::new(move |_pe| {});
            hits.push(HitRegion {
                id: hthumb_id,
                rect: crate::Rect {
                    x: thumb_x,
                    y: track_y,
                    w: thumb_w,
                    h: thickness_px,
                },
                on_click: None,
                on_scroll: None,
                focusable: false,
                on_pointer_down: Some(on_pd),
                on_pointer_move: on_pm,
                on_pointer_up: Some(on_pu),
                on_pointer_enter: None,
                on_pointer_leave: None,
                z_index: z + 1000.0,
                on_text_change: None,
                on_text_submit: None,
                tf_state_key: None,
            });
        }
    }

    let mut scene = Scene {
        clear_color: locals::theme().background,
        nodes: vec![],
    };
    let mut hits: Vec<HitRegion> = vec![];
    let mut sems: Vec<SemNode> = vec![];

    fn walk(
        v: &View,
        t: &TaffyTree<NodeCtx>,
        nodes: &HashMap<ViewId, taffy::NodeId>,
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        sems: &mut Vec<SemNode>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        parent_offset_px: (f32, f32),
        alpha_accum: f32,
        text_cache: &StdHashMap<taffy::NodeId, TextLayout>,
        font_px: &dyn Fn(f32) -> f32,
    ) {
        let local = layout_of(nodes[&v.id], t);
        let rect = add_offset(local, parent_offset_px);

        // Convert padding from dp to px for content rect
        let content_rect = {
            if let Some(pv_dp) = v.modifier.padding_values {
                crate::Rect {
                    x: rect.x + dp_to_px(pv_dp.left),
                    y: rect.y + dp_to_px(pv_dp.top),
                    w: (rect.w - dp_to_px(pv_dp.left) - dp_to_px(pv_dp.right)).max(0.0),
                    h: (rect.h - dp_to_px(pv_dp.top) - dp_to_px(pv_dp.bottom)).max(0.0),
                }
            } else if let Some(p_dp) = v.modifier.padding {
                let p_px = dp_to_px(p_dp);
                crate::Rect {
                    x: rect.x + p_px,
                    y: rect.y + p_px,
                    w: (rect.w - 2.0 * p_px).max(0.0),
                    h: (rect.h - 2.0 * p_px).max(0.0),
                }
            } else {
                rect
            }
        };

        let pad_dx = content_rect.x - rect.x;
        let pad_dy = content_rect.y - rect.y;

        let base_px = (parent_offset_px.0 + local.x, parent_offset_px.1 + local.y);

        let is_hovered = interactions.hover == Some(v.id);
        let is_pressed = interactions.pressed.contains(&v.id);
        let is_focused = focused == Some(v.id);

        // Background/border
        if let Some(bg) = v.modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                color: mul_alpha(bg, alpha_accum),
                radius: v.modifier.clip_rounded.map(dp_to_px).unwrap_or(0.0),
            });
        }

        // Border
        if let Some(b) = &v.modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: mul_alpha(b.color, alpha_accum),
                width: dp_to_px(b.width),
                radius: dp_to_px(b.radius.max(v.modifier.clip_rounded.unwrap_or(0.0))),
            });
        }

        // Transform and alpha
        let this_alpha = v.modifier.alpha.unwrap_or(1.0);
        let alpha_accum = (alpha_accum * this_alpha).clamp(0.0, 1.0);

        if let Some(tf) = v.modifier.transform {
            scene.nodes.push(SceneNode::PushTransform { transform: tf });
        }

        // Custom painter (Canvas)
        if let Some(p) = &v.modifier.painter {
            (p)(scene, rect);
        }

        let has_pointer = v.modifier.on_pointer_down.is_some()
            || v.modifier.on_pointer_move.is_some()
            || v.modifier.on_pointer_up.is_some()
            || v.modifier.on_pointer_enter.is_some()
            || v.modifier.on_pointer_leave.is_some();

        if has_pointer || v.modifier.click {
            hits.push(HitRegion {
                id: v.id,
                rect,
                on_click: None,  // unless ViewKind provides one
                on_scroll: None, // provided by ScrollV case
                focusable: false,
                on_pointer_down: v.modifier.on_pointer_down.clone(),
                on_pointer_move: v.modifier.on_pointer_move.clone(),
                on_pointer_up: v.modifier.on_pointer_up.clone(),
                on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                z_index: v.modifier.z_index,
                on_text_change: None,
                on_text_submit: None,
                tf_state_key: None,
            });
        }

        match &v.kind {
            ViewKind::Text {
                text,
                color,
                font_size: font_dp,
                soft_wrap,
                max_lines,
                overflow,
            } => {
                let nid = nodes[&v.id];
                let tl = text_cache.get(&nid);

                let (size_px_val, line_h_px_val, mut lines): (f32, f32, Vec<String>) =
                    if let Some(tl) = tl {
                        (tl.size_px, tl.line_h_px, tl.lines.clone())
                    } else {
                        // Fallback
                        let sz_px = font_px(*font_dp);
                        (sz_px, sz_px * 1.3, vec![text.clone()])
                    };
                // Work within the content box
                let mut draw_box = content_rect;
                let max_w_px = draw_box.w.max(0.0);
                let max_h_px = draw_box.h.max(0.0);

                // Vertical centering for single line within content box
                if lines.len() == 1 {
                    let dy_px = (draw_box.h - line_h_px_val) * 0.5;
                    if dy_px.is_finite() {
                        draw_box.y += dy_px.max(0.0);
                        draw_box.h = line_h_px_val;
                    }
                }

                // For if height is constrained by rect.h and lines overflow visually,
                let max_visual_lines = if max_h_px > 0.5 {
                    (max_h_px / line_h_px_val).floor().max(1.0) as usize
                } else {
                    usize::MAX
                };

                if lines.len() > max_visual_lines {
                    lines.truncate(max_visual_lines);
                    if *overflow == TextOverflow::Ellipsis && max_w_px > 0.5 {
                        // Ellipsize the last visible line
                        if let Some(last) = lines.last_mut() {
                            *last = repose_text::ellipsize_line(last, size_px_val, max_w_px);
                        }
                    }
                }

                let approx_w_px = (text.len() as f32) * size_px_val * 0.6;
                let need_clip = match overflow {
                    TextOverflow::Visible | TextOverflow::Ellipsis => false,
                    TextOverflow::Clip => approx_w_px > max_w_px + 0.5,
                };

                if need_clip {
                    scene.nodes.push(SceneNode::PushClip {
                        rect: draw_box,
                        radius: 0.0,
                    });
                }

                for (i, ln) in lines.iter().enumerate() {
                    scene.nodes.push(SceneNode::Text {
                        rect: crate::Rect {
                            x: draw_box.x,
                            y: draw_box.y + i as f32 * line_h_px_val,
                            w: draw_box.w,
                            h: line_h_px_val,
                        },
                        text: ln.clone(),
                        color: mul_alpha(*color, alpha_accum),
                        size: size_px_val,
                    });
                }

                if need_clip {
                    scene.nodes.push(SceneNode::PopClip);
                }

                sems.push(SemNode {
                    id: v.id,
                    role: Role::Text,
                    label: Some(text.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
            }

            ViewKind::Button { text, on_click } => {
                // Default background if none provided
                if v.modifier.background.is_none() {
                    let th = locals::theme();
                    let base = if is_pressed {
                        th.button_bg_pressed
                    } else if is_hovered {
                        th.button_bg_hover
                    } else {
                        th.button_bg
                    };
                    scene.nodes.push(SceneNode::Rect {
                        rect,
                        color: mul_alpha(base, alpha_accum),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(6.0_f32 /* dp */)
                            .max(0.0),
                    });
                }
                // Label
                let label_px = font_px(16.0);
                let approx_w_px = (text.len() as f32) * label_px * 0.6;
                let tx = rect.x + (rect.w - approx_w_px).max(0.0) * 0.5;
                let ty = rect.y + (rect.h - label_px).max(0.0) * 0.5;
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: tx,
                        y: ty,
                        w: approx_w_px,
                        h: label_px,
                    },
                    text: text.clone(),
                    color: mul_alpha(Color::WHITE, alpha_accum),
                    size: label_px,
                });

                if v.modifier.click || on_click.is_some() {
                    hits.push(HitRegion {
                        id: v.id,
                        rect,
                        on_click: on_click.clone(),
                        on_scroll: None,
                        focusable: true,
                        on_pointer_down: v.modifier.on_pointer_down.clone(),
                        on_pointer_move: v.modifier.on_pointer_move.clone(),
                        on_pointer_up: v.modifier.on_pointer_up.clone(),
                        on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                        on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                        z_index: v.modifier.z_index,
                        on_text_change: None,
                        on_text_submit: None,
                        tf_state_key: None,
                    });
                }
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Button,
                    label: Some(text.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                // Focus ring
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }
            ViewKind::Image { handle, tint, fit } => {
                scene.nodes.push(SceneNode::Image {
                    rect,
                    handle: *handle,
                    tint: mul_alpha(*tint, alpha_accum),
                    fit: *fit,
                });
            }

            ViewKind::TextField {
                state_key,
                hint,
                on_change,
                on_submit,
                ..
            } => {
                // Persistent key for platform-managed state
                let tf_key = if *state_key != 0 { *state_key } else { v.id };

                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: None,
                    focusable: true,
                    on_pointer_down: None,
                    on_pointer_move: None,
                    on_pointer_up: None,
                    on_pointer_enter: None,
                    on_pointer_leave: None,
                    z_index: v.modifier.z_index,
                    on_text_change: on_change.clone(),
                    on_text_submit: on_submit.clone(),
                    tf_state_key: Some(tf_key),
                });

                // Inner content rect (padding)
                let pad_x_px = dp_to_px(TF_PADDING_X_DP);
                let inner = repose_core::Rect {
                    x: rect.x + pad_x_px,
                    y: rect.y + dp_to_px(8.0),
                    w: rect.w - 2.0 * pad_x_px,
                    h: rect.h - dp_to_px(16.0),
                };
                scene.nodes.push(SceneNode::PushClip {
                    rect: inner,
                    radius: 0.0,
                });
                // TextField focus ring
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }

                if let Some(state_rc) = textfield_states
                    .get(&tf_key)
                    .or_else(|| textfield_states.get(&v.id))
                // fallback for older platforms
                {
                    state_rc.borrow_mut().set_inner_width(inner.w);

                    let state = state_rc.borrow();
                    let text_val = &state.text;
                    let font_px_u32 = TF_FONT_DP as u32;
                    let m = measure_text(text_val, font_px_u32);

                    // Selection highlight
                    if state.selection.start != state.selection.end {
                        let i0 = byte_to_char_index(&m, state.selection.start);
                        let i1 = byte_to_char_index(&m, state.selection.end);
                        let sx_px =
                            m.positions.get(i0).copied().unwrap_or(0.0) - state.scroll_offset;
                        let ex_px =
                            m.positions.get(i1).copied().unwrap_or(sx_px) - state.scroll_offset;
                        let sel_x_px = inner.x + sx_px.max(0.0);
                        let sel_w_px = (ex_px - sx_px).max(0.0);
                        scene.nodes.push(SceneNode::Rect {
                            rect: repose_core::Rect {
                                x: sel_x_px,
                                y: inner.y,
                                w: sel_w_px,
                                h: inner.h,
                            },
                            color: mul_alpha(Color::from_hex("#3B7BFF55"), alpha_accum),
                            radius: 0.0,
                        });
                    }

                    // Composition underline
                    if let Some(range) = &state.composition {
                        if range.start < range.end && !text_val.is_empty() {
                            let i0 = byte_to_char_index(&m, range.start);
                            let i1 = byte_to_char_index(&m, range.end);
                            let sx_px =
                                m.positions.get(i0).copied().unwrap_or(0.0) - state.scroll_offset;
                            let ex_px =
                                m.positions.get(i1).copied().unwrap_or(sx_px) - state.scroll_offset;
                            let ux = inner.x + sx_px.max(0.0);
                            let uw = (ex_px - sx_px).max(0.0);
                            scene.nodes.push(SceneNode::Rect {
                                rect: repose_core::Rect {
                                    x: ux,
                                    y: inner.y + inner.h - dp_to_px(2.0),
                                    w: uw,
                                    h: dp_to_px(2.0),
                                },
                                color: mul_alpha(locals::theme().focus, alpha_accum),
                                radius: 0.0,
                            });
                        }
                    }

                    // Text (offset by scroll)
                    let text_color = if text_val.is_empty() {
                        mul_alpha(Color::from_hex("#666666"), alpha_accum)
                    } else {
                        mul_alpha(locals::theme().on_surface, alpha_accum)
                    };
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: inner.x - state.scroll_offset,
                            y: inner.y,
                            w: inner.w,
                            h: inner.h,
                        },
                        text: if text_val.is_empty() {
                            hint.clone()
                        } else {
                            text_val.clone()
                        },
                        color: text_color,
                        size: font_px(TF_FONT_DP),
                    });

                    // Caret (blink)
                    if state.selection.start == state.selection.end && state.caret_visible() {
                        let i = byte_to_char_index(&m, state.selection.end);
                        let cx_px =
                            m.positions.get(i).copied().unwrap_or(0.0) - state.scroll_offset;
                        let caret_x_px = inner.x + cx_px.max(0.0);
                        scene.nodes.push(SceneNode::Rect {
                            rect: repose_core::Rect {
                                x: caret_x_px,
                                y: inner.y,
                                w: dp_to_px(1.0),
                                h: inner.h,
                            },
                            color: mul_alpha(Color::WHITE, alpha_accum),
                            radius: 0.0,
                        });
                    }
                    // end inner clip
                    scene.nodes.push(SceneNode::PopClip);

                    sems.push(SemNode {
                        id: v.id,
                        role: Role::TextField,
                        label: Some(text_val.clone()),
                        rect,
                        focused: is_focused,
                        enabled: true,
                    });
                } else {
                    // No state yet: show hint only
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: inner.x,
                            y: inner.y,
                            w: inner.w,
                            h: inner.h,
                        },
                        text: hint.clone(),
                        color: mul_alpha(Color::from_hex("#666666"), alpha_accum),
                        size: font_px(TF_FONT_DP),
                    });
                    scene.nodes.push(SceneNode::PopClip);

                    sems.push(SemNode {
                        id: v.id,
                        role: Role::TextField,
                        label: Some(hint.clone()),
                        rect,
                        focused: is_focused,
                        enabled: true,
                    });
                }
            }
            ViewKind::ScrollV {
                on_scroll,
                set_viewport_height,
                set_content_height,
                get_scroll_offset,
                set_scroll_offset,
            } => {
                // Keep hit region as outer rect so scroll works even on padding
                hits.push(HitRegion {
                    id: v.id,
                    rect, // outer
                    on_click: None,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });

                // Use the inner content box (after padding) as the true viewport
                let vp = content_rect; // already computed above with padding converted to px

                if let Some(set_vh) = set_viewport_height {
                    set_vh(vp.h.max(0.0));
                }

                // True content height (use subtree extents per child)
                fn subtree_extents(node: taffy::NodeId, t: &TaffyTree<NodeCtx>) -> (f32, f32) {
                    let l = t.layout(node).unwrap();
                    let mut w = l.size.width;
                    let mut h = l.size.height;
                    if let Ok(children) = t.children(node) {
                        for &ch in children.iter() {
                            let cl = t.layout(ch).unwrap();
                            let (cw, chh) = subtree_extents(ch, t);
                            w = w.max(cl.location.x + cw);
                            h = h.max(cl.location.y + chh);
                        }
                    }
                    (w, h)
                }
                let mut content_h_px = 0.0f32;
                for c in &v.children {
                    let nid = nodes[&c.id];
                    let l = t.layout(nid).unwrap();
                    let (_cw, chh) = subtree_extents(nid, t);
                    content_h_px = content_h_px.max(l.location.y + chh);
                }
                if let Some(set_ch) = set_content_height {
                    set_ch(content_h_px);
                }

                // Clip to the inner viewport
                scene.nodes.push(SceneNode::PushClip {
                    rect: vp,
                    radius: 0.0, // inner clip; keep simple (outer border already drawn if any)
                });

                // Walk children
                let hit_start = hits.len();
                let scroll_offset_px = if let Some(get) = get_scroll_offset {
                    get()
                } else {
                    0.0
                };
                let child_offset_px = (base_px.0 + pad_dx, base_px.1 + pad_dy - scroll_offset_px);
                for c in &v.children {
                    walk(
                        c,
                        t,
                        nodes,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        text_cache,
                        font_px,
                    );
                }

                // Clip descendant hit regions to the viewport
                let mut i = hit_start;
                while i < hits.len() {
                    if let Some(r) = intersect(hits[i].rect, vp) {
                        hits[i].rect = r;
                        i += 1;
                    } else {
                        hits.remove(i);
                    }
                }

                // Scrollbar overlay
                push_scrollbar_v(
                    scene,
                    hits,
                    interactions,
                    v.id,
                    vp,
                    content_h_px,
                    scroll_offset_px,
                    v.modifier.z_index,
                    set_scroll_offset.clone(),
                );

                scene.nodes.push(SceneNode::PopClip);
                return;
            }
            ViewKind::ScrollXY {
                on_scroll,
                set_viewport_width,
                set_viewport_height,
                set_content_width,
                set_content_height,
                get_scroll_offset_xy,
                set_scroll_offset_xy,
            } => {
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });

                let vp = content_rect;

                if let Some(set_w) = set_viewport_width {
                    set_w(vp.w.max(0.0));
                }
                if let Some(set_h) = set_viewport_height {
                    set_h(vp.h.max(0.0));
                }

                fn subtree_extents(node: taffy::NodeId, t: &TaffyTree<NodeCtx>) -> (f32, f32) {
                    let l = t.layout(node).unwrap();
                    let mut w = l.size.width;
                    let mut h = l.size.height;
                    if let Ok(children) = t.children(node) {
                        for &ch in children.iter() {
                            let cl = t.layout(ch).unwrap();
                            let (cw, chh) = subtree_extents(ch, t);
                            w = w.max(cl.location.x + cw);
                            h = h.max(cl.location.y + chh);
                        }
                    }
                    (w, h)
                }
                let mut content_w_px = 0.0f32;
                let mut content_h_px = 0.0f32;
                for c in &v.children {
                    let nid = nodes[&c.id];
                    let l = t.layout(nid).unwrap();
                    let (cw, chh) = subtree_extents(nid, t);
                    content_w_px = content_w_px.max(l.location.x + cw);
                    content_h_px = content_h_px.max(l.location.y + chh);
                }
                if let Some(set_cw) = set_content_width {
                    set_cw(content_w_px);
                }
                if let Some(set_ch) = set_content_height {
                    set_ch(content_h_px);
                }

                scene.nodes.push(SceneNode::PushClip {
                    rect: vp,
                    radius: 0.0,
                });

                let hit_start = hits.len();
                let (ox_px, oy_px) = if let Some(get) = get_scroll_offset_xy {
                    get()
                } else {
                    (0.0, 0.0)
                };
                let child_offset_px = (base_px.0 + pad_dx - ox_px, base_px.1 + pad_dy - oy_px);
                for c in &v.children {
                    walk(
                        c,
                        t,
                        nodes,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        text_cache,
                        font_px,
                    );
                }
                // Clip descendant hits to viewport
                let mut i = hit_start;
                while i < hits.len() {
                    if let Some(r) = intersect(hits[i].rect, vp) {
                        hits[i].rect = r;
                        i += 1;
                    } else {
                        hits.remove(i);
                    }
                }

                let set_scroll_y: Option<Rc<dyn Fn(f32)>> =
                    set_scroll_offset_xy.clone().map(|set_xy| {
                        let ox = ox_px; // keep x, move only y
                        Rc::new(move |y| set_xy(ox, y)) as Rc<dyn Fn(f32)>
                    });

                // Scrollbars against inner viewport
                push_scrollbar_v(
                    scene,
                    hits,
                    interactions,
                    v.id,
                    vp,
                    content_h_px,
                    oy_px,
                    v.modifier.z_index,
                    set_scroll_y,
                );
                push_scrollbar_h(
                    scene,
                    hits,
                    interactions,
                    v.id,
                    vp,
                    content_w_px,
                    ox_px,
                    v.modifier.z_index,
                    set_scroll_offset_xy.clone(),
                    oy_px,
                );

                scene.nodes.push(SceneNode::PopClip);
                return;
            }
            ViewKind::Checkbox {
                checked,
                label,
                on_change,
            } => {
                let theme = locals::theme();
                // Box at left (20x20 centered vertically)
                let box_size_px = dp_to_px(18.0);
                let bx = rect.x;
                let by = rect.y + (rect.h - box_size_px) * 0.5;
                // box bg/border
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size_px,
                        h: box_size_px,
                    },
                    color: if *checked {
                        mul_alpha(theme.primary, alpha_accum)
                    } else {
                        mul_alpha(theme.surface, alpha_accum)
                    },
                    radius: dp_to_px(3.0),
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size_px,
                        h: box_size_px,
                    },
                    color: mul_alpha(theme.outline, alpha_accum),
                    width: dp_to_px(1.0),
                    radius: dp_to_px(3.0),
                });
                // checkmark
                if *checked {
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: bx + dp_to_px(3.0),
                            y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                            w: rect.w - (box_size_px + dp_to_px(8.0)),
                            h: font_px(16.0),
                        },
                        text: "✓".to_string(),
                        color: mul_alpha(theme.on_primary, alpha_accum),
                        size: font_px(16.0),
                    });
                }
                // label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: bx + box_size_px + dp_to_px(8.0),
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.w - (box_size_px + dp_to_px(8.0)),
                        h: font_px(16.0),
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                // Hit + semantics + focus ring
                let toggled = !*checked;
                let on_click = on_change.as_ref().map(|cb| {
                    let cb = cb.clone();
                    Rc::new(move || cb(toggled)) as Rc<dyn Fn()>
                });
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click,
                    on_scroll: None,
                    focusable: true,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Checkbox,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }

            ViewKind::RadioButton {
                selected,
                label,
                on_select,
            } => {
                let theme = locals::theme();
                let d_px = dp_to_px(18.0);
                let cx = rect.x;
                let cy = rect.y + (rect.h - d_px) * 0.5;

                // outer circle (rounded rect as circle)
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: cx,
                        y: cy,
                        w: d_px,
                        h: d_px,
                    },
                    color: mul_alpha(theme.outline, alpha_accum),
                    width: dp_to_px(1.5),
                    radius: d_px * 0.5,
                });
                // inner dot if selected
                if *selected {
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: cx + dp_to_px(4.0),
                            y: cy + dp_to_px(4.0),
                            w: d_px - dp_to_px(8.0),
                            h: d_px - dp_to_px(8.0),
                        },
                        color: mul_alpha(theme.primary, alpha_accum),
                        radius: (d_px - dp_to_px(8.0)) * 0.5,
                    });
                }
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: cx + d_px + dp_to_px(8.0),
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.w - (d_px + dp_to_px(8.0)),
                        h: font_px(16.0),
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: on_select.clone(),
                    on_scroll: None,
                    focusable: true,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });
                sems.push(SemNode {
                    id: v.id,
                    role: Role::RadioButton,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }

            ViewKind::Switch {
                checked,
                label,
                on_change,
            } => {
                let theme = locals::theme();
                // track 46x26, knob 22x22
                let track_w_px = dp_to_px(46.0);
                let track_h_px = dp_to_px(26.0);
                let tx = rect.x;
                let ty = rect.y + (rect.h - track_h_px) * 0.5;
                let knob_px = dp_to_px(22.0);
                let on_col = theme.primary;
                let off_col = Color::from_hex("#333333");

                // track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: tx,
                        y: ty,
                        w: track_w_px,
                        h: track_h_px,
                    },
                    color: if *checked {
                        mul_alpha(on_col, alpha_accum)
                    } else {
                        mul_alpha(off_col, alpha_accum)
                    },
                    radius: track_h_px * 0.5,
                });
                // knob position
                let kx = if *checked {
                    tx + track_w_px - knob_px - dp_to_px(2.0)
                } else {
                    tx + dp_to_px(2.0)
                };
                let ky = ty + (track_h_px - knob_px) * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx,
                        y: ky,
                        w: knob_px,
                        h: knob_px,
                    },
                    color: mul_alpha(Color::from_hex("#EEEEEE"), alpha_accum),
                    radius: knob_px * 0.5,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: kx,
                        y: ky,
                        w: knob_px,
                        h: knob_px,
                    },
                    color: mul_alpha(theme.outline, alpha_accum),
                    width: dp_to_px(1.0),
                    radius: knob_px * 0.5,
                });

                // label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: tx + track_w_px + dp_to_px(8.0),
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.w - (track_w_px + dp_to_px(8.0)),
                        h: font_px(16.0),
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                let toggled = !*checked;
                let on_click = on_change.as_ref().map(|cb| {
                    let cb = cb.clone();
                    Rc::new(move || cb(toggled)) as Rc<dyn Fn()>
                });
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click,
                    on_scroll: None,
                    focusable: true,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Switch,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }
            ViewKind::Slider {
                value,
                min,
                max,
                step,
                label,
                on_change,
            } => {
                let theme = locals::theme();
                // Layout: [track | label]
                let track_h_px = dp_to_px(4.0);
                let knob_d_px = dp_to_px(20.0);
                let gap_px = dp_to_px(8.0);
                let label_x = rect.x + rect.w * 0.6; // simple split: 60% track, 40% label
                let track_x = rect.x;
                let track_w_px = (label_x - track_x).max(dp_to_px(60.0));
                let cy = rect.y + rect.h * 0.5;

                // Track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h_px * 0.5,
                        w: track_w_px,
                        h: track_h_px,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h_px * 0.5,
                });

                // Knob position
                let t = clamp01(norm(*value, *min, *max));
                let kx = track_x + t * track_w_px;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx - knob_d_px * 0.5,
                        y: cy - knob_d_px * 0.5,
                        w: knob_d_px,
                        h: knob_d_px,
                    },
                    color: mul_alpha(theme.surface, alpha_accum),
                    radius: knob_d_px * 0.5,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: kx - knob_d_px * 0.5,
                        y: cy - knob_d_px * 0.5,
                        w: knob_d_px,
                        h: knob_d_px,
                    },
                    color: mul_alpha(theme.outline, alpha_accum),
                    width: dp_to_px(1.0),
                    radius: knob_d_px * 0.5,
                });

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: label_x + gap_px,
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.x + rect.w - (label_x + gap_px),
                        h: font_px(16.0),
                    },
                    text: format!("{}: {:.2}", label, *value),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                // Interactions
                let on_change_cb: Option<Rc<dyn Fn(f32)>> = on_change.as_ref().cloned();
                let minv = *min;
                let maxv = *max;
                let stepv = *step;

                // per-hit-region current value (wheel deltas accumulate within a frame)
                let current = Rc::new(RefCell::new(*value));

                // pointer mapping closure (in global coords, px)
                let update_at = {
                    let on_change_cb = on_change_cb.clone();
                    let current = current.clone();
                    Rc::new(move |px_pos: f32| {
                        let tt = clamp01((px_pos - track_x) / track_w_px);
                        let v = snap_step(denorm(tt, minv, maxv), stepv, minv, maxv);
                        *current.borrow_mut() = v;
                        if let Some(cb) = &on_change_cb {
                            cb(v);
                        }
                    })
                };

                // on_pointer_down: update once at press
                let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                    let f = update_at.clone();
                    Rc::new(move |pe| {
                        f(pe.position.x);
                    })
                };

                // on_pointer_move: platform only delivers here while captured
                let on_pm: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                    let f = update_at.clone();
                    Rc::new(move |pe| {
                        f(pe.position.x);
                    })
                };

                // on_pointer_up: no-op
                let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> = Rc::new(move |_pe| {});

                // Mouse wheel nudge: accumulate via 'current'
                let on_scroll = {
                    let on_change_cb = on_change_cb.clone();
                    let current = current.clone();
                    Rc::new(move |d: Vec2| -> Vec2 {
                        let base = *current.borrow();
                        let delta = stepv.unwrap_or((maxv - minv) * 0.01);
                        // wheel-up (negative y) increases
                        let dir = if d.y.is_sign_negative() { 1.0 } else { -1.0 };
                        let new_v = snap_step(base + dir * delta, stepv, minv, maxv);
                        *current.borrow_mut() = new_v;
                        if let Some(cb) = &on_change_cb {
                            cb(new_v);
                        }
                        Vec2 { x: d.x, y: 0.0 } // we consumed all y, pass x through
                    })
                };

                // Register move handler only while pressed so hover doesn't change value
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: Some(on_scroll),
                    focusable: true,
                    on_pointer_down: Some(on_pd),
                    on_pointer_move: if is_pressed { Some(on_pm) } else { None },
                    on_pointer_up: Some(on_pu),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });

                sems.push(SemNode {
                    id: v.id,
                    role: Role::Slider,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }
            ViewKind::RangeSlider {
                start,
                end,
                min,
                max,
                step,
                label,
                on_change,
            } => {
                let theme = locals::theme();
                let track_h_px = dp_to_px(4.0);
                let knob_d_px = dp_to_px(20.0);
                let gap_px = dp_to_px(8.0);
                let label_x = rect.x + rect.w * 0.6;
                let track_x = rect.x;
                let track_w_px = (label_x - track_x).max(dp_to_px(80.0));
                let cy = rect.y + rect.h * 0.5;

                // Track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h_px * 0.5,
                        w: track_w_px,
                        h: track_h_px,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h_px * 0.5,
                });

                // Positions
                let t0 = clamp01(norm(*start, *min, *max));
                let t1 = clamp01(norm(*end, *min, *max));
                let k0x = track_x + t0 * track_w_px;
                let k1x = track_x + t1 * track_w_px;

                // Range fill
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: k0x.min(k1x),
                        y: cy - track_h_px * 0.5,
                        w: (k1x - k0x).abs(),
                        h: track_h_px,
                    },
                    color: mul_alpha(theme.primary, alpha_accum),
                    radius: track_h_px * 0.5,
                });

                // Knobs
                for &kx in &[k0x, k1x] {
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: kx - knob_d_px * 0.5,
                            y: cy - knob_d_px * 0.5,
                            w: knob_d_px,
                            h: knob_d_px,
                        },
                        color: mul_alpha(theme.surface, alpha_accum),
                        radius: knob_d_px * 0.5,
                    });
                    scene.nodes.push(SceneNode::Border {
                        rect: repose_core::Rect {
                            x: kx - knob_d_px * 0.5,
                            y: cy - knob_d_px * 0.5,
                            w: knob_d_px,
                            h: knob_d_px,
                        },
                        color: mul_alpha(theme.outline, alpha_accum),
                        width: dp_to_px(1.0),
                        radius: knob_d_px * 0.5,
                    });
                }

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: label_x + gap_px,
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.x + rect.w - (label_x + gap_px),
                        h: font_px(16.0),
                    },
                    text: format!("{}: {:.2} – {:.2}", label, *start, *end),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                // Interaction
                let on_change_cb = on_change.as_ref().cloned();
                let minv = *min;
                let maxv = *max;
                let stepv = *step;
                let start_val = *start;
                let end_val = *end;

                // which thumb is active during drag: Some(0) or Some(1)
                let active = Rc::new(RefCell::new(None::<u8>));

                // update for current active thumb; does nothing if None
                let update = {
                    let active = active.clone();
                    let on_change_cb = on_change_cb.clone();
                    Rc::new(move |px_pos: f32| {
                        if let Some(thumb) = *active.borrow() {
                            let tt = clamp01((px_pos - track_x) / track_w_px);
                            let v = snap_step(denorm(tt, minv, maxv), stepv, minv, maxv);
                            match thumb {
                                0 => {
                                    let new_start = v.min(end_val).min(maxv).max(minv);
                                    if let Some(cb) = &on_change_cb {
                                        cb(new_start, end_val);
                                    }
                                }
                                _ => {
                                    let new_end = v.max(start_val).max(minv).min(maxv);
                                    if let Some(cb) = &on_change_cb {
                                        cb(start_val, new_end);
                                    }
                                }
                            }
                        }
                    })
                };

                // on_pointer_down: choose nearest thumb and update once
                let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                    let active = active.clone();
                    let update = update.clone();
                    // snapshot thumb positions for hit decision
                    let k0x0 = k0x;
                    let k1x0 = k1x;
                    Rc::new(move |pe| {
                        let px_pos = pe.position.x;
                        let d0 = (px_pos - k0x0).abs();
                        let d1 = (px_pos - k1x0).abs();
                        *active.borrow_mut() = Some(if d0 <= d1 { 0 } else { 1 });
                        update(px_pos);
                    })
                };

                // on_pointer_move: update only while a thumb is active
                let on_pm: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                    let active = active.clone();
                    let update = update.clone();
                    Rc::new(move |pe| {
                        if active.borrow().is_some() {
                            update(pe.position.x);
                        }
                    })
                };

                // on_pointer_up: clear active thumb
                let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                    let active = active.clone();
                    Rc::new(move |_pe| {
                        *active.borrow_mut() = None;
                    })
                };

                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: None,
                    focusable: true,
                    on_pointer_down: Some(on_pd),
                    on_pointer_move: Some(on_pm),
                    on_pointer_up: Some(on_pu),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                    on_text_change: None,
                    on_text_submit: None,
                    tf_state_key: None,
                });
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Slider,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(locals::theme().focus, alpha_accum),
                        width: dp_to_px(2.0),
                        radius: v
                            .modifier
                            .clip_rounded
                            .map(dp_to_px)
                            .unwrap_or(dp_to_px(6.0)),
                    });
                }
            }
            ViewKind::ProgressBar {
                value,
                min,
                max,
                label,
                circular: _,
            } => {
                let theme = locals::theme();
                let track_h_px = dp_to_px(6.0);
                let gap_px = dp_to_px(8.0);
                let label_w_split_px = rect.w * 0.6;
                let track_x = rect.x;
                let track_w_px = (label_w_split_px - track_x).max(dp_to_px(60.0));
                let cy = rect.y + rect.h * 0.5;

                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h_px * 0.5,
                        w: track_w_px,
                        h: track_h_px,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h_px * 0.5,
                });

                let t = clamp01(norm(*value, *min, *max));
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h_px * 0.5,
                        w: track_w_px * t,
                        h: track_h_px,
                    },
                    color: mul_alpha(theme.primary, alpha_accum),
                    radius: track_h_px * 0.5,
                });

                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: rect.x + label_w_split_px + gap_px,
                        y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                        w: rect.w - (label_w_split_px + gap_px),
                        h: font_px(16.0),
                    },
                    text: format!("{}: {:.0}%", label, t * 100.0),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: font_px(16.0),
                });

                sems.push(SemNode {
                    id: v.id,
                    role: Role::ProgressBar,
                    label: Some(label.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
            }

            _ => {}
        }

        // Recurse (no extra clip by default)
        for c in &v.children {
            walk(
                c,
                t,
                nodes,
                scene,
                hits,
                sems,
                textfield_states,
                interactions,
                focused,
                base_px,
                alpha_accum,
                text_cache,
                font_px,
            );
        }

        if v.modifier.transform.is_some() {
            scene.nodes.push(SceneNode::PopTransform);
        }
    }

    let font_px = |dp_font: f32| dp_to_px(dp_font) * locals::text_scale().0;

    // Start with zero offset
    walk(
        &root,
        &taffy,
        &nodes_map,
        &mut scene,
        &mut hits,
        &mut sems,
        textfield_states,
        interactions,
        focused,
        (0.0, 0.0),
        1.0,
        &text_cache,
        &font_px,
    );

    // Ensure visual order: low z_index first. Topmost will be found by iter().rev().
    hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));

    (scene, hits, sems)
}

/// Method styling
pub trait TextStyle {
    fn color(self, c: Color) -> View;
    fn size(self, px: f32) -> View;
    fn max_lines(self, n: usize) -> View;
    fn single_line(self) -> View;
    fn overflow_ellipsize(self) -> View;
    fn overflow_clip(self) -> View;
    fn overflow_visible(self) -> View;
}
impl TextStyle for View {
    fn color(mut self, c: Color) -> View {
        if let ViewKind::Text {
            color: text_color, ..
        } = &mut self.kind
        {
            *text_color = c;
        }
        self
    }
    fn size(mut self, dp_font: f32) -> View {
        if let ViewKind::Text {
            font_size: text_size_dp,
            ..
        } = &mut self.kind
        {
            *text_size_dp = dp_font;
        }
        self
    }
    fn max_lines(mut self, n: usize) -> View {
        if let ViewKind::Text {
            max_lines,
            soft_wrap,
            ..
        } = &mut self.kind
        {
            *max_lines = Some(n);
            *soft_wrap = true;
        }
        self
    }
    fn single_line(mut self) -> View {
        if let ViewKind::Text {
            soft_wrap,
            max_lines,
            ..
        } = &mut self.kind
        {
            *soft_wrap = false;
            *max_lines = Some(1);
        }
        self
    }
    fn overflow_ellipsize(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Ellipsis;
        }
        self
    }
    fn overflow_clip(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Clip;
        }
        self
    }
    fn overflow_visible(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Visible;
        }
        self
    }
}
