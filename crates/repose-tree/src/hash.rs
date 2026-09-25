//! Content hashing for change detection.

use rapidhash::fast::RapidHasher;
use repose_core::{
    Brush, Color, Dp, Modifier, Px, Sp, TextOverflow, View, ViewKind,
    animation::{AnimationSpec, Easing},
    scroll::ScrollBinding,
    text::{Shadow, SpanStyle, TextStyle},
};
use std::hash::{Hash, Hasher};

#[allow(dead_code)]
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
    hash_view_facets(view).combined()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ViewHashes {
    pub layout: u64,
    pub measurement: u64,
    pub paint: u64,
    pub semantics: u64,
}

impl ViewHashes {
    pub fn combined(self) -> u64 {
        let mut hasher = RapidHasher::default();
        0x5245_504f_5345_2d48u64.hash(&mut hasher);
        self.layout.hash(&mut hasher);
        self.measurement.hash(&mut hasher);
        self.paint.hash(&mut hasher);
        self.semantics.hash(&mut hasher);
        hasher.finish()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SubtreeHashes {
    pub layout: u64,
    pub measurement: u64,
    pub paint: u64,
    pub semantics: u64,
}

impl SubtreeHashes {
    pub fn combined(self) -> u64 {
        let mut hasher = RapidHasher::default();
        0x5245_504f_5345_2d53u64.hash(&mut hasher);
        self.layout.hash(&mut hasher);
        self.measurement.hash(&mut hasher);
        self.paint.hash(&mut hasher);
        self.semantics.hash(&mut hasher);
        hasher.finish()
    }
}

pub(crate) struct SubtreeHashBuilder {
    layout: RapidHasher<'static>,
    measurement: RapidHasher<'static>,
    paint: RapidHasher<'static>,
    semantics: RapidHasher<'static>,
}

impl SubtreeHashBuilder {
    pub fn new(child_count: usize) -> Self {
        let mut builder = Self {
            layout: RapidHasher::default(),
            measurement: RapidHasher::default(),
            paint: RapidHasher::default(),
            semantics: RapidHasher::default(),
        };
        builder.layout.write_u64(0x5355_4254_5245_45);
        builder.measurement.write_u64(0x534d_4541_5355_5245);
        builder.paint.write_u64(0x5041_494e_545f_5355);
        builder.semantics.write_u64(0x5345_4d41_4e54_4943);
        builder.layout.write_usize(child_count);
        builder.measurement.write_usize(child_count);
        builder.paint.write_usize(child_count);
        builder.semantics.write_usize(child_count);
        builder
    }

    pub fn push(&mut self, child: SubtreeHashes) {
        self.layout.write_u64(child.layout);
        self.measurement.write_u64(child.measurement);
        self.paint.write_u64(child.paint);
        self.semantics.write_u64(child.semantics);
    }

    pub fn finish(mut self, content: ViewHashes) -> SubtreeHashes {
        self.layout.write_u64(content.layout);
        self.measurement.write_u64(content.measurement);
        self.paint.write_u64(content.paint);
        self.semantics.write_u64(content.semantics);
        SubtreeHashes {
            layout: self.layout.finish(),
            measurement: self.measurement.finish(),
            paint: self.paint.finish(),
            semantics: self.semantics.finish(),
        }
    }
}

const FACET_LAYOUT: u8 = 1;
const FACET_MEASUREMENT: u8 = 2;
const FACET_PAINT: u8 = 4;
const FACET_SEMANTICS: u8 = 8;
const FACET_LAYOUT_MEASUREMENT: u8 = FACET_LAYOUT | FACET_MEASUREMENT;
const FACET_LAYOUT_PAINT: u8 = FACET_LAYOUT | FACET_PAINT;
const FACET_LAYOUT_MEASUREMENT_PAINT: u8 = FACET_LAYOUT | FACET_MEASUREMENT | FACET_PAINT;
const FACET_ALL: u8 = FACET_LAYOUT | FACET_MEASUREMENT | FACET_PAINT | FACET_SEMANTICS;

type FacetHasher = RapidHasher<'static>;

struct FacetHashers {
    hashers: [FacetHasher; 4],
}

impl Default for FacetHashers {
    fn default() -> Self {
        Self {
            hashers: [
                FacetHasher::default(),
                FacetHasher::default(),
                FacetHasher::default(),
                FacetHasher::default(),
            ],
        }
    }
}

impl FacetHashers {
    fn hash(&mut self, mask: u8, write: impl Fn(&mut FacetHasher)) {
        for (index, hasher) in self.hashers.iter_mut().enumerate() {
            if mask & (1 << index) != 0 {
                write(hasher);
            }
        }
    }

    fn finish(self) -> ViewHashes {
        ViewHashes {
            layout: self.hashers[0].finish(),
            measurement: self.hashers[1].finish(),
            paint: self.hashers[2].finish(),
            semantics: self.hashers[3].finish(),
        }
    }
}

pub fn hash_view_facets(view: &View) -> ViewHashes {
    let mut hashers = FacetHashers::default();
    hashers.hash(FACET_ALL, |hasher| {
        0x5245_504f_5345_2d56u64.hash(hasher);
    });
    facet_hash_view_kind(&view.kind, &mut hashers);
    facet_hash_modifier(&view.modifier, &mut hashers);
    hashers.hash(FACET_LAYOUT_PAINT, |hasher| view.id.hash(hasher));
    hashers.hash(FACET_ALL ^ FACET_SEMANTICS, |hasher| {
        view.scope_key.hash(hasher);
    });
    facet_hash_semantics(view.semantics.as_ref(), &mut hashers, FACET_SEMANTICS);
    hashers.finish()
}

pub fn hash_view_layout(view: &View) -> u64 {
    hash_view_facets(view).layout
}

pub fn hash_view_measurement(view: &View) -> u64 {
    hash_view_facets(view).measurement
}

pub fn hash_view_paint(view: &View) -> u64 {
    hash_view_facets(view).paint
}

pub fn hash_view_semantics(view: &View) -> u64 {
    hash_view_facets(view).semantics
}

pub fn hash_subtree_facets(content: ViewHashes, children: &[SubtreeHashes]) -> SubtreeHashes {
    let mut builder = SubtreeHashBuilder::new(children.len());
    for child in children {
        builder.push(*child);
    }
    builder.finish(content)
}

fn facet_hash_view_kind(kind: &ViewKind, hashers: &mut FacetHashers) {
    hashers.hash(FACET_ALL, |hasher| {
        std::mem::discriminant(kind).hash(hasher);
    });
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
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| text.hash(h));
            hashers.hash(FACET_PAINT, |h| hash_color(color, h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| hash_sp(*font_size, h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| soft_wrap.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| max_lines.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| {
                hash_text_overflow(overflow, h)
            });
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| font_family.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| text_align.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| font_weight.0.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| font_style.hash(h));
            hashers.hash(FACET_PAINT, |h| hash_text_decoration(text_decoration, h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| hash_sp(*letter_spacing, h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| hash_sp(*line_height, h));
            hashers.hash(FACET_PAINT, |h| url.hash(h));
            hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| {
                font_variation_settings.hash(h)
            });
            hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| {
                hash_draw_style(draw_style, h)
            });
            if let Some(annotations) = annotations {
                hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| {
                    1u8.hash(h);
                    annotations.len().hash(h);
                    for span in annotations.iter() {
                        span.start.hash(h);
                        span.end.hash(h);
                    }
                });
                for span in annotations.iter() {
                    facet_hash_span_style(&span.style, hashers, FACET_ALL);
                    if let Some(url) = &span.url {
                        hashers.hash(FACET_PAINT, |h| url.hash(h));
                    } else {
                        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
                    }
                }
            } else {
                hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| 0u8.hash(h));
            }
        }
        ViewKind::Image {
            handle,
            tint,
            fit,
            filter,
            source_rect,
        } => {
            hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| handle.hash(h));
            hashers.hash(FACET_PAINT, |h| hash_color(tint, h));
            hashers.hash(FACET_PAINT, |h| std::mem::discriminant(fit).hash(h));
            hashers.hash(FACET_PAINT, |h| std::mem::discriminant(filter).hash(h));
            hashers.hash(FACET_PAINT, |h| source_rect.hash(h));
        }
        ViewKind::Box
        | ViewKind::Row
        | ViewKind::Column
        | ViewKind::ZStack
        | ViewKind::OverlayHost
        | ViewKind::SubcomposeLayout { .. } => {}
        _ => {}
    }
}

