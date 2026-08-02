#![allow(non_snake_case)]


use repose_core::*;
use repose_tree::{NodeId, ViewTree};



pub(crate) fn open_url(url: &str) {
    let _ = webbrowser::open(url);
}

pub(crate) fn push_focus_ring(scene: &mut Scene, rect: repose_core::Rect, radius_dp: f32) {
    scene.nodes.push(SceneNode::Border {
        rect,
        color: locals::theme().focus,
        width: dp_to_px(2.0),
        radius: [dp_to_px(radius_dp); 4],
    });
}

pub(crate) fn focus_radius(modifier: &Modifier) -> f32 {
    modifier.clip_rounded.map(|r| r[0]).unwrap_or(6.0)
}

/// Associate a `FocusRequester` (if present on the modifier) with the view.
pub(crate) fn set_focus_requester(modifier: &Modifier, view_id: u64) {
    if let Some(ref fr) = modifier.focus_requester {
        FocusManager::set_requester_target(fr, view_id);
    }
}

// Helpers
pub(crate) fn infer_label(tree: &ViewTree, node_id: NodeId) -> Option<String> {
    let mut stack = vec![node_id];
    while let Some(id) = stack.pop() {
        let n = tree.get(id)?;
        if let ViewKind::Text { text, .. } = &n.kind
            && !text.is_empty()
        {
            return Some(text.clone());
        }
        for &ch in n.children.iter().rev() {
            stack.push(ch);
        }
    }
    None
}

pub(crate) fn intersect_rect(a: repose_core::Rect, b: repose_core::Rect) -> Option<repose_core::Rect> {
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

pub(crate) fn clip_hits_to_viewport(hits: &mut Vec<HitRegion>, start: usize, vp: repose_core::Rect) {
    let mut i = start;
    while i < hits.len() {
        if let Some(r) = intersect_rect(hits[i].rect, vp) {
            hits[i].rect = r;
            i += 1;
        } else {
            hits.remove(i);
        }
    }
}

pub(crate) fn mul_alpha_color(c: Color, a: f32) -> Color {
    Color(c.0, c.1, c.2, ((c.3 as f32) * a).clamp(0.0, 255.0) as u8)
}
pub(crate) fn mul_alpha_brush(b: Brush, a: f32) -> Brush {
    match b {
        Brush::Solid(c) => Brush::Solid(mul_alpha_color(c, a)),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start,
            end,
            start_color: mul_alpha_color(start_color, a),
            end_color: mul_alpha_color(end_color, a),
        },
        _ => b,
    }
}

pub(crate) fn clamp_radius(r: f32, w: f32, h: f32) -> f32 {
    r.max(0.0).min(0.5 * w.max(0.0)).min(0.5 * h.max(0.0))
}
pub(crate) fn clamp_radii(r: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    [
        clamp_radius(r[0], w, h),
        clamp_radius(r[1], w, h),
        clamp_radius(r[2], w, h),
        clamp_radius(r[3], w, h),
    ]
}
pub(crate) fn max_radii(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}
