#![allow(non_snake_case)]

pub mod defaults;
pub use defaults::*;

mod components;
pub use components::*;

pub mod dialog;
pub use dialog::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::Duration;

use crate::{Icon, Symbol};
use repose_core::animation::{AnimationSpec, Easing, RepeatableSpec};
use repose_core::*;
use repose_ui::LazyRowState;
use repose_ui::lazy::LazyRow;
use repose_ui::{
    BasicTextField as UiTextField, Box, Column, Row, Spacer, Stack, Text, TextStyle, ViewExt, ZStack,
    anim::{animate_color, animate_f32, animate_f32_from},
    overlay::OverlayHandle,
    overlay::SnackbarAction,
    overlay::snackbar_is_dismissing,
};

pub(crate) fn alert_dialog_body(
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
) -> View {
    Column(Modifier::new()).child((
        title,
        Box(Modifier::new().fill_max_width().height(16.0)),
        text,
        Spacer(),
        Row(Modifier::new()).child((
            dismiss_button.unwrap_or(Box(Modifier::new())),
            Spacer(),
            confirm_button,
        )),
    ))
}

static BOTTOMSHEET_COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn BottomSheet(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
    config: BottomSheetConfig, // HACK: use ot
) -> View {
    let th = theme();
    let id = remember(|| BOTTOMSHEET_COUNTER.fetch_add(1, Ordering::Relaxed));

    let opacity = animate_f32_from(
        format!("bs_opacity_{id}"),
        if visible { 0.0 } else { 1.0 },
        if visible { 1.0 } else { 0.0 },
        th.motion.layout,
    );

    let keep = visible || opacity > 0.01;
    if keep {
        Column(Modifier::new()).child((
            Box(modifier.alpha(opacity)).child(content),
            Box(Modifier::new()
                .width(1.0)
                .height(0.0)
                .fill_max_width()
                .alpha(opacity)
                .hit_passthrough()
                .on_pointer_down(move |_| on_dismiss())),
        ))
    } else {
        Box(Modifier::new())
    }
}

static NAVBAR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Navigation Bar - a bottom navigation bar with animated selection.
/// Colors and indicator background transition with 200ms FastOutSlowIn.
pub fn NavigationBar(
    selected_index: usize,
    items: Vec<NavItem>,
    config: NavigationBarConfig,
) -> View {
    let th = theme();
    let id = remember(|| NAVBAR_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.shape;

    let mut bar_m = Modifier::new()
        .fill_max_size()
        .min_height(config.height)
        .background(config.container_color)
        .padding(8.0)
        .then(config.modifier);

    if config.tonal_elevation > 0.0 {
        bar_m = bar_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    Box(bar_m).child(
        Row(Modifier::new().fill_max_size().align_items(AlignItems::Center)).child(
            items
                .into_iter()
                .enumerate()
                .map(|(i, item)| {
                    let selected = i == selected_index;
                    let is_enabled = item.enabled;
                    let fg = animate_color(
                        format!("nb_fg_{}_{}", id, i),
                        if selected {
                            config.selected_icon_color
                        } else {
                            config.unselected_icon_color
                        },
                        spec,
                    );
                    let bg_alpha = animate_f32(
                        format!("nb_bg_{}_{}", id, i),
                        if selected { 1.0 } else { 0.0 },
                        spec,
                    );
                    let indicator_bg = config
                        .indicator_color
                        .with_alpha_f32(bg_alpha * config.indicator_opacity);
                    let cb = item.on_click.clone();

                    let mut item_m = Modifier::new()
                        .flex_grow(1.0)
                        .padding_values(PaddingValues {
                            left: config.item_horizontal_padding,
                            right: config.item_horizontal_padding,
                            top: config.item_vertical_padding,
                            bottom: config.item_vertical_padding,
                        })
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center)
                        .background(indicator_bg)
                        .clip_rounded(config.indicator_radius)
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        });

                    if is_enabled {
                        item_m = item_m.clickable().on_pointer_down(move |_| cb());
                    }

                    Column(item_m).child((
                        item.icon,
                        Text(item.label)
                            .color(fg)
                            .size(th.typography.label_medium)
                            .single_line(),
                    ))
                })
                .collect::<Vec<_>>(),
        ),
    )
}

pub struct NavItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
}

pub fn Snackbar(
    message: impl Into<String>,
    action: Option<SnackbarAction>,
    modifier: Modifier,
    config: SnackbarConfig,
) -> View {
    let msg = message.into();
    let th = theme();
    let bg = config.container_color;
    let fg = config.content_color;
    let action_color = config.action_color;

    let dismissing = snackbar_is_dismissing();

    let slide_target = if dismissing { 80.0 } else { 0.0 };
    let slide = animate_f32_from("snackbar_slide", 80.0, slide_target, th.motion.overlay);

    let alpha_target = if dismissing { 0.0 } else { 1.0 };
    let alpha = animate_f32_from("snackbar_alpha", 0.0, alpha_target, th.motion.overlay);

    let snackbar = Box(Modifier::new()
        .translate(0.0, slide)
        .alpha(alpha)
        .min_height(48.0)
        .min_width(280.0)
        .max_width(600.0)
        .background(bg)
        .clip_rounded(config.shape_radius));

    let snackbar = if config.action_on_new_line {
        snackbar.child(Column(Modifier::new().padding_values(PaddingValues {
            left: 16.0,
            right: 8.0,
            top: 0.0,
            bottom: 0.0,
        })).child((
            Text(msg)
                .modifier(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 0.0,
                    top: 14.0,
                    bottom: 14.0,
                }))
                .color(fg)
                .size(th.typography.body_medium)
                .max_lines(2)
                .overflow_ellipsize(),
            action
                .map(|a| {
                    let label = a.label.clone();
                    Row(Modifier::new().fill_max_width().justify_content(repose_core::JustifyContent::End)).child(
                        TextButton(
                            Modifier::new(),
                            move || (a.on_click)(),
                            ButtonConfig::default(),
                            || {
                                Text(label)
                                    .color(action_color)
                                    .size(th.typography.label_large)
                                    .single_line()
                            },
                        )
                    )
                })
                .unwrap_or(Box(Modifier::new())),
        )))
    } else {
        snackbar.child(
            Row(Modifier::new()
                .fill_max_width()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .align_items(repose_core::AlignItems::Center))
            .child((
                Text(msg)
                    .modifier(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 0.0,
                        top: 14.0,
                        bottom: 14.0,
                    }))
                    .color(fg)
                    .size(th.typography.body_medium)
                    .max_lines(2)
                    .overflow_ellipsize(),
                Spacer(),
                action
                    .map(|a| {
                        let label = a.label.clone();
                        TextButton(
                            Modifier::new(),
                            move || (a.on_click)(),
                            ButtonConfig::default(),
                            || {
                                Text(label)
                                    .color(action_color)
                                    .size(th.typography.label_large)
                                    .single_line()
                            },
                        )
                    })
                    .unwrap_or(Box(Modifier::new())),
            ))
        )
    };

    Box(Modifier::new()
        .absolute()
        .offset_bottom(0.0)
        .fill_max_width()
        .justify_content(repose_core::JustifyContent::Center)
        .then(modifier))
    .child(snackbar)
}

pub fn FilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let is_enabled = config.enabled;
    let colors = &config.colors;

    let bg = animate_color(
        format!("fc_bg_{}", id),
        colors.container(is_enabled, selected),
        spec,
    );
    let label_color = animate_color(
        format!("fc_lc_{}", id),
        colors.label(is_enabled, selected),
        spec,
    );
    let leading_color = animate_color(
        format!("fc_lic_{}", id),
        colors.leading_icon(is_enabled, selected),
        spec,
    );
    let trailing_color = animate_color(
        format!("fc_tic_{}", id),
        colors.trailing_icon(is_enabled, selected),
        spec,
    );
    let border = if !is_enabled {
        if selected { config.disabled_selected_border_color } else { config.disabled_border_color }
    } else {
        if selected { config.selected_border_color } else { config.border_color }
    };
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
            pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

