#![allow(non_snake_case)]
pub mod lazy;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use compose_core::*;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Style};

pub mod textfield;
pub use textfield::{TextField, TextFieldState};

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
        color: ref mut text_color,
        ..
    } = v.kind
    {
        *text_color = color;
    }
    v
}

#[allow(non_snake_case)]
pub fn TextSize(mut v: View, size: f32) -> View {
    if let ViewKind::Text {
        font_size: ref mut text_size,
        ..
    } = v.kind
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
        clear_color: Color::from_hex("#121212"),
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
    ) {
        let rect = layout_of(nodes[&v.id], t);

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
                scene.nodes.push(SceneNode::Text {
                    rect,
                    text: text.clone(),
                    color: *color,
                    size: *font_size,
                });
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Text,
                    label: Some(text.clone()),
                    rect,
                    focused: false,
                });
            }

            ViewKind::Button { text, on_click } => {
                if v.modifier.click || on_click.is_some() {
                    hits.push(HitRegion {
                        id: v.id,
                        rect,
                        on_click: on_click.clone(),
                        on_scroll: None,
                        focusable: false,
                    });
                }
                sems.push(SemNode {
                    id: v.id,
                    role: Role::Button,
                    label: Some(text.clone()),
                    rect,
                    focused: false,
                });
            }

            ViewKind::TextField { hint, .. } => {
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: None,
                    focusable: true,
                });

                // Render TextField content from state
                let display_text = if let Some(state) = textfield_states.get(&v.id) {
                    let state = state.borrow();
                    if state.text.is_empty() {
                        hint.clone()
                    } else {
                        state.text.clone()
                    }
                } else {
                    hint.clone()
                };

                scene.nodes.push(SceneNode::Text {
                    rect: compose_core::Rect {
                        x: rect.x + 8.0,
                        y: rect.y + 8.0,
                        w: rect.w - 16.0,
                        h: rect.h - 16.0,
                    },
                    text: display_text.clone(),
                    color: Color::from_hex("#CCCCCC"),
                    size: 16.0,
                });

                sems.push(SemNode {
                    id: v.id,
                    role: Role::TextField,
                    label: Some(display_text),
                    rect,
                    focused: false,
                });
            }

            ViewKind::ScrollV {
                on_scroll,
                set_viewport_height,
            } => {
                // Provide scroll hit region over the viewport
                hits.push(HitRegion {
                    id: v.id,
                    rect,
                    on_click: None,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                });

                // Inform state of viewport height if requested
                if let Some(set_vh) = set_viewport_height {
                    set_vh(rect.h);
                }
            }

            _ => {}
        }

        for c in &v.children {
            walk(c, t, nodes, scene, hits, sems, textfield_states);
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
    );
    (scene, hits, sems)
}
