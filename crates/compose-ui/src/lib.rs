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
    }

    let mut taffy: TaffyTree<NodeCtx> = TaffyTree::new();
    let mut nodes_map = HashMap::new();

    fn style_from_modifier(m: &Modifier, kind: &ViewKind) -> Style {
        let mut s = Style::default();
        s.display = Display::Flex;

        match kind {
            ViewKind::Row => s.flex_direction = FlexDirection::Row,
            ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV { .. } => {
                s.flex_direction = FlexDirection::Column;
            }
            ViewKind::Stack => s.display = Display::Grid,
            _ => {}
        }

        if let Some(p) = m.padding {
            let v = taffy::style::LengthPercentage::length(p);
            s.padding = taffy::geometry::Rect {
                left: v,
                right: v,
                top: v,
                bottom: v,
            };
        }

        if let Some(sz) = m.size {
            if sz.width.is_finite() {
                s.size.width = Dimension::length(sz.width);
            }
            if sz.height.is_finite() {
                s.size.height = Dimension::length(sz.height);
            }
        }

        if m.fill_max {
            s.size.width = Dimension::percent(1.0);
            s.size.height = Dimension::percent(1.0);
            s.flex_grow = 1.0;
            s.flex_shrink = 1.0;
        }

        if matches!(kind, ViewKind::ScrollV { .. }) {
            s.overflow = taffy::Point {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            };
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
        let style = style_from_modifier(&v.modifier, &v.kind);
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
    ) {
        let rect = layout_of(nodes[&v.id], t);

        let is_hovered = interactions.hover == Some(v.id);
        let is_pressed = interactions.pressed.contains(&v.id);
        let is_focused = focused == Some(v.id);

        // Background
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

            _ => {}
        }

        let clip_children = matches!(v.kind, ViewKind::ScrollV { .. });
        if clip_children {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: v.modifier.clip_rounded.unwrap_or(0.0),
            });
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
            );
        }
        if clip_children {
            scene.nodes.push(SceneNode::PopClip);
        }
    }

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
    );
    // Ensure visual order: low z_index first. Topmost will be found by iter().rev().
    hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));

    (scene, hits, sems)
}