/// M3 Elevated Filter Chip - like [`FilterChip`] but with elevation and filled container.
pub fn ElevatedFilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let is_enabled = config.enabled;
    let colors = &config.colors;

    let bg = animate_color(
        format!("efc_bg_{}", id),
        colors.container(is_enabled, selected),
        spec,
    );
    let label_color = animate_color(
        format!("efc_lc_{}", id),
        colors.label(is_enabled, selected),
        spec,
    );
    let leading_color = animate_color(
        format!("efc_lic_{}", id),
        colors.leading_icon(is_enabled, selected),
        spec,
    );
    let trailing_color = animate_color(
        format!("efc_tic_{}", id),
        colors.trailing_icon(is_enabled, selected),
        spec,
    );
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
            pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn SuggestionChip(
    on_click: impl Fn() + 'static,
    label: View,
    icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let border = if is_enabled { config.border_color } else { config.disabled_border_color };
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            icon.map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(with_content_color(leading_color, move || v))
            })
            .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
        )),
    )
}

/// M3 Elevated Suggestion Chip - like [`SuggestionChip`] but with elevation and filled bg.
pub fn ElevatedSuggestionChip(
    on_click: impl Fn() + 'static,
    label: View,
    icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            icon.map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(with_content_color(leading_color, move || v))
            })
            .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
        )),
    )
}

pub fn InputChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    avatar: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let is_enabled = config.enabled;
    let colors = &config.colors;

    let bg = animate_color(
        format!("ic_bg_{}", id),
        colors.container(is_enabled, selected),
        spec,
    );
    let label_color = animate_color(
        format!("ic_lc_{}", id),
        colors.label(is_enabled, selected),
        spec,
    );
    let leading_color = animate_color(
        format!("ic_lic_{}", id),
        colors.leading_icon(is_enabled, selected),
        spec,
    );
    let trailing_color = animate_color(
        format!("ic_tic_{}", id),
        colors.trailing_icon(is_enabled, selected),
        spec,
    );
    let border = if !is_enabled {
        if selected { config.disabled_selected_border_color } else { config.disabled_border_color }
    } else {
        if selected { config.selected_border_color } else { config.border_color }
    };
    let shape = config.shape_radius;

    let mut m = Modifier::new()
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
            pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 8.0,
            bottom: 8.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    if is_enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            avatar
                .or(leading_icon)
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn Scaffold(
    top_bar: Option<View>,
    bottom_bar: Option<View>,
    floating_action_button: Option<View>,
    content: impl Fn(PaddingValues) -> View,
) -> View {
    let insets = window_insets();

    let content_padding = PaddingValues {
        top: if top_bar.is_some() { 64.0 } else { insets.top },
        bottom: if bottom_bar.is_some() {
            80.0 + insets.bottom + insets.ime_bottom
        } else {
            insets.bottom + insets.ime_bottom
        },
        left: insets.left,
        right: insets.right,
    };

    Stack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                top: if top_bar.is_some() {
                    64.0 + insets.top
                } else {
                    0.0
                },
                bottom: if bottom_bar.is_some() {
                    80.0 + insets.bottom + insets.ime_bottom
                } else {
                    insets.bottom + insets.ime_bottom
                },
                ..Default::default()
            }))
        .child(content(content_padding)),
        if let Some(bar) = top_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(insets.top), Some(0.0), None))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(bar) = bottom_bar {
            Box(Modifier::new().absolute().offset(
                Some(0.0),
                None,
                Some(insets.bottom + insets.ime_bottom),
                Some(0.0),
            ))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(fab) = floating_action_button {
            Box(Modifier::new().absolute().offset(
                None,
                None,
                Some(16.0 + insets.bottom + insets.ime_bottom),
                Some(16.0),
            ))
            .child(fab)
        } else {
            Box(Modifier::new())
        },
    ))
}

/// State controlling tooltip visibility.
pub struct TooltipState {
    visible: Signal<bool>,
}

impl TooltipState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            visible: signal(false),
        })
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// Wraps `content` with a tooltip label shown above it when `state` is visible.
///
/// Usage:
/// ```ignore
/// let tip = TooltipState::new();
/// TooltipBox("I'm a tooltip", tip.clone(), Modifier::new(), Button("Hover me", {
///     let tip = tip.clone();
///     move || tip.show()
/// }));
/// ```
pub fn TooltipBox(
    text: impl Into<String>,
    state: Rc<TooltipState>,
    modifier: Modifier,
    content: View,
) -> View {
    let text: Rc<str> = Rc::from(text.into());
    let th = theme();
    let spec = th.motion.overlay;

    let alpha = animate_f32(
        "tooltip_alpha",
        if state.is_visible() { 1.0 } else { 0.0 },
        spec,
    );

    let tooltip_visible = state.is_visible() || alpha > 0.01;
    let scale = 0.92 + 0.08 * alpha;

    Stack(modifier).child((
        Box(Modifier::new().fill_max_size()).child(content),
        if tooltip_visible {
            Box(Modifier::new()
                .background(th.inverse_surface)
                .clip_rounded(th.shapes.extra_small)
                .padding_values(PaddingValues {
                    left: 8.0,
                    right: 8.0,
                    top: 4.0,
                    bottom: 4.0,
                })
                .absolute()
                .offset(None, Some(-28.0), None, None)
                .align_self(AlignSelf::Center)
                .render_z_index(10000.0)
                .alpha(alpha)
                .scale(scale))
            .child(
                Text((*text).to_string())
                    .color(th.inverse_on_surface)
                    .size(th.typography.label_medium)
                    .single_line(),
            )
        } else {
            Box(Modifier::new())
        },
    ))
}

/// State controlling drawer open/close.
pub struct DrawerState {
    visible: Signal<bool>,
}

impl DrawerState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            visible: signal(false),
        })
    }

    pub fn is_open(&self) -> bool {
        self.visible.get()
    }

    pub fn open(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// A modal navigation drawer that slides in from the left with a scrim overlay.
pub fn ModalNavigationDrawer(
    drawer_state: Rc<DrawerState>,
    drawer_content: View,
    content: View,
    config: NavigationDrawerConfig,
) -> View {
    let th = theme();

    let drawer_offset = animate_f32(
        "modal_drawer_offset",
        if drawer_state.is_open() { 0.0 } else { -360.0 },
        theme().motion.spring,
    );

    let mut drawer_m = Modifier::new()
        .absolute()
        .offset(Some(drawer_offset), Some(0.0), None, Some(0.0))
        .fill_max_height()
        .width(config.width)
        .background(config.container_color)
        .clip_rounded(config.shape_radius);

    if config.tonal_elevation > 0.0 {
        drawer_m = drawer_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    ZStack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new().fill_max_size().background(config.content_color)).child(content),
        if drawer_state.is_open() {
            Box(Modifier::new()
                .fill_max_size()
                .background(config.scrim_color)
                .clickable()
                .on_pointer_down({
                    let ds = drawer_state.clone();
                    move |_| ds.dismiss()
                }))
            .child(Box(Modifier::new()))
        } else {
            Box(Modifier::new())
        },
        Box(drawer_m).child(drawer_content),
    ))
}

