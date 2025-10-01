use std::rc::Rc;

use compose_core::*;
use taffy::style::{AlignItems, Dimension, Display, FlexDirection, JustifyContent, Style};

pub fn Surface(modifier: Modifier, child: View) -> View {
    let mut v = View::new(0, ViewKind::Surface).modifier(modifier);
    v.children = vec![child];
    v
}

pub fn Box(modifier: Modifier) -> View { View::new(0, ViewKind::Box).modifier(modifier) }
pub fn Row(modifier: Modifier) -> View { View::new(0, ViewKind::Row).modifier(modifier) }
pub fn Column(modifier: Modifier) -> View { View::new(0, ViewKind::Column).modifier(modifier) }
pub fn Stack(modifier: Modifier) -> View { View::new(0, ViewKind::Stack).modifier(modifier) }
pub fn Scroll(modifier: Modifier) -> View { View::new(0, ViewKind::ScrollV).modifier(modifier) }

pub fn Text(text: impl Into<String>) -> View {
    View::new(0, ViewKind::Text { text: text.into(), color: Color::WHITE, font_size: 16.0 })
}

pub fn TextColor(mut v: View, color: Color) -> View {
    if let ViewKind::Text { ref mut color: c, .. } = v.kind { *c = color; }
    v
}
pub fn TextSize(mut v: View, size: f32) -> View {
    if let ViewKind::Text { ref mut font_size: s, .. } = v.kind { *s = size; }
    v
}

pub fn Button(text: impl Into<String>, on_click: impl Fn() + 'static) -> View {
    View::new(0, ViewKind::Button { text: text.into(), on_click: Some(Rc::new(on_click)) })
        .semantics(Semantics { role: Role::Button, label: None, focused: false, enabled: true })
}

pub fn TextField(state_key: ViewId, hint: impl Into<String>) -> View {
    View::new(0, ViewKind::TextField { state_key, hint: hint.into() })
        .semantics(Semantics { role: Role::TextField, label: None, focused: false, enabled: true })
}

