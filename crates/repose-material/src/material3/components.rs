#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::anim::{animate_color, animate_f32};
use repose_ui::{Box, Column, Row, Stack, Text, TextStyle, ViewExt};

use crate::{Icon, Symbol};

/// M3 Top App Bar (small). Displays a title with optional navigation icon and
/// trailing action buttons.
pub fn TopAppBar(
    title: impl Into<String>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
) -> View {
    let th = theme();
    Row(Modifier::new()
        .min_width(200.0)
        .height(64.0)
        .background(th.surface)
        .padding_values(PaddingValues {
            left: 4.0,
            right: 4.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center))
    .child((
        navigation_icon.unwrap_or(Box(Modifier::new().size(16.0, 1.0))),
        Box(Modifier::new()
            .padding_values(PaddingValues {
                left: 16.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            })
            .flex_grow(1.0))
        .child(
            Text(title)
                .color(th.on_surface)
                .size(th.typography.title_large),
        ),
        Row(Modifier::new().align_items(AlignItems::Center)).child(actions),
    ))
}

/// M3 Icon Button - a tappable circular container for an icon.
pub fn IconButton(icon: View, on_click: impl Fn() + 'static) -> View {
    let th = theme();
    let _bg = Color::TRANSPARENT;
    Box(Modifier::new()
        .size(48.0, 48.0)
        .clip_rounded(24.0)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(icon)
}

/// M3 Filled Icon Button - icon button with a filled container background.
pub fn FilledIconButton(icon: View, on_click: impl Fn() + 'static) -> View {
    let th = theme();
    let bg = th.primary;
    Box(Modifier::new()
        .size(40.0, 40.0)
        .clip_rounded(20.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_primary.with_alpha_f32(0.08),
            pressed: th.on_primary.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(icon)
}

fn button_impl(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
    content_color: Color,
    bg: Option<Color>,
    state_colors: StateColors,
    state_elevation: Option<StateElevation>,
    border: Option<(f32, Color, f32)>,
    padding_left: f32,
    padding_right: f32,
) -> View {
    let content = with_content_color(content_color, content);
    let mut m = Modifier::new().height(40.0).min_width(48.0);
    if let Some(bg) = bg {
        m = m.background(bg);
    }
    m = m.state_colors(state_colors);
    if let Some(se) = state_elevation {
        m = m.state_elevation(se);
    }
    if let Some((w, c, r)) = border {
        m = m.border(w, c, r);
    }
    m = m
        .clip_rounded(20.0)
        .padding_values(PaddingValues {
            left: padding_left,
            right: padding_right,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(modifier);
    Box(m).child(content)
}

/// M3 Filled Button - the basic Material3 button (equivalent to Compose's `Button`).
pub fn Button(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    FilledButton(modifier, on_click, content)
}

/// M3 Filled Button - prominent action button with primary color fill.
pub fn FilledButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    button_impl(
        modifier,
        on_click,
        content,
        th.on_primary,
        Some(th.primary),
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_primary.with_alpha_f32(0.08),
            pressed: th.on_primary.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
        Some(StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 8.0,
            disabled: 0.0,
        }),
        None,
        24.0,
        24.0,
    )
}

/// M3 Filled Tonal Button - uses secondary container colors.
pub fn FilledTonalButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    button_impl(
        modifier,
        on_click,
        content,
        th.on_secondary_container,
        Some(th.secondary_container),
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_secondary_container.with_alpha_f32(0.08),
            pressed: th.on_secondary_container.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
        Some(StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 8.0,
            disabled: 0.0,
        }),
        None,
        24.0,
        24.0,
    )
}

/// M3 Outlined Button - button with an outline border and no fill.
pub fn OutlinedButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    button_impl(
        modifier,
        on_click,
        content,
        th.outline,
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        None,
        Some((1.0, th.outline, 20.0)),
        24.0,
        24.0,
    )
}

/// M3 Text Button - a low-emphasis button.
pub fn TextButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    button_impl(
        modifier,
        on_click,
        content,
        th.on_surface,
        None,
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        },
        None,
        None,
        12.0,
        12.0,
    )
}