/// M3 Dismissible Navigation Drawer - slides alongside content without scrim.
/// Uses [`DrawerState`] to control open/close.
pub fn DismissibleNavigationDrawer(
    drawer_state: Rc<DrawerState>,
    drawer_content: View,
    content: View,
    config: NavigationDrawerConfig,
) -> View {
    let th = theme();
    let drawer_offset = animate_f32(
        "dismissible_drawer_offset",
        if drawer_state.is_open() { 0.0 } else { -360.0 },
        theme().motion.spring,
    );

    let mut drawer_m = Modifier::new()
        .absolute()
        .offset(Some(drawer_offset), Some(0.0), None, Some(0.0))
        .fill_max_height()
        .width(config.width)
        .background(config.container_color)
        .clip_rounded(config.shape_radius);

    if config.tonal_elevation > 0.0 {
        drawer_m = drawer_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    ZStack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new().fill_max_size().background(config.content_color)).child(content),
        Box(drawer_m).child(drawer_content),
    ))
}

/// M3 Permanent Navigation Drawer - always visible alongside content.
pub fn PermanentNavigationDrawer(
    drawer_content: View,
    content: View,
    config: NavigationDrawerConfig,
) -> View {
    Row(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .width(config.width)
            .fill_max_height()
            .background(config.container_color))
        .child(Box(Modifier::new()).color(config.content_color).child(drawer_content)),
        Box(Modifier::new().flex_grow(1.0)).child(content),
    ))
}

/// A destination entry inside a NavigationDrawer.
pub fn NavigationDrawerItem(
    label: View,
    selected: bool,
    on_click: impl Fn() + 'static,
    icon: Option<View>,
    badge: Option<View>,
    enabled: bool,
) -> View {
    let th = theme();
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let bg = animate_color(
        format!("ndi_bg_{}", id),
        if selected {
            th.secondary_container
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let fg = animate_color(
        format!("ndi_fg_{}", id),
        if selected {
            th.on_secondary_container
        } else {
            th.on_surface_variant
        },
        spec,
    );

    let mut m = Modifier::new()
        .fill_max_width()
        .padding_values(PaddingValues {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        })
        .min_height(56.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .clip_rounded(th.shapes.large);

    if enabled {
        m = m.clickable().on_pointer_down(move |_| on_click());
    }

    Box(m).child(with_content_color(fg, || {
        Row(Modifier::new()
            .align_items(AlignItems::Center)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 24.0,
                top: 0.0,
                bottom: 0.0,
            }))
        .child((
            icon.unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
            Box(Modifier::new().width(12.0).height(1.0)),
            Box(Modifier::new().flex_grow(1.0)).child(label),
            badge.unwrap_or(Box(Modifier::new())),
        ))
    }))
}

/// A single item inside a `DropdownMenu`.
#[derive(Clone)]
pub struct DropdownMenuItem {
    pub text: String,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        Self {
            text: text.into(),
            leading_icon: None,
            trailing_icon: None,
            on_click: Rc::new(on_click),
            enabled: true,
        }
    }

    pub fn leading_icon(mut self, icon: View) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    pub fn trailing_icon(mut self, icon: View) -> Self {
        self.trailing_icon = Some(icon);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// A menu divider line.
pub struct MenuDivider;

/// State for controlling `DropdownMenu` visibility.
pub struct MenuState {
    visible: Signal<bool>,
    anchor: Signal<Option<Vec2>>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            visible: signal(false),
            anchor: signal(None),
        }
    }

    pub fn is_open(&self) -> bool {
        self.visible.get()
    }

    pub fn open(&self) {
        self.visible.set(true);
    }

    pub fn open_at(&self, screen_pos: Vec2) {
        self.anchor.set(Some(screen_pos));
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

static DROPDOWN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Dropdown Menu anchored to a trigger element.
///
/// Renders a full-screen scrim in the overlay (to dismiss taps outside the card)
/// while keeping the menu positioned inline below the trigger for correct position.
/// The menu card fades and scales in/out with a 120ms FastOutSlowIn animation.
/// Items can be `DropdownMenuItem` or `MenuDivider`.
pub fn DropdownMenu(
    state: Rc<MenuState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    trigger: View,
    items: Vec<DropdownMenuEntry>,
    config: DropdownMenuConfig,
) -> View {
    let th = theme();
    let ddm_id = remember(|| DROPDOWN_COUNTER.fetch_add(1, Ordering::Relaxed));
    let overlay_id = remember_with_key(format!("ddm_oid_{}", ddm_id), || signal(0u64));

    // Animated open/close progress
    let anim = remember_state_with_key(format!("ddm_anim_{}", ddm_id), || {
        AnimatedValue::new(0.0, theme().motion.overlay)
    });
    let last_target = remember_state_with_key(format!("ddm_lt_{}", ddm_id), || f32::NAN);
    let anim_target = if state.is_open() { 1.0 } else { 0.0 };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            a.set_target(anim_target);
            *lt = anim_target;
        }
        drop(lt);
        if a.update() {
            request_frame();
        }
    }

    let progress = *anim.borrow().get();
    let menu_visible = state.is_open() || progress > 0.01;

    // Scrim overlay - keep alive during exit animation
    if menu_visible {
        if overlay_id.get() == 0 {
            let scrim = Box(Modifier::new().fill_max_size().absolute().on_pointer_down({
                let s = state.clone();
                move |_| s.dismiss()
            }));
            let id = overlay.show_with(scrim, 899.0, true);
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
    }

    let scale = 0.92 + 0.08 * progress;
    let alpha = progress;

    Stack(modifier).child((
        trigger,
        if menu_visible {
            Box(Modifier::new()
                .absolute()
                .offset(None, Some(40.0), None, None)
                .render_z_index(900.0)
                .scale(scale)
                .alpha(alpha))
            .child(render_dropdown_menu_content(
                &th,
                &items,
                state.clone(),
                &config,
            ))
        } else {
            Box(Modifier::new())
        },
    ))
}

/// Either a menu item or a divider.
#[derive(Clone)]
pub enum DropdownMenuEntry {
    Item(DropdownMenuItem),
    Divider,
}

fn render_dropdown_menu_content(
    th: &Theme,
    items: &[DropdownMenuEntry],
    state: Rc<MenuState>,
    config: &DropdownMenuConfig,
) -> View {
    let children: Vec<View> = items
        .iter()
        .map(|entry| match entry {
            DropdownMenuEntry::Item(item) => {
                let text_color = if item.enabled {
                    config.item_text_color
                } else {
                    config.disabled_item_text_color
                };
                let on_click = item.on_click.clone();
                let state = state.clone();
                let mut modifier = Modifier::new()
                    .fill_max_width()
                    .min_height(40.0)
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .align_items(AlignItems::Center);

                if item.enabled {
                    modifier = modifier
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        })
                        .clickable()
                        .on_pointer_down(move |_| {
                            on_click();
                            state.dismiss();
                        });
                }

                Row(modifier).child((
                    item.leading_icon
                        .clone()
                        .unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
                    Box(Modifier::new().width(12.0).fill_max_height()),
                    Box(Modifier::new().flex_grow(1.0)).child(
                        Text(item.text.clone())
                            .color(text_color)
                            .size(th.typography.body_large)
                            .single_line(),
                    ),
                    item.trailing_icon.clone().unwrap_or(Box(Modifier::new())),
                ))
            }
            DropdownMenuEntry::Divider => Box(Modifier::new()
                .fill_max_width()
                .height(1.0)
                .margin(12.0)
                .background(config.divider_color)),
        })
        .collect();

    Box(Modifier::new()
        .state_elevation(StateElevation {
            default: th.elevation.level2,
            hovered: th.elevation.level3,
            pressed: th.elevation.level3,
            disabled: 0.0,
        })
        .min_width(config.min_width)
        .padding(4.0)
        .background(config.container_color)
        .clip_rounded(th.shapes.small))
    .child(Column(Modifier::new()).with_children(children))
}

