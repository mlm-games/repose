#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{
    BasicTextField, Box, Column, Row, Text, TextFieldConfig as BasicTextFieldConfig,
    TextFieldState, TextStyle, ViewExt, ZStack,
    anim::{animate_color, animate_f32},
    textfield::{TextMeasureConfig, measure_text},
};

use super::*;

static OTF_COUNTER: AtomicU64 = AtomicU64::new(0);
static OTFS_COUNTER: AtomicU64 = AtomicU64::new(0);
static TF_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Tint a leading icon with the M3 icon color (12dp gap to the input, Compose style).
fn tint_icon(color: Color, icon: Option<View>) -> View {
    match icon {
        Some(v) => Box(Modifier::new().padding_values(PaddingValues {
            left: 0.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        }))
        .child(with_content_color(color, move || v)),
        None => Box(Modifier::new()),
    }
}

/// Tint a trailing icon with the M3 icon color (12dp gap to the input, Compose style).
fn tint_trailing_icon(color: Color, icon: Option<View>) -> View {
    match icon {
        Some(v) => Box(Modifier::new().padding_values(PaddingValues {
            left: 12.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        }))
        .child(with_content_color(color, move || v)),
        None => Box(Modifier::new()),
    }
}

/// Color slots for text fields -> matches Compose Material3 `TextFieldColors`.
/// All 42 color fields (focused/unfocused/disabled/error variants of each slot).
#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct TextFieldColors {
    pub focused_text_color: Color,
    pub unfocused_text_color: Color,
    pub disabled_text_color: Color,
    pub error_text_color: Color,
    pub focused_container_color: Color,
    pub unfocused_container_color: Color,
    pub disabled_container_color: Color,
    pub error_container_color: Color,
    pub cursor_color: Color,
    pub error_cursor_color: Color,
    pub focused_indicator_color: Color,
    pub unfocused_indicator_color: Color,
    pub disabled_indicator_color: Color,
    pub error_indicator_color: Color,
    pub focused_leading_icon_color: Color,
    pub unfocused_leading_icon_color: Color,
    pub disabled_leading_icon_color: Color,
    pub error_leading_icon_color: Color,
    pub focused_trailing_icon_color: Color,
    pub unfocused_trailing_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub error_trailing_icon_color: Color,
    pub focused_label_color: Color,
    pub unfocused_label_color: Color,
    pub disabled_label_color: Color,
    pub error_label_color: Color,
    pub focused_placeholder_color: Color,
    pub unfocused_placeholder_color: Color,
    pub disabled_placeholder_color: Color,
    pub error_placeholder_color: Color,
    pub focused_supporting_text_color: Color,
    pub unfocused_supporting_text_color: Color,
    pub disabled_supporting_text_color: Color,
    pub error_supporting_text_color: Color,
    pub focused_prefix_color: Color,
    pub unfocused_prefix_color: Color,
    pub disabled_prefix_color: Color,
    pub error_prefix_color: Color,
    pub focused_suffix_color: Color,
    pub unfocused_suffix_color: Color,
    pub disabled_suffix_color: Color,
    pub error_suffix_color: Color,
}

