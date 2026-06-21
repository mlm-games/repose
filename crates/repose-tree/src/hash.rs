//! Content hashing for change detection.

use rapidhash::fast::RapidHasher;
use repose_core::{
    Brush, Color, Modifier, TextOverflow, View, ViewKind,
    animation::{AnimationSpec, Easing},
};
use std::hash::{Hash, Hasher};

/// Compute a content hash for a View's immediate properties.
/// This does NOT include children - that's handled separately.
pub fn hash_view_content(view: &View) -> u64 {
    let mut hasher = RapidHasher::default();

    // Hash the kind
    hash_view_kind(&view.kind, &mut hasher);

    // Hash relevant modifier properties
    hash_modifier(&view.modifier, &mut hasher);

    // Hash user key if present
    if let Some(key) = view.modifier.key {
        key.hash(&mut hasher);
    }

    hasher.finish()
}

/// Compute a hash that includes the subtree structure.
/// This combines the node's content hash with its children's subtree hashes.
pub fn hash_subtree(content_hash: u64, children_hashes: &[u64]) -> u64 {
    let mut hasher = RapidHasher::default();
    content_hash.hash(&mut hasher);
    children_hashes.len().hash(&mut hasher);
    for &h in children_hashes {
        h.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_view_kind(kind: &ViewKind, hasher: &mut impl Hasher) {
    // Discriminant
    std::mem::discriminant(kind).hash(hasher);

    match kind {
        ViewKind::Text {
            text,
            color,
            font_size,
            soft_wrap,
            max_lines,
            overflow,
            font_family,
            annotations,
        } => {
            font_family.hash(hasher);
            text.hash(hasher);
            hash_color(color, hasher);
            ((font_size * 100.0) as u32).hash(hasher);
            soft_wrap.hash(hasher);
            max_lines.hash(hasher);
            hash_text_overflow(overflow, hasher);
            if let Some(annos) = annotations {
                annos.len().hash(hasher);
                for span in annos.iter() {
                    span.start.hash(hasher);
                    span.end.hash(hasher);
                    if let Some(c) = &span.style.color {
                        hash_color(c, hasher);
                    }
                    if let Some(fs) = span.style.font_size {
                        ((fs * 100.0) as u32).hash(hasher);
                    }
                }
            }
        }
        ViewKind::Button { .. } => {
            // on_click is a closure, can't hash it
            // We rely on the closure being recreated each frame anyway
        }
        ViewKind::TextField {
            state_key,
            hint,
            value,
            ..
        } => {
            state_key.hash(hasher);
            hint.hash(hasher);
            value.hash(hasher);
        }
        ViewKind::Image { handle, tint, fit } => {
            handle.hash(hasher);
            hash_color(tint, hasher);
            std::mem::discriminant(fit).hash(hasher);
        }
        ViewKind::Ellipse { rect, color } => {
            hash_rect(rect, hasher);
            hash_color(color, hasher);
        }
        ViewKind::EllipseBorder { rect, color, width } => {
            hash_rect(rect, hasher);
            hash_color(color, hasher);
            ((width * 100.0) as u32).hash(hasher);
        }
        ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. } => {
            // Scroll state is external, not part of content hash
        }
        ViewKind::OverlayHost
        | ViewKind::Surface
        | ViewKind::Box
        | ViewKind::Row
        | ViewKind::Column
        | ViewKind::Stack
        | ViewKind::ZStack => {
            // These are just containers, discriminant is enough
        }
        ViewKind::SubcomposeLayout { .. } => {
            // Closure contents are not part of the hash; the content closure
            // is re-invoked on every reconcile of a SubcomposeLayout.
        }
        _ => {} // Future ViewKind variants
    }
}

fn hash_modifier(m: &Modifier, hasher: &mut impl Hasher) {
    // Size
    if let Some(s) = &m.size {
        ((s.width * 100.0) as i32).hash(hasher);
        ((s.height * 100.0) as i32).hash(hasher);
    }
    m.width.map(|w| (w * 100.0) as i32).hash(hasher);
    m.height.map(|h| (h * 100.0) as i32).hash(hasher);
    m.fill_max.hash(hasher);
    m.fill_max_w.hash(hasher);
    m.fill_max_h.hash(hasher);
    m.repaint_boundary.hash(hasher);

    // Padding
    m.padding.map(|p| (p * 100.0) as i32).hash(hasher);
    if let Some(pv) = &m.padding_values {
        ((pv.left * 100.0) as i32).hash(hasher);
        ((pv.right * 100.0) as i32).hash(hasher);
        ((pv.top * 100.0) as i32).hash(hasher);
        ((pv.bottom * 100.0) as i32).hash(hasher);
    }

    // Min/max size
    m.min_width.map(|v| (v * 100.0) as i32).hash(hasher);
    m.min_height.map(|v| (v * 100.0) as i32).hash(hasher);
    m.max_width.map(|v| (v * 100.0) as i32).hash(hasher);
    m.max_height.map(|v| (v * 100.0) as i32).hash(hasher);

    // Background
    if let Some(bg) = &m.background {
        hash_brush(bg, hasher);
    }

    // Border
    if let Some(b) = &m.border {
        ((b.width * 100.0) as i32).hash(hasher);
        hash_color(&b.color, hasher);
        ((b.radius * 100.0) as i32).hash(hasher);
    }

    // Flex
    m.flex_grow.map(|v| (v * 100.0) as i32).hash(hasher);
    m.flex_shrink.map(|v| (v * 100.0) as i32).hash(hasher);
    m.flex_basis.map(|v| (v * 100.0) as i32).hash(hasher);
    m.flex_wrap.map(|v| std::mem::discriminant(&v)).hash(hasher);
    m.flex_dir.map(|v| std::mem::discriminant(&v)).hash(hasher);
    m.align_self
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);
    m.justify_content
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);
    m.align_items_container
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);
    m.align_content
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);

    // Clip
    m.clip_rounded.map(|v| (v * 100.0) as i32).hash(hasher);

    // Transform
    if let Some(t) = &m.transform {
        ((t.translate_x * 100.0) as i32).hash(hasher);
        ((t.translate_y * 100.0) as i32).hash(hasher);
        ((t.scale_x * 100.0) as i32).hash(hasher);
        ((t.scale_y * 100.0) as i32).hash(hasher);
        ((t.rotate * 1000.0) as i32).hash(hasher);
    }

    // Alpha
    m.alpha.map(|a| (a * 255.0) as u8).hash(hasher);

    // Position
    m.position_type
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);
    m.offset_left.map(|v| (v * 100.0) as i32).hash(hasher);
    m.offset_right.map(|v| (v * 100.0) as i32).hash(hasher);
    m.offset_top.map(|v| (v * 100.0) as i32).hash(hasher);
    m.offset_bottom.map(|v| (v * 100.0) as i32).hash(hasher);

    // Grid
    if let Some(g) = &m.grid {
        g.columns.hash(hasher);
        ((g.row_gap * 100.0) as i32).hash(hasher);
        ((g.column_gap * 100.0) as i32).hash(hasher);
    }
    m.grid_col_span.hash(hasher);
    m.grid_row_span.hash(hasher);

    // Aspect ratio
    m.aspect_ratio.map(|v| (v * 100.0) as i32).hash(hasher);

    // Z-index
    ((m.z_index * 100.0) as i32).hash(hasher);
    m.render_z_index.map(|v| (v * 100.0) as i32).hash(hasher);
    m.input_blocker.hash(hasher);

    // Clickable
    m.click.hash(hasher);
    m.disabled.hash(hasher);
    (m.on_action.is_some()).hash(hasher);

    (m.on_drag_start.is_some()).hash(hasher);
    (m.on_drag_end.is_some()).hash(hasher);
    (m.on_drag_enter.is_some()).hash(hasher);
    (m.on_drag_over.is_some()).hash(hasher);
    (m.on_drag_leave.is_some()).hash(hasher);
    (m.on_drop.is_some()).hash(hasher);

    // State colors
    if let Some(sc) = &m.state_colors {
        hash_color(&sc.default, hasher);
        hash_color(&sc.hovered, hasher);
        hash_color(&sc.pressed, hasher);
        hash_color(&sc.disabled, hasher);
    }

    if let Some(se) = &m.state_elevation {
        ((se.default * 100.0) as i32).hash(hasher);
        ((se.hovered * 100.0) as i32).hash(hasher);
        ((se.pressed * 100.0) as i32).hash(hasher);
        ((se.disabled * 100.0) as i32).hash(hasher);
    }

    if let Some(spec) = &m.animate_content_size {
        hash_animation_spec(spec, hasher);
    }

    m.focus_requester.is_some().hash(hasher);

    m.on_focus_changed.is_some().hash(hasher);
}