/// State for `SearchBar` - manages expanded/collapsed and query text.
pub struct SearchBarState {
    pub query: Signal<String>,
    pub expanded: Signal<bool>,
    pub active: Signal<bool>,
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBarState {
    pub fn new() -> Self {
        Self {
            query: signal(String::new()),
            expanded: signal(false),
            active: signal(false),
        }
    }

    pub fn query(&self) -> String {
        self.query.get()
    }

    pub fn set_query(&self, q: String) {
        self.query.set(q);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn expand(&self) {
        self.expanded.set(true);
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
        self.active.set(false);
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn activate(&self) {
        self.active.set(true);
        self.expanded.set(true);
    }

    pub fn deactivate(&self) {
        self.active.set(false);
    }
}

/// M3 Search Bar - an expandable search input with leading icon and optional
/// suggestions that expand below the search bar (pushing content down).
pub fn SearchBar(
    state: Rc<SearchBarState>,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    placeholder: impl Into<String>,
    on_query_change: Option<Rc<dyn Fn(String)>>,
    content: View,
) -> View {
    let th = theme();
    let placeholder = placeholder.into();
    let expanded = state.is_expanded();
    let query = state.query();
    let active = state.is_active();

    let width = animate_f32(
        "searchbar_width",
        if expanded { 360.0 } else { 240.0 },
        theme().motion.expand,
    );

    let input_field: View = if active {
        UiTextField(
            placeholder.clone(),
            query.clone(),
            Modifier::new().flex_grow(1.0).padding(4.0),
            repose_ui::BasicTextFieldConfig {
                on_change: Some({
                    let s = state.clone();
                    let cb = on_query_change.clone();
                    Rc::new(move |text| {
                        s.set_query(text);
                        if let Some(ref cb) = cb {
                            cb(s.query());
                        }
                    })
                }),
                ..Default::default()
            },
        )
        .color(th.on_surface)
        .size(th.typography.body_large)
    } else {
        Box(Modifier::new().flex_grow(1.0)).child(
            Text(if query.is_empty() {
                placeholder.clone()
            } else {
                query.clone()
            })
            .color(if query.is_empty() {
                th.on_surface_variant
            } else {
                th.on_surface
            })
            .size(th.typography.body_large)
            .single_line(),
        )
    };

    let bar_modifier = modifier.clone();
    let bar_bg = if active {
        th.surface_container_high
    } else {
        th.surface_container
    };
    let bar = Box(bar_modifier
        .width(width)
        .height(56.0)
        .state_elevation(StateElevation {
            default: if active { th.elevation.level3 } else { 0.0 },
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            disabled: 0.0,
        })
        .padding_values(PaddingValues {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        })
        .clickable()
        .on_pointer_down({
            let s = state.clone();
            move |_| s.activate()
        })
        .background(bar_bg)
        .clip_rounded(th.shapes.large))
    .child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::Center))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(8.0).fill_max_height()),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    );

    if expanded {
        Stack(modifier).child((
            bar,
            Box(Modifier::new()
                .width(width)
                .max_height(400.0)
                .background(th.surface_container)
                .clip_rounded(th.shapes.extra_small))
            .child(content),
        ))
    } else {
        bar
    }
}

/// M3 Docked Search Bar - full-width variant anchored to the top of the screen.
/// Shows a search bar with animated suggestions dropdown.
pub fn DockedSearchBar(
    state: Rc<SearchBarState>,
    modifier: Modifier,
    leading_icon: Option<View>,
    placeholder: impl Into<String>,
    on_query_change: Option<Rc<dyn Fn(String)>>,
    content: View,
) -> View {
    let th = theme();
    let placeholder = placeholder.into();
    let expanded = state.is_expanded();
    let query = state.query();
    let active = state.is_active();

    let content_target = if expanded { 400.0 } else { 0.0 };
    let content_height = animate_f32("docked_sh", content_target, theme().motion.expand);
    let content_alpha = animate_f32(
        "docked_sa",
        if expanded { 1.0 } else { 0.0 },
        theme().motion.color,
    );

    let input_field: View = if active {
        UiTextField(
            placeholder.clone(),
            query.clone(),
            Modifier::new().flex_grow(1.0),
            repose_ui::BasicTextFieldConfig {
                on_change: Some({
                    let s = state.clone();
                    let cb = on_query_change.clone();
                    Rc::new(move |text| {
                        s.set_query(text);
                        if let Some(ref cb) = cb {
                            cb(s.query());
                        }
                    })
                }),
                ..Default::default()
            },
        )
        .color(th.on_surface)
        .size(th.typography.body_large)
    } else {
        Box(Modifier::new().flex_grow(1.0)).child(if query.is_empty() {
            Text(placeholder.clone())
                .color(th.on_surface_variant)
                .size(th.typography.body_large)
                .single_line()
        } else {
            Text(query.clone())
                .color(th.on_surface)
                .size(th.typography.body_large)
                .single_line()
        })
    };

    let bar_bg = if active {
        th.surface_container_high
    } else {
        th.surface_container
    };
    let bar = Box(modifier
        .fill_max_width()
        .height(56.0)
        .state_elevation(StateElevation {
            default: if active { th.elevation.level3 } else { 0.0 },
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            disabled: 0.0,
        })
        .padding_values(PaddingValues {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        })
        .clickable()
        .on_pointer_down({
            let s = state.clone();
            move |_| {
                if !s.is_active() {
                    s.activate()
                }
            }
        })
        .background(bar_bg)
        .clip_rounded(th.shapes.large))
    .child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::Center))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(12.0).fill_max_height()),
            input_field,
            if active {
                Box(Modifier::new()
                    .size(24.0, 24.0)
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| {
                            s.set_query(String::new());
                            s.collapse();
                        }
                    }))
                .child(Text("✕").size(16.0).color(th.on_surface_variant))
            } else {
                Box(Modifier::new())
            },
        )),
    );

    let show_content = expanded || content_height > 1.0;
    if show_content {
        Column(Modifier::new().fill_max_width()).child((
            bar,
            Box(Modifier::new()
                .fill_max_width()
                .height(content_height)
                .alpha(content_alpha)
                .clip_rounded(th.shapes.small)
                .background(th.surface_container)
                .state_elevation(StateElevation {
                    default: th.elevation.level3,
                    hovered: th.elevation.level3,
                    pressed: th.elevation.level3,
                    disabled: 0.0,
                }))
            .child(
                Column(Modifier::new().fill_max_width()).child((
                    Box(Modifier::new()
                        .fill_max_width()
                        .height(1.0)
                        .background(th.outline_variant)),
                    content,
                )),
            ),
        ))
    } else {
        bar
    }
}

/// State for `ModalBottomSheet` - manages visibility and drag offset.
pub struct SheetState {
    visible: Signal<bool>,
    drag_offset: Signal<f32>,
    peek_height: Signal<f32>,
}

