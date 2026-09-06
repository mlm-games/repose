//! Content hashing for change detection.

use rapidhash::fast::RapidHasher;
use repose_core::{
    Brush, Color, Modifier, TextOverflow, View, ViewKind,
    animation::{AnimationSpec, Easing},
    scroll::ScrollBinding,
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
            text_align,
            font_weight,
            font_style,
            text_decoration,
            letter_spacing,
            line_height,
            url: _,
            font_variation_settings,
        } => {
            font_family.hash(hasher);
            text.hash(hasher);
            hash_color(color, hasher);
            hash_f32(*font_size, hasher);
            soft_wrap.hash(hasher);
            max_lines.hash(hasher);
            hash_text_overflow(overflow, hasher);
            text_align.hash(hasher);
            font_weight.0.hash(hasher);
            font_style.hash(hasher);
            text_decoration.underline.hash(hasher);
            text_decoration.strikethrough.hash(hasher);
            if let Some(c) = &text_decoration.color {
                hash_color(c, hasher);
            }
            hash_f32(*letter_spacing, hasher);
            hash_f32(*line_height, hasher);
            font_variation_settings.hash(hasher);
            if let Some(annos) = annotations {
                annos.len().hash(hasher);
                for span in annos.iter() {
                    span.start.hash(hasher);
                    span.end.hash(hasher);
                    if let Some(c) = &span.style.color {
                        hash_color(c, hasher);
                    }
                    if let Some(fs) = span.style.font_size {
                        hash_f32(fs, hasher);
                    }
                    if let Some(fw) = span.style.font_weight {
                        fw.hash(hasher);
                    }
                    if let Some(fam) = &span.style.font_family {
                        fam.hash(hasher);
                    }
                    if let Some(fs) = span.style.font_style {
                        fs.hash(hasher);
                    }
                    if let Some(ls) = span.style.letter_spacing {
                        hash_f32(ls, hasher);
                    }
                    if let Some(lh) = span.style.line_height {
                        hash_f32(lh, hasher);
                    }
                    if let Some(bg) = &span.style.background {
                        hash_color(bg, hasher);
                    }
                    if let Some(td) = &span.style.text_decoration {
                        td.underline.hash(hasher);
                        td.strikethrough.hash(hasher);
                    }
                    if let Some(bs) = &span.style.baseline_shift {
                        hash_f32(bs.0, hasher);
                    }
                    if let Some(ds) = &span.style.draw_style {
                        match ds {
                            repose_core::DrawStyle::Fill => 0u8.hash(hasher),
                            repose_core::DrawStyle::Stroke {
                                width,
                                cap,
                                join,
                                miter,
                                path_effect,
                            } => {
                                1u8.hash(hasher);
                                hash_f32(*width, hasher);
                                (*cap as u8).hash(hasher);
                                (*join as u8).hash(hasher);
                                hash_f32(*miter, hasher);
                                if let Some(pe) = path_effect {
                                    match pe {
                                        repose_core::PathEffect::Corner { radius } => {
                                            0u8.hash(hasher);
                                            hash_f32(*radius, hasher);
                                        }
                                        repose_core::PathEffect::Dash { intervals, phase } => {
                                            1u8.hash(hasher);
                                            intervals.len().hash(hasher);
                                            for v in intervals {
                                                hash_f32(*v, hasher);
                                            }
                                            hash_f32(*phase, hasher);
                                        }
                                    }
                                } else {
                                    2u8.hash(hasher);
                                }
                            }
                        }
                    }
                    hash_f32(span.style.alpha, hasher);
                    if let Some(fvs) = &span.style.font_variation_settings {
                        fvs.hash(hasher);
                    }
                    if let Some(url) = &span.url {
                        url.hash(hasher);
                    }
                }
            }
        }
        ViewKind::Image { handle, tint, fit } => {
            handle.hash(hasher);
            hash_color(tint, hasher);
            std::mem::discriminant(fit).hash(hasher);
        }
        ViewKind::OverlayHost
        | ViewKind::Box
        | ViewKind::Row
        | ViewKind::Column
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