/// Assign stable ids (keys) to nodes depth-first, and build a Taffy tree for layout.
/// Then emit Scene + input hit regions + semantics nodes.
pub fn layout_and_paint(root: &View, size: (u32,u32)) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
    // Assign ids
    let mut id = 1u64;
    fn stamp(mut v: View, id: &mut u64) -> View {
        v.id = *id; *id += 1;
        v.children = v.children.into_iter().map(|c| stamp(c, id)).collect();
        v
    }
    let root = stamp(root.clone(), &mut id);

    // Build Taffy tree
    let mut taffy = taffy::Taffy::new();
    let mut nodes_map = std::collections::HashMap::new();

    fn style_from_modifier(m: &Modifier, kind: &ViewKind) -> Style {
        let mut s = Style::default();
        s.display = Display::Flex;
        match kind {
            ViewKind::Row => { s.flex_direction = FlexDirection::Row; }
            ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV => { s.flex_direction = FlexDirection::Column; }
            ViewKind::Stack => { s.display = Display::Grid; }
            _ => {}
        }
        if let Some(p) = m.padding { let v = taffy::style::LengthPercentage::Length(p); s.padding = taffy::geometry::Rect { left: v, right: v, top: v, bottom: v }; }
        if let Some(sz) = m.size {
            if sz.width.is_finite() { s.size.width = Dimension::Length(sz.width); }
            if sz.height.is_finite() { s.size.height = Dimension::Length(sz.height); }
        }
        if m.fill_max {
            s.size.width = Dimension::Percent(1.0);
            s.size.height = Dimension::Percent(1.0);
        }
        s.align_items = Some(AlignItems::FlexStart);
        s.justify_content = Some(JustifyContent::FlexStart);
        s
    }

    fn build_node(v: &View, t: &mut taffy::Taffy, nodes_map: &mut std::collections::HashMap<ViewId, taffy::node::Node>) -> taffy::node::Node {
        let style = style_from_modifier(&v.modifier, &v.kind);
        let children: Vec<_> = v.children.iter().map(|c| build_node(c, t, nodes_map)).collect();

        // measure Text and Button intrinsic sizes
        use taffy::prelude::*;
        let node = match &v.kind {
            ViewKind::Text { text, font_size, .. } => {
                let txt = text.clone(); let fs = *font_size;
                t.new_with_children(
                    style,
                    &[],
                ).unwrap()
                .tap(|n| {
                    let _ = t.set_measure(n, Some(Box::new(move |known, _| {
                        // crude width estimate: 0.6em per char
                        let w = known.width.or_else(|| Some(AvailableSpace::Definite(txt.len() as f32 * fs * 0.6))).unwrap();
                        let wv = match w { AvailableSpace::Definite(v) => v, _ => txt.len() as f32 * fs * 0.6 };
                        Ok(Size { width: wv, height: fs * 1.3 })
                    })));
                })
            }
            ViewKind::Button { text, .. } => {
                let label = text.clone();
                let fs = 16.0f32;
                t.new_with_children(style, &[]).unwrap()
                 .tap(|n| {
                    let _ = t.set_measure(n, Some(Box::new(move |_known, _| {
                        Ok(Size { width: (label.len() as f32 * fs * 0.6) + 24.0, height: 32.0 })
                    })));
                 })
            }
            ViewKind::TextField { .. } => {
                t.new_with_children(style, &[]).unwrap()
                 .tap(|n| {
                    let _ = t.set_measure(n, Some(Box::new(move |_known, _| {
                        Ok(Size { width: 220.0, height: 36.0 })
                    })));
                 })
            }
            _ => t.new_with_children(style, &children).unwrap(),
        };
        nodes_map.insert(v.id, node);
        node
    }

    trait Tap: Sized { fn tap<F: FnOnce(Self) -> Self>(self, f: F) -> Self { f(self) } }
    impl<T> Tap for T {}

    let root_node = build_node(&root, &mut taffy, &mut nodes_map);

    let size_points = taffy::geometry::Size { width: size.0 as f32, height: size.1 as f32 };
    taffy.compute_layout(root_node, size_points).unwrap();

    fn layout_of(node: taffy::node::Node, t: &taffy::Taffy) -> Rect {
        let l = t.layout(node).unwrap();
        Rect { x: l.location.x, y: l.location.y, w: l.size.width, h: l.size.height }
    }

    let mut scene = Scene { clear_color: Color::from_hex("#121212"), nodes: vec![] };
    let mut hits: Vec<HitRegion> = vec![];
    let mut sems: Vec<SemNode> = vec![];

    fn walk(v: &View, t: &taffy::Taffy, nodes: &std::collections::HashMap<ViewId, taffy::node::Node>, scene: &mut Scene, hits: &mut Vec<HitRegion>, sems: &mut Vec<SemNode>) {
        let rect = layout_of(nodes[&v.id], t);
        // background
        if let Some(bg) = v.modifier.background {
            scene.nodes.push(SceneNode::Rect { rect, color: bg, radius: v.modifier.clip_rounded.unwrap_or(0.0) });
        }
        if let Some(b) = &v.modifier.border {
            scene.nodes.push(SceneNode::Border { rect, color: b.color, width: b.width, radius: b.radius.max(v.modifier.clip_rounded.unwrap_or(0.0)) });
        }
        match &v.kind {
            ViewKind::Text { text, color, font_size } => {
                scene.nodes.push(SceneNode::Text { rect, text: text.clone(), color: *color, size: *font_size });
                sems.push(SemNode { id: v.id, role: Role::Text, label: Some(text.clone()), rect, focused: false });
            }
            ViewKind::Button { text, on_click } => {
                if v.modifier.click || on_click.is_some() {
                    hits.push(HitRegion { id: v.id, rect, on_click: on_click.clone(), focusable: false });
                }
                sems.push(SemNode { id: v.id, role: Role::Button, label: Some(text.clone()), rect, focused: false });
            }
            ViewKind::TextField { .. } => {
                hits.push(HitRegion { id: v.id, rect, on_click: None, focusable: true });
                sems.push(SemNode { id: v.id, role: Role::TextField, label: v.modifier.semantics_label.clone(), rect, focused: false });
            }
            _ => {}
        }
        for c in &v.children { walk(c, t, nodes, scene, hits, sems); }
    }

    walk(&root, &taffy, &nodes_map, &mut scene, &mut hits, &mut sems);
    (scene, hits, sems)
}