/// M3 Elevated Button - uses `surface_container_low` background with elevation.
pub fn ElevatedButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    button_impl(
        modifier,
        on_click,
        content,
        th.primary,
        Some(th.surface_container_low),
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.primary.with_alpha_f32(0.08),
            pressed: th.primary.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        },
        Some(StateElevation {
            default: th.elevation.level1,
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            disabled: 0.0,
        }),
        None,
        24.0,
        24.0,
    )
}

fn fab_impl(icon: View, on_click: impl Fn() + 'static, size: f32) -> View {
    let th = theme();
    Box(Modifier::new()
        .size(size, size)
        .background(th.primary_container)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_primary_container.with_alpha_f32(0.08),
            pressed: th.on_primary_container.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(StateElevation {
            default: 6.0,
            hovered: 8.0,
            pressed: 12.0,
            disabled: 0.0,
        })
        .clip_rounded(28.0)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(icon)
}

/// M3 Floating Action Button (regular, 56dp).
pub fn FAB(icon: View, on_click: impl Fn() + 'static) -> View {
    fab_impl(icon, on_click, 56.0)
}

/// M3 Large FAB (96dp).
pub fn LargeFAB(icon: View, on_click: impl Fn() + 'static) -> View {
    fab_impl(icon, on_click, 96.0)
}

/// M3 Extended FAB - FAB with icon + label.
pub fn ExtendedFAB(
    icon: Option<View>,
    label: impl Into<String>,
    on_click: impl Fn() + 'static,
) -> View {
    let th = theme();
    let has_icon = icon.is_some();
    let bg = th.primary_container;
    Row(Modifier::new()
        .height(56.0)
        .min_width(80.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_primary_container.with_alpha_f32(0.08),
            pressed: th.on_primary_container.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(StateElevation {
            default: 6.0,
            hovered: 8.0,
            pressed: 12.0,
            disabled: 0.0,
        })
        .clip_rounded(16.0)
        .padding_values(PaddingValues {
            left: 16.0,
            right: 20.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child((
        icon.unwrap_or(Box(Modifier::new())),
        Box(Modifier::new().size(if has_icon { 12.0 } else { 0.0 }, 1.0)),
        Text(label)
            .color(th.on_primary_container)
            .size(th.typography.label_large)
            .single_line(),
    ))
}

/// M3 Horizontal Divider - a thin 1dp line.
pub fn Divider() -> View {
    let th = theme();
    Box(Modifier::new()
        .min_width(200.0)
        .height(1.0)
        .background(th.outline_variant))
}

/// M3 Vertical Divider - a thin 1dp vertical line.
pub fn VerticalDivider() -> View {
    let th = theme();
    Box(Modifier::new()
        .width(1.0)
        .fill_max_height()
        .background(th.outline_variant))
}

/// M3 Badge - a small notification indicator. If `label` is `None`, shows a
/// small 6dp dot; otherwise shows the label text inside a 16dp pill.
pub fn Badge(label: Option<impl Into<String>>) -> View {
    let th = theme();
    match label {
        None => Box(Modifier::new()
            .size(6.0, 6.0)
            .background(th.error)
            .clip_rounded(3.0)),
        Some(text) => {
            let text = text.into();
            Box(Modifier::new()
                .min_width(16.0)
                .height(16.0)
                .background(th.error)
                .clip_rounded(8.0)
                .padding_values(PaddingValues {
                    left: 4.0,
                    right: 4.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center))
            .child(
                Text(text)
                    .color(th.on_error)
                    .size(th.typography.label_small)
                    .single_line(),
            )
        }
    }
}

/// M3 List Item - a single row in a list with optional leading/trailing content.
pub fn ListItem(
    headline: impl Into<String>,
    supporting_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
) -> View {
    let th = theme();
    let mut modifier = Modifier::new()
        .min_width(200.0)
        .min_height(if supporting_text.is_some() {
            72.0
        } else {
            56.0
        })
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: 16.0,
            right: 24.0,
            top: 8.0,
            bottom: 8.0,
        })
        .align_items(AlignItems::Center);

    if let Some(cb) = on_click {
        modifier = modifier.clickable().on_pointer_down(move |_| cb());
    }

    Row(modifier).child((
        leading
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 16.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(v)
            })
            .unwrap_or(Box(Modifier::new())),
        Column(
            Modifier::new()
                .flex_grow(1.0)
                .justify_content(JustifyContent::Center),
        )
        .child((
            Text(headline)
                .color(th.on_surface)
                .size(th.typography.body_large)
                .single_line(),
            supporting_text
                .map(|st| {
                    Text(st)
                        .color(th.on_surface_variant)
                        .size(th.typography.body_medium)
                        .max_lines(2)
                        .overflow_ellipsize()
                })
                .unwrap_or(Box(Modifier::new())),
        )),
        trailing
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 16.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(v)
            })
            .unwrap_or(Box(Modifier::new())),
    ))
}