impl SheetState {
    pub fn new(peek_height: f32) -> Self {
        Self {
            visible: signal(false),
            drag_offset: signal(0.0),
            peek_height: signal(peek_height),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
        self.drag_offset.set(0.0);
    }

    pub fn set_peek_height(&self, h: f32) {
        self.peek_height.set(h);
    }
}

/// M3 Modal Bottom Sheet - slides up from the bottom with a drag handle.
///
/// Renders as an overlay so it is not clipped by parent containers.
/// Shows on `state.show()`, dismisses on `state.dismiss()` or scrim tap.
pub fn ModalBottomSheet(
    state: Rc<SheetState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    content: View,
    config: BottomSheetConfig,
) -> View {
    let th = theme();
    let peek_h = state.peek_height.get().max(config.peek_height);
    let anim_distance = peek_h.max(48.0).max(400.0);
    let overlay_id = remember_with_key("mbs_oid", || signal(0u64));

    // Drag state -> offset_at_drag_start is the anim value when the drag began
    let drag_anchor_y: Rc<RefCell<f32>> = remember_state_with_key("mbs_drag_y", || 0.0);
    let offset_at_drag_start: Rc<RefCell<f32>> = remember_state_with_key("mbs_drag_base", || 0.0);
    let is_dragging: Rc<RefCell<bool>> = remember_state_with_key("mbs_drag", || false);

    // Animated offset: anim_distance px (off-screen) → 0px (visible)
    let anim = remember_state_with_key("mbs_anim", || {
        AnimatedValue::new(anim_distance, theme().motion.spring)
    });
    let last_target = remember_state_with_key("mbs_anim_target", || f32::NAN);
    let anim_target = if state.is_visible() {
        0.0
    } else {
        anim_distance
    };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            if state.is_visible() {
                a.set_spec(th.motion.spring);
            } else {
                a.set_spec(AnimationSpec::fast());
            }
            a.set_target(anim_target);
            *lt = anim_target;
        }
        drop(lt);
        let still_animating = a.update();
        if still_animating {
            request_frame();
        }
    }

    let offset = *anim.borrow().get();
    let sheet_visible = state.is_visible() || offset < anim_distance - 10.0;

    if sheet_visible {
        if overlay_id.get() == 0 {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let modifier = modifier.clone();
                let content = content.clone();
                let drag_anchor_y = drag_anchor_y.clone();
                let offset_at_drag_start = offset_at_drag_start.clone();
                let is_dragging = is_dragging.clone();
                let anim_distance = anim_distance;
                move || {
                    let off = *anim.borrow().get();

                    let sheet_body = Box(modifier
                        .clone()
                        .fill_max_width()
                        .max_width(dp_to_px(config.max_width))
                        .translate(0.0, off)
                        .background(config.container_color)
                        .clip_rounded(config.shape_radius)
                        .on_pointer_down({
                            let anim = anim.clone();
                            let drag_anchor_y = drag_anchor_y.clone();
                            let offset_at_drag_start = offset_at_drag_start.clone();
                            let is_dragging = is_dragging.clone();
                            move |ev| {
                                *drag_anchor_y.borrow_mut() = ev.position.y;
                                *offset_at_drag_start.borrow_mut() = *anim.borrow().get();
                                *is_dragging.borrow_mut() = true;
                            }
                        })
                        .on_pointer_move({
                            let anim = anim.clone();
                            let drag_anchor_y = drag_anchor_y.clone();
                            let offset_at_drag_start = offset_at_drag_start.clone();
                            let is_dragging = is_dragging.clone();
                            move |ev| {
                                if !*is_dragging.borrow() {
                                    return;
                                }
                                let delta = ev.position.y - *drag_anchor_y.borrow();
                                let start_off = *offset_at_drag_start.borrow();
                                let total = (start_off + delta).max(0.0);
                                anim.borrow_mut().snap_to(total);
                                request_frame();
                            }
                        })
                        .on_pointer_up({
                            let anim = anim.clone();
                            let is_dragging = is_dragging.clone();
                            let state = state.clone();
                            let anim_distance = anim_distance;
                            move |_| {
                                *is_dragging.borrow_mut() = false;
                                let current_off = *anim.borrow().get();
                                let threshold = anim_distance * 0.3;
                                if current_off > threshold {
                                    anim.borrow_mut().set_target(anim_distance);
                                    state.dismiss();
                                } else {
                                    anim.borrow_mut().set_target(0.0);
                                }
                            }
                        }))
                    .child(
                        Column(Modifier::new().fill_max_width()).child((
                            Row(Modifier::new()
                                .fill_max_width()
                                .justify_content(JustifyContent::Center))
                            .child(Box(Modifier::new()
                                .margin_vertical(22.0)
                                .width(config.drag_handle_width)
                                .height(config.drag_handle_height)
                                .background(config.drag_handle_color)
                                .clip_rounded(2.0))),
                            content.clone(),
                        )),
                    );

                    let sheet = Box(Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::Center)
                        .align_items(AlignItems::FlexEnd))
                    .child(sheet_body);

                    let scrim_alpha = if state.is_visible() {
                        config.scrim_color.3
                    } else {
                        let t = (off / anim_distance).clamp(0.0, 1.0);
                        (config.scrim_color.3 as f32 * (1.0 - t)) as u8
                    };
                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.scrim_color.with_alpha(scrim_alpha))
                        .on_pointer_down({
                            let s = state.clone();
                            move |_| s.dismiss()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, sheet))
                }
            });

            let id = overlay.show_entry(builder, 900.0, false);
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
    }

    Box(Modifier::new())
}

/// State for `PullToRefresh` - tracks pull progress and refresh trigger.
///
/// Connect to a [`ScrollState`](repose_ui::scroll::ScrollState) via
/// [`set_scroll_state`](PullToRefreshState::set_scroll_state) so that the
/// pull offset is automatically driven by scroll overscroll.
pub struct PullToRefreshState {
    refreshing: Signal<bool>,
    scroll_state: RefCell<Option<Rc<repose_ui::scroll::ScrollState>>>,
    threshold: f32,
    triggered: Cell<bool>,
}

impl Default for PullToRefreshState {
    fn default() -> Self {
        Self::new()
    }
}

impl PullToRefreshState {
    pub fn new() -> Self {
        Self {
            refreshing: signal(false),
            scroll_state: RefCell::new(None),
            threshold: 64.0,
            triggered: Cell::new(false),
        }
    }

    /// Connect this PullToRefresh state to a scroll state.
    /// The pull offset is then derived from the scroll state's overscroll.
    pub fn set_scroll_state(&self, state: Rc<repose_ui::scroll::ScrollState>) {
        *self.scroll_state.borrow_mut() = Some(state);
    }

    /// Set the overscroll threshold that triggers a refresh (default 64px).
    pub fn set_threshold(&mut self, px: f32) {
        self.threshold = px;
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.get()
    }

    pub fn set_refreshing(&self, v: bool) {
        self.refreshing.set(v);
        if !v && let Some(sc) = self.scroll_state.borrow().as_ref() {
            sc.set_overscroll(0.0);
        }
    }

    /// Read the current pull offset from the connected scroll state's overscroll.
    pub fn pull_offset(&self) -> f32 {
        if let Some(sc) = self.scroll_state.borrow().as_ref() {
            let os = sc.overscroll_offset();
            if os < 0.0 { -os } else { 0.0 }
        } else {
            0.0
        }
    }
}