#[allow(dead_code)]
impl TextFieldColors {
    pub fn text_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_text_color
        } else if is_error {
            self.error_text_color
        } else if focused {
            self.focused_text_color
        } else {
            self.unfocused_text_color
        }
    }
    pub fn container_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_container_color
        } else if is_error {
            self.error_container_color
        } else if focused {
            self.focused_container_color
        } else {
            self.unfocused_container_color
        }
    }
    pub fn cursor_color(&self, is_error: bool) -> Color {
        if is_error {
            self.error_cursor_color
        } else {
            self.cursor_color
        }
    }
    pub fn indicator_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_indicator_color
        } else if is_error {
            self.error_indicator_color
        } else if focused {
            self.focused_indicator_color
        } else {
            self.unfocused_indicator_color
        }
    }
    pub fn leading_icon_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if is_error {
            self.error_leading_icon_color
        } else if focused {
            self.focused_leading_icon_color
        } else {
            self.unfocused_leading_icon_color
        }
    }
    pub fn trailing_icon_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if is_error {
            self.error_trailing_icon_color
        } else if focused {
            self.focused_trailing_icon_color
        } else {
            self.unfocused_trailing_icon_color
        }
    }
    pub fn label_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_label_color
        } else if is_error {
            self.error_label_color
        } else if focused {
            self.focused_label_color
        } else {
            self.unfocused_label_color
        }
    }
    pub fn placeholder_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_placeholder_color
        } else if is_error {
            self.error_placeholder_color
        } else if focused {
            self.focused_placeholder_color
        } else {
            self.unfocused_placeholder_color
        }
    }
    pub fn supporting_text_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_supporting_text_color
        } else if is_error {
            self.error_supporting_text_color
        } else if focused {
            self.focused_supporting_text_color
        } else {
            self.unfocused_supporting_text_color
        }
    }
    pub fn prefix_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_prefix_color
        } else if is_error {
            self.error_prefix_color
        } else if focused {
            self.focused_prefix_color
        } else {
            self.unfocused_prefix_color
        }
    }
    pub fn suffix_color(&self, enabled: bool, is_error: bool, focused: bool) -> Color {
        if !enabled {
            self.disabled_suffix_color
        } else if is_error {
            self.error_suffix_color
        } else if focused {
            self.focused_suffix_color
        } else {
            self.unfocused_suffix_color
        }
    }
}

/// Defaults for text field colors.
pub struct TextFieldDefaults;

impl TextFieldDefaults {
    /// Default minimum height for a filled TextField (56dp matches M3 spec).
    pub const MIN_HEIGHT: f32 = 56.0;
    /// Default minimum width for a filled TextField (280dp matches M3 spec).
    pub const MIN_WIDTH: f32 = 280.0;

    pub fn colors() -> TextFieldColors {
        let th = theme();
        TextFieldColors {
            focused_text_color: th.on_surface,
            unfocused_text_color: th.on_surface,
            disabled_text_color: th.on_surface.with_alpha_f32(0.38),
            error_text_color: th.on_surface,
            focused_container_color: th.surface_container_highest,
            unfocused_container_color: th.surface_container_highest,
            disabled_container_color: th.on_surface.with_alpha_f32(0.04),
            error_container_color: th.surface_container_highest,
            cursor_color: th.primary,
            error_cursor_color: th.error,
            focused_indicator_color: th.primary,
            unfocused_indicator_color: th.on_surface_variant,
            disabled_indicator_color: th.on_surface.with_alpha_f32(0.12),
            error_indicator_color: th.error,
            focused_leading_icon_color: th.on_surface_variant,
            unfocused_leading_icon_color: th.on_surface_variant,
            disabled_leading_icon_color: th.on_surface.with_alpha_f32(0.38),
            error_leading_icon_color: th.error,
            focused_trailing_icon_color: th.on_surface_variant,
            unfocused_trailing_icon_color: th.on_surface_variant,
            disabled_trailing_icon_color: th.on_surface.with_alpha_f32(0.38),
            error_trailing_icon_color: th.error,
            focused_label_color: th.primary,
            unfocused_label_color: th.on_surface_variant,
            disabled_label_color: th.on_surface.with_alpha_f32(0.38),
            error_label_color: th.error,
            focused_placeholder_color: th.on_surface_variant,
            unfocused_placeholder_color: th.on_surface_variant,
            disabled_placeholder_color: th.on_surface.with_alpha_f32(0.38),
            error_placeholder_color: th.error,
            focused_supporting_text_color: th.on_surface_variant,
            unfocused_supporting_text_color: th.on_surface_variant,
            disabled_supporting_text_color: th.on_surface.with_alpha_f32(0.38),
            error_supporting_text_color: th.error,
            focused_prefix_color: th.on_surface,
            unfocused_prefix_color: th.on_surface,
            disabled_prefix_color: th.on_surface.with_alpha_f32(0.38),
            error_prefix_color: th.on_surface,
            focused_suffix_color: th.on_surface,
            unfocused_suffix_color: th.on_surface,
            disabled_suffix_color: th.on_surface.with_alpha_f32(0.38),
            error_suffix_color: th.on_surface,
        }
    }
}