/// A single tab definition for use with `TabRow`.
pub struct Tab {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
}

static TABROW_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Tab Row - a horizontal row of tabs with an active indicator.
/// Text colors and indicator height animate with 150ms FastOutSlowIn.
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>) -> View {
    let th = theme();
    let id = remember(|| TABROW_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    Column(Modifier::new().fill_max_width()).child((
        Row(Modifier::new()
            .fill_max_width()
            .height(48.0)
            .background(th.surface))
        .child(
            tabs.into_iter()
                .enumerate()
                .map(|(i, tab)| {
                    let selected = i == selected_index;
                    let color = animate_color(
                        format!("tab_clr_{}_{}", id, i),
                        if selected {
                            th.primary
                        } else {
                            th.on_surface_variant
                        },
                        spec,
                    );
                    let indicator_h = animate_f32(
                        format!("tab_ind_{}_{}", id, i),
                        if selected { 3.0 } else { 0.0 },
                        spec,
                    );
                    let cb = tab.on_click.clone();

                    Column(
                        Modifier::new()
                            .flex_grow(1.0)
                            .fill_max_height()
                            .align_items(AlignItems::Center)
                            .justify_content(JustifyContent::Center)
                            .state_colors(StateColors {
                                default: Color::TRANSPARENT,
                                hovered: th.on_surface.with_alpha_f32(0.08),
                                pressed: th.on_surface.with_alpha_f32(0.12),
                                disabled: Color::TRANSPARENT,
                            })
                            .clickable()
                            .on_pointer_down(move |_| cb()),
                    )
                    .child((
                        tab.icon.unwrap_or(Box(Modifier::new())),
                        Text(tab.label)
                            .color(color)
                            .size(th.typography.title_small)
                            .single_line(),
                        Box(Modifier::new()
                            .min_width(200.0)
                            .height(indicator_h)
                            .background(th.primary)
                            .clip_rounded(1.5)),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
        Box(Modifier::new()
            .min_width(200.0)
            .height(1.0)
            .background(th.outline_variant)),
    ))
}

/// A single segment definition for `SegmentedButton`.
pub struct Segment {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
}

static SEGBUTTON_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Segmented Button - a row of toggle segments. `selected` contains the
/// indices of selected segments (single-select: pass a single-element set).
pub fn SegmentedButton(selected: &[usize], segments: Vec<Segment>) -> View {
    let th = theme();
    let count = segments.len();
    let id = remember(|| SEGBUTTON_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;

    Row(Modifier::new()
        .height(40.0)
        .border(1.0, th.outline, 20.0)
        .clip_rounded(20.0))
    .child(
        segments
            .into_iter()
            .enumerate()
            .map(|(i, seg)| {
                let is_selected = selected.contains(&i);

                let bg = animate_color(
                    format!("sb_bg_{}_{}", id, i),
                    if is_selected {
                        th.secondary_container
                    } else {
                        Color::TRANSPARENT
                    },
                    spec,
                );
                let fg = animate_color(
                    format!("sb_fg_{}_{}", id, i),
                    if is_selected {
                        th.on_secondary_container
                    } else {
                        th.on_surface
                    },
                    spec,
                );

                let cb = seg.on_click.clone();

                let mut modifier = Modifier::new()
                    .flex_grow(1.0)
                    .fill_max_height()
                    .background(bg)
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center)
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .clickable()
                    .on_pointer_down(move |_| cb());

                if i < count - 1 {
                    modifier = modifier.border(1.0, th.outline, 0.0);
                }

                Row(modifier).child((
                    seg.icon.unwrap_or(Box(Modifier::new())),
                    Text(seg.label)
                        .color(fg)
                        .size(th.typography.label_large)
                        .single_line(),
                ))
            })
            .collect::<Vec<_>>(),
    )
}

pub fn CircularProgressIndicator(value: Option<f32>) -> View {
    let th = theme();
    let sz = dp_to_px(40.0);
    let stroke = dp_to_px(4.0);
    let val = value.unwrap_or(0.0).clamp(0.0, 1.0);

    Box(Modifier::new()
        .size(sz, sz)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let cx = rect.x + rect.w * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let r = (rect.w.min(rect.h)) * 0.5 - stroke * 0.5;
            let circle = Rect {
                x: cx - r,
                y: cy - r,
                w: r * 2.0,
                h: r * 2.0,
            };

            // Track ring
            scene.nodes.push(SceneNode::EllipseBorder {
                rect: circle,
                color: mul_c(th.secondary_container),
                width: stroke,
            });

            // Indicator: bottom-up fill approximation inside circle
            if val > 0.0 {
                let fill_h = r * 2.0 * val;
                scene.nodes.push(SceneNode::PushClip {
                    rect: Rect {
                        x: cx - r,
                        y: cy + r - fill_h,
                        w: r * 2.0,
                        h: fill_h,
                    },
                    radius: 0.0,
                });
                scene.nodes.push(SceneNode::EllipseBorder {
                    rect: circle,
                    color: mul_c(th.primary),
                    width: stroke,
                });
                scene.nodes.push(SceneNode::PopClip);
            }
        }))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

/// M3 Linear Progress Indicator.
pub fn LinearProgressIndicator(value: Option<f32>) -> View {
    let th = theme();

    Box(Modifier::new().fill_max_width().height(4.0).painter(
        move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = rect.h;
            let corner = track_h * 0.5;
            let gap = dp_to_px(4.0);
            let dot_r = dp_to_px(2.0);
            let cy = rect.y + rect.h * 0.5;
            let t = value.unwrap_or(0.0).clamp(0.0, 1.0);

            // Indicator (active portion from left)
            if t > 0.0 {
                let ind_w = t * rect.w;
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x,
                        y: cy - corner,
                        w: ind_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                    radius: corner,
                });
            }

            // Track (inactive portion after gap)
            let track_start = rect.x + t * rect.w + gap;
            let track_w = (rect.x + rect.w - track_start).max(0.0);
            if t < 1.0 && track_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_start,
                        y: cy - corner,
                        w: track_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.secondary_container)),
                    radius: corner,
                });
            }

            // Stop indicator at right end (4dp circle, Primary)
            if t < 1.0 {
                let sx = rect.x + rect.w - dot_r;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                });
            }
        },
    ))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