/// Wraps scrollable content with a pull-to-refresh indicator.
///
/// Renders a small spinner at the top when the user pulls down past a threshold,
/// or shows the current pull offset as a visual indicator.
///
/// The `state` must be connected to a [`ScrollState`](repose_ui::scroll::ScrollState)
/// via [`set_scroll_state`](PullToRefreshState::set_scroll_state) for the pull
/// offset to be derived from the scroll overscroll automatically.
pub fn PullToRefresh(
    state: Rc<PullToRefreshState>,
    modifier: Modifier,
    on_refresh: Rc<dyn Fn()>,
    content: View,
    config: PullToRefreshConfig,
) -> View {
    let pull = state.pull_offset();
    let refreshing = state.is_refreshing();
    let threshold = config.threshold;

    if state.triggered.get() && !refreshing && pull < threshold {
        state.triggered.set(false);
    }

    if !refreshing && !state.triggered.get() && pull >= threshold {
        state.triggered.set(true);
        state.refreshing.set(true);
        (on_refresh)();
    }

    let frac_key = format!("ptr_frac_{}", Rc::as_ptr(&state) as u64);
    let raw_frac = if refreshing {
        1.0
    } else if pull > 0.0 {
        pull / threshold
    } else {
        0.0
    };
    let distance_fraction = animate_f32_from(frac_key, 0.0, raw_frac, theme().motion.color);

    let adjusted_percent = (distance_fraction.min(1.0) - 0.4).max(0.0) * 5.0 / 3.0;
    let overshoot_percent = (distance_fraction - 1.0).max(0.0);
    let linear_tension = overshoot_percent.min(2.0);
    let tension_percent = linear_tension - linear_tension.powi(2) / 4.0;
    let rotation_turns = (-0.25 + 0.4 * adjusted_percent + tension_percent) * 0.5;
    // rotate by 360° to convert turns → degrees, then to radians for the modifier
    let spinner_rotation_rad = rotation_turns * std::f32::consts::TAU;

    // Indicator at top (pushed into view by overscroll) + content below.
    let indicator_h = distance_fraction * threshold;
    let comp_scale = adjusted_percent.min(1.0);
    let icon_size = if refreshing {
        24.0
    } else {
        (16.0 + comp_scale * 8.0).min(24.0)
    };
    let rotation = if refreshing {
        animate_f32_from(
            "ptr_spin",
            0.0,
            std::f32::consts::TAU,
            AnimationSpec::tween(Duration::from_millis(1000), Easing::Linear)
                .repeated(RepeatableSpec::infinite()),
        )
    } else {
        spinner_rotation_rad
    };
    let alpha = if refreshing {
        1.0
    } else if distance_fraction >= 1.0 {
        1.0
    } else {
        0.3
    };
    Column(modifier).child((
        if distance_fraction > 0.01 {
            Box(Modifier::new()
                .fill_max_width()
                .height(indicator_h)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center))
            .child(
                Box(Modifier::new()
                    .size(icon_size, icon_size)
                    .translate(icon_size * 0.5, icon_size * 0.5)
                    .rotate(rotation)
                    .translate(-icon_size * 0.5, -icon_size * 0.5))
                .child(if refreshing {
                    Icon(Symbol::new("refresh", '\u{E5D5}'))
                        .size(24.0)
                        .color(config.indicator_color)
                } else {
                    Icon(Symbol::new("arrow_downward", '\u{E5DB}'))
                        .size(icon_size)
                        .color(config.indicator_color.with_alpha_f32(alpha))
                }),
            )
        } else {
            Box(Modifier::new())
        },
        content,
    ))
}

/// State for `DatePicker` - manages selected date.
pub struct DatePickerState {
    pub year: Signal<i32>,
    pub month: Signal<u32>, // 1-12
    pub day: Signal<u32>,
}