fn hash_animation_spec(spec: &AnimationSpec, hasher: &mut impl Hasher) {
    spec.duration.as_millis().hash(hasher);
    hash_easing(&spec.easing, hasher);
    spec.delay.as_millis().hash(hasher);
    if let Some(spring) = &spec.spring {
        ((spring.damping_ratio * 100.0) as i32).hash(hasher);
        ((spring.stiffness * 100.0) as i32).hash(hasher);
    }
    if let Some(repeat) = &spec.repeat {
        repeat.iterations.hash(hasher);
        repeat.reverse.hash(hasher);
        repeat.delay_between.as_millis().hash(hasher);
    }
}

fn hash_easing(easing: &Easing, hasher: &mut impl Hasher) {
    std::mem::discriminant(easing).hash(hasher);
    if let Easing::SpringCrit { omega } = easing {
        ((omega * 100.0) as i32).hash(hasher);
    }
}

fn hash_color(c: &Color, hasher: &mut impl Hasher) {
    c.0.hash(hasher);
    c.1.hash(hasher);
    c.2.hash(hasher);
    c.3.hash(hasher);
}

fn hash_brush(b: &Brush, hasher: &mut impl Hasher) {
    std::mem::discriminant(b).hash(hasher);
    match b {
        Brush::Solid(c) => hash_color(c, hasher),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => {
            ((start.x * 100.0) as i32).hash(hasher);
            ((start.y * 100.0) as i32).hash(hasher);
            ((end.x * 100.0) as i32).hash(hasher);
            ((end.y * 100.0) as i32).hash(hasher);
            hash_color(start_color, hasher);
            hash_color(end_color, hasher);
        }
        _ => {} // Future Brush variants
    }
}

