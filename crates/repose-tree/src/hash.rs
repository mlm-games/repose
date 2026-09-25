//! Content hashing for change detection.

use rapidhash::fast::RapidHasher;
use repose_core::{
    Brush, Color, Dp, Modifier, Px, Sp, TextOverflow, View, ViewKind,
    animation::{AnimationSpec, Easing},
    scroll::ScrollBinding,
    text::{Shadow, SpanStyle, TextStyle},
};
use std::hash::{Hash, Hasher};

fn hash_view_content_inner(view: &View, hasher: &mut impl Hasher) {
    view.id.hash(hasher);
    hash_view_kind(&view.kind, hasher);
    hash_modifier(&view.modifier, hasher);
    view.scope_key.hash(hasher);
    if let Some(semantics) = &view.semantics {
        1u8.hash(hasher);
        std::mem::discriminant(&semantics.role).hash(hasher);
        semantics.label.hash(hasher);
        semantics.focused.hash(hasher);
        semantics.enabled.hash(hasher);
        semantics.selectable_group.hash(hasher);
        semantics.checked.hash(hasher);
        semantics.selected.hash(hasher);
        semantics.value.hash(hasher);
    } else {
        0u8.hash(hasher);
    }
    match view.modifier.key {
        Some(key) => {
            1u8.hash(hasher);
            key.hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

pub fn hash_view_content(view: &View) -> u64 {
    let mut hasher = RapidHasher::default();
    hash_view_content_inner(view, &mut hasher);
    hasher.finish()
}

pub fn hash_subtree(content_hash: u64, children_hashes: &[u64]) -> u64 {
    let mut hasher = RapidHasher::default();
    content_hash.hash(&mut hasher);
    children_hashes.len().hash(&mut hasher);
    for hash in children_hashes {
        hash.hash(&mut hasher);
    }
    hasher.finish()
}

fn hash_opt_color(color: Option<&Color>, hasher: &mut impl Hasher) {
    match color {
        Some(color) => {
            1u8.hash(hasher);
            hash_color(color, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_path_effect(path_effect: &Option<repose_core::PathEffect>, hasher: &mut impl Hasher) {
    match path_effect {
        Some(repose_core::PathEffect::Corner { radius }) => {
            0u8.hash(hasher);
            hash_f32(*radius, hasher);
        }
        Some(repose_core::PathEffect::Dash { intervals, phase }) => {
            1u8.hash(hasher);
            intervals.len().hash(hasher);
            for interval in intervals {
                hash_f32(*interval, hasher);
            }
            hash_f32(*phase, hasher);
        }
        None => 2u8.hash(hasher),
    }
}

fn hash_draw_style(style: &repose_core::DrawStyle, hasher: &mut impl Hasher) {
    match style {
        repose_core::DrawStyle::Fill => 0u8.hash(hasher),
        repose_core::DrawStyle::Stroke {
            width,
            cap,
            join,
            miter,
            path_effect,
        }
        | repose_core::DrawStyle::FillAndStroke {
            width,
            cap,
            join,
            miter,
            path_effect,
        } => {
            std::mem::discriminant(style).hash(hasher);
            hash_f32(*width, hasher);
            (*cap as u8).hash(hasher);
            (*join as u8).hash(hasher);
            hash_f32(*miter, hasher);
            hash_path_effect(path_effect, hasher);
        }
    }
}

fn hash_text_decoration(decoration: &repose_core::TextDecoration, hasher: &mut impl Hasher) {
    decoration.underline.hash(hasher);
    decoration.strikethrough.hash(hasher);
    hash_opt_color(decoration.color.as_ref(), hasher);
}

fn hash_shadow(shadow: &Shadow, hasher: &mut impl Hasher) {
    hash_color(&shadow.color, hasher);
    hash_dp(shadow.offset_x, hasher);
    hash_dp(shadow.offset_y, hasher);
    hash_dp(shadow.blur_radius, hasher);
}

fn hash_text_style(style: &TextStyle, hasher: &mut impl Hasher) {
    hash_sp(style.font_size, hasher);
    hash_opt_color(style.color.as_ref(), hasher);
    style.font_weight.hash(hasher);
    style.font_family.hash(hasher);
    style.font_style.hash(hasher);
    style.text_align.hash(hasher);
    hash_sp(style.letter_spacing, hasher);
    hash_sp(style.line_height, hasher);
    hash_opt_color(style.background.as_ref(), hasher);
    match &style.text_decoration {
        Some(decoration) => {
            1u8.hash(hasher);
            hash_text_decoration(decoration, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &style.shadow {
        Some(shadow) => {
            1u8.hash(hasher);
            hash_shadow(shadow, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_discriminant_option(&style.text_direction, hasher);
    std::mem::discriminant(&style.font_synthesis).hash(hasher);
    hash_f32(style.baseline_shift.0, hasher);
    std::mem::discriminant(&style.hyphens).hash(hasher);
    std::mem::discriminant(&style.line_break).hash(hasher);
    match &style.text_indent {
        Some(indent) => {
            1u8.hash(hasher);
            hash_dp(indent.first_line, hasher);
            hash_dp(indent.rest_lines, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_draw_style(&style.draw_style, hasher);
    hash_f32(style.alpha, hasher);
    style.locale_list.hash(hasher);
    style.font_feature_settings.hash(hasher);
    style.font_variation_settings.hash(hasher);
}

fn hash_span_style(style: &SpanStyle, hasher: &mut impl Hasher) {
    hash_opt_color(style.color.as_ref(), hasher);
    match style.font_size {
        Some(size) => {
            1u8.hash(hasher);
            hash_sp(size, hasher);
        }
        None => 0u8.hash(hasher),
    }
    style.font_weight.hash(hasher);
    style.font_family.hash(hasher);
    style.font_style.hash(hasher);
    style.text_align.hash(hasher);
    match style.letter_spacing {
        Some(value) => {
            1u8.hash(hasher);
            hash_sp(value, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match style.line_height {
        Some(value) => {
            1u8.hash(hasher);
            hash_sp(value, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_color(style.background.as_ref(), hasher);
    match &style.text_decoration {
        Some(decoration) => {
            1u8.hash(hasher);
            hash_text_decoration(decoration, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_discriminant_option(&style.text_direction, hasher);
    hash_discriminant_option(&style.font_synthesis, hasher);
    match style.baseline_shift {
        Some(value) => {
            1u8.hash(hasher);
            hash_f32(value.0, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_discriminant_option(&style.hyphens, hasher);
    hash_discriminant_option(&style.line_break, hasher);
    match &style.text_indent {
        Some(indent) => {
            1u8.hash(hasher);
            hash_dp(indent.first_line, hasher);
            hash_dp(indent.rest_lines, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &style.draw_style {
        Some(style) => {
            1u8.hash(hasher);
            hash_draw_style(style, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_f32(style.alpha, hasher);
    style.font_variation_settings.hash(hasher);
}

fn hash_view_kind(kind: &ViewKind, hasher: &mut impl Hasher) {
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
            url,
            font_variation_settings,
            draw_style,
        } => {
            text.hash(hasher);
            hash_color(color, hasher);
            hash_sp(*font_size, hasher);
            soft_wrap.hash(hasher);
            max_lines.hash(hasher);
            hash_text_overflow(overflow, hasher);
            font_family.hash(hasher);
            text_align.hash(hasher);
            font_weight.0.hash(hasher);
            font_style.hash(hasher);
            hash_text_decoration(text_decoration, hasher);
            hash_sp(*letter_spacing, hasher);
            hash_sp(*line_height, hasher);
            url.hash(hasher);
            font_variation_settings.hash(hasher);
            hash_draw_style(draw_style, hasher);
            match annotations {
                Some(annotations) => {
                    1u8.hash(hasher);
                    annotations.len().hash(hasher);
                    for span in annotations.iter() {
                        span.start.hash(hasher);
                        span.end.hash(hasher);
                        hash_span_style(&span.style, hasher);
                        match &span.url {
                            Some(url) => {
                                1u8.hash(hasher);
                                url.hash(hasher);
                            }
                            None => 0u8.hash(hasher),
                        }
                    }
                }
                None => 0u8.hash(hasher),
            }
        }
        ViewKind::Image {
            handle,
            tint,
            fit,
            filter,
            source_rect,
        } => {
            handle.hash(hasher);
            hash_color(tint, hasher);
            std::mem::discriminant(fit).hash(hasher);
            std::mem::discriminant(filter).hash(hasher);
            source_rect.hash(hasher);
        }
        ViewKind::OverlayHost
        | ViewKind::Box
        | ViewKind::Row
        | ViewKind::Column
        | ViewKind::ZStack
        | ViewKind::SubcomposeLayout { .. } => {}
        _ => {}
    }
}

fn hash_discriminant_option<T>(value: &Option<T>, hasher: &mut impl Hasher) {
    match value {
        Some(value) => {
            1u8.hash(hasher);
            std::mem::discriminant(value).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_f32(v: f32, hasher: &mut impl Hasher) {
    v.to_bits().hash(hasher);
}

fn hash_rc_identity<T: ?Sized>(value: &Option<std::rc::Rc<T>>, hasher: &mut impl Hasher) {
    value.is_some().hash(hasher);
}

fn hash_control_visual(visual: &repose_core::ControlVisual, hasher: &mut impl Hasher) {
    std::mem::discriminant(visual).hash(hasher);
    match visual {
        repose_core::ControlVisual::Rect { brush, radius } => {
            hash_brush(brush, hasher);
            for value in radius {
                value.0.to_bits().hash(hasher);
            }
        }
        repose_core::ControlVisual::Image {
            handle,
            source_rect,
            tint,
            fit,
            filter,
        } => {
            handle.hash(hasher);
            source_rect.hash(hasher);
            hash_color(tint, hasher);
            std::mem::discriminant(fit).hash(hasher);
            std::mem::discriminant(filter).hash(hasher);
        }
        repose_core::ControlVisual::Custom(_) => {
            visual.custom_identity().unwrap_or_default().hash(hasher)
        }
        _ => {}
    }
}

fn hash_control_visual_set(set: &repose_core::ControlVisualSet, hasher: &mut impl Hasher) {
    for visual in [
        &set.normal,
        &set.hovered,
        &set.pressed,
        &set.dragged,
        &set.focused,
        &set.disabled,
    ] {
        match visual {
            Some(visual) => {
                true.hash(hasher);
                hash_control_visual(visual, hasher);
            }
            None => false.hash(hasher),
        }
    }
}

fn hash_scrollbar_style(style: &repose_core::ScrollbarStyle, hasher: &mut impl Hasher) {
    style.thickness.0.to_bits().hash(hasher);
    style.track_inset.0.to_bits().hash(hasher);
    style.min_thumb_length.0.to_bits().hash(hasher);
    style
        .fixed_thumb_length
        .map(|value| value.0.to_bits())
        .hash(hasher);
    style.radius.map(|value| value.0.to_bits()).hash(hasher);
    hash_control_visual_set(&style.track_visuals, hasher);
    hash_control_visual_set(&style.thumb_visuals, hasher);
}

fn hash_scroll_axis_binding(
    binding: &repose_core::scroll::ScrollAxisBinding,
    hasher: &mut impl Hasher,
) {
    binding.show_scrollbar.hash(hasher);
    hash_rc_identity(&binding.on_scroll, hasher);
    hash_rc_identity(&binding.set_viewport_main, hasher);
    hash_rc_identity(&binding.set_content_main, hasher);
    hash_rc_identity(&binding.get_offset_main, hasher);
    hash_rc_identity(&binding.set_offset_main, hasher);
    hash_rc_identity(&binding.tick, hasher);
    hash_rc_identity(&binding.set_nested_scroll_parent, hasher);
}

fn hash_scroll_binding(binding: &ScrollBinding, hasher: &mut impl Hasher) {
    match binding {
        ScrollBinding::Vertical(binding) => {
            1u8.hash(hasher);
            hash_scroll_axis_binding(binding, hasher);
        }
        ScrollBinding::Horizontal(binding) => {
            2u8.hash(hasher);
            hash_scroll_axis_binding(binding, hasher);
        }
        ScrollBinding::Both(binding) => {
            3u8.hash(hasher);
            binding.show_scrollbar.hash(hasher);
            hash_rc_identity(&binding.on_scroll, hasher);
            hash_rc_identity(&binding.set_viewport_width, hasher);
            hash_rc_identity(&binding.set_viewport_height, hasher);
            hash_rc_identity(&binding.set_content_width, hasher);
            hash_rc_identity(&binding.set_content_height, hasher);
            hash_rc_identity(&binding.get_offset_xy, hasher);
            hash_rc_identity(&binding.set_offset_xy, hasher);
            hash_rc_identity(&binding.tick, hasher);
            hash_rc_identity(&binding.set_nested_scroll_parent, hasher);
        }
    }
}

fn hash_cursor(cursor: &Option<repose_core::CursorIcon>, hasher: &mut impl Hasher) {
    match cursor {
        Some(repose_core::CursorIcon::Custom(image)) => {
            10u8.hash(hasher);
            image.size.hash(hasher);
            image.hotspot.hash(hasher);
            image.rgba.hash(hasher);
        }
        Some(cursor) => {
            1u8.hash(hasher);
            std::mem::discriminant(cursor).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_opt_f32(v: Option<f32>, hasher: &mut impl Hasher) {
    match v {
        Some(value) => {
            1u8.hash(hasher);
            hash_f32(value, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_dp(v: Dp, hasher: &mut impl Hasher) {
    hash_f32(v.0, hasher);
}

fn hash_opt_dp(v: Option<Dp>, hasher: &mut impl Hasher) {
    match v {
        Some(value) => {
            1u8.hash(hasher);
            hash_dp(value, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_sp(v: Sp, hasher: &mut impl Hasher) {
    hash_f32(v.0, hasher);
}

#[allow(dead_code)]
fn hash_px(v: Px, hasher: &mut impl Hasher) {
    hash_f32(v.0, hasher);
}

fn hash_modifier(m: &Modifier, hasher: &mut impl Hasher) {
    match m.size {
        Some(size) => {
            1u8.hash(hasher);
            hash_dp(size.width, hasher);
            hash_dp(size.height, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_dp(m.width, hasher);
    hash_opt_dp(m.height, hasher);
    match m.required_size {
        Some(size) => {
            1u8.hash(hasher);
            hash_dp(size.width, hasher);
            hash_dp(size.height, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_dp(m.required_min_width, hasher);
    hash_opt_dp(m.required_max_width, hasher);
    hash_opt_dp(m.required_min_height, hasher);
    hash_opt_dp(m.required_max_height, hasher);
    hash_opt_dp(m.default_min_width, hasher);
    hash_opt_dp(m.default_min_height, hasher);
    hash_opt_f32(m.fill_max, hasher);
    hash_opt_f32(m.fill_max_w, hasher);
    hash_opt_f32(m.fill_max_h, hasher);
    m.repaint_boundary.hash(hasher);
    hash_opt_dp(m.padding, hasher);
    match m.padding_values {
        Some(padding) => {
            1u8.hash(hasher);
            hash_dp(padding.left, hasher);
            hash_dp(padding.right, hasher);
            hash_dp(padding.top, hasher);
            hash_dp(padding.bottom, hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_dp(m.min_width, hasher);
    hash_opt_dp(m.min_height, hasher);
    hash_opt_dp(m.max_width, hasher);
    hash_opt_dp(m.max_height, hasher);
    match &m.background {
        Some(background) => {
            1u8.hash(hasher);
            hash_brush(background, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &m.border {
        Some(border) => {
            1u8.hash(hasher);
            hash_dp(border.width, hasher);
            hash_brush(&border.brush, hasher);
            for &radius in &border.radius {
                hash_dp(radius, hasher);
            }
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_f32(m.flex_grow, hasher);
    hash_opt_f32(m.flex_shrink, hasher);
    hash_opt_dp(m.flex_basis, hasher);
    m.flex_wrap
        .map(|value| std::mem::discriminant(&value))
        .hash(hasher);
    m.flex_basis_content.hash(hasher);
    m.flex_line_count.hash(hasher);
    m.flex_dir
        .map(|value| std::mem::discriminant(&value))
        .hash(hasher);
    m.align_self
        .map(|value| (value.keyword as u8, value.safety as u8))
        .hash(hasher);
    m.justify_content
        .map(|value| (value.keyword as u8, value.safety as u8))
        .hash(hasher);
    m.align_items_container
        .map(|value| (value.keyword as u8, value.safety as u8))
        .hash(hasher);
    m.align_content
        .map(|value| (value.keyword as u8, value.safety as u8))
        .hash(hasher);
    m.baseline_align.hash(hasher);
    match m.clip_rounded {
        Some(radius) => {
            1u8.hash(hasher);
            for value in radius {
                hash_dp(value, hasher);
            }
        }
        None => 0u8.hash(hasher),
    }
    match m.transform {
        Some(transform) => {
            1u8.hash(hasher);
            hash_f32(transform.translate_x, hasher);
            hash_f32(transform.translate_y, hasher);
            hash_f32(transform.scale_x, hasher);
            hash_f32(transform.scale_y, hasher);
            hash_f32(transform.rotate, hasher);
            hash_f32(transform.shear_x, hasher);
            hash_f32(transform.shear_y, hasher);
            hash_f32(transform.origin_x, hasher);
            hash_f32(transform.origin_y, hasher);
            for value in transform.perspective {
                hash_f32(value, hasher);
            }
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_f32(m.alpha, hasher);
    m.position_type
        .map(|value| std::mem::discriminant(&value))
        .hash(hasher);
    hash_opt_dp(m.offset_left, hasher);
    hash_opt_dp(m.offset_right, hasher);
    hash_opt_dp(m.offset_top, hasher);
    hash_opt_dp(m.offset_bottom, hasher);
    match &m.grid {
        Some(grid) => {
            1u8.hash(hasher);
            grid.columns.hash(hasher);
            hash_dp(grid.row_gap, hasher);
            hash_dp(grid.column_gap, hasher);
        }
        None => 0u8.hash(hasher),
    }
    m.grid_col_span.hash(hasher);
    m.grid_row_span.hash(hasher);
    hash_opt_f32(m.aspect_ratio, hasher);
    m.intrinsic_width.hash(hasher);
    m.intrinsic_height.hash(hasher);
    hash_opt_dp(m.fit_content_width, hasher);
    hash_opt_dp(m.fit_content_height, hasher);
    match m.contain {
        None => 0u8.hash(hasher),
        Some(value) if value == taffy::Contain::CONTENT => 3u8.hash(hasher),
        Some(value) if value == taffy::Contain::LAYOUT => 1u8.hash(hasher),
        Some(value) if value == taffy::Contain::PAINT => 2u8.hash(hasher),
        Some(_) => 4u8.hash(hasher),
    }
    hash_f32(m.z_index, hasher);
    hash_opt_f32(m.render_z_index, hasher);
    m.input_blocker.hash(hasher);
    m.click.hash(hasher);
    m.disabled.hash(hasher);
    m.focusable.hash(hasher);
    m.hit_passthrough.hash(hasher);
    m.propagate_min.hash(hasher);
    m.focus_group.hash(hasher);
    match &m.blur {
        Some(blur) => {
            1u8.hash(hasher);
            hash_dp(blur.radius_x, hasher);
            hash_dp(blur.radius_y, hasher);
            std::mem::discriminant(&blur.edge_treatment).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &m.scroll {
        Some(binding) => hash_scroll_binding(binding, hasher),
        None => 0u8.hash(hasher),
    }
    hash_scrollbar_style(&m.scrollbar_style, hasher);
    m.overflow
        .map(|value| std::mem::discriminant(&value))
        .hash(hasher);
    match m.clip_rect {
        Some(rect) => {
            1u8.hash(hasher);
            hash_dp(rect.left, hasher);
            hash_dp(rect.top, hasher);
            hash_dp(rect.right, hasher);
            hash_dp(rect.bottom, hasher);
            std::mem::discriminant(&rect.op).hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_opt_dp(m.gap, hasher);
    hash_opt_dp(m.row_gap, hasher);
    hash_opt_dp(m.column_gap, hasher);
    hash_opt_dp(m.margin_top, hasher);
    hash_opt_dp(m.margin_left, hasher);
    hash_opt_dp(m.margin_right, hasher);
    hash_opt_dp(m.margin_bottom, hasher);
    hash_opt_f32(m.graphics_layer, hasher);
    match &m.shadow {
        Some(shadow) => {
            1u8.hash(hasher);
            hash_dp(shadow.blur_radius, hasher);
            hash_dp(shadow.offset_y, hasher);
            hash_color(&shadow.color, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &m.semantics {
        Some(semantics) => {
            1u8.hash(hasher);
            std::mem::discriminant(&semantics.role).hash(hasher);
            semantics.label.hash(hasher);
            semantics.focused.hash(hasher);
            semantics.enabled.hash(hasher);
            semantics.selectable_group.hash(hasher);
            semantics.checked.hash(hasher);
            semantics.selected.hash(hasher);
            semantics.value.hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
    hash_cursor(&m.cursor, hasher);
    match &m.text_input {
        Some(input) => {
            1u8.hash(hasher);
            input.hint.hash(hasher);
            input.multiline.hash(hasher);
            input.value.hash(hasher);
            input.enabled.hash(hasher);
            input.read_only.hash(hasher);
            input.sensitive.hash(hasher);
            input.max_lines.hash(hasher);
            input.min_lines.hash(hasher);
            std::mem::discriminant(&input.keyboard_type).hash(hasher);
            std::mem::discriminant(&input.capitalization).hash(hasher);
            std::mem::discriminant(&input.ime_action).hash(hasher);
            input.auto_correct_enabled.hash(hasher);
            match input.cursor_color {
                Some(color) => {
                    1u8.hash(hasher);
                    hash_color(&color, hasher);
                }
                None => 0u8.hash(hasher),
            }
            match &input.text_style {
                Some(style) => {
                    1u8.hash(hasher);
                    hash_text_style(style, hasher);
                }
                None => 0u8.hash(hasher),
            }
            match &input.line_limits {
                Some(repose_core::text::TextFieldLineLimits::SingleLine) => 1u8.hash(hasher),
                Some(repose_core::text::TextFieldLineLimits::MultiLine {
                    min_height_in_lines,
                    max_height_in_lines,
                }) => {
                    2u8.hash(hasher);
                    min_height_in_lines.hash(hasher);
                    max_height_in_lines.hash(hasher);
                }
                None => 0u8.hash(hasher),
            }
        }
        None => 0u8.hash(hasher),
    }
    match &m.state_colors {
        Some(colors) => {
            1u8.hash(hasher);
            hash_color(&colors.default, hasher);
            hash_color(&colors.hovered, hasher);
            hash_color(&colors.focused, hasher);
            hash_color(&colors.pressed, hasher);
            hash_color(&colors.disabled, hasher);
            hash_color(&colors.dragged, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &m.state_elevation {
        Some(elevation) => {
            1u8.hash(hasher);
            hash_dp(elevation.default, hasher);
            hash_dp(elevation.hovered, hasher);
            hash_dp(elevation.focused, hasher);
            hash_dp(elevation.pressed, hasher);
            hash_dp(elevation.disabled, hasher);
            hash_dp(elevation.dragged, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &m.animate_content_size {
        Some(spec) => {
            1u8.hash(hasher);
            hash_animation_spec(spec, hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_animation_spec(spec: &AnimationSpec, hasher: &mut impl Hasher) {
    spec.duration.as_nanos().hash(hasher);
    hash_easing(&spec.easing, hasher);
    spec.delay.as_nanos().hash(hasher);
    match &spec.spring {
        Some(spring) => {
            1u8.hash(hasher);
            hash_f32(spring.damping_ratio, hasher);
            hash_f32(spring.stiffness, hasher);
            hash_f32(spring.settle_progress, hasher);
            hash_f32(spring.settle_velocity, hasher);
        }
        None => 0u8.hash(hasher),
    }
    match &spec.repeat {
        Some(repeat) => {
            1u8.hash(hasher);
            repeat.iterations.hash(hasher);
            repeat.reverse.hash(hasher);
            repeat.delay_between.as_nanos().hash(hasher);
        }
        None => 0u8.hash(hasher),
    }
}

fn hash_easing(easing: &Easing, hasher: &mut impl Hasher) {
    std::mem::discriminant(easing).hash(hasher);
    match easing {
        Easing::SpringCrit { omega } => hash_f32(*omega, hasher),
        Easing::Custom(cb) => {
            hash_f32(cb.p1x, hasher);
            hash_f32(cb.p1y, hasher);
            hash_f32(cb.p2x, hasher);
            hash_f32(cb.p2y, hasher);
        }
        _ => {}
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
        Brush::LinearNormalized {
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
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => {
            hash_f32(center.x, hasher);
            hash_f32(center.y, hasher);
            hash_f32(*radius, hasher);
            hash_color(start_color, hasher);
            hash_color(end_color, hasher);
        }
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => {
            hash_f32(center.x, hasher);
            hash_f32(center.y, hasher);
            hash_color(start_color, hasher);
            hash_color(end_color, hasher);
        }
        _ => {}
    }
}

fn hash_text_overflow(o: &TextOverflow, hasher: &mut impl Hasher) {
    std::mem::discriminant(o).hash(hasher);
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::{
        DrawStyle, FontStyle, FontWeight, ImageFilter, ImageFit, ImageSourceRect, Modifier,
        TextAlign, TextDecoration, UnitExt, View, ViewKind,
    };

    #[test]
    fn test_same_view_same_hash() {
        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0.dp()));
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0.dp()));

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
    }

    #[test]
    fn image_source_rect_changes_hash() {
        let image = |source_rect| {
            View::new(
                0,
                ViewKind::Image {
                    handle: 1,
                    tint: Color::WHITE,
                    fit: ImageFit::FillBounds,
                    filter: ImageFilter::Nearest,
                    source_rect,
                },
            )
        };
        let full = image(None);
        let first = image(Some(ImageSourceRect::new(0, 0, 8, 8)));
        let second = image(Some(ImageSourceRect::new(8, 0, 8, 8)));
        assert_ne!(hash_view_content(&full), hash_view_content(&first));
        assert_ne!(hash_view_content(&first), hash_view_content(&second));
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
        let v1 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(100.0.dp()));
        let v2 = View::new(0, ViewKind::Box).modifier(Modifier::new().width(200.0.dp()));

        assert_ne!(hash_view_content(&v1), hash_view_content(&v2));
    }

    #[test]
    fn test_text_content_hash() {
        let v1 = View::new(
            0,
            ViewKind::Text {
                text: "Hello".to_string(),
                color: Color::WHITE,
                font_size: 16.0.sp(),
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: Sp::ZERO,
                line_height: Sp::ZERO,
                url: None,
                font_variation_settings: None,
                draw_style: DrawStyle::Fill,
            },
        );
        let v2 = View::new(
            0,
            ViewKind::Text {
                text: "Hello".to_string(),
                color: Color::WHITE,
                font_size: 16.0.sp(),
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: Sp::ZERO,
                line_height: Sp::ZERO,
                url: None,
                font_variation_settings: None,
                draw_style: DrawStyle::Fill,
            },
        );
        let v3 = View::new(
            0,
            ViewKind::Text {
                text: "World".to_string(),
                color: Color::WHITE,
                font_size: 16.0.sp(),
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: Sp::ZERO,
                line_height: Sp::ZERO,
                url: None,
                font_variation_settings: None,
                draw_style: DrawStyle::Fill,
            },
        );

        assert_eq!(hash_view_content(&v1), hash_view_content(&v2));
        assert_ne!(hash_view_content(&v1), hash_view_content(&v3));
    }

    fn text_view_with_url(url: Option<&str>) -> View {
        View::new(
            0,
            ViewKind::Text {
                text: "link".to_string(),
                color: Color::WHITE,
                font_size: 16.0.sp(),
                soft_wrap: true,
                max_lines: None,
                overflow: TextOverflow::Visible,
                font_family: None,
                annotations: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: Sp::ZERO,
                line_height: Sp::ZERO,
                url: url.map(|u| std::sync::Arc::from(u)),
                font_variation_settings: None,
                draw_style: DrawStyle::Fill,
            },
        )
    }

    #[test]
    fn view_id_and_span_baseline_change_hash() {
        let first = View::new(1, ViewKind::Box);
        let second = View::new(2, ViewKind::Box);
        assert_ne!(hash_view_content(&first), hash_view_content(&second));

        let mut a = text_view_with_url(None);
        let mut b = a.clone();
        let span = repose_core::TextSpan {
            start: 0,
            end: 4,
            style: repose_core::SpanStyle::default()
                .baseline_shift(repose_core::BaselineShift::Superscript),
            url: None,
        };
        if let ViewKind::Text { annotations, .. } = &mut a.kind {
            *annotations = Some(std::sync::Arc::from([span.clone()]));
        }
        if let ViewKind::Text { annotations, .. } = &mut b.kind {
            *annotations = Some(std::sync::Arc::from([repose_core::TextSpan {
                style: repose_core::SpanStyle::default()
                    .baseline_shift(repose_core::BaselineShift::Subscript),
                ..span
            }]));
        }
        assert_ne!(hash_view_content(&a), hash_view_content(&b));
    }

    #[test]
    fn test_text_url_change_invalidates() {
        let a = text_view_with_url(None);
        let b = text_view_with_url(Some("https://a.example"));
        let c = text_view_with_url(Some("https://b.example"));
        assert_ne!(hash_view_content(&a), hash_view_content(&b));
        assert_ne!(hash_view_content(&b), hash_view_content(&c));
    }

    #[test]
    fn test_semantics_state_invalidates() {
        let mk = |checked: Option<bool>| {
            View::new(0, ViewKind::Box).semantics(
                repose_core::Semantics::new(repose_core::Role::Checkbox)
                    .with_checked(checked.unwrap_or(false)),
            )
        };
        assert_ne!(
            hash_view_content(&mk(Some(true))),
            hash_view_content(&mk(Some(false)))
        );
        let m1 = View::new(0, ViewKind::Box).modifier(
            Modifier::new().semantics(repose_core::Semantics::new(repose_core::Role::Button)),
        );
        let m2 = View::new(0, ViewKind::Box).modifier(
            Modifier::new()
                .semantics(repose_core::Semantics::new(repose_core::Role::Button).with_value("v")),
        );
        assert_ne!(hash_view_content(&m1), hash_view_content(&m2));
    }

    #[test]
    fn test_then_preserves_layer_fields() {
        let base = Modifier::new().width(10.0.dp());
        let overlay = Modifier::new()
            .blur(4.0.dp())
            .shadow_with_color(8.0.dp(), 2.0.dp(), Color::BLACK)
            .graphics_layer(0.5);
        let merged = base.then(overlay);
        assert!(merged.blur.is_some());
        assert!(merged.shadow.is_some());
        assert!(merged.graphics_layer.is_some());
        assert_eq!(
            Modifier::new()
                .z_index(5.0)
                .then(Modifier::new().z_index(0.0))
                .z_index,
            0.0
        );
    }
}