fn hash_f32(v: f32, hasher: &mut impl Hasher) {
    let mut bits = v.to_bits();
    if bits == 0x8000_0000 {
        bits = 0;
    }
    if v.is_nan() {
        bits = 0x7FC0_0000;
    }
    bits.hash(hasher);
}
fn hash_opt_f32(v: Option<f32>, hasher: &mut impl Hasher) {
    match v {
        Some(x) => {
            1u8.hash(hasher);
            hash_f32(x, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_modifier(m: &Modifier, hasher: &mut impl Hasher) {
    // Size
    if let Some(s) = &m.size {
        hash_f32(s.width, hasher);
        hash_f32(s.height, hasher);
    }
    hash_opt_f32(m.width, hasher);
    hash_opt_f32(m.height, hasher);
    if let Some(s) = &m.required_size {
        hash_f32(s.width, hasher);
        hash_f32(s.height, hasher);
    }
    hash_opt_f32(m.required_min_width, hasher);
    hash_opt_f32(m.required_max_width, hasher);
    hash_opt_f32(m.required_min_height, hasher);
    hash_opt_f32(m.required_max_height, hasher);
    hash_opt_f32(m.default_min_width, hasher);
    hash_opt_f32(m.default_min_height, hasher);
    hash_opt_f32(m.fill_max, hasher);
    hash_opt_f32(m.fill_max_w, hasher);
    hash_opt_f32(m.fill_max_h, hasher);
    m.repaint_boundary.hash(hasher);

    // Padding
    hash_opt_f32(m.padding, hasher);
    if let Some(pv) = &m.padding_values {
        hash_f32(pv.left, hasher);
        hash_f32(pv.right, hasher);
        hash_f32(pv.top, hasher);
        hash_f32(pv.bottom, hasher);
    }

    // Min/max size
    hash_opt_f32(m.min_width, hasher);
    hash_opt_f32(m.min_height, hasher);
    hash_opt_f32(m.max_width, hasher);
    hash_opt_f32(m.max_height, hasher);

    // Background
    if let Some(bg) = &m.background {
        hash_brush(bg, hasher);
    }

    // Border
    if let Some(b) = &m.border {
        hash_f32(b.width, hasher);
        hash_color(&b.color, hasher);
        for &r in &b.radius {
            hash_f32(r, hasher);
        }
    }

    // Flex
    hash_opt_f32(m.flex_grow, hasher);
    hash_opt_f32(m.flex_shrink, hasher);
    hash_opt_f32(m.flex_basis, hasher);
    m.flex_wrap.map(|v| std::mem::discriminant(&v)).hash(hasher);
    m.flex_basis_content.hash(hasher);
    m.flex_line_count.hash(hasher);
    m.flex_dir.map(|v| std::mem::discriminant(&v)).hash(hasher);
    m.align_self
        .map(|v| (v.keyword as u8, v.safety as u8))
        .hash(hasher);
    m.justify_content
        .map(|v| (v.keyword as u8, v.safety as u8))
        .hash(hasher);
    m.align_items_container
        .map(|v| (v.keyword as u8, v.safety as u8))
        .hash(hasher);
    m.align_content
        .map(|v| (v.keyword as u8, v.safety as u8))
        .hash(hasher);
    m.baseline_align.hash(hasher);

    // Clip
    if let Some(r) = &m.clip_rounded {
        for &v in r {
            hash_f32(v, hasher);
        }
    }

    // Transform
    if let Some(t) = &m.transform {
        hash_f32(t.translate_x, hasher);
        hash_f32(t.translate_y, hasher);
        hash_f32(t.scale_x, hasher);
        hash_f32(t.scale_y, hasher);
        hash_f32(t.rotate, hasher);
    }

    // Alpha
    hash_opt_f32(m.alpha, hasher);

    // Position
    m.position_type
        .map(|v| std::mem::discriminant(&v))
        .hash(hasher);
    hash_opt_f32(m.offset_left, hasher);
    hash_opt_f32(m.offset_right, hasher);
    hash_opt_f32(m.offset_top, hasher);
    hash_opt_f32(m.offset_bottom, hasher);

    // Grid
    if let Some(g) = &m.grid {
        g.columns.hash(hasher);
        hash_f32(g.row_gap, hasher);
        hash_f32(g.column_gap, hasher);
    }
    m.grid_col_span.hash(hasher);
    m.grid_row_span.hash(hasher);

    // Aspect ratio
    hash_opt_f32(m.aspect_ratio, hasher);
    m.intrinsic_width.hash(hasher);
    m.intrinsic_height.hash(hasher);
    hash_opt_f32(m.fit_content_width, hasher);
    hash_opt_f32(m.fit_content_height, hasher);
    // Contain has no Hash impl upstream; map to a discriminant byte.
    match m.contain {
        None => 0u8.hash(hasher),
        Some(c) if c == taffy::Contain::CONTENT => 3u8.hash(hasher),
        Some(c) if c == taffy::Contain::LAYOUT => 1u8.hash(hasher),
        Some(c) if c == taffy::Contain::PAINT => 2u8.hash(hasher),
        Some(_) => 4u8.hash(hasher),
    }

    // Z-index
    hash_f32(m.z_index, hasher);
    hash_opt_f32(m.render_z_index, hasher);
    m.input_blocker.hash(hasher);

    // Clickable
    m.click.hash(hasher);
    m.disabled.hash(hasher);
    m.focusable.hash(hasher);
    m.propagate_min.hash(hasher);
    m.focus_group.hash(hasher);
    (m.on_action.is_some()).hash(hasher);
    (m.on_double_click.is_some()).hash(hasher);
    (m.on_long_click.is_some()).hash(hasher);

    // Scroll (presence + axis + show_scrollbar - closures intentionally not hashed)
    match &m.scroll {
        None => 0u8.hash(hasher),
        Some(ScrollBinding::Vertical(b)) => {
            1u8.hash(hasher);
            b.show_scrollbar.hash(hasher);
        }
        Some(ScrollBinding::Horizontal(b)) => {
            2u8.hash(hasher);
            b.show_scrollbar.hash(hasher);
        }
        Some(ScrollBinding::Both(b)) => {
            3u8.hash(hasher);
            b.show_scrollbar.hash(hasher);
        }
    }
    m.nested_scroll_connection.is_some().hash(hasher);
    m.on_scroll.is_some().hash(hasher);

    // Overflow / clip_rect
    m.overflow.map(|o| std::mem::discriminant(&o)).hash(hasher);
    if let Some(cr) = &m.clip_rect {
        ((cr.left * 100.0) as i32).hash(hasher);
        ((cr.top * 100.0) as i32).hash(hasher);
        ((cr.right * 100.0) as i32).hash(hasher);
        ((cr.bottom * 100.0) as i32).hash(hasher);
        std::mem::discriminant(&cr.op).hash(hasher);
    }

    // Gaps & margins
    hash_opt_f32(m.gap, hasher);
    hash_opt_f32(m.row_gap, hasher);
    hash_opt_f32(m.column_gap, hasher);
    hash_opt_f32(m.margin_top, hasher);
    hash_opt_f32(m.margin_left, hasher);
    hash_opt_f32(m.margin_right, hasher);
    hash_opt_f32(m.margin_bottom, hasher);

    // Layers / custom paint / custom layout
    hash_opt_f32(m.graphics_layer, hasher);
    m.painter.is_some().hash(hasher);
    m.paint_callback.is_some().hash(hasher);
    m.layout.is_some().hash(hasher);
    if let Some(sh) = &m.shadow {
        hash_f32(sh.blur_radius, hasher);
        hash_f32(sh.offset_y, hasher);
        hash_color(&sh.color, hasher);
    }

    // Side effects / a11y / cursor (callbacks stay presence-only)
    (m.on_globally_positioned.is_some()).hash(hasher);
    (m.on_size_changed.is_some()).hash(hasher);
    if let Some(sem) = &m.semantics {
        std::mem::discriminant(&sem.role).hash(hasher);
        sem.label.hash(hasher);
        sem.focused.hash(hasher);
        sem.enabled.hash(hasher);
        sem.selectable_group.hash(hasher);
    }
    if let Some(c) = &m.cursor {
        std::mem::discriminant(c).hash(hasher);
    }

    // Text input (presence + stable flags)
    if let Some(ti) = &m.text_input {
        true.hash(hasher);
        ti.multiline.hash(hasher);
    } else {
        false.hash(hasher);
    }

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
        hash_f32(se.default, hasher);
        hash_f32(se.hovered, hasher);
        hash_f32(se.pressed, hasher);
        hash_f32(se.disabled, hasher);
    }

    if let Some(spec) = &m.animate_content_size {
        hash_animation_spec(spec, hasher);
    }

    m.focus_requester.is_some().hash(hasher);
    m.interaction_source.is_some().hash(hasher);

    m.on_focus_changed.is_some().hash(hasher);
}

fn hash_animation_spec(spec: &AnimationSpec, hasher: &mut impl Hasher) {
    spec.duration.as_millis().hash(hasher);
    hash_easing(&spec.easing, hasher);
    spec.delay.as_millis().hash(hasher);
    if let Some(spring) = &spec.spring {
        hash_f32(spring.damping_ratio, hasher);
        hash_f32(spring.stiffness, hasher);
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
        hash_f32(*omega, hasher);
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
            hash_f32(start.x, hasher);
            hash_f32(start.y, hasher);
            hash_f32(end.x, hasher);
            hash_f32(end.y, hasher);
            hash_color(start_color, hasher);
            hash_color(end_color, hasher);
        }
        _ => {} // Future Brush variants
    }
}

fn hash_text_overflow(o: &TextOverflow, hasher: &mut impl Hasher) {
    std::mem::discriminant(o).hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::{FontStyle, FontWeight, Modifier, TextAlign, TextDecoration, View, ViewKind};

    #[test]
    fn test_same_view_same_hash() {
        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0));
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0));

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
    }

    #[test]
    fn test_scroll_presence_changes_hash() {
        use repose_core::scroll::ScrollAxisBinding;

        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new());
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().vertical_scroll(
            ScrollAxisBinding {
                show_scrollbar: true,
                ..Default::default()
            },
        ));

        assert_ne!(hash_view_content(&v1), hash_view_content(&v2));
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
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                url: None,
                font_variation_settings: None,
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
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                url: None,
                font_variation_settings: None,
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
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                url: None,
                font_variation_settings: None,
            },
        );

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
        assert_ne!(hash_view_content(&v1), hash_view_content(&v3));
    }
}