fn hash_rect(r: &repose_core::Rect, hasher: &mut impl Hasher) {
    ((r.x * 100.0) as i32).hash(hasher);
    ((r.y * 100.0) as i32).hash(hasher);
    ((r.w * 100.0) as i32).hash(hasher);
    ((r.h * 100.0) as i32).hash(hasher);
}

fn hash_text_overflow(o: &TextOverflow, hasher: &mut impl Hasher) {
    std::mem::discriminant(o).hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::{Modifier, View, ViewKind};

    #[test]
    fn test_same_view_same_hash() {
        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0));
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0));

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
    }

    #[test]
    fn test_different_view_different_hash() {
        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0));
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(200.0));

        assert_ne!(hash_view_content(&v1), hash_view_content(&v2));
    }

    #[test]
    fn test_text_content_hash() {
        let v1 = View::new(
            0,
            ViewKind::Text {
                text: "Hello".to_string(),
                color: Color::WHITE,
                font_size: 16.0,
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
            },
        );
        let v2 = View::new(
            0,
            ViewKind::Text {
                text: "Hello".to_string(),
                color: Color::WHITE,
                font_size: 16.0,
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
            },
        );
        let v3 = View::new(
            0,
            ViewKind::Text {
                text: "World".to_string(),
                color: Color::WHITE,
                font_size: 16.0,
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
            },
        );

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
        assert_ne!(hash_view_content(&v1), hash_view_content(&v3));
    }
}