/// Configuration for an `OutlinedTextField`.
#[derive(Clone)]
pub struct OutlinedTextFieldConfig {
    /// Floating label shown above the input when the field has text or is focused.
    /// When set, this acts as the visual placeholder (the TextField's own placeholder
    /// is suppressed). When the label floats, it animates to the top border.
    pub label: Option<String>,
    /// Placeholder text shown inside the TextField when empty and unfocused.
    /// Only shown when `label` is `None`. When a label is present the label
    /// itself serves as the visual placeholder.
    pub placeholder: Option<String>,
    /// Icon displayed at the start of the input.
    pub leading_icon: Option<View>,
    /// Icon displayed at the end of the input.
    pub trailing_icon: Option<View>,
    /// If true, Enter submits. If false, Enter inserts a newline.
    pub single_line: bool,
    /// If true, border and label color switch to error color.
    pub is_error: bool,
    /// If false, input is visually disabled and `on_value_change` won't fire.
    pub enabled: bool,
    /// If true, the field can be focused and text selected/copied but not modified.
    pub read_only: bool,
    /// Transforms the displayed text without changing the underlying value
    /// (e.g. password masking). Passed through to the lower-level text field.
    pub visual_transformation: Option<Rc<dyn VisualTransformation>>,
    /// Supporting text shown below the field.
    pub supporting_text: Option<String>,
    /// Static text prefix inside the field, before the input.
    pub prefix: Option<String>,
    /// Static text suffix inside the field, after the input.
    pub suffix: Option<String>,
    /// Called when the user presses Enter on a single-line field.
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    /// Colors for all text field UI elements.
    pub colors: Option<TextFieldColors>,
    /// Optional external focus tracker. When `None`, an internal focus tracker
    /// is created (keyed by label). Pass a tracker to synchronize focus state
    /// (e.g. to avoid overriding external text while the user is editing).
    pub focus_tracker: Option<Rc<Cell<bool>>>,
}

impl Default for OutlinedTextFieldConfig {
    fn default() -> Self {
        Self {
            label: None,
            placeholder: None,
            leading_icon: None,
            trailing_icon: None,
            single_line: true,
            is_error: false,
            enabled: true,
            read_only: false,
            visual_transformation: None,
            supporting_text: None,
            prefix: None,
            suffix: None,
            on_submit: None,
            colors: None,
            focus_tracker: None,
        }
    }
}

