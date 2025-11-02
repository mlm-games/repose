//! Widgets, layout (Taffy), painting into a platform-agnostic Scene, and text fields.
//!
//! compose-render-wgpu is responsible for GPU interaction

#![allow(non_snake_case)]
pub mod anim;
pub mod lazy;

use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::{cell::RefCell, cmp::Ordering};

use compose_core::*;
use taffy::Overflow;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Style};

use taffy::prelude::{Position, Size, auto, length, percent};

pub mod textfield;
pub use textfield::{TextField, TextFieldState};

use crate::textfield::{TF_FONT_PX, TF_PADDING_X, byte_to_char_index, measure_text, positions_for};
use compose_core::locals;

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
            get_scroll_offset: None,
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
            if compose_core::locals::text_direction() == compose_core::locals::TextDirection::Rtl {
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
        Text { text: String, font_px: f32 },
        Button { label: String },
        TextField,
        Container,
        Checkbox { label: String },
        Radio { label: String },
        Switch { label: String },
        Slider { label: String },
        Range { label: String },
        Progress { label: String },
    }

    let mut taffy: TaffyTree<NodeCtx> = TaffyTree::new();
    let mut nodes_map = HashMap::new();

    fn style_from_modifier(m: &Modifier, kind: &ViewKind) -> Style {
        let mut s = Style::default();
        s.display = match kind {
            ViewKind::Row => Display::Flex,
            ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV { .. } => Display::Flex,
            ViewKind::Stack => Display::Grid, // stack is grid overlay
            _ => Display::Flex,
        };
        if matches!(kind, ViewKind::Row) {
            s.flex_direction = FlexDirection::Row;
        }
        if matches!(
            kind,
            ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV { .. }
        ) {
            s.flex_direction = FlexDirection::Column;
        }

        if let Some(r) = m.aspect_ratio {
            s.aspect_ratio = Some(r);
        }

        // Flex
        if let Some(g) = m.flex_grow {
            s.flex_grow = g;
        }
        if let Some(sh) = m.flex_shrink {
            s.flex_shrink = sh;
        }
        if let Some(b) = m.flex_basis {
            s.flex_basis = length(b);
        }

        // Align self (including baseline)
        if let Some(a) = m.align_self {
            s.align_self = Some(a);
        }

        // Absolute positioning
        if let Some(compose_core::modifier::PositionType::Absolute) = m.position_type {
            s.position = Position::Absolute;
            s.inset = taffy::geometry::Rect {
                left: m.offset_left.map(length).unwrap_or_else(auto),
                right: m.offset_right.map(length).unwrap_or_else(auto),
                top: m.offset_top.map(length).unwrap_or_else(auto),
                bottom: m.offset_bottom.map(length).unwrap_or_else(auto),
            };
        }

        // Grid
        if let Some(cfg) = &m.grid {
            s.display = Display::Grid;

            // Explicit N equal columns: repeat Single(flex(1.0)) N times
            s.grid_template_columns = (0..cfg.columns)
                .map(|_| GridTemplateComponent::Single(flex(1.0f32)))
                .collect();

            // Set gaps
            s.gap = Size {
                width: length(cfg.column_gap),
                height: length(cfg.row_gap),
            };
        }

        // Baseline alignment default for rows/columns
        s.align_items = Some(AlignItems::FlexStart);
        s.justify_content = Some(JustifyContent::FlexStart);

        // Overflow for ScrollV
        if matches!(kind, ViewKind::ScrollV { .. }) {
            s.overflow = taffy::Point {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            };
        }

        if let Some(dir) = flex_dir_for(kind) {
            s.flex_direction = dir;
        }
        if let Some(p) = m.padding {
            let v = length(p);
            s.padding = taffy::geometry::Rect {
                left: v,
                right: v,
                top: v,
                bottom: v,
            };
        }

        if let Some(sz) = m.size {
            if sz.width.is_finite() {
                s.size.width = length(sz.width);
            }
            if sz.height.is_finite() {
                s.size.height = length(sz.height);
            }
        }

        if m.fill_max {
            s.size.width = percent(100.0);
            s.size.height = percent(100.0);
            s.flex_grow = 1.0;
            s.flex_shrink = 1.0;
        }

        s.align_items = Some(AlignItems::FlexStart);
        s.justify_content = Some(JustifyContent::FlexStart);
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

            if let Some(cs) = v.modifier.grid_col_span {
                style.grid_column = Line {
                    start: GridPlacement::Auto,
                    end: GridPlacement::Span(cs as u16),
                };
            }
            if let Some(rs) = v.modifier.grid_row_span {
                style.grid_row = Line {
                    start: GridPlacement::Auto,
                    end: GridPlacement::Span(rs as u16),
                };
            }
        }

        let children: Vec<_> = v
            .children
            .iter()
            .map(|c| build_node(c, t, nodes_map))
            .collect();

        let node = match &v.kind {
            ViewKind::Text {
                text, font_size, ..
            } => t
                .new_leaf_with_context(
                    style,
                    NodeCtx::Text {
                        text: text.clone(),
                        font_px: *font_size,
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

    let available = taffy::geometry::Size {
        width: AvailableSpace::Definite(size.0 as f32),
        height: AvailableSpace::Definite(size.1 as f32),
    };

    // Measure function for intrinsic content
    taffy
        .compute_layout_with_measure(root_node, available, |known, _avail, _node, ctx, _style| {
            match ctx {
                Some(NodeCtx::Text { text, font_px }) => {
                    let approx_w = text.len() as f32 * *font_px * 0.6;
                    let w = known.width.unwrap_or(approx_w);
                    taffy::geometry::Size {
                        width: w,
                        height: *font_px * 1.3,
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
                Some(NodeCtx::Container) | None => taffy::geometry::Size::ZERO,
            }
        })
        .unwrap();

    fn layout_of(node: taffy::NodeId, t: &TaffyTree<impl Clone>) -> compose_core::Rect {
        let l = t.layout(node).unwrap();
        compose_core::Rect {
            x: l.location.x,
            y: l.location.y,
            w: l.size.width,
            h: l.size.height,
        }
    }

    fn add_offset(mut r: compose_core::Rect, off: (f32, f32)) -> compose_core::Rect {
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
    ) {
        let local = layout_of(nodes[&v.id], t);
        let rect = add_offset(local, parent_offset);
        let base = (parent_offset.0 + local.x, parent_offset.1 + local.y);

        let is_hovered = interactions.hover == Some(v.id);
        let is_pressed = interactions.pressed.contains(&v.id);
        let is_focused = focused == Some(v.id);

        // Background/border (unchanged, but use 'rect')
        if let Some(bg) = v.modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                color: bg,
                radius: v.modifier.clip_rounded.unwrap_or(0.0),
            });
        }

        // Border
        if let Some(b) = &v.modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: b.color,
                width: b.width,
                radius: b.radius.max(v.modifier.clip_rounded.unwrap_or(0.0)),
            });
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
            });
        }

        match &v.kind {
            ViewKind::Text {
                text,
                color,
                font_size,
            } => {
                // Apply text scale from CompositionLocal
                let scaled_size = *font_size * locals::text_scale().0;
                scene.nodes.push(SceneNode::Text {
                    rect,
                    text: text.clone(),
                    color: *color,
                    size: scaled_size,
                });
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
                        color: base,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: rect.x + 12.0,
                        y: rect.y + 10.0,
                        w: rect.w - 24.0,
                        h: rect.h - 20.0,
                    },
                    text: text.clone(),
                    color: Color::WHITE,
                    size: 16.0,
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
                        color: Color::from_hex("#88CCFF"),
                        width: 2.0,
                        radius: v.modifier.clip_rounded.unwrap_or(6.0),
                    });
                }
            }

            ViewKind::TextField { hint, .. } => {
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
                });

                // Inner content rect (padding)
                let inner = compose_core::Rect {
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
                        color: Color::from_hex("#88CCFF"),
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
                            rect: compose_core::Rect {
                                x: sel_x,
                                y: inner.y,
                                w: sel_w,
                                h: inner.h,
                            },
                            color: Color::from_hex("#3B7BFF55"),
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
                                rect: compose_core::Rect {
                                    x: ux,
                                    y: inner.y + inner.h - 2.0,
                                    w: uw,
                                    h: 2.0,
                                },
                                color: Color::from_hex("#88CCFF"),
                                radius: 0.0,
                            });
                        }
                    }

                    // Text (offset by scroll)
                    scene.nodes.push(SceneNode::Text {
                        rect: compose_core::Rect {
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
                            Color::from_hex("#666666")
                        } else {
                            Color::from_hex("#CCCCCC")
                        },
                        size: TF_FONT_PX,
                    });

                    // Caret (blink)
                    if state.selection.start == state.selection.end && state.caret_visible() {
                        let i = byte_to_char_index(&m, state.selection.end);
                        let cx = m.positions.get(i).copied().unwrap_or(0.0) - state.scroll_offset;
                        let caret_x = inner.x + cx.max(0.0);
                        scene.nodes.push(SceneNode::Rect {
                            rect: compose_core::Rect {
                                x: caret_x,
                                y: inner.y,
                                w: 1.0,
                                h: inner.h,
                            },
                            color: Color::WHITE,
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
                        rect: compose_core::Rect {
                            x: inner.x,
                            y: inner.y,
                            w: inner.w,
                            h: inner.h,
                        },
                        text: hint.clone(),
                        color: Color::from_hex("#666666"),
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
                get_scroll_offset,
            } => {
                // Register hit region (use local rect for hit testing)
                hits.push(HitRegion {
                    id: v.id,
                    rect, // viewport in global coords
                    on_click: None,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                    on_pointer_down: v.modifier.on_pointer_down.clone(),
                    on_pointer_move: v.modifier.on_pointer_move.clone(),
                    on_pointer_up: v.modifier.on_pointer_up.clone(),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
                });

                // Report viewport height
                if let Some(set_vh) = set_viewport_height {
                    set_vh(local.h); // use local height before offsets
                }

                // Clip to viewport
                scene.nodes.push(SceneNode::PushClip {
                    rect,
                    radius: v.modifier.clip_rounded.unwrap_or(0.0),
                });

                // Child offset includes scroll translation (0, -scroll_offset)
                let mut child_offset = parent_offset;
                if let Some(get) = get_scroll_offset {
                    let so = get();
                    child_offset.1 -= so;
                }

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
                    );
                }

                scene.nodes.push(SceneNode::PopClip);
                return; // done with children
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
                    rect: compose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size,
                        h: box_size,
                    },
                    color: if *checked {
                        theme.primary
                    } else {
                        theme.surface
                    },
                    radius: 3.0,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: compose_core::Rect {
                        x: bx,
                        y: by,
                        w: box_size,
                        h: box_size,
                    },
                    color: Color::from_hex("#555555"),
                    width: 1.0,
                    radius: 3.0,
                });
                // checkmark
                if *checked {
                    scene.nodes.push(SceneNode::Text {
                        rect: compose_core::Rect {
                            x: bx + 3.0,
                            y: by + 1.0,
                            w: box_size,
                            h: box_size,
                        },
                        text: "✓".to_string(),
                        color: theme.on_primary,
                        size: 16.0,
                    });
                }
                // label
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: bx + box_size + 8.0,
                        y: rect.y,
                        w: rect.w - (box_size + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: theme.on_surface,
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
                        color: Color::from_hex("#88CCFF"),
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
                    rect: compose_core::Rect {
                        x: cx,
                        y: cy,
                        w: d,
                        h: d,
                    },
                    color: Color::from_hex("#888888"),
                    width: 1.5,
                    radius: d * 0.5,
                });
                // inner dot if selected
                if *selected {
                    scene.nodes.push(SceneNode::Rect {
                        rect: compose_core::Rect {
                            x: cx + 4.0,
                            y: cy + 4.0,
                            w: d - 8.0,
                            h: d - 8.0,
                        },
                        color: theme.primary,
                        radius: (d - 8.0) * 0.5,
                    });
                }
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: cx + d + 8.0,
                        y: rect.y,
                        w: rect.w - (d + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: theme.on_surface,
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
                        color: Color::from_hex("#88CCFF"),
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
                    rect: compose_core::Rect {
                        x: tx,
                        y: ty,
                        w: track_w,
                        h: track_h,
                    },
                    color: if *checked { on_col } else { off_col },
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
                    rect: compose_core::Rect {
                        x: kx,
                        y: ky,
                        w: knob,
                        h: knob,
                    },
                    color: Color::from_hex("#EEEEEE"),
                    radius: knob * 0.5,
                });

                // label
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: tx + track_w + 8.0,
                        y: rect.y,
                        w: rect.w - (track_w + 8.0),
                        h: rect.h,
                    },
                    text: label.clone(),
                    color: theme.on_surface,
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
                        color: Color::from_hex("#88CCFF"),
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
                    rect: compose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: Color::from_hex("#333333"),
                    radius: track_h * 0.5,
                });

                // Knob position
                let t = clamp01(norm(*value, *min, *max));
                let kx = track_x + t * track_w;
                scene.nodes.push(SceneNode::Rect {
                    rect: compose_core::Rect {
                        x: kx - knob_d * 0.5,
                        y: cy - knob_d * 0.5,
                        w: knob_d,
                        h: knob_d,
                    },
                    color: theme.surface,
                    radius: knob_d * 0.5,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: compose_core::Rect {
                        x: kx - knob_d * 0.5,
                        y: cy - knob_d * 0.5,
                        w: knob_d,
                        h: knob_d,
                    },
                    color: Color::from_hex("#888888"),
                    width: 1.0,
                    radius: knob_d * 0.5,
                });

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: label_x + gap,
                        y: rect.y,
                        w: rect.x + rect.w - (label_x + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.2}", label, *value),
                    color: theme.on_surface,
                    size: 16.0,
                });

                // Interactions
                let on_change_cb: Option<Rc<dyn Fn(f32)>> = on_change.as_ref().cloned();
                let minv = *min;
                let maxv = *max;
                let stepv = *step;

                // per-hit-region current value (wheel deltas accumulate within a frame)
                let current = Rc::new(RefCell::new(*value));
                // drag state
                let dragging = Rc::new(RefCell::new(false));

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

                // on_pointer_down: start drag and update once
                let on_pd: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
                    let f = update_at.clone();
                    let dragging = dragging.clone();
                    Rc::new(move |pe| {
                        *dragging.borrow_mut() = true;
                        f(pe.position.x);
                    })
                };

                // on_pointer_move: update only while dragging
                let on_pm: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
                    let f = update_at.clone();
                    let dragging = dragging.clone();
                    Rc::new(move |pe| {
                        if *dragging.borrow() {
                            f(pe.position.x);
                        }
                    })
                };

                // on_pointer_up: stop drag
                let on_pu: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
                    let dragging = dragging.clone();
                    Rc::new(move |_pe| {
                        *dragging.borrow_mut() = false;
                    })
                };

                // Mouse wheel nudge: accumulate via 'current'
                let on_scroll = {
                    let on_change_cb = on_change_cb.clone();
                    let current = current.clone();
                    Rc::new(move |dy: f32| -> f32 {
                        let base = *current.borrow();
                        let delta = stepv.unwrap_or((maxv - minv) * 0.01);
                        // winit: negative dy for wheel-up; treat that as increase
                        let dir = if dy.is_sign_negative() { 1.0 } else { -1.0 };
                        let new_v = snap_step(base + dir * delta, stepv, minv, maxv);
                        *current.borrow_mut() = new_v;
                        if let Some(cb) = &on_change_cb {
                            cb(new_v);
                        }
                        0.0
                    })
                };

                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: Some(on_scroll),
                    focusable: true,
                    on_pointer_down: Some(on_pd),
                    on_pointer_move: Some(on_pm),
                    on_pointer_up: Some(on_pu),
                    on_pointer_enter: v.modifier.on_pointer_enter.clone(),
                    on_pointer_leave: v.modifier.on_pointer_leave.clone(),
                    z_index: v.modifier.z_index,
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
                        color: Color::from_hex("#88CCFF"),
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
                    rect: compose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: Color::from_hex("#333333"),
                    radius: track_h * 0.5,
                });

                // Positions
                let t0 = clamp01(norm(*start, *min, *max));
                let t1 = clamp01(norm(*end, *min, *max));
                let k0x = track_x + t0 * track_w;
                let k1x = track_x + t1 * track_w;

                // Range fill
                scene.nodes.push(SceneNode::Rect {
                    rect: compose_core::Rect {
                        x: k0x.min(k1x),
                        y: cy - track_h * 0.5,
                        w: (k1x - k0x).abs(),
                        h: track_h,
                    },
                    color: theme.primary,
                    radius: track_h * 0.5,
                });

                // Knobs
                for &kx in &[k0x, k1x] {
                    scene.nodes.push(SceneNode::Rect {
                        rect: compose_core::Rect {
                            x: kx - knob_d * 0.5,
                            y: cy - knob_d * 0.5,
                            w: knob_d,
                            h: knob_d,
                        },
                        color: theme.surface,
                        radius: knob_d * 0.5,
                    });
                    scene.nodes.push(SceneNode::Border {
                        rect: compose_core::Rect {
                            x: kx - knob_d * 0.5,
                            y: cy - knob_d * 0.5,
                            w: knob_d,
                            h: knob_d,
                        },
                        color: Color::from_hex("#888888"),
                        width: 1.0,
                        radius: knob_d * 0.5,
                    });
                }

                // Label
                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: label_x + gap,
                        y: rect.y,
                        w: rect.x + rect.w - (label_x + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.2} – {:.2}", label, *start, *end),
                    color: theme.on_surface,
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
                let on_pd: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
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
                let on_pm: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
                    let active = active.clone();
                    let update = update.clone();
                    Rc::new(move |pe| {
                        if active.borrow().is_some() {
                            update(pe.position.x);
                        }
                    })
                };

                // on_pointer_up: clear active thumb
                let on_pu: Rc<dyn Fn(compose_core::input::PointerEvent)> = {
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
                        color: Color::from_hex("#88CCFF"),
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
                    rect: compose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w,
                        h: track_h,
                    },
                    color: Color::from_hex("#333333"),
                    radius: track_h * 0.5,
                });

                let t = clamp01(norm(*value, *min, *max));
                scene.nodes.push(SceneNode::Rect {
                    rect: compose_core::Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: track_w * t,
                        h: track_h,
                    },
                    color: theme.primary,
                    radius: track_h * 0.5,
                });

                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: rect.x + label_w_split + gap,
                        y: rect.y,
                        w: rect.w - (label_w_split + gap),
                        h: rect.h,
                    },
                    text: format!("{}: {:.0}%", label, t * 100.0),
                    color: theme.on_surface,
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
            );
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
    );

    // Ensure visual order: low z_index first. Topmost will be found by iter().rev().
    hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));

    (scene, hits, sems)
}
