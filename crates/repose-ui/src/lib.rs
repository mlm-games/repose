//! Widgets, layout (Taffy), painting into a platform-agnostic Scene, and text fields.
//!
//! repose-render-wgpu is responsible for GPU interaction

#![allow(non_snake_case)]
pub mod anim;
pub mod anim_ext;
pub mod canvas;
pub mod gestures;
pub mod lazy;
pub mod material3;
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

use crate::textfield::{TF_FONT_PX, TF_PADDING_X, byte_to_char_index, measure_text, positions_for};
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
            font_size: 16.0,
            soft_wrap: false,
            max_lines: None,
            overflow: TextOverflow::Visible,
        },
    )
}

pub fn Spacer() -> View {
    Box(Modifier::new().flex_grow(1.0))
}

pub fn Grid(columns: usize, modifier: Modifier, children: Vec<View>) -> View {
    Column(modifier.grid(columns, 0.0, 0.0)).with_children(children)
}

#[allow(non_snake_case)]
pub fn TextColor(mut v: View, color: Color) -> View {
    if let ViewKind::Text {
        color: text_color, ..
    } = &mut v.kind
    {
        *text_color = color;
    }
    v
}

#[allow(non_snake_case)]
pub fn TextSize(mut v: View, size: f32) -> View {
    if let ViewKind::Text {
        font_size: text_size,
        ..
    } = &mut v.kind
    {
        *text_size = size;
    }
    v
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
    size: (u32, u32),
    textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
    interactions: &Interactions,
    focused: Option<u64>,
) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
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
            font_px: f32,
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
        line_h: f32,
    }
    use std::collections::HashMap as StdHashMap;
    let mut text_cache: StdHashMap<taffy::NodeId, TextLayout> = StdHashMap::new();

    fn style_from_modifier(m: &Modifier, kind: &ViewKind) -> Style {
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
            ViewKind::Column
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
        if let Some(b) = m.flex_basis {
            s.flex_basis = length(b.max(0.0));
        }

        // Align self
        if let Some(a) = m.align_self {
            s.align_self = Some(a);
        }

        // Absolute positioning
        if let Some(crate::modifier::PositionType::Absolute) = m.position_type {
            s.position = Position::Absolute;
            s.inset = taffy::geometry::Rect {
                left: m.offset_left.map(length).unwrap_or_else(auto),
                right: m.offset_right.map(length).unwrap_or_else(auto),
                top: m.offset_top.map(length).unwrap_or_else(auto),
                bottom: m.offset_bottom.map(length).unwrap_or_else(auto),
            };
        }

        // Grid config
        if let Some(cfg) = &m.grid {
            s.display = Display::Grid;
            s.grid_template_columns = (0..cfg.columns.max(1))
                .map(|_| GridTemplateComponent::Single(flex(1.0)))
                .collect();
            s.gap = Size {
                width: length(cfg.column_gap),
                height: length(cfg.row_gap),
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
        if let Some(pv) = m.padding_values {
            s.padding = taffy::geometry::Rect {
                left: length(pv.left),
                right: length(pv.right),
                top: length(pv.top),
                bottom: length(pv.bottom),
            };
        } else if let Some(p) = m.padding {
            let v = length(p);
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
        if let Some(sz) = m.size {
            if sz.width.is_finite() {
                s.size.width = length(sz.width.max(0.0));
                width_set = true;
            }
            if sz.height.is_finite() {
                s.size.height = length(sz.height.max(0.0));
                height_set = true;
            }
        }
        if let Some(w) = m.width {
            s.size.width = length(w.max(0.0));
            width_set = true;
        }
        if let Some(h) = m.height {
            s.size.height = length(h.max(0.0));
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
        if let Some(v) = m.min_width {
            s.min_size.width = length(v.max(0.0));
        }
        if let Some(v) = m.min_height {
            s.min_size.height = length(v.max(0.0));
        }
        if let Some(v) = m.max_width {
            s.max_size.width = length(v.max(0.0));
        }
        if let Some(v) = m.max_height {
            s.max_size.height = length(v.max(0.0));
        }

        s
    }

    fn build_node(
        v: &View,
        t: &mut TaffyTree<NodeCtx>,
        nodes_map: &mut HashMap<ViewId, taffy::NodeId>,
    ) -> taffy::NodeId {
        let mut style = style_from_modifier(&v.modifier, &v.kind);

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
                font_size,
                soft_wrap,
                max_lines,
                overflow,
                ..
            } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Text {
                        text: text.clone(),
                        font_px: *font_size,
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
        rs.size.width = length(size.0 as f32);
        rs.size.height = length(size.1 as f32);
        taffy.set_style(root_node, rs).unwrap();
    }

    let available = taffy::geometry::Size {
        width: AvailableSpace::Definite(size.0 as f32),
        height: AvailableSpace::Definite(size.1 as f32),
    };

    // Measure function for intrinsic content
    taffy
        .compute_layout_with_measure(root_node, available, |known, avail, node, ctx, _style| {
            match ctx {
                Some(NodeCtx::Text {
                    text,
                    font_px,
                    soft_wrap,
                    max_lines,
                    overflow,
                }) => {
                    // Apply text scale in measure so paint matches exactly
                    let scale = locals::text_scale().0;
                    let size_px = *font_px * scale;
                    let line_h = size_px * 1.3;

                    // Content-hugging width by default (unless caller set known.width).
                    let approx_w = text.len() as f32 * *font_px * 0.6;
                    let measured_w = known.width.unwrap_or(approx_w);

                    // Content-hugging width by default (unless caller set known.width).
                    let wrap_w = if *soft_wrap {
                        match avail.width {
                            AvailableSpace::Definite(w) => w,
                            _ => measured_w,
                        }
                    } else {
                        measured_w
                    };

                    // Produce final lines once and cache
                    let lines_vec: Vec<String> = if *soft_wrap {
                        let (ls, _trunc) =
                            repose_text::wrap_lines(text, size_px, wrap_w, *max_lines, true);
                        ls
                    } else {
                        match overflow {
                            TextOverflow::Ellipsis => {
                                vec![repose_text::ellipsize_line(text, size_px, wrap_w)]
                            }
                            _ => vec![text.clone()],
                        }
                    };
                    text_cache.insert(
                        node,
                        TextLayout {
                            lines: lines_vec.clone(),
                            size_px,
                            line_h,
                        },
                    );
                    let n_lines = lines_vec.len().max(1);

                    taffy::geometry::Size {
                        width: measured_w,
                        height: line_h * n_lines as f32,
                    }
                }
                Some(NodeCtx::Button { label }) => taffy::geometry::Size {
                    width: (label.len() as f32 * 16.0 * 0.6) + 24.0,
                    height: 36.0,
                },
                Some(NodeCtx::TextField) => {
                    let w = known.width.unwrap_or(220.0);
                    taffy::geometry::Size {
                        width: w,
                        height: 36.0,
                    }
                }
                Some(NodeCtx::Checkbox { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = 24.0 + 8.0 + label_w; // box + gap + text estimate
                    taffy::geometry::Size {
                        width: known.width.unwrap_or(w),
                        height: 24.0,
                    }
                }
                Some(NodeCtx::Radio { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = 24.0 + 8.0 + label_w; // circle + gap + text estimate
                    taffy::geometry::Size {
                        width: known.width.unwrap_or(w),
                        height: 24.0,
                    }
                }
                Some(NodeCtx::Switch { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = 46.0 + 8.0 + label_w; // track + gap + text
                    taffy::geometry::Size {
                        width: known.width.unwrap_or(w),
                        height: 28.0,
                    }
                }
                Some(NodeCtx::Slider { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = (known.width).unwrap_or(200.0f32.max(46.0 + 8.0 + label_w));
                    taffy::geometry::Size {
                        width: w,
                        height: 28.0,
                    }
                }
                Some(NodeCtx::Range { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = (known.width).unwrap_or(220.0f32.max(46.0 + 8.0 + label_w));
                    taffy::geometry::Size {
                        width: w,
                        height: 28.0,
                    }
                }
                Some(NodeCtx::Progress { label }) => {
                    let label_w = (label.len() as f32) * 16.0 * 0.6;
                    let w = (known.width).unwrap_or(200.0f32.max(100.0 + 8.0 + label_w));
                    taffy::geometry::Size {
                        width: w,
                        height: 12.0 + 8.0,
                    } // track + small padding
                }
                Some(NodeCtx::ScrollContainer) => {
                    taffy::geometry::Size {
                        width: known.width.unwrap_or_else(|| {
                            match avail.width {
                                AvailableSpace::Definite(w) => w,
                                _ => 300.0, // Fallback width
                            }
                        }),
                        height: known.height.unwrap_or_else(|| {
                            match avail.height {
                                AvailableSpace::Definite(h) => h,
                                _ => 600.0, // Fallback height
                            }
                        }),
                    }
                }
                Some(NodeCtx::Container) | None => taffy::geometry::Size::ZERO,
            }
        })
        .unwrap();

    // eprintln!(
    //     "win {:?}x{:?} root {:?}",
    //     size.0,
    //     size.1,
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
        parent_offset: (f32, f32),
        alpha_accum: f32,
        text_cache: &StdHashMap<taffy::NodeId, TextLayout>,
    ) {
        let local = layout_of(nodes[&v.id], t);
        let rect = add_offset(local, parent_offset);

        let content_rect = {
            // padding_values beats uniform padding
            if let Some(pv) = v.modifier.padding_values {
                crate::Rect {
                    x: rect.x + pv.left,
                    y: rect.y + pv.top,
                    w: (rect.w - pv.left - pv.right).max(0.0),
                    h: (rect.h - pv.top - pv.bottom).max(0.0),
                }
            } else if let Some(p) = v.modifier.padding {
                crate::Rect {
                    x: rect.x + p,
                    y: rect.y + p,
                    w: (rect.w - 2.0 * p).max(0.0),
                    h: (rect.h - 2.0 * p).max(0.0),
                }
            } else {
                rect
            }
        };

        let base = (parent_offset.0 + local.x, parent_offset.1 + local.y);

        let is_hovered = interactions.hover == Some(v.id);
        let is_pressed = interactions.pressed.contains(&v.id);
        let is_focused = focused == Some(v.id);

        // Background/border
        if let Some(bg) = v.modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                color: mul_alpha(bg, alpha_accum),
                radius: v.modifier.clip_rounded.unwrap_or(0.0),
            });
        }

        // Border
        if let Some(b) = &v.modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: mul_alpha(b.color, alpha_accum),
                width: b.width,
                radius: b.radius.max(v.modifier.clip_rounded.unwrap_or(0.0)),
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
            });
        }

        match &v.kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                soft_wrap,
                max_lines,
                overflow,
            } => {
                let nid = nodes[&v.id];
                let tl = text_cache.get(&nid);

                let (size, line_h, mut lines): (f32, f32, Vec<String>) = if let Some(tl) = tl {
                    (tl.size_px, tl.line_h, tl.lines.clone())
                } else {
                    // Fallback (shouldn’t happen; cache is built in measure)
                    let sz = *font_size * locals::text_scale().0;
                    (sz, sz * 1.3, vec![text.clone()])
                };
                // Work within the content box (padding removed)
                let mut draw_box = content_rect;
                let max_w = draw_box.w.max(0.0);
                let max_h = draw_box.h.max(0.0);

                // Vertical centering for single line within content box
                if lines.len() == 1 {
                    let dy = (draw_box.h - size) * 0.5;
                    if dy.is_finite() {
                        draw_box.y += dy.max(0.0);
                        draw_box.h = size;
                    }
                }

                // For if height is constrained by rect.h and lines overflow visually,
                let max_visual_lines = if max_h > 0.5 {
                    (max_h / line_h).floor().max(1.0) as usize
                } else {
                    usize::MAX
                };

                if lines.len() > max_visual_lines {
                    lines.truncate(max_visual_lines);
                    if *overflow == TextOverflow::Ellipsis && max_w > 0.5 {
                        // Ellipsize the last visible line
                        if let Some(last) = lines.last_mut() {
                            *last = repose_text::ellipsize_line(last, size, max_w);
                        }
                    }
                }

                let need_clip = match overflow {
                    TextOverflow::Visible => false,
                    TextOverflow::Clip | TextOverflow::Ellipsis => true,
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
                            y: draw_box.y + i as f32 * line_h,
                            w: draw_box.w,
                            h: line_h,
                        },
                        text: ln.clone(),
                        color: mul_alpha(*color, alpha_accum),
                        size: size,
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
                    let base = if is_pressed {
                        Color::from_hex("#1f7556")
                    } else if is_hovered {
                        Color::from_hex("#2a8f6a")
                    } else {
                        Color::from_hex("#34af82")
                    };
                    scene.nodes.push(SceneNode::Rect {
                        rect,
                        color: mul_alpha(base, alpha_accum),
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
                // Label
                let px = 16.0;
                let approx_w = (text.len() as f32) * px * 0.6;
                let tx = rect.x + (rect.w - approx_w).max(0.0) * 0.5;
                let ty = rect.y + (rect.h - px).max(0.0) * 0.5;
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: tx,
                        y: ty,
                        w: approx_w,
                        h: px,
                    },
                    text: text.clone(),
                    color: mul_alpha(Color::WHITE, alpha_accum),
                    size: px,
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
            }

            ViewKind::TextField {
                hint,
                on_change,
                on_submit,
                ..
            } => {
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
                });

                // Inner content rect (padding)
                let inner = repose_core::Rect {
                    x: rect.x + TF_PADDING_X,
                    y: rect.y + 8.0,
                    w: rect.w - 2.0 * TF_PADDING_X,
                    h: rect.h - 16.0,
                };
                scene.nodes.push(SceneNode::PushClip {
                    rect: inner,
                    radius: 0.0,
                });
                // TextField focus ring
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
                if let Some(state_rc) = textfield_states.get(&v.id) {
                    state_rc.borrow_mut().set_inner_width(inner.w);

                    let state = state_rc.borrow();
                    let text = &state.text;
                    let px = TF_FONT_PX as u32;
                    let m = measure_text(text, px);

                    // Selection highlight
                    if state.selection.start != state.selection.end {
                        let i0 = byte_to_char_index(&m, state.selection.start);
                        let i1 = byte_to_char_index(&m, state.selection.end);
                        let sx = m.positions.get(i0).copied().unwrap_or(0.0) - state.scroll_offset;
                        let ex = m.positions.get(i1).copied().unwrap_or(sx) - state.scroll_offset;
                        let sel_x = inner.x + sx.max(0.0);
                        let sel_w = (ex - sx).max(0.0);
                        scene.nodes.push(SceneNode::Rect {
                            rect: repose_core::Rect {
                                x: sel_x,
                                y: inner.y,
                                w: sel_w,
                                h: inner.h,
                            },
                            color: mul_alpha(Color::from_hex("#3B7BFF55"), alpha_accum),
                            radius: 0.0,
                        });
                    }

                    // Composition underline
                    if let Some(range) = &state.composition {
                        if range.start < range.end && !text.is_empty() {
                            let i0 = byte_to_char_index(&m, range.start);
                            let i1 = byte_to_char_index(&m, range.end);
                            let sx =
                                m.positions.get(i0).copied().unwrap_or(0.0) - state.scroll_offset;
                            let ex =
                                m.positions.get(i1).copied().unwrap_or(sx) - state.scroll_offset;
                            let ux = inner.x + sx.max(0.0);
                            let uw = (ex - sx).max(0.0);
                            scene.nodes.push(SceneNode::Rect {
                                rect: repose_core::Rect {
                                    x: ux,
                                    y: inner.y + inner.h - 2.0,
                                    w: uw,
                                    h: 2.0,
                                },
                                color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                                radius: 0.0,
                            });
                        }
                    }

                    // Text (offset by scroll)
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: inner.x - state.scroll_offset,
                            y: inner.y,
                            w: inner.w,
                            h: inner.h,
                        },
                        text: if text.is_empty() {
                            hint.clone()
                        } else {
                            text.clone()
                        },
                        color: if text.is_empty() {
                            mul_alpha(Color::from_hex("#666666"), alpha_accum)
                        } else {
                            mul_alpha(Color::from_hex("#CCCCCC"), alpha_accum)
                        },
                        size: TF_FONT_PX,
                    });

                    // Caret (blink)
                    if state.selection.start == state.selection.end && state.caret_visible() {
                        let i = byte_to_char_index(&m, state.selection.end);
                        let cx = m.positions.get(i).copied().unwrap_or(0.0) - state.scroll_offset;
                        let caret_x = inner.x + cx.max(0.0);
                        scene.nodes.push(SceneNode::Rect {
                            rect: repose_core::Rect {
                                x: caret_x,
                                y: inner.y,
                                w: 1.0,
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
                        label: Some(text.clone()),
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
                        size: TF_FONT_PX,
                    });
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
                log::debug!("ScrollV: registering hit region at rect {:?}", rect);

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
                });

                if let Some(set_vh) = set_viewport_height {
                    set_vh(rect.h);
                }

                // Compute content height from children (local coords)
                let mut content_h = 0.0f32;
                for c in &v.children {
                    let nid = nodes[&c.id];
                    let l = t.layout(nid).unwrap();
                    content_h = content_h.max(l.location.y + l.size.height);
                }
                if let Some(set_ch) = set_content_height {
                    set_ch(content_h);
                }

                scene.nodes.push(SceneNode::PushClip {
                    rect,
                    radius: v.modifier.clip_rounded.unwrap_or(0.0),
                });

                let scroll_offset = if let Some(get) = get_scroll_offset {
                    let offset = get();
                    log::debug!("ScrollV walk: applying scroll offset = {}", offset);
                    offset
                } else {
                    0.0
                };

                // Translate children by -scroll_offset in Y
                let child_offset = (base.0, base.1 - scroll_offset);
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
                        child_offset,
                        alpha_accum,
                        text_cache,
                    );
                }

                // Scrollbar overlay (vertical)
                if content_h > rect.h + 0.5 {
                    let thickness = 6.0f32;
                    let margin = 2.0f32;
                    let min_thumb = 24.0f32;

                    // Track along right edge inside the viewport
                    let track_x = rect.x + rect.w - margin - thickness;
                    let track_y = rect.y + margin;
                    let track_h = (rect.h - 2.0 * margin).max(0.0);

                    // Thumb length proportional to viewport/content
                    let ratio = (rect.h / content_h).clamp(0.0, 1.0);
                    let mut thumb_h = (track_h * ratio).clamp(min_thumb, track_h);

                    // Position: 0..1 along track
                    let denom = (content_h - rect.h).max(1.0);
                    let tpos = (scroll_offset / denom).clamp(0.0, 1.0);
                    let max_pos = (track_h - thumb_h).max(0.0);
                    let thumb_y = track_y + tpos * max_pos;

                    // Colors from theme, with lowered alpha
                    let th = locals::theme();
                    let mut track_col = th.on_surface;
                    track_col.3 = 32; // ~12% alpha
                    let mut thumb_col = th.on_surface;
                    thumb_col.3 = 140; // ~55% alpha

                    scene.nodes.push(SceneNode::Rect {
                        rect: crate::Rect {
                            x: track_x,
                            y: track_y,
                            w: thickness,
                            h: track_h,
                        },
                        color: track_col,
                        radius: thickness * 0.5,
                    });

                    // Thumb
                    scene.nodes.push(SceneNode::Rect {
                        rect: crate::Rect {
                            x: track_x,
                            y: thumb_y,
                            w: thickness,
                            h: thumb_h,
                        },
                        color: thumb_col,
                        radius: thickness * 0.5,
                    });

                    if let Some(setter) = set_scroll_offset.clone() {
                        let thumb_id: u64 = v.id ^ 0x8000_0001;

                        // Map pointer y -> scroll offset (center thumb on pointer)
                        let map_to_off = Rc::new(move |py: f32| -> f32 {
                            let denom = (content_h - rect.h).max(1.0);
                            let max_pos = (track_h - thumb_h).max(0.0);
                            let pos = ((py - track_y) - thumb_h * 0.5).clamp(0.0, max_pos);
                            let t = if max_pos > 0.0 { pos / max_pos } else { 0.0 };
                            t * denom
                        });

                        // Handlers
                        let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                            let setter = setter.clone();
                            let map = map_to_off.clone();
                            Rc::new(move |pe| {
                                setter(map(pe.position.y)); // center-on-press
                            })
                        };

                        // Only install move while pressed to avoid hover-driven updates
                        let is_pressed = interactions.pressed.contains(&thumb_id);
                        let on_pm: Option<Rc<dyn Fn(repose_core::input::PointerEvent)>> =
                            if is_pressed {
                                let setter = setter.clone();
                                let map = map_to_off.clone();
                                Some(Rc::new(move |pe| setter(map(pe.position.y))))
                            } else {
                                None
                            };

                        let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> =
                            Rc::new(move |_pe| {});

                        hits.push(HitRegion {
                            id: thumb_id,
                            rect: crate::Rect {
                                x: track_x,
                                y: thumb_y,
                                w: thickness,
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
                            z_index: v.modifier.z_index + 1000.0,
                            on_text_change: None,
                            on_text_submit: None,
                        });
                    }
                }

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
                });

                if let Some(set_w) = set_viewport_width {
                    set_w(rect.w);
                }
                if let Some(set_h) = set_viewport_height {
                    set_h(rect.h);
                }

                let mut content_w = 0.0f32;
                let mut content_h = 0.0f32;
                for c in &v.children {
                    let nid = nodes[&c.id];
                    let l = t.layout(nid).unwrap();
                    content_w = content_w.max(l.location.x + l.size.width);
                    content_h = content_h.max(l.location.y + l.size.height);
                }
                if let Some(set_cw) = set_content_width {
                    set_cw(content_w);
                }
                if let Some(set_ch) = set_content_height {
                    set_ch(content_h);
                }

                // Clip to viewport
                scene.nodes.push(SceneNode::PushClip {
                    rect,
                    radius: v.modifier.clip_rounded.unwrap_or(0.0),
                });

                // Offsets
                let (ox, oy) = if let Some(get) = get_scroll_offset_xy {
                    get()
                } else {
                    (0.0, 0.0)
                };

                // Children translated by (-ox, -oy)
                let child_offset = (base.0 - ox, base.1 - oy);
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
                        child_offset,
                        alpha_accum,
                        text_cache,
                    );
                }

                // Scrollbars overlay (XY)
                let show_v = content_h > rect.h + 0.5;
                let show_h = content_w > rect.w + 0.5;

                if show_v || show_h {
                    let thickness = 6.0f32;
                    let margin = 2.0f32;
                    let min_thumb = 24.0f32;

                    let thm = locals::theme();
                    let mut track_col = thm.on_surface;
                    track_col.3 = 32;
                    let mut thumb_col = thm.on_surface;
                    thumb_col.3 = 140;

                    // Vertical
                    if show_v {
                        let track_x = rect.x + rect.w - margin - thickness;
                        let track_y = rect.y + margin;
                        let mut track_h = (rect.h - 2.0 * margin).max(0.0);
                        if show_h {
                            track_h = (track_h - (thickness + margin)).max(0.0);
                        }

                        let ratio = (rect.h / content_h).clamp(0.0, 1.0);
                        let mut thumb_h = (track_h * ratio).clamp(min_thumb, track_h);
                        let denom = (content_h - rect.h).max(1.0);
                        let tpos = (oy / denom).clamp(0.0, 1.0);
                        let max_pos = (track_h - thumb_h).max(0.0);
                        let thumb_y = track_y + tpos * max_pos;

                        scene.nodes.push(SceneNode::Rect {
                            rect: crate::Rect {
                                x: track_x,
                                y: track_y,
                                w: thickness,
                                h: track_h,
                            },
                            color: track_col,
                            radius: thickness * 0.5,
                        });
                        scene.nodes.push(SceneNode::Rect {
                            rect: crate::Rect {
                                x: track_x,
                                y: thumb_y,
                                w: thickness,
                                h: thumb_h,
                            },
                            color: thumb_col,
                            radius: thickness * 0.5,
                        });

                        if let Some(set_xy) = set_scroll_offset_xy.clone() {
                            let vthumb_id: u64 = v.id ^ 0x8000_0011;

                            let map_to_off_y = Rc::new(move |py: f32| -> f32 {
                                let denom = (content_h - rect.h).max(1.0);
                                let max_pos = (track_h - thumb_h).max(0.0);
                                let pos = ((py - track_y) - thumb_h * 0.5).clamp(0.0, max_pos);
                                let t = if max_pos > 0.0 { pos / max_pos } else { 0.0 };
                                t * denom
                            });

                            let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                                let set_xy = set_xy.clone();
                                let map = map_to_off_y.clone();
                                Rc::new(move |pe| set_xy(ox, map(pe.position.y))) // center-on-press
                            };

                            let is_pressed = interactions.pressed.contains(&vthumb_id);
                            let on_pm: Option<Rc<dyn Fn(repose_core::input::PointerEvent)>> =
                                if is_pressed {
                                    let set_xy = set_xy.clone();
                                    let map = map_to_off_y.clone();
                                    Some(Rc::new(move |pe| set_xy(ox, map(pe.position.y))))
                                } else {
                                    None
                                };

                            let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> =
                                Rc::new(move |_pe| {});

                            hits.push(HitRegion {
                                id: vthumb_id,
                                rect: crate::Rect {
                                    x: track_x,
                                    y: thumb_y,
                                    w: thickness,
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
                                z_index: v.modifier.z_index + 1000.0,
                                on_text_change: None,
                                on_text_submit: None,
                            });
                        }
                    }

                    // Horizontal
                    if show_h {
                        let track_x = rect.x + margin;
                        let track_y = rect.y + rect.h - margin - thickness;
                        let mut track_w = (rect.w - 2.0 * margin).max(0.0);
                        if show_v {
                            track_w = (track_w - (thickness + margin)).max(0.0);
                        }

                        let ratio = (rect.w / content_w).clamp(0.0, 1.0);
                        let mut thumb_w = (track_w * ratio).clamp(min_thumb, track_w);
                        let denom = (content_w - rect.w).max(1.0);
                        let tpos = (ox / denom).clamp(0.0, 1.0);
                        let max_pos = (track_w - thumb_w).max(0.0);
                        let thumb_x = track_x + tpos * max_pos;

                        scene.nodes.push(SceneNode::Rect {
                            rect: crate::Rect {
                                x: track_x,
                                y: track_y,
                                w: track_w,
                                h: thickness,
                            },
                            color: track_col,
                            radius: thickness * 0.5,
                        });
                        scene.nodes.push(SceneNode::Rect {
                            rect: crate::Rect {
                                x: thumb_x,
                                y: track_y,
                                w: thumb_w,
                                h: thickness,
                            },
                            color: thumb_col,
                            radius: thickness * 0.5,
                        });

                        if let Some(set_xy) = set_scroll_offset_xy.clone() {
                            let hthumb_id: u64 = v.id ^ 0x8000_0012;

                            let map_to_off_x = Rc::new(move |px: f32| -> f32 {
                                let denom = (content_w - rect.w).max(1.0);
                                let max_pos = (track_w - thumb_w).max(0.0);
                                let pos = ((px - track_x) - thumb_w * 0.5).clamp(0.0, max_pos);
                                let t = if max_pos > 0.0 { pos / max_pos } else { 0.0 };
                                t * denom
                            });

                            let on_pd: Rc<dyn Fn(repose_core::input::PointerEvent)> = {
                                let set_xy = set_xy.clone();
                                let map = map_to_off_x.clone();
                                Rc::new(move |pe| set_xy(map(pe.position.x), oy)) // center-on-press
                            };

                            let is_pressed = interactions.pressed.contains(&hthumb_id);
                            let on_pm: Option<Rc<dyn Fn(repose_core::input::PointerEvent)>> =
                                if is_pressed {
                                    let set_xy = set_xy.clone();
                                    let map = map_to_off_x.clone();
                                    Some(Rc::new(move |pe| set_xy(map(pe.position.x), oy)))
                                } else {
                                    None
                                };

                            let on_pu: Rc<dyn Fn(repose_core::input::PointerEvent)> =
                                Rc::new(move |_pe| {});

                            hits.push(HitRegion {
                                id: hthumb_id,
                                rect: crate::Rect {
                                    x: thumb_x,
                                    y: track_y,
                                    w: thumb_w,
                                    h: thickness,
                                },
                                on_click: None,
                                on_scroll: None,
                                focusable: false,
                                on_pointer_down: Some(on_pd),
                                on_pointer_move: on_pm,
                                on_pointer_up: Some(on_pu),
                                on_pointer_enter: None,
                                on_pointer_leave: None,
                                z_index: v.modifier.z_index + 1000.0,
                                on_text_change: None,
                                on_text_submit: None,
                            });
                        }
                    }
                }

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
                let box_size = 18.0f32;
                let bx = rect.x;
                let by = rect.y + (rect.h - box_size) * 0.5;
                // box bg/border
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size,
                        h: box_size,
                    },
                    color: if *checked {
                        mul_alpha(theme.primary, alpha_accum)
                    } else {
                        mul_alpha(theme.surface, alpha_accum)
                    },
                    radius: 3.0,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size,
                        h: box_size,
                    },
                    color: mul_alpha(Color::from_hex("#555555"), alpha_accum),
                    width: 1.0,
                    radius: 3.0,
                });
                // checkmark
                if *checked {
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: bx + 3.0,
                            y: by + 1.0,
                            w: box_size,
                            h: box_size,
                        },
                        text: "✓".to_string(),
                        color: mul_alpha(theme.on_primary, alpha_accum),
                        size: 16.0,
                    });
                }
                // label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: bx + box_size + 8.0,
                        y: rect.y,
                        w: rect.w - (box_size + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
            }

            ViewKind::RadioButton {
                selected,
                label,
                on_select,
            } => {
                let theme = locals::theme();
                let d = 18.0f32;
                let cx = rect.x;
                let cy = rect.y + (rect.h - d) * 0.5;

                // outer circle (rounded rect as circle)
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: cx,
                        y: cy,
                        w: d,
                        h: d,
                    },
                    color: mul_alpha(Color::from_hex("#888888"), alpha_accum),
                    width: 1.5,
                    radius: d * 0.5,
                });
                // inner dot if selected
                if *selected {
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: cx + 4.0,
                            y: cy + 4.0,
                            w: d - 8.0,
                            h: d - 8.0,
                        },
                        color: mul_alpha(theme.primary, alpha_accum),
                        radius: (d - 8.0) * 0.5,
                    });
                }
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: cx + d + 8.0,
                        y: rect.y,
                        w: rect.w - (d + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
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
                let track_w = 46.0f32;
                let track_h = 26.0f32;
                let tx = rect.x;
                let ty = rect.y + (rect.h - track_h) * 0.5;
                let knob = 22.0f32;
                let on_col = theme.primary;
                let off_col = Color::from_hex("#333333");

                // track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: tx,
                        y: ty,
                        w: track_w,
                        h: track_h,
                    },
                    color: if *checked {
                        mul_alpha(on_col, alpha_accum)
                    } else {
                        mul_alpha(off_col, alpha_accum)
                    },
                    radius: track_h * 0.5,
                });
                // knob position
                let kx = if *checked {
                    tx + track_w - knob - 2.0
                } else {
                    tx + 2.0
                };
                let ky = ty + (track_h - knob) * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx,
                        y: ky,
                        w: knob,
                        h: knob,
                    },
                    color: mul_alpha(Color::from_hex("#EEEEEE"), alpha_accum),
                    radius: knob * 0.5,
                });

                // label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: tx + track_w + 8.0,
                        y: rect.y,
                        w: rect.w - (track_w + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
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
                let track_h = 4.0f32;
                let knob_d = 20.0f32;
                let gap = 8.0f32;
                let label_x = rect.x + rect.w * 0.6; // simple split: 60% track, 40% label
                let track_x = rect.x;
                let track_w = (label_x - track_x).max(60.0);
                let cy = rect.y + rect.h * 0.5;

                // Track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h * 0.5,
                });

                // Knob position
                let t = clamp01(norm(*value, *min, *max));
                let kx = track_x + t * track_w;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx - knob_d * 0.5,
                        y: cy - knob_d * 0.5,
                        w: knob_d,
                        h: knob_d,
                    },
                    color: mul_alpha(theme.surface, alpha_accum),
                    radius: knob_d * 0.5,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: kx - knob_d * 0.5,
                        y: cy - knob_d * 0.5,
                        w: knob_d,
                        h: knob_d,
                    },
                    color: mul_alpha(Color::from_hex("#888888"), alpha_accum),
                    width: 1.0,
                    radius: knob_d * 0.5,
                });

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: label_x + gap,
                        y: rect.y,
                        w: rect.x + rect.w - (label_x + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.2}", label, *value),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
                });

                // Interactions
                let on_change_cb: Option<Rc<dyn Fn(f32)>> = on_change.as_ref().cloned();
                let minv = *min;
                let maxv = *max;
                let stepv = *step;

                // per-hit-region current value (wheel deltas accumulate within a frame)
                let current = Rc::new(RefCell::new(*value));

                // pointer mapping closure (in global coords)
                let update_at = {
                    let on_change_cb = on_change_cb.clone();
                    let current = current.clone();
                    Rc::new(move |px: f32| {
                        let tt = clamp01((px - track_x) / track_w);
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

                // on_pointer_move: no gating inside; platform only delivers here while captured
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
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
                let track_h = 4.0f32;
                let knob_d = 20.0f32;
                let gap = 8.0f32;
                let label_x = rect.x + rect.w * 0.6;
                let track_x = rect.x;
                let track_w = (label_x - track_x).max(80.0);
                let cy = rect.y + rect.h * 0.5;

                // Track
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h * 0.5,
                });

                // Positions
                let t0 = clamp01(norm(*start, *min, *max));
                let t1 = clamp01(norm(*end, *min, *max));
                let k0x = track_x + t0 * track_w;
                let k1x = track_x + t1 * track_w;

                // Range fill
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: k0x.min(k1x),
                        y: cy - track_h * 0.5,
                        w: (k1x - k0x).abs(),
                        h: track_h,
                    },
                    color: mul_alpha(theme.primary, alpha_accum),
                    radius: track_h * 0.5,
                });

                // Knobs
                for &kx in &[k0x, k1x] {
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: kx - knob_d * 0.5,
                            y: cy - knob_d * 0.5,
                            w: knob_d,
                            h: knob_d,
                        },
                        color: mul_alpha(theme.surface, alpha_accum),
                        radius: knob_d * 0.5,
                    });
                    scene.nodes.push(SceneNode::Border {
                        rect: repose_core::Rect {
                            x: kx - knob_d * 0.5,
                            y: cy - knob_d * 0.5,
                            w: knob_d,
                            h: knob_d,
                        },
                        color: mul_alpha(Color::from_hex("#888888"), alpha_accum),
                        width: 1.0,
                        radius: knob_d * 0.5,
                    });
                }

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: label_x + gap,
                        y: rect.y,
                        w: rect.x + rect.w - (label_x + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.2} – {:.2}", label, *start, *end),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
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
                    Rc::new(move |px: f32| {
                        if let Some(thumb) = *active.borrow() {
                            let tt = clamp01((px - track_x) / track_w);
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
                        let px = pe.position.x;
                        let d0 = (px - k0x0).abs();
                        let d1 = (px - k1x0).abs();
                        *active.borrow_mut() = Some(if d0 <= d1 { 0 } else { 1 });
                        update(px);
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
                        color: mul_alpha(Color::from_hex("#88CCFF"), alpha_accum),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
            }
            ViewKind::ProgressBar {
                value,
                min,
                max,
                label,
                circular,
            } => {
                let theme = locals::theme();
                let track_h = 6.0f32;
                let gap = 8.0f32;
                let label_w_split = rect.w * 0.6;
                let track_x = rect.x;
                let track_w = (label_w_split - track_x).max(60.0);
                let cy = rect.y + rect.h * 0.5;

                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: mul_alpha(Color::from_hex("#333333"), alpha_accum),
                    radius: track_h * 0.5,
                });

                let t = clamp01(norm(*value, *min, *max));
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w * t,
                        h: track_h,
                    },
                    color: mul_alpha(theme.primary, alpha_accum),
                    radius: track_h * 0.5,
                });

                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: rect.x + label_w_split + gap,
                        y: rect.y,
                        w: rect.w - (label_w_split + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.0}%", label, t * 100.0),
                    color: mul_alpha(theme.on_surface, alpha_accum),
                    size: 16.0,
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
                base,
                alpha_accum,
                text_cache,
            );
        }

        if v.modifier.transform.is_some() {
            scene.nodes.push(SceneNode::PopTransform);
        }
    }

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
    );

    // Ensure visual order: low z_index first. Topmost will be found by iter().rev().
    hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));

    (scene, hits, sems)
}

/// Method styling for m3-like components
pub trait TextStyleExt {
    fn color(self, c: Color) -> View;
    fn size(self, px: f32) -> View;
    fn max_lines(self, n: usize) -> View;
    fn single_line(self) -> View;
    fn overflow_ellipsize(self) -> View;
    fn overflow_clip(self) -> View;
    fn overflow_visible(self) -> View;
}
impl TextStyleExt for View {
    fn color(self, c: Color) -> View {
        TextColor(self, c)
    }
    fn size(self, px: f32) -> View {
        TextSize(self, px)
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