/// Configuration for an `OutlinedTextField`.
#[derive(Clone)]
pub struct OutlinedTextFieldConfig {
    /// Floating label shown above the input when the field has text or is focused.
    /// When set, this acts as the visual placeholder (the TextField's own placeholder
    /// is suppressed). When the label floats, it animates to the top border.
    pub label: Option<String>,
    /// Placeholder text shown inside the TextField when empty and unfocused.
    /// Only shown when `label` is `None`; when a label is present the label
    /// itself serves as the visual placeholder.
    pub placeholder: Option<String>,
    /// Icon displayed at the start of the input.
    pub leading_icon: Option<View>,
    /// Icon displayed at the end of the input.
    pub trailing_icon: Option<View>,
    /// If true, Enter submits; if false, Enter inserts a newline.
    pub single_line: bool,
    /// If true, border and label color switch to error color.
    pub is_error: bool,
    /// If false, input is visually disabled and `on_value_change` won't fire.
    pub enabled: bool,
    /// Called when the user presses Enter on a single-line field.
    pub on_submit: Option<Rc<dyn Fn(String)>>,
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
            on_submit: None,
        }
    }
}

/// M3 Outlined Text Field with floating label, leading/trailing icons, and error state.
///
/// The label floats up when `value` is non-empty or when the field is focused.
/// Note: focus-based floating is approximated via animated `float_t` - the label
/// begins floating once `on_value_change` fires (i.e. when the user types).
/// For strict focus-on-tap floating, pair with an external focus signal.
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
    let th = theme();
    let label_str: Option<Rc<str>> = config.label.map(Rc::from);
    let has_label = label_str.is_some();

    // Unique animation key per label to avoid conflicts when multiple fields exist
    let anim_key = match &label_str {
        Some(l) => format!("otf_{}", &l[..l.len().min(32)]),
        None => "otf_nolabel".into(),
    };

    // Persistent focus tracker - set by layout/paint when this field is focused,
    // read here on the next frame. This gives a one-frame delay on tap-to-float,
    // which is negligible at 60fps.
    let focus_tracker: Rc<Cell<bool>> =
        remember_with_key(format!("otf_focus_{}", anim_key), || Cell::new(false));
    let is_focused = focus_tracker.get();
    let should_float = !value.is_empty() || is_focused;

    let float_t = animate_f32(
        anim_key.clone(),
        if should_float { 1.0 } else { 0.0 },
        th.motion.color,
    );

    // Border color: error > focused (float) > default
    let border_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.outline
    };

    // Label color: error > focused > default
    let label_color = if config.is_error {
        th.error
    } else if float_t > 0.5 {
        th.primary
    } else {
        th.on_surface_variant
    };

    // Label font size: 16dp at rest (placeholder position) → 12dp when floating
    let label_size = 16.0 - 4.0 * float_t;

    // Label Y offset: 16dp (same line as text) → -4dp (overlapping top border)
    let label_y = 16.0 - 20.0 * float_t;

    // The TextField inside uses no placeholder when a label is present -
    // the label itself serves as the visual placeholder.
    let tf_placeholder = if has_label {
        String::new()
    } else {
        config.placeholder.unwrap_or_default()
    };

    Box(modifier
        .clip_rounded(th.shapes.small)
        .border(1.0, border_color, th.shapes.small)
        .background(th.surface))
    .child(
        Stack(Modifier::new().fill_max_size()).child((
            // Input row - always at the same position, with room at the top
            // for the floating label to overlap.
            Row(Modifier::new()
                .fill_max_size()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 16.0,
                    bottom: 8.0,
                })
                .align_items(AlignItems::Center))
            .child((
                config.leading_icon.unwrap_or(Box(Modifier::new())),
                View::new(
                    0,
                    ViewKind::TextField {
                        state_key: 0,
                        hint: tf_placeholder,
                        multiline: false,
                        on_change: Some(Rc::new(on_value_change) as _),
                        on_submit: config.on_submit.clone().map(|f| {
                            let f = f.clone();
                            Rc::new(move |s| f(s)) as Rc<dyn Fn(String)>
                        }),
                        focus_tracker: Some(focus_tracker.clone()),
                        value: value.clone(),
                        visual_transformation: None,
                        keyboard_type: None,
                        ime_action: None,
                    },
                )
                .modifier(
                    Modifier::new()
                        .flex_grow(1.0)
                        .padding_values(PaddingValues {
                            left: 8.0,
                            right: 8.0,
                            top: 0.0,
                            bottom: 0.0,
                        }),
                )
                .semantics(Semantics {
                    role: Role::TextField,
                    label: None,
                    focused: false,
                    enabled: true,
                }),
                config.trailing_icon.unwrap_or(Box(Modifier::new())),
            )),
            // Floating label - absolutely positioned, animates between text-line
            // and top-border positions as the field gains content / focus.
            // A surface-colored background box hides the border stroke behind the label.
            if let Some(lbl) = label_str {
                Box(Modifier::new()
                    .min_width(200.0)
                    .padding_values(PaddingValues {
                        left: 20.0,
                        right: 20.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .absolute()
                    .offset(Some(0.0), Some(label_y), None, None))
                .child(
                    Box(Modifier::new()
                        .background(th.surface)
                        .padding_values(PaddingValues {
                            left: 4.0,
                            right: 4.0,
                            top: 2.0,
                            bottom: 2.0,
                        }))
                    .child(
                        Text(lbl.as_ref().to_string())
                            .color(label_color)
                            .size(label_size),
                    ),
                )
            } else {
                Box(Modifier::new())
            },
        )),
    )
}