/// M3 Outlined Text Field with floating label, leading/trailing icons, and error state.
///
/// The label floats up when `value` is non-empty or when the field is focused.
/// Focus state comes from the persistent `focus_tracker`, which paint updates
/// on the frame the field gains/loses focus (one-frame delay on tap-to-float).
///
/// # Example
/// ```ignore
/// let text = remember(|| signal(String::new()));
/// OutlinedTextField(
///     Modifier::new().fill_max_width().padding(16.0),
///     text.get(),
///     { let t = text.clone(); move |v| t.set(v) },
///     OutlinedTextFieldConfig {
///         label: Some("Email".into()),
///         placeholder: Some("user@example.com".into()),
///         ..Default::default()
///     },
/// );
/// ```
pub fn OutlinedTextField(
    modifier: Modifier,
    value: String,
    on_value_change: impl Fn(String) + 'static,
    config: OutlinedTextFieldConfig,
) -> View {
    let label_str: Option<Rc<str>> = config.label.clone().map(Rc::from);
    let has_label = label_str.is_some();

    // Unique stable animation key (label text collides when two fields share a label)
    let id = *remember(|| OTF_COUNTER.fetch_add(1, Ordering::Relaxed));
    let anim_key = format!("otf_{id}");

    // Persistent focus tracker - set by layout/paint when this field is focused,
    // read here on the next frame. This gives a one-frame delay on tap-to-float,
    // which is negligible at 60fps. An external tracker takes precedence.
    let focus_tracker: Rc<Cell<bool>> = match config.focus_tracker.clone() {
        Some(ft) => ft,
        None => remember_with_key(format!("otf_focus_{}", anim_key), || Cell::new(false)),
    };
    let is_focused = focus_tracker.get();
    let should_float = !value.is_empty() || is_focused;

    let tf_placeholder = if has_label {
        if should_float {
            config.placeholder.clone().unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        config.placeholder.clone().unwrap_or_default()
    };

    let text_input = View::new(0, ViewKind::Box)
        .modifier(
            Modifier::new().flex_grow(1.0).text_input(TextInputConfig {
                hint: tf_placeholder,
                multiline: !config.single_line,
                on_change: Some(Rc::new(on_value_change) as _),
                on_submit: config.on_submit.clone().map(|f| {
                    let f = f.clone();
                    Rc::new(move |s| f(s)) as Rc<dyn Fn(String)>
                }),
                focus_tracker: Some(focus_tracker),
                value: value.clone(),
                visual_transformation: config.visual_transformation.clone(),
                enabled: config.enabled,
                read_only: config.read_only,
                cursor_color: config
                    .colors
                    .as_ref()
                    .map(|c| c.cursor_color(config.is_error)),
                ..Default::default()
            }),
        )
        .semantics(Semantics {
            role: Role::TextField,
            label: config
                .label
                .clone()
                .or_else(|| config.supporting_text.clone()),
            enabled: config.enabled,
            ..Default::default()
        });

    outlined_field_decoration(
        modifier,
        anim_key,
        label_str,
        &config,
        is_focused,
        !value.is_empty(),
        text_input,
    )
}

/// State-based M3 Outlined Text Field.
pub fn OutlinedTextFieldState(
    modifier: Modifier,
    state: Rc<RefCell<TextFieldState>>,
    on_value_change: impl Fn(String) + 'static,
    config: OutlinedTextFieldConfig,
) -> View {
    let label_str: Option<Rc<str>> = config.label.clone().map(Rc::from);
    let has_label = label_str.is_some();

    // Unique stable animation key (label text collides when two fields share a label)
    let id = *remember(|| OTFS_COUNTER.fetch_add(1, Ordering::Relaxed));
    let anim_key = format!("otfs_{id}");

    let focus_tracker: Rc<Cell<bool>> = match config.focus_tracker.clone() {
        Some(ft) => ft,
        None => remember_with_key(format!("otf_focus_{}", anim_key), || Cell::new(false)),
    };
    let is_focused = focus_tracker.get();
    let has_content = !state.borrow().text.is_empty();
    let should_float = has_content || is_focused;

    // Placeholder shows when there's no label, or when label is floating (focused/has content)
    let tf_placeholder = if has_label {
        if should_float {
            config.placeholder.clone().unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        config.placeholder.clone().unwrap_or_default()
    };

    let text_input = BasicTextField(
        state,
        Modifier::new().flex_grow(1.0),
        tf_placeholder,
        BasicTextFieldConfig {
            line_limits: if config.single_line {
                TextFieldLineLimits::SingleLine
            } else {
                TextFieldLineLimits::MultiLine {
                    min_height_in_lines: 1,
                    max_height_in_lines: usize::MAX,
                }
            },
            on_change: Some(Rc::new(on_value_change)),
            on_submit: config.on_submit.clone(),
            focus_tracker: Some(focus_tracker),
            enabled: config.enabled,
            read_only: config.read_only,
            ..Default::default()
        },
    );

    outlined_field_decoration(
        modifier,
        anim_key,
        label_str,
        &config,
        is_focused,
        has_content,
        text_input,
    )
}

fn outlined_field_decoration(
    modifier: Modifier,
    anim_key: String,
    label_str: Option<Rc<str>>,
    config: &OutlinedTextFieldConfig,
    is_focused: bool,
    has_content: bool,
    text_input: View,
) -> View {
    let th = theme();
    let has_label = label_str.is_some();

    let should_float = has_content || is_focused;
    let float_t = animate_f32(
        anim_key.clone(),
        if should_float { 1.0 } else { 0.0 },
        th.motion.color,
    );

    let target_border_w = if config.is_error || is_focused {
        OutlinedTextFieldDefaults::FOCUSED_BORDER_THICKNESS
    } else {
        OutlinedTextFieldDefaults::UNFOCUSED_BORDER_THICKNESS
    };
    let border_w = animate_f32(
        format!("otf_bw_{}", anim_key),
        target_border_w,
        th.motion.color,
    );

    let (border_color_target, label_color_target, container_bg) =
        if let Some(ref tc) = config.colors {
            (
                tc.indicator_color(config.enabled, config.is_error, is_focused),
                tc.label_color(config.enabled, config.is_error, is_focused),
                tc.container_color(config.enabled, config.is_error, is_focused),
            )
        } else {
            (
                if config.is_error {
                    th.error
                } else if is_focused {
                    th.primary
                } else {
                    th.outline
                },
                if config.is_error {
                    th.error
                } else if is_focused {
                    th.primary
                } else {
                    th.on_surface_variant
                },
                Color::TRANSPARENT,
            )
        };

    let border_color = animate_color(
        format!("otf_bc_{}", anim_key),
        border_color_target,
        th.motion.color,
    );
    let label_color = animate_color(
        format!("otf_lc_{}", anim_key),
        label_color_target,
        th.motion.color,
    );

    // Label font size: 16dp (expanded, inside) -> 12dp (minimized, at border)
    let label_size = 16.0 - 4.0 * float_t;

    // Minimized label half-height matches bodySmall line height (~16dp) / 2
    let min_label_half_h: f32 = if has_label { 8.0 } else { 0.0 };

    // Label Y: expanded centered within 56dp field -> minimized overlapping top border (-labelHeight/2)
    let label_start_y = (56.0 - 16.0) / 2.0;
    let label_end_y = -min_label_half_h;
    let label_y = label_start_y - (label_start_y - label_end_y) * float_t;

    // Label X: expanded at text-input start (~24dp) -> minimized at border-start (~20dp)
    let label_start_x = if has_label { 24.0 } else { 0.0 };
    let label_end_x = if has_label { 20.0 } else { 0.0 };
    let label_x = label_start_x - (label_start_x - label_end_x) * float_t;

    // Container padding matches reference: 8dp top/bottom with label, 16dp without
    let (top_pad, bottom_pad) = if has_label { (8.0, 8.0) } else { (16.0, 16.0) };

    let (prefix_color, suffix_color) = if let Some(ref tc) = config.colors {
        (
            tc.prefix_color(config.enabled, config.is_error, is_focused),
            tc.suffix_color(config.enabled, config.is_error, is_focused),
        )
    } else {
        (
            if config.is_error {
                th.error
            } else {
                th.on_surface
            },
            if config.is_error {
                th.error
            } else {
                th.on_surface
            },
        )
    };

    let (lead_c, trail_c) = if let Some(ref tc) = config.colors {
        (
            tc.leading_icon_color(config.enabled, config.is_error, is_focused),
            tc.trailing_icon_color(config.enabled, config.is_error, is_focused),
        )
    } else {
        let c = if !config.enabled {
            th.on_surface.with_alpha_f32(0.38)
        } else if config.is_error {
            th.error
        } else {
            th.on_surface_variant
        };
        (c, c)
    };

    let text_c = config
        .colors
        .as_ref()
        .map(|c| c.text_color(config.enabled, config.is_error, is_focused))
        .unwrap_or(if config.enabled {
            th.on_surface
        } else {
            th.on_surface.with_alpha_f32(0.38)
        });

    let supporting = config.supporting_text.as_ref().map(|st| {
        let c = if let Some(ref tc) = config.colors {
            tc.supporting_text_color(config.enabled, config.is_error, is_focused)
        } else if config.is_error {
            th.error
        } else {
            th.on_surface_variant
        };
        Text(st.clone())
            .color(c)
            .size(th.typography.body_small)
            .modifier(Modifier::new().padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 4.0,
                bottom: 0.0,
            }))
    });

    // Outer Stack holds both the clipped content and the unclipped label.
    // The label sits outside the clipped Box so it can extend above the border.
    let label_cutout = label_str.as_ref().map(|lbl| {
        let font_px = dp_to_px(label_size) * repose_core::locals::text_scale().0;
        let m = measure_text(lbl, font_px, TextMeasureConfig::default());
        let text_width_px = m.positions.last().copied().unwrap_or(0.0);
        let text_width_dp = px_to_dp(text_width_px);
        let pad = 1.0;
        let line_h = 16.0;
        (
            label_x - pad,
            label_y - pad,
            label_x + text_width_dp + pad,
            label_y + line_h + pad,
        )
    });

    Column(modifier.min_width(OutlinedTextFieldDefaults::MIN_WIDTH)).child((
        ZStack(
            Modifier::new()
                .fill_max_width()
                .min_height(OutlinedTextFieldDefaults::MIN_HEIGHT),
        )
        .child((
            Box(Modifier::new()
                .fill_max_size()
                .clip_rounded(th.shapes.small)
                .background(container_bg)),
            if has_label {
                let mut bm = Modifier::new()
                    .fill_max_size()
                    .clip_rounded(th.shapes.small)
                    .border(border_w, border_color, th.shapes.small);
                if let Some((l, t, r, b)) = label_cutout {
                    bm = bm.clip_rect(l, t, r, b, ClipOp::Difference);
                }
                Box(bm)
            } else {
                Box(Modifier::new()
                    .fill_max_size()
                    .clip_rounded(th.shapes.small)
                    .border(border_w, border_color, th.shapes.small))
            },
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: top_pad,
                    bottom: bottom_pad,
                })
                .align_items(AlignItems::CENTER))
            .child((
                tint_icon(lead_c, config.leading_icon.clone()),
                config
                    .prefix
                    .as_ref()
                    .map(|p| {
                        Text(p.clone())
                            .color(prefix_color)
                            .size(th.typography.body_large)
                            .single_line()
                    })
                    .unwrap_or(Box(Modifier::new())),
                with_content_color(text_c, move || text_input),
                config
                    .suffix
                    .as_ref()
                    .map(|s| {
                        Text(s.clone())
                            .color(suffix_color)
                            .size(th.typography.body_large)
                            .single_line()
                    })
                    .unwrap_or(Box(Modifier::new())),
                tint_trailing_icon(trail_c, config.trailing_icon.clone()),
            )),
            if let Some(lbl) = label_str {
                Box(Modifier::new()
                    .min_width(200.0)
                    .padding_values(PaddingValues {
                        left: label_x,
                        right: 20.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .absolute()
                    .offset(Some(0.0), Some(label_y), None, None))
                .child(
                    Text(lbl.as_ref().to_string())
                        .color(label_color)
                        .size(label_size),
                )
            } else {
                Box(Modifier::new())
            },
        )),
        supporting.unwrap_or(Box(Modifier::new())),
    ))
}

