#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, Row, Text, TextStyle, ViewExt};

/// M3 Top App Bar (small). Displays a title with optional navigation icon and
/// trailing action buttons.
pub fn TopAppBar(
    title: impl Into<String>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
) -> View {
    let th = theme();
    Row(Modifier::new()
        .fill_max_width()
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

/// M3 Icon Button — a tappable circular container for an icon.
pub fn IconButton(
    icon: View,
    on_click: impl Fn() + 'static,
) -> View {
    let th = theme();
    let bg = Color::TRANSPARENT;
    Box(Modifier::new()
        .size(40.0, 40.0)
        .clip_rounded(20.0)
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

/// M3 Filled Icon Button — icon button with a filled container background.
pub fn FilledIconButton(
    icon: View,
    on_click: impl Fn() + 'static,
) -> View {
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

/// M3 Filled Button — prominent action button with primary color fill.
pub fn FilledButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let content = with_content_color(th.on_primary, content);
    let bg = th.primary;
    Box(Modifier::new()
        .height(40.0)
        .min_width(48.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_primary.with_alpha_f32(0.08),
            pressed: th.on_primary.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 8.0,
            disabled: 0.0,
        })
        .clip_rounded(20.0)
        .padding_values(PaddingValues {
            left: 24.0,
            right: 24.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(modifier))
    .child(content)
}

/// M3 Filled Tonal Button — uses secondary container colors.
pub fn FilledTonalButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let content = with_content_color(th.on_secondary_container, content);
    let bg = th.secondary_container;
    Box(Modifier::new()
        .height(40.0)
        .min_width(48.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_secondary_container.with_alpha_f32(0.08),
            pressed: th.on_secondary_container.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        })
        .state_elevation(StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 8.0,
            disabled: 0.0,
        })
        .clip_rounded(20.0)
        .padding_values(PaddingValues {
            left: 24.0,
            right: 24.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(modifier))
    .child(content)
}

/// M3 Outlined Button — button with an outline border and no fill.
pub fn OutlinedButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let content = with_content_color(th.on_surface, content);
    let bg = Color::TRANSPARENT;
    Box(Modifier::new()
        .height(40.0)
        .min_width(48.0)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .border(1.0, th.outline_variant, 20.0)
        .clip_rounded(20.0)
        .padding_values(PaddingValues {
            left: 24.0,
            right: 24.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(modifier))
    .child(content)
}

/// M3 Text Button — a low-emphasis button.
pub fn TextButton(
    modifier: Modifier,
    on_click: impl Fn() + 'static,
    content: impl FnOnce() -> View,
) -> View {
    let th = theme();
    let content = with_content_color(th.on_surface, content);
    let bg = Color::TRANSPARENT;
    Box(Modifier::new()
        .height(40.0)
        .min_width(48.0)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .clip_rounded(20.0)
        .padding_values(PaddingValues {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        })
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()
        .on_pointer_down(move |_| on_click())
        .then(modifier))
    .child(content)
}

/// M3 Floating Action Button (regular, 56dp).
pub fn FAB(
    icon: View,
    on_click: impl Fn() + 'static,
) -> View {
    let th = theme();
    let bg = th.primary_container;
    Box(Modifier::new()
        .size(56.0, 56.0)
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
        .clip_rounded(28.0)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()

        .on_pointer_down(move |_| on_click()))
    .child(icon)
}

/// M3 Large FAB (96dp).
pub fn LargeFAB(
    icon: View,
    on_click: impl Fn() + 'static,
) -> View {
    let th = theme();
    let bg = th.primary_container;
    Box(Modifier::new()
        .size(96.0, 96.0)
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
        .clip_rounded(28.0)
        .align_items(AlignItems::Center)
        .justify_content(JustifyContent::Center)
        .clickable()

        .on_pointer_down(move |_| on_click()))
    .child(icon)
}

/// M3 Extended FAB — FAB with icon + label.
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

/// M3 Horizontal Divider — a thin 1dp line.
pub fn Divider() -> View {
    let th = theme();
    Box(Modifier::new()
        .fill_max_width()
        .height(1.0)
        .background(th.outline_variant))
}

/// M3 Vertical Divider — a thin 1dp vertical line.
pub fn VerticalDivider() -> View {
    let th = theme();
    Box(Modifier::new()
        .width(1.0)
        .fill_max_height()
        .background(th.outline_variant))
}

/// M3 Badge — a small notification indicator. If `label` is `None`, shows a
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

/// M3 List Item — a single row in a list with optional leading/trailing content.
pub fn ListItem(
    headline: impl Into<String>,
    supporting_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
) -> View {
    let th = theme();
    let mut modifier = Modifier::new()
        .fill_max_width()
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

/// M3 Tab Row — a horizontal row of tabs with an active indicator.
pub fn TabRow(selected_index: usize, tabs: Vec<Tab>) -> View {
    let th = theme();
    Row(Modifier::new()
        .fill_max_width()
        .height(48.0)
        .background(th.surface))
    .child(
        tabs.into_iter()
            .enumerate()
            .map(|(i, tab)| {
                let selected = i == selected_index;
                let color = if selected {
                    th.primary
                } else {
                    th.on_surface_variant
                };
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
                    if selected {
                        Box(Modifier::new()
                            .fill_max_width()
                            .height(3.0)
                            .background(th.primary)
                            .clip_rounded(1.5))
                    } else {
                        Box(Modifier::new().height(3.0))
                    },
                ))
            })
            .collect::<Vec<_>>(),
    )
}

/// A single segment definition for `SegmentedButton`.
pub struct Segment {
    pub label: String,
    pub icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
}

/// M3 Segmented Button — a row of toggle segments. `selected` contains the
/// indices of selected segments (single-select: pass a single-element set).
pub fn SegmentedButton(selected: &[usize], segments: Vec<Segment>) -> View {
    let th = theme();
    let count = segments.len();

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
                let bg = if is_selected {
                    th.secondary_container
                } else {
                    Color::TRANSPARENT
                };
                let fg = if is_selected {
                    th.on_secondary_container
                } else {
                    th.on_surface
                };
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

/// M3 Circular Progress Indicator. Uses the built-in `ProgressBar` view kind
/// with `circular: true`.
///
/// - `value`: `Some(0.0..=1.0)` for determinate, `None` for indeterminate.
pub fn CircularProgressIndicator(value: Option<f32>) -> View {
    View::new(
        0,
        ViewKind::ProgressBar {
            value: value.unwrap_or(0.0),
            min: 0.0,
            max: 1.0,
            circular: true,
        },
    )
    .modifier(Modifier::new().size(48.0, 48.0))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}