impl DatePickerState {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            year: signal(year),
            month: signal(month.clamp(1, 12)),
            day: signal(day.clamp(1, 31)),
        }
    }

    pub fn selected_date(&self) -> (i32, u32, u32) {
        (self.year.get(), self.month.get(), self.day.get())
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// Day of week for the first day of the given month/year.
/// Returns 0=Mon ... 6=Sun using Zeller-like formula for Gregorian calendar.
fn first_day_of_month(year: i32, month: u32) -> u32 {
    let m = month as i32;
    let (y, adj_m) = if m <= 2 {
        (year - 1, m + 12)
    } else {
        (year, m)
    };
    let k = y % 100;
    let j = y / 100;
    let h = (1 + (13 * (adj_m + 1)) / 5 + k + k / 4 + j / 4 + 5 * j) % 7;
    // Convert Zeller's Saturday=0 to Monday=0, Sunday=6
    ((h + 5) % 7) as u32
}

/// Simple calendar date for today-highlighting in DatePicker.
struct ReposeDate {
    year: i32,
    month: u32,
    day: u32,
}

impl ReposeDate {
    /// Compute today's date from the system clock.
    fn now() -> Self {
        let duration = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default();
        let days = (duration.as_secs() / 86_400) as i64;
        // Howard Hinnant's civil_from_days
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = (z - era * 146_097) as u64;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = (yoe as i64) + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        Self {
            year: y as i32,
            month: m as u32,
            day: d as u32,
        }
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const DOW_HEADERS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// M3 Date Picker dialog with month/year navigation, proper calendar grid,
/// today indicator, and confirm/cancel actions.
pub fn DatePicker(
    state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
) -> View {
    let th = theme();
    let (year, month, day) = state.selected_date();
    let dim = days_in_month(year, month);
    let start_dow = first_day_of_month(year, month);

    // Year step helpers
    let prev_year = {
        let s = state.clone();
        move || {
            s.year.set(s.year.get() - 1);
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };
    let next_year = {
        let s = state.clone();
        move || {
            s.year.set(s.year.get() + 1);
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };

    let prev_month = {
        let s = state.clone();
        move || {
            if s.month.get() == 1 {
                s.year.set(s.year.get() - 1);
                s.month.set(12);
            } else {
                s.month.set(s.month.get() - 1);
            }
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };

    let next_month = {
        let s = state.clone();
        move || {
            if s.month.get() == 12 {
                s.year.set(s.year.get() + 1);
                s.month.set(1);
            } else {
                s.month.set(s.month.get() + 1);
            }
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };

    // Determine today for highlight
    let now = ReposeDate::now();
    let today = (now.year, now.month, now.day);

    Column(Modifier::new().padding(16.0)).child((
        // Month header
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::Center))
        .child((
            IconButton(
                Box(Modifier::new()).child(Text("◀").color(th.on_surface).size(16.0)),
                prev_month,
                IconButtonConfig::default(),
            ),
            Spacer(),
            Column(Modifier::new().align_items(AlignItems::Center)).child((
                Text(MONTH_NAMES[(month - 1) as usize].to_string())
                    .size(th.typography.title_medium)
                    .color(th.on_surface),
                Row(Modifier::new().gap(8.0).align_items(AlignItems::Center)).child((
                    IconButton(
                        Box(Modifier::new())
                            .child(Text("‹").color(th.on_surface_variant).size(14.0)),
                        prev_year,
                        IconButtonConfig::default(),
                    ),
                    Text(year.to_string())
                        .size(th.typography.body_small)
                        .color(th.on_surface_variant),
                    IconButton(
                        Box(Modifier::new())
                            .child(Text("›").color(th.on_surface_variant).size(14.0)),
                        next_year,
                        IconButtonConfig::default(),
                    ),
                )),
            )),
            Spacer(),
            IconButton(
                Box(Modifier::new()).child(Text("▶").color(th.on_surface).size(16.0)),
                next_month,
                IconButtonConfig::default(),
            ),
        )),
        Box(Modifier::new().fill_max_width().height(12.0)),
        // Day grid
        Column(Modifier::new()).child({
            let mut rows: Vec<View> = Vec::new();
            // Day-of-week headers
            let dow_headers: Vec<View> = DOW_HEADERS
                .iter()
                .map(|d| {
                    Box(Modifier::new()
                        .width(40.0)
                        .height(40.0)
                        .align_items(AlignItems::Center)
                        .justify_content(JustifyContent::Center))
                    .child(
                        Text(d.to_string())
                            .size(th.typography.label_small)
                            .color(th.on_surface_variant),
                    )
                })
                .collect();
            rows.push(Row(Modifier::new()).with_children(dow_headers));

            // Proper calendar grid: offset by start_dow, 6 rows
            let total_cells = start_dow + dim;
            let num_rows = total_cells.div_ceil(7).min(6);
            for w in 0..num_rows {
                let mut week: Vec<View> = Vec::new();
                for d in 0..7 {
                    let cell_idx = w * 7 + d;
                    if cell_idx < start_dow {
                        week.push(Box(Modifier::new().width(40.0).height(40.0)));
                    } else {
                        let day_num = (cell_idx - start_dow + 1) as i32;
                        if day_num <= dim as i32 {
                            let is_selected = day_num == day as i32;
                            let is_today =
                                today.0 == year && today.1 == month && today.2 == day_num as u32;
                            let s = state.clone();
                            week.push(
                                Box(Modifier::new()
                                    .width(40.0)
                                    .height(40.0)
                                    .background(if is_selected {
                                        th.primary
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .clip_rounded(20.0)
                                    .align_items(AlignItems::Center)
                                    .justify_content(JustifyContent::Center)
                                    .clickable()
                                    .on_pointer_down(move |_| {
                                        s.day.set(day_num as u32);
                                    }))
                                .child({
                                    let mut t = Text(day_num.to_string())
                                        .size(th.typography.body_medium)
                                        .color(if is_selected {
                                            th.on_primary
                                        } else {
                                            th.on_surface
                                        });
                                    if is_today && !is_selected {
                                        t = t.modifier(
                                            Modifier::new().border(1.0, th.primary, 10.0),
                                        );
                                    }
                                    t
                                }),
                            );
                        } else {
                            week.push(Box(Modifier::new().width(40.0).height(40.0)));
                        }
                    }
                }
                rows.push(Row(Modifier::new()).with_children(week));
            }
            rows
        }),
        Box(Modifier::new().fill_max_width().height(12.0)),
        // Cancel / Confirm
        Row(Modifier::new()
            .fill_max_width()
            .justify_content(JustifyContent::End)
            .gap(8.0))
        .child((
            TextButton(
                Modifier::new(),
                {
                    let on_dismiss = on_dismiss.clone();
                    move || (on_dismiss)()
                },
                ButtonConfig::default(),
                || Text("Cancel").size(14.0),
            ),
            Button(
                Modifier::new(),
                {
                    let on_confirm = on_confirm.clone();
                    let s = state.clone();
                    move || {
                        let (y, m, d) = s.selected_date();
                        on_confirm(y, m, d);
                    }
                },
                ButtonConfig::default(),
                || Text("OK").size(14.0),
            ),
        )),
    ))
}

/// State for `TimePicker` - manages selected hour and minute.
pub struct TimePickerState {
    pub hour: Signal<u32>,
    pub minute: Signal<u32>,
    pub is_am: Signal<bool>,
}

impl TimePickerState {
    pub fn new(hour: u32, minute: u32) -> Self {
        let h = hour % 12;
        let am = hour < 12;
        Self {
            hour: signal(if h == 0 { 12 } else { h }),
            minute: signal(minute.min(59)),
            is_am: signal(am),
        }
    }

    pub fn selected_time(&self) -> (u32, u32) {
        let mut h = self.hour.get();
        if !self.is_am.get() {
            h = (h % 12) + 12;
        } else if h == 12 {
            h = 0;
        }
        (h, self.minute.get())
    }
}

/// M3 Time Picker - a simple time picker with hour/minute fields and AM/PM toggle.
pub fn TimePicker(
    state: Rc<TimePickerState>,
    on_confirm: Rc<dyn Fn(u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
) -> View {
    let th = theme();
    let hour = state.hour.get();
    let minute = state.minute.get();
    let is_am = state.is_am.get();

    let hour_str = format!("{:02}", hour);
    let min_str = format!("{:02}", minute);

    Column(
        Modifier::new()
            .width(256.0)
            .padding(24.0)
            .align_items(AlignItems::Center),
    )
    .child((
        // Time display
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            Box(Modifier::new()
                .clickable()
                .on_pointer_down({
                    let s = state.clone();
                    move |_| s.hour.set((s.hour.get() % 12) + 1)
                })
                .padding(8.0))
            .child(Text(hour_str).size(48.0).color(th.on_surface).single_line()),
            Text(":")
                .size(48.0)
                .color(th.on_surface_variant)
                .single_line(),
            Box(Modifier::new()
                .clickable()
                .on_pointer_down({
                    let s = state.clone();
                    move |_| s.minute.set((s.minute.get() + 1) % 60)
                })
                .padding(8.0))
            .child(Text(min_str).size(48.0).color(th.on_surface).single_line()),
        )),
        Box(Modifier::new().fill_max_width().height(16.0)),
        // AM/PM toggle
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 4.0,
                    bottom: 4.0,
                })
                .background(if is_am {
                    th.primary
                } else {
                    Color::TRANSPARENT
                })
                .clip_rounded(8.0)
                .clickable()
                .on_pointer_down({
                    let s = state.clone();
                    move |_| {
                        if !s.is_am.get() {
                            s.is_am.set(true);
                            let h = s.hour.get();
                            s.hour.set(if h == 12 { 12 } else { (h + 12) % 24 });
                            if s.hour.get() == 0 {
                                s.hour.set(12);
                            }
                        }
                    }
                }))
            .child(Text("AM").size(th.typography.label_large).color(if is_am {
                th.on_primary
            } else {
                th.on_surface
            })),
            Box(Modifier::new().width(8.0).height(1.0)),
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 4.0,
                    bottom: 4.0,
                })
                .background(if !is_am {
                    th.primary
                } else {
                    Color::TRANSPARENT
                })
                .clip_rounded(8.0)
                .clickable()
                .on_pointer_down({
                    let s = state.clone();
                    move |_| {
                        if s.is_am.get() {
                            s.is_am.set(false);
                            let h = s.hour.get();
                            s.hour.set(if h == 12 { 12 } else { (h + 12) % 24 });
                            if s.hour.get() == 0 {
                                s.hour.set(12);
                            }
                        }
                    }
                }))
            .child(Text("PM").size(th.typography.label_large).color(if !is_am {
                th.on_primary
            } else {
                th.on_surface
            })),
        )),
        Box(Modifier::new().fill_max_width().height(16.0)),
        Row(Modifier::new().fill_max_width()).child((
            Spacer(),
            Box(Modifier::new().padding(8.0).clickable().on_pointer_down({
                let on_dismiss = on_dismiss.clone();
                move |_| on_dismiss()
            }))
            .child(
                Text("Cancel")
                    .color(th.primary)
                    .size(th.typography.label_large)
                    .single_line(),
            ),
            Box(Modifier::new().width(8.0).height(1.0)),
            Box(Modifier::new().padding(8.0).clickable().on_pointer_down({
                let on_confirm = on_confirm.clone();
                let state = state.clone();
                move |_| {
                    let (h, m) = state.selected_time();
                    on_confirm(h, m);
                }
            }))
            .child(
                Text("OK")
                    .color(th.primary)
                    .size(th.typography.label_large)
                    .single_line(),
            ),
        )),
    ))
}

/// A destination entry inside a NavigationRail.
pub struct NavRailItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub badge: Option<View>,
}