/// M3 Checkbox.
/// Renders a 40dp touch-target with an 18dp check box inside.
/// Fill, border, and check mark animate with 100ms FastOutSlowIn.
static CHECKBOX_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Checkbox(checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    let th = theme();
    let sz = 18.0;

    let id = remember(|| CHECKBOX_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color_fast;

    let fill = animate_color(
        format!("cb_fill_{}", id),
        if checked {
            th.primary
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("cb_bw_{}", id),
        if checked { 0.0 } else { 2.0 },
        spec,
    );
    let bd = animate_color(
        format!("cb_bd_{}", id),
        if checked {
            Color::TRANSPARENT
        } else {
            th.on_surface_variant
        },
        spec,
    );
    let check_alpha = animate_f32(
        format!("cb_ca_{}", id),
        if checked { 1.0 } else { 0.0 },
        spec,
    );

    Box(Modifier::new()
        .width(40.0)
        .height(40.0)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .clickable()
        .on_pointer_down(move |_| on_change(!checked)))
    .child(
        Box(Modifier::new()
            .size(sz, sz)
            .background(fill)
            .border(bd_w, bd, 2.0)
            .clip_rounded(2.0)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center))
        .child(if check_alpha > 0.01 {
            Box(Modifier::new().alpha(check_alpha)).child(
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(th.on_primary)
                    .size(14.0),
            )
        } else {
            Box(Modifier::new())
        }),
    )
}

/// M3 RadioButton.
/// Renders a 40dp touch-target with a 20dp outer circle + inner dot.
/// Ring color animates with 100ms FastOutSlowIn; dot size animates with spring.
static RADIO_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn RadioButton(selected: bool, on_select: impl Fn() + 'static) -> View {
    let th = theme();
    let d = 20.0;

    let id = remember(|| RADIO_COUNTER.fetch_add(1, Ordering::Relaxed));
    let color_spec = th.motion.color_fast;
    let spring = th.motion.spring;

    let ring_col = animate_color(
        format!("rb_ring_{}", id),
        if selected {
            th.primary
        } else {
            th.on_surface_variant
        },
        color_spec,
    );
    let dot_size = animate_f32(
        format!("rb_dot_{}", id),
        if selected { 10.0 } else { 0.0 },
        spring,
    );

    Box(Modifier::new()
        .width(40.0)
        .height(40.0)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .clickable()
        .on_pointer_down(move |_| on_select()))
    .child(
        Box(Modifier::new()
            .size(d, d)
            .border(2.0, ring_col, d * 0.5)
            .clip_rounded(d * 0.5)
            .align_items(AlignItems::Center)
            .justify_content(JustifyContent::Center))
        .child(if dot_size > 0.5 {
            Box(Modifier::new()
                .size(dot_size, dot_size)
                .background(th.primary)
                .clip_rounded(dot_size * 0.5))
        } else {
            Box(Modifier::new())
        }),
    )
}

/// M3 Switch.
/// Renders a pill track with an animated thumb knob.
/// Thumb position, size, and colors animate with spring/tween physics.
static SWITCH_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Switch(checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    let th = theme();
    let track_w = 52.0;
    let track_h = 32.0;

    let id = remember(|| SWITCH_COUNTER.fetch_add(1, Ordering::Relaxed));

    // Thumb: spring-animated position and size
    let thumb_target_pos = if checked { track_w - 24.0 - 4.0 } else { 8.0 };
    let thumb_target_d = if checked { 24.0 } else { 16.0 };
    let spring = th.motion.spring;

    let thumb_left = animate_f32(format!("sw_pos_{}", id), thumb_target_pos, spring);
    let thumb_d = animate_f32(format!("sw_d_{}", id), thumb_target_d, spring);
    let thumb_top = (track_h - thumb_d) * 0.5;

    let color_spec = th.motion.color_fast;
    let track_bg = animate_color(
        format!("sw_tbg_{}", id),
        if checked {
            th.primary
        } else {
            th.surface_container_highest
        },
        color_spec,
    );
    let thumb_bg = animate_color(
        format!("sw_tmbg_{}", id),
        if checked { th.on_primary } else { th.outline },
        color_spec,
    );
    let track_border = animate_f32(
        format!("sw_tb_{}", id),
        if checked { 0.0 } else { 2.0 },
        color_spec,
    );
    let border_color = animate_color(
        format!("sw_bc_{}", id),
        if checked {
            Color::TRANSPARENT
        } else {
            th.outline
        },
        color_spec,
    );

    Box(Modifier::new()
        .size(track_w, track_h)
        .padding(0.0)
        .clip_rounded(track_h * 0.5)
        .background(Color::TRANSPARENT)
        .clickable()
        .on_pointer_down(move |_| on_change(!checked)))
    .child(
        Box(Modifier::new()
            .size(track_w, track_h)
            .background(track_bg)
            .border(track_border, border_color, track_h * 0.5)
            .clip_rounded(track_h * 0.5))
        .child(Box(Modifier::new()
            .size(thumb_d, thumb_d)
            .background(thumb_bg)
            .clip_rounded(thumb_d * 0.5)
            .absolute()
            .offset(Some(thumb_left), Some(thumb_top), None, None))),
    )
}

static SLIDER_COUNTER: AtomicU64 = AtomicU64::new(0);

fn snap_step(v: f32, min: f32, max: f32, step: Option<f32>) -> f32 {
    let v = v.clamp(min, max);
    if let Some(s) = step.filter(|s| *s > 0.0) {
        let t = ((v - min) / s).round();
        (min + t * s).clamp(min, max)
    } else {
        v
    }
}

fn value_from_x(x: f32, rect: Rect, min: f32, max: f32, step: Option<f32>) -> f32 {
    let w = rect.w.max(1.0);
    let t = ((x - rect.x) / w).clamp(0.0, 1.0);
    let v = min + t * (max - min);
    snap_step(v, min, max, step)
}

pub fn M3Slider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("ms_rect_{}", id), || Rect::default());
    let drag_active = remember_state_with_key(format!("ms_da_{}", id), || false);

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let th = theme();
    let range_size = (max - min).max(1e-6);
    let t = ((value - min) / range_size).clamp(0.0, 1.0);

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    Box(Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(16.0);
            let thumb_w = dp_to_px(4.0);
            let thumb_h = dp_to_px(44.0);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = dp_to_px(8.0);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            // Thumb center: for steps, clamp within corner inset (Compose SliderImpl:924-930)
            let kx = if step.is_some() && !tick_frac.is_empty() {
                let is_first = (t - tick_frac[0]).abs() < 1e-6;
                let is_last = (t - tick_frac[tick_frac.len() - 1]).abs() < 1e-6;
                if is_first || is_last {
                    track_x + t * track_w
                } else {
                    track_x + (track_w - track_h) * t + corner
                }
            } else {
                track_x + t * track_w
            };

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            // Inactive track (after thumb gap)
            let inactive_x = track_x.max(kx + gap);
            let inactive_w = (track_x + track_w - inactive_x).max(0.0);
            if inactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: inactive_x,
                        y: cy - track_h * 0.5,
                        w: inactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.secondary_container)),
                    radius: corner,
                });
            }
            // Active track fill (from left to thumb gap)
            let fill_w = (kx - gap - track_x).max(0.0);
            if fill_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: fill_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                    radius: corner,
                });
            }
            // Tick marks at step positions (skipping gap region)
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for &tf in &tick_frac {
                let tx = tick_start + tf * (tick_end - tick_start);
                if tx >= kx - gap && tx <= kx + gap {
                    continue;
                }
                let on_active = tx <= kx - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active {
                        th.secondary_container
                    } else {
                        th.primary
                    })),
                });
            }
            // Stop indicator at right end (only when inactive track visible)
            if inactive_w > 0.0 {
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                });
            }
            // Thumb pill (shrinks to 2dp when dragging, matching Compose Thumb:2469-2478)
            let da = *drag_active_p.borrow();
            let tw = if da { thumb_w * 0.5 } else { thumb_w };
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: kx - tw * 0.5,
                    y: cy - thumb_h * 0.5,
                    w: tw,
                    h: thumb_h,
                },
                brush: Brush::Solid(mul_c(th.primary)),
                radius: tw * 0.5,
            });
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            move |pe: PointerEvent| {
                *drag_active.borrow_mut() = true;
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position.x, r, min, max, step));
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            move |pe: PointerEvent| {
                if !*drag_active.borrow() {
                    return;
                }
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position.x, r, min, max, step));
            }
        })
        .on_pointer_up(move |_pe: PointerEvent| {
            *drag_active.borrow_mut() = false;
        })
        .on_scroll({
            let oc = oc.clone();
            move |d: Vec2| -> Vec2 {
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let new_val = snap_step(value + (dir as f32) * step_val, min, max, step);
                if (new_val - value).abs() > 1e-6 {
                    (oc)(new_val);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        }))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn M3RangeSlider(
    start: f32,
    end: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32, f32) + 'static,
) -> View {
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("mrs_rect_{}", id), || Rect::default());
    let drag_active = remember_state_with_key(format!("mrs_da_{}", id), || false);
    let active_thumb = remember_state_with_key(format!("mrs_at_{}", id), || false);

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let th = theme();
    let range_size = (max - min).max(1e-6);
    let t0 = ((start - min) / range_size).clamp(0.0, 1.0);
    let t1 = ((end - min) / range_size).clamp(0.0, 1.0);

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let active_thumb_p = active_thumb.clone();

    Box(Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(16.0);
            let thumb_w = dp_to_px(4.0);
            let thumb_h = dp_to_px(44.0);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = dp_to_px(8.0);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            // Thumb centers with corner-inset for steps
            let thumb_pos = |tf: f32, fracs: &[f32]| {
                if step.is_some() && !fracs.is_empty() {
                    let is_first = (tf - fracs[0]).abs() < 1e-6;
                    let is_last = (tf - fracs[fracs.len() - 1]).abs() < 1e-6;
                    if is_first || is_last {
                        track_x + tf * track_w
                    } else {
                        track_x + (track_w - track_h) * tf + corner
                    }
                } else {
                    track_x + tf * track_w
                }
            };
            let k0 = thumb_pos(t0, &tick_frac);
            let k1 = thumb_pos(t1, &tick_frac);
            let active_l = k0.min(k1);
            let active_r = k0.max(k1);

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            let linactive_w = (active_l - gap - track_x).max(0.0);
            if linactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: linactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.secondary_container)),
                    radius: corner,
                });
            }
            let rinactive_x = (active_r + gap).min(track_x + track_w);
            let rinactive_w = (track_x + track_w - rinactive_x).max(0.0);
            if rinactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rinactive_x,
                        y: cy - track_h * 0.5,
                        w: rinactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.secondary_container)),
                    radius: corner,
                });
            }
            // Active range between thumbs
            let active_w = (active_r - gap - (active_l + gap)).max(0.0);
            if active_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: active_l + gap,
                        y: cy - track_h * 0.5,
                        w: active_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                    radius: corner,
                });
            }
            // Tick marks at step positions (skipping gap regions)
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for &tf in &tick_frac {
                let tx = tick_start + tf * (tick_end - tick_start);
                if tx >= active_l - gap && tx <= active_r + gap {
                    continue;
                }
                let on_active = tx >= active_l + gap && tx <= active_r - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active {
                        th.secondary_container
                    } else {
                        th.primary
                    })),
                });
            }
            // Stop indicators (only when corresponding inactive track visible)
            if linactive_w > 0.0 {
                let sx0 = track_x + corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx0 - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                });
            }
            if rinactive_w > 0.0 {
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                });
            }
            // Thumb pills (shrink to 2dp when dragging that specific thumb)
            let da = *drag_active_p.borrow();
            let at = *active_thumb_p.borrow();
            let thumb_sizes = [
                (k0, if da && !at { thumb_w * 0.5 } else { thumb_w }),
                (k1, if da && at { thumb_w * 0.5 } else { thumb_w }),
            ];
            for &(kx, tw) in &thumb_sizes {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: kx - tw * 0.5,
                        y: cy - thumb_h * 0.5,
                        w: tw,
                        h: thumb_h,
                    },
                    brush: Brush::Solid(mul_c(th.primary)),
                    radius: tw * 0.5,
                });
            }
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |pe: PointerEvent| {
                *drag_active.borrow_mut() = true;
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position.x, r, min, max, step);
                let use_end = (v - end).abs() < (v - start).abs();
                *active_thumb.borrow_mut() = use_end;
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |pe: PointerEvent| {
                if !*drag_active.borrow() {
                    return;
                }
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position.x, r, min, max, step);
                let use_end = *active_thumb.borrow();
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
            }
        })
        .on_pointer_up({
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            move |_pe: PointerEvent| {
                *drag_active.borrow_mut() = false;
                *active_thumb.borrow_mut() = false;
            }
        })
        .on_scroll({
            let oc = oc.clone();
            let active_thumb = active_thumb.clone();
            move |d: Vec2| -> Vec2 {
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let use_end = *active_thumb.borrow();
                let (mut a, mut b) = (start, end);
                if use_end {
                    b = snap_step(end + (dir as f32) * step_val, min, max, step).max(a);
                } else {
                    a = snap_step(start + (dir as f32) * step_val, min, max, step).min(b);
                }
                if (a - start).abs() > 1e-6 || (b - end).abs() > 1e-6 {
                    (oc)(a, b);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        }))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}