/// Configuration for a filled M3 [`TextField`].
#[derive(Clone)]
pub struct TextFieldConfig {
    pub label: Option<String>,
    pub placeholder: Option<String>,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub single_line: bool,
    pub is_error: bool,
    pub enabled: bool,
    /// If true, the field can be focused and text selected/copied but not modified.
    pub read_only: bool,
    /// Transforms the displayed text without changing the underlying value
    /// (e.g. password masking). Passed through to the lower-level text field.
    pub visual_transformation: Option<Rc<dyn VisualTransformation>>,
    /// Supporting text shown below the field.
    pub supporting_text: Option<String>,
    /// Static text prefix inside the field, before the input.
    pub prefix: Option<String>,
    /// Static text suffix inside the field, after the input.
    pub suffix: Option<String>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    pub colors: Option<TextFieldColors>,
}

impl Default for TextFieldConfig {
    fn default() -> Self {
        Self {
            label: None,
            placeholder: None,
            leading_icon: None,
            trailing_icon: None,
            single_line: true,
            is_error: false,
            enabled: true,
            read_only: false,
            visual_transformation: None,
            supporting_text: None,
            prefix: None,
            suffix: None,
            on_submit: None,
            colors: None,
        }
    }
}

/// M3 Filled Text Field with floating label, leading/trailing icons, error state,
/// and a bottom indicator line. (Equivalent to Compose Material3's `TextField`.)
///
/// The label floats up when `value` is non-empty or when the field is focused.
/// Container: `SurfaceContainerHighest` bg, top-rounded corners (4dp), flat bottom.
/// Indicator: always visible, 1dp (unfocused) / 2dp (focused/error), animated color+thickness.
pub fn TextField(
    modifier: Modifier,
    value: String,
    on_value_change: impl Fn(String) + 'static,
    config: TextFieldConfig,
) -> View {
    let th = theme();
    let label_str: Option<Rc<str>> = config.label.clone().map(Rc::from);
    let has_label = label_str.is_some();

    let id = *remember(|| TF_COUNTER.fetch_add(1, Ordering::Relaxed));
    let anim_key = format!("tf_{id}");

    let focus_tracker: Rc<Cell<bool>> =
        remember_with_key(format!("tf_focus_{}", anim_key), || Cell::new(false));
    let is_focused = focus_tracker.get();
    let should_float = !value.is_empty() || is_focused;

    let float_t = animate_f32(
        anim_key.clone(),
        if should_float { 1.0 } else { 0.0 },
        th.motion.color,
    );

    let (indicator_color_target, label_color_target, container_bg) =
        if let Some(ref tc) = config.colors {
            let enf = config.enabled && is_focused;
            let ind = tc.indicator_color(config.enabled, config.is_error, enf);
            let lb = tc.label_color(config.enabled, config.is_error, enf);
            let bg = tc.container_color(config.enabled, config.is_error, enf);
            (ind, lb, bg)
        } else {
            let ind = if !config.enabled {
                th.on_surface.with_alpha_f32(0.38)
            } else if config.is_error {
                th.error
            } else if is_focused {
                th.primary
            } else {
                th.on_surface_variant
            };
            let lb = if !config.enabled {
                th.on_surface.with_alpha_f32(0.38)
            } else if config.is_error {
                th.error
            } else if is_focused {
                th.primary
            } else {
                th.on_surface_variant
            };
            let bg = if config.enabled {
                th.surface_container_highest
            } else {
                th.on_surface
                    .with_alpha_f32(0.04)
                    .composite_over(th.surface)
            };
            (ind, lb, bg)
        };

    let indicator_color = animate_color(
        format!("tf_ind_c_{}", anim_key),
        indicator_color_target,
        th.motion.color,
    );
    let label_color = animate_color(
        format!("tf_lc_{}", anim_key),
        label_color_target,
        th.motion.color,
    );

    let label_size = 16.0 - 4.0 * float_t;

    let label_start_y = (56.0 - 16.0) / 2.0;
    let label_end_y = if has_label { 8.0 } else { 0.0 };
    let label_y = label_start_y - (label_start_y - label_end_y) * float_t;

    let label_start_x = if has_label { 24.0 } else { 0.0 };
    let label_end_x = if has_label { 20.0 } else { 0.0 };
    let label_x = label_start_x - (label_start_x - label_end_x) * float_t;

    let tf_placeholder = if has_label {
        if should_float {
            config.placeholder.unwrap_or_default()
        } else {
            String::new()
        }
    } else {
        config.placeholder.unwrap_or_default()
    };

    let indicator_active = config.is_error || (config.enabled && is_focused);
    let indicator_target_w = if indicator_active { 2.0 } else { 1.0 };
    let indicator_w = animate_f32(
        format!("tf_ind_w_{}", anim_key),
        indicator_target_w,
        th.motion.color,
    );

    let (top_pad, bottom_pad) = if has_label { (8.0, 8.0) } else { (16.0, 16.0) };

    let (prefix_color, suffix_color) = if let Some(ref tc) = config.colors {
        (
            tc.prefix_color(config.enabled, config.is_error, is_focused),
            tc.suffix_color(config.enabled, config.is_error, is_focused),
        )
    } else {
        (
            if config.is_error {
                th.error
            } else {
                th.on_surface
            },
            if config.is_error {
                th.error
            } else {
                th.on_surface
            },
        )
    };

    let (lead_c, trail_c) = if let Some(ref tc) = config.colors {
        (
            tc.leading_icon_color(config.enabled, config.is_error, is_focused),
            tc.trailing_icon_color(config.enabled, config.is_error, is_focused),
        )
    } else {
        let c = if !config.enabled {
            th.on_surface.with_alpha_f32(0.38)
        } else if config.is_error {
            th.error
        } else {
            th.on_surface_variant
        };
        (c, c)
    };

    let text_c = config
        .colors
        .as_ref()
        .map(|c| c.text_color(config.enabled, config.is_error, is_focused))
        .unwrap_or(if config.enabled {
            th.on_surface
        } else {
            th.on_surface.with_alpha_f32(0.38)
        });

    let supporting = config.supporting_text.as_ref().map(|st| {
        let c = if let Some(ref tc) = config.colors {
            tc.supporting_text_color(config.enabled, config.is_error, is_focused)
        } else if config.is_error {
            th.error
        } else {
            th.on_surface_variant
        };
        Text(st.clone())
            .color(c)
            .size(th.typography.body_small)
            .modifier(Modifier::new().padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 4.0,
                bottom: 0.0,
            }))
    });

    let text_input = View::new(0, ViewKind::Box)
        .modifier(
            Modifier::new().flex_grow(1.0).text_input(TextInputConfig {
                hint: tf_placeholder,
                multiline: !config.single_line,
                on_change: Some(Rc::new(on_value_change) as _),
                on_submit: config.on_submit.clone().map(|f| {
                    let f = f.clone();
                    Rc::new(move |s| f(s)) as Rc<dyn Fn(String)>
                }),
                focus_tracker: Some(focus_tracker),
                value: value.clone(),
                visual_transformation: config.visual_transformation.clone(),
                enabled: config.enabled,
                read_only: config.read_only,
                cursor_color: config
                    .colors
                    .as_ref()
                    .map(|c| c.cursor_color(config.is_error)),
                ..Default::default()
            }),
        )
        .semantics(Semantics {
            role: Role::TextField,
            label: config
                .label
                .clone()
                .or_else(|| config.supporting_text.clone()),
            enabled: config.enabled,
            ..Default::default()
        });

    Column(modifier.min_width(TextFieldDefaults::MIN_WIDTH)).child((
        ZStack(
            Modifier::new()
                .fill_max_width()
                .min_height(TextFieldDefaults::MIN_HEIGHT),
        )
        .child((
            // Container: top-rounded only (M3 filled shape)
            Box(Modifier::new()
                .fill_max_size()
                .clip_rounded_radii([
                    0.0,                   // BL
                    0.0,                   // BR
                    th.shapes.extra_small, // TR
                    th.shapes.extra_small, // TL
                ])
                .background(container_bg)),
            // Input row
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: top_pad,
                    bottom: bottom_pad,
                })
                .align_items(AlignItems::CENTER))
            .child((
                tint_icon(lead_c, config.leading_icon.clone()),
                config
                    .prefix
                    .as_ref()
                    .map(|p| {
                        Text(p.clone())
                            .color(prefix_color)
                            .size(th.typography.body_large)
                            .single_line()
                    })
                    .unwrap_or(Box(Modifier::new())),
                with_content_color(text_c, move || text_input),
                config
                    .suffix
                    .as_ref()
                    .map(|s| {
                        Text(s.clone())
                            .color(suffix_color)
                            .size(th.typography.body_large)
                            .single_line()
                    })
                    .unwrap_or(Box(Modifier::new())),
                tint_trailing_icon(trail_c, config.trailing_icon.clone()),
            )),
            // Bottom indicator line
            Box(Modifier::new()
                .fill_max_width()
                .height(indicator_w)
                .absolute()
                .offset(None, None, None, Some(0.0))
                .background(indicator_color)),
            // Floating label inside the stack
            if let Some(lbl) = label_str {
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(label_x), Some(label_y), None, None))
                .child(
                    Text(lbl.as_ref().to_string())
                        .color(label_color)
                        .size(label_size),
                )
            } else {
                Box(Modifier::new())
            },
        )),
        supporting.unwrap_or(Box(Modifier::new())),
    ))
}