static NAVRAIL_COUNTER: AtomicU64 = AtomicU64::new(0);
static FILTERCHIP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 Navigation Rail - a compact vertical navigation sidebar.
///
/// Typically placed on the left side of the screen. Contains navigation items
/// (icon + label) with animated selection indicator.
pub fn NavigationRail(
    selected_index: usize,
    items: Vec<NavRailItem>,
    header: Option<View>,
    fab: Option<View>,
    config: NavigationRailConfig,
) -> View {
    let th = theme();
    let id = remember(|| NAVRAIL_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.shape;

    let mut top_children: Vec<View> = Vec::new();
    let mut item_views: Vec<View> = Vec::new();

    let has_header = header.is_some();
    let has_fab = fab.is_some();

    if let Some(h) = header {
        top_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 12.0,
                    bottom: 12.0,
                })
                .align_self(AlignSelf::Center))
            .child(h),
        );
    }

    if let Some(f) = fab {
        top_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 8.0,
                    bottom: 8.0,
                })
                .align_self(AlignSelf::Center))
            .child(f),
        );
    }

    if has_header || has_fab {
        top_children.push(Box(Modifier::new()
            .fill_max_width()
            .height(1.0)
            .background(th.outline_variant)));
    }

    for (i, item) in items.into_iter().enumerate() {
        let selected = i == selected_index;

        let fg = animate_color(
            format!("nr_fg_{}_{}", id, i),
            if selected {
                config.selected_icon_color
            } else {
                config.unselected_icon_color
            },
            spec,
        );
        let bg = animate_color(
            format!("nr_bg_{}_{}", id, i),
            if selected {
                config.selected_container_color
            } else {
                Color::TRANSPARENT
            },
            spec,
        );

        let cb = item.on_click.clone();

        item_views.push(
            Column(
                Modifier::new()
                    .fill_max_width()
                    .padding_values(PaddingValues {
                        left: 4.0,
                        right: 4.0,
                        top: 4.0,
                        bottom: 4.0,
                    })
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center)
                    .background(bg)
                    .state_colors(StateColors {
                        default: Color::TRANSPARENT,
                        hovered: th.on_surface.with_alpha_f32(0.08),
                        pressed: th.on_surface.with_alpha_f32(0.12),
                        disabled: Color::TRANSPARENT,
                    })
                    .clip_rounded(config.item_radius)
                    .clickable()
                    .on_pointer_down(move |_| cb()),
            )
            .child((
                Stack(Modifier::new()).child((
                    Box(Modifier::new().size(24.0, 24.0))
                        .child(with_content_color(fg, move || item.icon)),
                    item.badge
                        .map(|b| {
                            Box(Modifier::new()
                                .absolute()
                                .offset(None, None, None, Some(0.0)))
                            .child(b)
                        })
                        .unwrap_or(Box(Modifier::new())),
                )),
                Box(Modifier::new().fill_max_width().height(4.0)),
                Text(item.label)
                    .color(fg)
                    .size(th.typography.label_medium)
                    .single_line(),
            )),
        );
    }

    Column(
        Modifier::new()
            .width(config.width)
            .fill_max_height()
            .background(config.container_color)
            .align_items(AlignItems::Center)
            .then(config.modifier),
    )
    .child((
        Column(Modifier::new()).with_children(top_children),
        Box(Modifier::new().flex_grow(1.0)).child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .justify_content(JustifyContent::SpaceBetween)
                    .align_items(AlignItems::Center),
            )
            .with_children(item_views),
        ),
    ))
}

/// State for `SwipeToDismiss` - tracks swipe offset with spring animation.
pub struct SwipeToDismissState {
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Set when `dismiss()` starts the spring; checked when the spring settles
    /// to fire `on_dismiss` exactly once per dismiss gesture.
    dismiss_handled: Rc<RefCell<bool>>,
}

impl Default for SwipeToDismissState {
    fn default() -> Self {
        Self::new()
    }
}

impl SwipeToDismissState {
    pub fn new() -> Self {
        Self {
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            dismiss_handled: Rc::new(RefCell::new(true)),
        }
    }

    /// Read and tick the animated offset.  Call every frame that needs the value.
    /// Requests a re-draw while the spring is still running.
    pub fn offset(&self) -> f32 {
        let mut anim = self.anim.borrow_mut();
        if anim.update() {
            request_frame();
        }
        *anim.get()
    }

    /// Snap instantly to an offset (used during active drag).
    pub fn set_offset_instant(&self, off: f32) {
        self.anim.borrow_mut().snap_to(off);
        request_frame();
    }

    /// Whether the current position is past the dismiss threshold.
    pub fn is_dismissed(&self) -> bool {
        *self.anim.borrow().get() < -150.0
    }

    /// Animate to the dismissed position.
    pub fn dismiss(&self) {
        *self.dismiss_handled.borrow_mut() = false;
        self.anim.borrow_mut().set_target(-300.0);
        request_frame();
    }

    /// Animate to the dismissed position with custom offset.
    pub fn dismiss_to(&self, offset: f32) {
        *self.dismiss_handled.borrow_mut() = false;
        self.anim.borrow_mut().set_target(-offset);
        request_frame();
    }

    /// Animate back to origin.
    pub fn reset(&self) {
        *self.dismiss_handled.borrow_mut() = true;
        self.anim.borrow_mut().set_target(0.0);
        request_frame();
    }

    /// Fire the dismiss callback once when the spring settles past threshold.
    fn try_handle_dismiss(&self, on_dismiss: &Option<Rc<dyn Fn()>>) {
        let anim = self.anim.borrow();
        if !anim.is_animating() && !*self.dismiss_handled.borrow() && *anim.get() < -150.0 {
            *self.dismiss_handled.borrow_mut() = true;
            if let Some(cb) = on_dismiss {
                cb();
            }
        }
    }

    /// Fire the dismiss callback once when the spring settles past a given threshold.
    fn try_handle_dismiss_with_threshold(&self, on_dismiss: &Option<Rc<dyn Fn()>>, threshold: f32) {
        let anim = self.anim.borrow();
        if !anim.is_animating() && !*self.dismiss_handled.borrow() && *anim.get() < -threshold {
            *self.dismiss_handled.borrow_mut() = true;
            if let Some(cb) = on_dismiss {
                cb();
            }
        }
    }
}

/// M3 SwipeToDismiss - wraps content that can be swiped left to reveal
/// a `background` action view.  On release past the threshold the content
/// springs to the dismissed position and `on_dismiss` fires **once**.
pub fn SwipeToDismiss(
    state: Rc<SwipeToDismissState>,
    on_dismiss: Option<Rc<dyn Fn()>>,
    background: View,
    content: View,
    modifier: Modifier,
    config: SwipeToDismissConfig,
) -> View {
    let offset = state.offset();
    state.try_handle_dismiss_with_threshold(&on_dismiss, config.dismiss_threshold);

    let drag_start_x = remember_with_key("swipe_drag_start", || RefCell::new(None::<f32>));
    let drag_base = remember_with_key("swipe_drag_base", || RefCell::new(0.0f32));

    let st = state.clone();
    let on_down = {
        let d = drag_start_x.clone();
        let base = drag_base.clone();
        move |e: PointerEvent| {
            *d.borrow_mut() = Some(e.position.x);
            *base.borrow_mut() = *st.anim.borrow().get();
        }
    };

    let st = state.clone();
    let on_move = {
        let d = drag_start_x.clone();
        let base = drag_base.clone();
        move |e: PointerEvent| {
            if let Some(start) = *d.borrow() {
                let dx = e.position.x - start;
                st.set_offset_instant(*base.borrow() + dx);
            }
        }
    };

    let st = state.clone();
    let on_up = {
        let d = drag_start_x.clone();
        let dt = config.dismiss_threshold;
        move |_e: PointerEvent| {
            *d.borrow_mut() = None;
            let off = *st.anim.borrow().get();
            if off > -dt * 0.333 {
                st.reset();
            } else {
                st.dismiss();
            }
        }
    };

    let display_offset = offset.max(-config.dismissed_offset).min(0.0);

    Stack(modifier.fill_max_width()).child((
        Box(Modifier::new().fill_max_size().absolute()).child(background),
        Box(Modifier::new()
            .fill_max_width()
            .translate(display_offset, 0.0)
            .on_pointer_down(on_down)
            .on_pointer_move(on_move)
            .on_pointer_up(on_up))
        .child(content),
    ))
}

/// M3 Carousel - a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
pub fn Carousel<T, F>(
    items: Vec<T>,
    item_width: f32,
    peek_amount: f32,
    modifier: Modifier,
    state: Rc<LazyRowState>,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let padded_modifier = modifier.clone().padding_values(PaddingValues {
        left: peek_amount,
        right: peek_amount,
        top: 0.0,
        bottom: 0.0,
    });

    LazyRow(items, item_width, state, padded_modifier, item_builder)
}