fn facet_hash_modifier(modifier: &Modifier, hashers: &mut FacetHashers) {
    macro_rules! hash_value {
        ($mask:expr, $value:expr) => {
            hashers.hash($mask, |hasher| $value.hash(hasher));
        };
    }
    macro_rules! hash_f32_value {
        ($mask:expr, $value:expr) => {
            hashers.hash($mask, |hasher| hash_f32($value, hasher));
        };
    }
    macro_rules! hash_opt_f32_value {
        ($mask:expr, $value:expr) => {
            hashers.hash($mask, |hasher| hash_opt_f32($value, hasher));
        };
    }
    macro_rules! hash_opt_dp_value {
        ($mask:expr, $value:expr) => {
            hashers.hash($mask, |hasher| hash_opt_dp($value, hasher));
        };
    }

    hash_value!(FACET_LAYOUT_PAINT, modifier.key);
    match modifier.size {
        Some(size) => {
            hashers.hash(FACET_LAYOUT, |h| {
                hash_dp(size.width, h);
                hash_dp(size.height, h);
            });
        }
        None => hashers.hash(FACET_LAYOUT, |h| 0u8.hash(h)),
    }
    hash_opt_dp_value!(FACET_LAYOUT, modifier.width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.height);
    match modifier.required_size {
        Some(size) => hashers.hash(FACET_LAYOUT, |h| {
            hash_dp(size.width, h);
            hash_dp(size.height, h);
        }),
        None => hashers.hash(FACET_LAYOUT, |h| 0u8.hash(h)),
    }
    hash_opt_dp_value!(FACET_LAYOUT, modifier.required_min_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.required_max_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.required_min_height);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.required_max_height);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.default_min_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.default_min_height);
    hash_opt_f32_value!(FACET_LAYOUT, modifier.fill_max);
    hash_opt_f32_value!(FACET_LAYOUT, modifier.fill_max_w);
    hash_opt_f32_value!(FACET_LAYOUT, modifier.fill_max_h);
    hashers.hash(FACET_PAINT, |h| modifier.repaint_boundary.hash(h));
    hash_opt_dp_value!(FACET_LAYOUT, modifier.padding);
    match modifier.padding_values {
        Some(padding) => hashers.hash(FACET_LAYOUT, |h| {
            hash_dp(padding.left, h);
            hash_dp(padding.right, h);
            hash_dp(padding.top, h);
            hash_dp(padding.bottom, h);
        }),
        None => hashers.hash(FACET_LAYOUT, |h| 0u8.hash(h)),
    }
    hash_opt_dp_value!(FACET_LAYOUT, modifier.min_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.min_height);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.max_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.max_height);
    if let Some(background) = &modifier.background {
        hashers.hash(FACET_PAINT, |h| hash_brush(background, h));
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    if let Some(border) = &modifier.border {
        hashers.hash(FACET_PAINT, |h| {
            hash_dp(border.width, h);
            hash_brush(&border.brush, h);
            for radius in border.radius {
                hash_dp(radius, h);
            }
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    hash_opt_f32_value!(FACET_LAYOUT, modifier.flex_grow);
    hash_opt_f32_value!(FACET_LAYOUT, modifier.flex_shrink);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.flex_basis);
    hashers.hash(FACET_LAYOUT, |h| {
        modifier
            .flex_wrap
            .map(|value| std::mem::discriminant(&value))
            .hash(h);
        modifier.flex_basis_content.hash(h);
        modifier.flex_line_count.hash(h);
        modifier
            .flex_dir
            .map(|value| std::mem::discriminant(&value))
            .hash(h);
    });
    hashers.hash(FACET_LAYOUT, |h| {
        modifier
            .align_self
            .map(|value| (value.keyword as u8, value.safety as u8))
            .hash(h);
        modifier
            .justify_content
            .map(|value| (value.keyword as u8, value.safety as u8))
            .hash(h);
        modifier
            .align_items_container
            .map(|value| (value.keyword as u8, value.safety as u8))
            .hash(h);
        modifier
            .align_content
            .map(|value| (value.keyword as u8, value.safety as u8))
            .hash(h);
        modifier.baseline_align.hash(h);
    });
    if let Some(radius) = modifier.clip_rounded {
        hashers.hash(FACET_PAINT, |h| {
            for value in radius {
                hash_dp(value, h);
            }
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    if let Some(transform) = &modifier.transform {
        hashers.hash(FACET_PAINT, |h| {
            hash_f32(transform.translate_x, h);
            hash_f32(transform.translate_y, h);
            hash_f32(transform.scale_x, h);
            hash_f32(transform.scale_y, h);
            hash_f32(transform.rotate, h);
            hash_f32(transform.shear_x, h);
            hash_f32(transform.shear_y, h);
            hash_f32(transform.origin_x, h);
            hash_f32(transform.origin_y, h);
            for value in transform.perspective {
                hash_f32(value, h);
            }
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    hash_opt_f32_value!(FACET_PAINT, modifier.alpha);
    hashers.hash(FACET_LAYOUT, |h| {
        modifier
            .position_type
            .map(|value| std::mem::discriminant(&value))
            .hash(h);
    });
    hash_opt_dp_value!(FACET_LAYOUT, modifier.offset_left);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.offset_right);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.offset_top);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.offset_bottom);
    if let Some(grid) = &modifier.grid {
        hashers.hash(FACET_LAYOUT, |h| {
            grid.columns.hash(h);
            hash_dp(grid.row_gap, h);
            hash_dp(grid.column_gap, h);
        });
    } else {
        hashers.hash(FACET_LAYOUT, |h| 0u8.hash(h));
    }
    hashers.hash(FACET_LAYOUT, |h| {
        modifier.grid_col_span.hash(h);
        modifier.grid_row_span.hash(h);
    });
    hash_opt_f32_value!(FACET_LAYOUT, modifier.aspect_ratio);
    hashers.hash(FACET_LAYOUT, |h| modifier.intrinsic_width.hash(h));
    hashers.hash(FACET_LAYOUT, |h| modifier.intrinsic_height.hash(h));
    hash_opt_dp_value!(FACET_LAYOUT, modifier.fit_content_width);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.fit_content_height);
    hashers.hash(FACET_LAYOUT, |h| match modifier.contain {
        None => 0u8.hash(h),
        Some(value) if value == taffy::Contain::CONTENT => 3u8.hash(h),
        Some(value) if value == taffy::Contain::LAYOUT => 1u8.hash(h),
        Some(value) if value == taffy::Contain::PAINT => 2u8.hash(h),
        Some(_) => 4u8.hash(h),
    });
    hash_f32_value!(FACET_PAINT, modifier.z_index);
    hash_opt_f32_value!(FACET_PAINT, modifier.render_z_index);
    hashers.hash(FACET_PAINT, |h| {
        modifier.input_blocker.hash(h);
        modifier.click.hash(h);
        modifier.disabled.hash(h);
        modifier.focusable.hash(h);
        modifier.hit_passthrough.hash(h);
    });
    hashers.hash(FACET_LAYOUT, |h| modifier.propagate_min.hash(h));
    hashers.hash(FACET_PAINT, |h| modifier.focus_group.hash(h));
    if let Some(blur) = &modifier.blur {
        hashers.hash(FACET_PAINT, |h| {
            hash_dp(blur.radius_x, h);
            hash_dp(blur.radius_y, h);
            std::mem::discriminant(&blur.edge_treatment).hash(h);
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    facet_hash_scroll_binding(modifier.scroll.as_ref(), hashers);
    facet_hash_nested_scroll_connection(modifier.nested_scroll_connection.as_ref(), hashers);
    facet_hash_scrollbar_style(modifier.scrollbar_style.as_ref(), hashers);
    hashers.hash(FACET_PAINT, |h| {
        modifier
            .overflow
            .map(|value| std::mem::discriminant(&value))
            .hash(h);
    });
    if let Some(rect) = modifier.clip_rect {
        hashers.hash(FACET_PAINT, |h| {
            hash_dp(rect.left, h);
            hash_dp(rect.top, h);
            hash_dp(rect.right, h);
            hash_dp(rect.bottom, h);
            std::mem::discriminant(&rect.op).hash(h);
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    hash_opt_dp_value!(FACET_LAYOUT, modifier.gap);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.row_gap);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.column_gap);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.margin_top);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.margin_left);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.margin_right);
    hash_opt_dp_value!(FACET_LAYOUT, modifier.margin_bottom);
    hash_opt_f32_value!(FACET_PAINT, modifier.graphics_layer);
    if let Some(shadow) = &modifier.shadow {
        hashers.hash(FACET_PAINT, |h| {
            hash_dp(shadow.blur_radius, h);
            hash_dp(shadow.offset_y, h);
            hash_color(&shadow.color, h);
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    facet_hash_semantics(modifier.semantics.as_ref(), hashers, FACET_SEMANTICS);
    hashers.hash(FACET_PAINT, |h| {
        hash_cursor(&modifier.cursor, h);
        modifier.painter.is_some().hash(h);
        modifier.paint_callback.is_some().hash(h);
        modifier.indication.is_some().hash(h);
        modifier.drag_preview.is_some().hash(h);
        modifier.on_scroll.is_some().hash(h);
        modifier.on_pointer_down.is_some().hash(h);
        modifier.on_pointer_move.is_some().hash(h);
        modifier.on_pointer_up.is_some().hash(h);
        modifier.on_pointer_cancel.is_some().hash(h);
        modifier.on_pointer_enter.is_some().hash(h);
        modifier.on_pointer_leave.is_some().hash(h);
        modifier.on_click.is_some().hash(h);
        modifier.on_double_click.is_some().hash(h);
        modifier.on_long_click.is_some().hash(h);
        modifier.on_globally_positioned.is_some().hash(h);
        modifier.on_size_changed.is_some().hash(h);
        modifier.on_key_event.is_some().hash(h);
        modifier.on_preview_key_event.is_some().hash(h);
        modifier.on_drag_start.is_some().hash(h);
        modifier.on_drag_end.is_some().hash(h);
        modifier.on_drag_enter.is_some().hash(h);
        modifier.on_drag_over.is_some().hash(h);
        modifier.on_drag_leave.is_some().hash(h);
        modifier.on_drop.is_some().hash(h);
        modifier.on_action.is_some().hash(h);
        modifier.focus_requester.is_some().hash(h);
        modifier.on_focus_changed.is_some().hash(h);
        modifier.interaction_source.is_some().hash(h);
    });
    hashers.hash(FACET_LAYOUT, |h| {
        if let Some(layout) = &modifier.layout {
            1u8.hash(h);
            hash_rc_ptr_identity(layout, h);
        } else {
            0u8.hash(h);
        }
    });
    if let Some(input) = &modifier.text_input {
        facet_hash_text_input(input, hashers);
    } else {
        hashers.hash(FACET_ALL, |h| 0u8.hash(h));
    }
    if let Some(colors) = &modifier.state_colors {
        hashers.hash(FACET_PAINT, |h| {
            hash_color(&colors.default, h);
            hash_color(&colors.hovered, h);
            hash_color(&colors.focused, h);
            hash_color(&colors.pressed, h);
            hash_color(&colors.disabled, h);
            hash_color(&colors.dragged, h);
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    if let Some(elevation) = &modifier.state_elevation {
        hashers.hash(FACET_PAINT, |h| {
            hash_dp(elevation.default, h);
            hash_dp(elevation.hovered, h);
            hash_dp(elevation.focused, h);
            hash_dp(elevation.pressed, h);
            hash_dp(elevation.disabled, h);
            hash_dp(elevation.dragged, h);
        });
    } else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
    }
    if let Some(spec) = &modifier.animate_content_size {
        facet_hash_animation_spec(spec, hashers, FACET_LAYOUT);
    } else {
        hashers.hash(FACET_LAYOUT, |h| 0u8.hash(h));
    }
}

fn facet_hash_semantics(
    semantics: Option<&repose_core::Semantics>,
    hashers: &mut FacetHashers,
    mask: u8,
) {
    hashers.hash(mask, |hasher| {
        if let Some(semantics) = semantics {
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
    });
}

fn facet_hash_span_style(style: &SpanStyle, hashers: &mut FacetHashers, mask: u8) {
    hashers.hash(mask & FACET_PAINT, |h| {
        hash_opt_color(style.color.as_ref(), h)
    });
    hashers.hash(mask & FACET_LAYOUT_MEASUREMENT, |h| {
        if let Some(size) = style.font_size {
            1u8.hash(h);
            hash_sp(size, h);
        } else {
            0u8.hash(h);
        }
    });
    hashers.hash(mask & FACET_LAYOUT_MEASUREMENT, |h| {
        style.font_weight.hash(h);
        style.font_family.hash(h);
        style.font_style.hash(h);
        style.text_align.hash(h);
        if let Some(value) = style.letter_spacing {
            1u8.hash(h);
            hash_sp(value, h);
        } else {
            0u8.hash(h);
        }
        if let Some(value) = style.line_height {
            1u8.hash(h);
            hash_sp(value, h);
        } else {
            0u8.hash(h);
        }
    });
    hashers.hash(mask & FACET_PAINT, |h| {
        hash_opt_color(style.background.as_ref(), h);
        if let Some(decoration) = &style.text_decoration {
            1u8.hash(h);
            hash_text_decoration(decoration, h);
        } else {
            0u8.hash(h);
        }
        hash_f32(style.alpha, h);
    });
    hashers.hash(mask & FACET_LAYOUT_MEASUREMENT, |h| {
        hash_discriminant_option(&style.text_direction, h);
        hash_discriminant_option(&style.font_synthesis, h);
        if let Some(value) = style.baseline_shift {
            1u8.hash(h);
            hash_f32(value.0, h);
        } else {
            0u8.hash(h);
        }
        hash_discriminant_option(&style.hyphens, h);
        hash_discriminant_option(&style.line_break, h);
        if let Some(indent) = &style.text_indent {
            1u8.hash(h);
            hash_dp(indent.first_line, h);
            hash_dp(indent.rest_lines, h);
        } else {
            0u8.hash(h);
        }
    });
    hashers.hash(mask & FACET_LAYOUT_MEASUREMENT_PAINT, |h| {
        if let Some(style) = &style.draw_style {
            1u8.hash(h);
            hash_draw_style(style, h);
        } else {
            0u8.hash(h);
        }
    });
    hashers.hash(mask & FACET_LAYOUT_MEASUREMENT, |h| {
        style.font_variation_settings.hash(h);
    });
}

fn facet_hash_text_style(style: &TextStyle, hashers: &mut FacetHashers) {
    hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| {
        hash_sp(style.font_size, h);
        style.font_weight.hash(h);
        style.font_family.hash(h);
        style.font_style.hash(h);
        style.text_align.hash(h);
        hash_sp(style.letter_spacing, h);
        hash_sp(style.line_height, h);
        hash_discriminant_option(&style.text_direction, h);
        std::mem::discriminant(&style.font_synthesis).hash(h);
        hash_f32(style.baseline_shift.0, h);
        std::mem::discriminant(&style.hyphens).hash(h);
        std::mem::discriminant(&style.line_break).hash(h);
        if let Some(indent) = &style.text_indent {
            1u8.hash(h);
            hash_dp(indent.first_line, h);
            hash_dp(indent.rest_lines, h);
        } else {
            0u8.hash(h);
        }
        hash_draw_style(&style.draw_style, h);
        style.locale_list.hash(h);
        style.font_feature_settings.hash(h);
        style.font_variation_settings.hash(h);
    });
    hashers.hash(FACET_PAINT, |h| {
        hash_opt_color(style.color.as_ref(), h);
        hash_opt_color(style.background.as_ref(), h);
        if let Some(decoration) = &style.text_decoration {
            1u8.hash(h);
            hash_text_decoration(decoration, h);
        } else {
            0u8.hash(h);
        }
        if let Some(shadow) = &style.shadow {
            1u8.hash(h);
            hash_shadow(shadow, h);
        } else {
            0u8.hash(h);
        }
        hash_f32(style.alpha, h);
        hash_draw_style(&style.draw_style, h);
    });
}

fn facet_hash_text_input(input: &repose_core::TextInputConfig, hashers: &mut FacetHashers) {
    hashers.hash(FACET_LAYOUT_MEASUREMENT, |h| {
        input.hint.hash(h);
        input.multiline.hash(h);
        input.value.hash(h);
        input.max_lines.hash(h);
        input.min_lines.hash(h);
        match &input.line_limits {
            Some(repose_core::text::TextFieldLineLimits::SingleLine) => 1u8.hash(h),
            Some(repose_core::text::TextFieldLineLimits::MultiLine {
                min_height_in_lines,
                max_height_in_lines,
            }) => {
                2u8.hash(h);
                min_height_in_lines.hash(h);
                max_height_in_lines.hash(h);
            }
            None => 0u8.hash(h),
        }
    });
    hashers.hash(FACET_PAINT, |h| {
        input.on_change.is_some().hash(h);
        input.on_submit.is_some().hash(h);
        input.focus_tracker.is_some().hash(h);
        input.enabled.hash(h);
        input.read_only.hash(h);
        input.sensitive.hash(h);
        std::mem::discriminant(&input.keyboard_type).hash(h);
        std::mem::discriminant(&input.capitalization).hash(h);
        std::mem::discriminant(&input.ime_action).hash(h);
        input.auto_correct_enabled.hash(h);
        if let Some(color) = input.cursor_color {
            1u8.hash(h);
            hash_color(&color, h);
        } else {
            0u8.hash(h);
        }
        input.on_text_layout.is_some().hash(h);
        input.interaction_source.is_some().hash(h);
        input.visual_transformation.is_some().hash(h);
        if let Some(actions) = &input.keyboard_actions {
            1u8.hash(h);
            actions.on_done.is_some().hash(h);
            actions.on_go.is_some().hash(h);
            actions.on_next.is_some().hash(h);
            actions.on_previous.is_some().hash(h);
            actions.on_search.is_some().hash(h);
            actions.on_send.is_some().hash(h);
        } else {
            0u8.hash(h);
        }
    });
    if let Some(style) = &input.text_style {
        hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| 1u8.hash(h));
        facet_hash_text_style(style, hashers);
    } else {
        hashers.hash(FACET_LAYOUT_MEASUREMENT_PAINT, |h| 0u8.hash(h));
    }
}

fn facet_hash_nested_scroll_connection(
    connection: Option<&repose_core::nested_scroll::NestedScrollConnection>,
    hashers: &mut FacetHashers,
) {
    hashers.hash(FACET_PAINT, |h| match connection {
        Some(connection) => {
            1u8.hash(h);
            if let Some(callback) = &connection.on_pre_scroll {
                1u8.hash(h);
                hash_rc_ptr_identity(callback, h);
            } else {
                0u8.hash(h);
            }
            if let Some(callback) = &connection.on_post_scroll {
                1u8.hash(h);
                hash_rc_ptr_identity(callback, h);
            } else {
                0u8.hash(h);
            }
            if let Some(callback) = &connection.on_pre_fling {
                1u8.hash(h);
                hash_rc_ptr_identity(callback, h);
            } else {
                0u8.hash(h);
            }
            if let Some(callback) = &connection.on_post_fling {
                1u8.hash(h);
                hash_rc_ptr_identity(callback, h);
            } else {
                0u8.hash(h);
            }
        }
        None => 0u8.hash(h),
    });
}

fn facet_hash_scrollbar_style(
    style: Option<&repose_core::ScrollbarStyle>,
    hashers: &mut FacetHashers,
) {
    let Some(style) = style else {
        hashers.hash(FACET_PAINT, |h| 0u8.hash(h));
        return;
    };
    hashers.hash(FACET_PAINT, |h| {
        1u8.hash(h);
        style.thickness.0.to_bits().hash(h);
        style.track_inset.0.to_bits().hash(h);
        style.min_thumb_length.0.to_bits().hash(h);
        style
            .fixed_thumb_length
            .map(|value| value.0.to_bits())
            .hash(h);
        style.radius.map(|value| value.0.to_bits()).hash(h);
    });
    facet_hash_control_visual_set(&style.track_visuals, hashers);
    facet_hash_control_visual_set(&style.thumb_visuals, hashers);
}

fn facet_hash_control_visual_set(set: &repose_core::ControlVisualSet, hashers: &mut FacetHashers) {
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
                hashers.hash(FACET_PAINT, |h| 1u8.hash(h));
                facet_hash_control_visual(visual, hashers);
            }
            None => hashers.hash(FACET_PAINT, |h| 0u8.hash(h)),
        }
    }
}

fn facet_hash_control_visual(visual: &repose_core::ControlVisual, hashers: &mut FacetHashers) {
    hashers.hash(FACET_PAINT, |h| std::mem::discriminant(visual).hash(h));
    match visual {
        repose_core::ControlVisual::Rect { brush, radius } => {
            hashers.hash(FACET_PAINT, |h| {
                hash_brush(brush, h);
                for value in radius {
                    value.0.to_bits().hash(h);
                }
            });
        }
        repose_core::ControlVisual::Image {
            handle,
            source_rect,
            tint,
            fit,
            filter,
        } => {
            hashers.hash(FACET_PAINT, |h| {
                handle.hash(h);
                source_rect.hash(h);
                hash_color(tint, h);
                std::mem::discriminant(fit).hash(h);
                std::mem::discriminant(filter).hash(h);
            });
        }
        repose_core::ControlVisual::Custom(_) => {
            hashers.hash(FACET_PAINT, |h| {
                visual.custom_identity().unwrap_or_default().hash(h)
            });
        }
        _ => {}
    }
}

fn facet_hash_scroll_binding(binding: Option<&ScrollBinding>, hashers: &mut FacetHashers) {
    match binding {
        Some(ScrollBinding::Vertical(binding)) => {
            hashers.hash(FACET_LAYOUT_PAINT, |h| 1u8.hash(h));
            facet_hash_scroll_axis(binding, hashers);
        }
        Some(ScrollBinding::Horizontal(binding)) => {
            hashers.hash(FACET_LAYOUT_PAINT, |h| 2u8.hash(h));
            facet_hash_scroll_axis(binding, hashers);
        }
        Some(ScrollBinding::Both(binding)) => {
            hashers.hash(FACET_LAYOUT_PAINT, |h| 3u8.hash(h));
            hashers.hash(FACET_LAYOUT, |h| binding.show_scrollbar.hash(h));
            hashers.hash(FACET_PAINT, |h| {
                binding.show_scrollbar.hash(h);
                hash_rc_identity(&binding.on_scroll, h);
                hash_rc_identity(&binding.set_viewport_width, h);
                hash_rc_identity(&binding.set_viewport_height, h);
                hash_rc_identity(&binding.set_content_width, h);
                hash_rc_identity(&binding.set_content_height, h);
                hash_rc_identity(&binding.get_offset_xy, h);
                hash_rc_identity(&binding.set_offset_xy, h);
                hash_rc_identity(&binding.tick, h);
                hash_rc_identity(&binding.set_nested_scroll_parent, h);
            });
        }
        None => hashers.hash(FACET_LAYOUT_PAINT, |h| 0u8.hash(h)),
    }
}

fn facet_hash_scroll_axis(
    binding: &repose_core::scroll::ScrollAxisBinding,
    hashers: &mut FacetHashers,
) {
    hashers.hash(FACET_LAYOUT, |h| binding.show_scrollbar.hash(h));
    hashers.hash(FACET_PAINT, |h| {
        binding.show_scrollbar.hash(h);
        hash_rc_identity(&binding.on_scroll, h);
        hash_rc_identity(&binding.set_viewport_main, h);
        hash_rc_identity(&binding.set_content_main, h);
        hash_rc_identity(&binding.get_offset_main, h);
        hash_rc_identity(&binding.set_offset_main, h);
        hash_rc_identity(&binding.tick, h);
        hash_rc_identity(&binding.set_nested_scroll_parent, h);
    });
}

fn facet_hash_animation_spec(spec: &AnimationSpec, hashers: &mut FacetHashers, mask: u8) {
    hashers.hash(mask, |h| {
        spec.duration.as_nanos().hash(h);
        hash_easing(&spec.easing, h);
        spec.delay.as_nanos().hash(h);
        if let Some(spring) = &spec.spring {
            1u8.hash(h);
            hash_f32(spring.damping_ratio, h);
            hash_f32(spring.stiffness, h);
            hash_f32(spring.settle_progress, h);
            hash_f32(spring.settle_velocity, h);
        } else {
            0u8.hash(h);
        }
        if let Some(repeat) = &spec.repeat {
            1u8.hash(h);
            repeat.iterations.hash(h);
            repeat.reverse.hash(h);
            repeat.delay_between.as_nanos().hash(h);
        } else {
            0u8.hash(h);
        }
    });
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

fn hash_rc_ptr_identity<T: ?Sized>(value: &std::rc::Rc<T>, hasher: &mut impl Hasher) {
    (std::rc::Rc::as_ptr(value) as *const () as usize).hash(hasher);
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
fn hash_scrollbar_style(style: Option<&repose_core::ScrollbarStyle>, hasher: &mut impl Hasher) {
    let Some(style) = style else {
        0u8.hash(hasher);
        return;
    };
    1u8.hash(hasher);
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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

#[allow(dead_code)]
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
    hash_scrollbar_style(m.scrollbar_style.as_ref(), hasher);
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

#[allow(dead_code)]
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
