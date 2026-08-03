#![allow(non_snake_case)]

pub mod defaults;
pub use defaults::*;

mod components;
pub use components::*;

pub mod dialog;
pub use dialog::*;

pub mod advbuttons;
pub use advbuttons::*;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::Duration;

use crate::ripple::{RippleConfig, ripple};
use crate::{Icon, Symbol};
use repose_core::NestedScrollConnection;
use repose_core::animation::{AnimationSpec, Easing, RepeatableSpec};
use repose_core::text::ImeAction;
use repose_core::*;
use repose_ui::LazyRowState;
use repose_ui::lazy::LazyRow;
use repose_ui::lazy_states::LazyRowConfig;
use repose_ui::{
    BasicSecureTextField, BasicTextField, Box, Column, Row, Spacer, Text, TextFieldState,
    TextStyle, ViewExt, ZStack,
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

static TOOLTIP_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    let mut bar_m = Modifier::new()
        .fill_max_size()
        .min_height(config.height)
        .background(config.container_color)
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
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER)
            .column_gap(config.item_spacing)
            .semantics(Semantics::new(Role::Container).with_selectable_group()))
        .child(
            items
                .into_iter()
                .enumerate()
                .map(|(i, item)| {
                    let selected = i == selected_index;
                    let is_enabled = item.enabled;
                    let default_effects = AnimationSpec::spring_crit(40.0);
                    let fg_icon = animate_color(
                        format!("nb_fi_{}_{}", id, i),
                        if selected {
                            config.selected_icon_color
                        } else {
                            config.unselected_icon_color
                        },
                        default_effects,
                    );
                    let fg_label = animate_color(
                        format!("nb_fl_{}_{}", id, i),
                        if selected {
                            config.selected_text_color
                        } else {
                            config.unselected_text_color
                        },
                        default_effects,
                    );
                    let bg_alpha = animate_f32(
                        format!("nb_bg_{}_{}", id, i),
                        if selected { 1.0 } else { 0.0 },
                        default_effects,
                    );
                    let indicator_bg = config
                        .indicator_color
                        .with_alpha_f32(bg_alpha * config.indicator_opacity);
                    let cb = item.on_click.clone();
                    let nb_source: Rc<MutableInteractionSource> = item
                        .interaction_source
                        .clone()
                        .map(Rc::new)
                        .unwrap_or_else(|| remember(MutableInteractionSource::new));

                    let mut item_m = Modifier::new()
                        .flex_grow(1.0)
                        .interaction_source(&*nb_source)
                        .semantics(Semantics::new(Role::Tab).with_label(&item.label));

                    if is_enabled {
                        item_m = item_m.clickable().on_click({
                            let cb = cb.clone();
                            move || cb()
                        });
                    }

                    Box(item_m).child(
                        Column(
                            Modifier::new()
                                .fill_max_size()
                                .align_items(AlignItems::CENTER)
                                .justify_content(JustifyContent::CENTER),
                        )
                        .child((
                            // Indicator pill behind icon
                            Column(
                                Modifier::new()
                                    .align_items(AlignItems::CENTER)
                                    .justify_content(JustifyContent::CENTER),
                            )
                            .child((
                                Box(Modifier::new()
                                    .absolute()
                                    .offset(
                                        Some((24.0 - config.indicator_width) / 2.0),
                                        Some((24.0 - config.indicator_height) / 2.0),
                                        None,
                                        None,
                                    )
                                    .width(config.indicator_width)
                                    .height(config.indicator_height)
                                    .background(indicator_bg)
                                    .clip_rounded(config.indicator_radius)
                                    .state_colors(StateColors {
                                        default: Color::TRANSPARENT,
                                        hovered: th.on_surface.with_alpha_f32(0.08),
                                        pressed: th.on_surface.with_alpha_f32(0.12),
                                        disabled: Color::TRANSPARENT,
                                    })),
                                with_content_color(fg_icon, move || item.icon),
                            )),
                            // 8dp gap: 4dp IndicatorVerticalPadding + 4dp IndicatorToLabelPadding
                            Box(Modifier::new().height(8.0)),
                            Text(item.label)
                                .color(fg_label)
                                .size(th.typography.label_medium)
                                .single_line(),
                        )),
                    )
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
    pub interaction_source: Option<MutableInteractionSource>,
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
        snackbar.child(
            Column(Modifier::new().padding_values(PaddingValues {
                left: 16.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            }))
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
                action
                    .map(|a| {
                        let label = a.label.clone();
                        Row(Modifier::new()
                            .fill_max_width()
                            .justify_content(repose_core::JustifyContent::END))
                        .child(TextButton(
                            Modifier::new(),
                            move || (a.on_click)(),
                            ButtonConfig::default(),
                            || {
                                Text(label)
                                    .color(action_color)
                                    .size(th.typography.label_large)
                                    .single_line()
                            },
                        ))
                    })
                    .unwrap_or(Box(Modifier::new())),
            )),
        )
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
                .align_items(repose_core::AlignItems::CENTER))
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
            )),
        )
    };

    Box(Modifier::new()
        .absolute()
        .offset_bottom(0.0)
        .fill_max_width()
        .justify_content(repose_core::JustifyContent::CENTER)
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
        if selected {
            config.disabled_selected_border_color
        } else {
            config.disabled_border_color
        }
    } else {
        if selected {
            config.selected_border_color
        } else {
            config.border_color
        }
    };
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
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
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
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
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
    let border = if is_enabled {
        config.border_color
    } else {
        config.disabled_border_color
    };
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
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
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
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
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
        if selected {
            config.disabled_selected_border_color
        } else {
            config.disabled_border_color
        }
    } else {
        if selected {
            config.selected_border_color
        } else {
            config.border_color
        }
    };
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
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
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

/// Position of the floating action button within a Scaffold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FabPosition {
    End,
    Center,
}

impl Default for FabPosition {
    fn default() -> Self {
        Self::End
    }
}

#[derive(Clone)]
pub struct ScaffoldConfig {
    pub modifier: Modifier,
    pub top_bar: Option<View>,
    pub bottom_bar: Option<View>,
    pub floating_action_button: Option<View>,
    pub snackbar_host: Option<View>,
    pub container_color: Color,
    pub content_color: Color,
    pub fab_position: FabPosition,
}

impl Default for ScaffoldConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            top_bar: None,
            bottom_bar: None,
            floating_action_button: None,
            snackbar_host: None,
            container_color: ScaffoldDefaults::container_color(),
            content_color: ScaffoldDefaults::content_color(),
            fab_position: FabPosition::End,
        }
    }
}

pub fn Scaffold(content: impl Fn(PaddingValues) -> View, config: ScaffoldConfig) -> View {
    let insets = window_insets();
    let itop = px_to_dp(insets.top);
    let ibottom = px_to_dp(insets.bottom);
    let iime = px_to_dp(insets.ime_bottom);
    let ileft = px_to_dp(insets.left);
    let iright = px_to_dp(insets.right);

    let content_padding = PaddingValues {
        top: if config.top_bar.is_some() {
            64.0
        } else {
            itop
        },
        bottom: if config.bottom_bar.is_some() {
            80.0 + ibottom + iime
        } else {
            ibottom + iime
        },
        left: ileft,
        right: iright,
    };

    Column(
        config
            .modifier
            .fill_max_size()
            .background(config.container_color),
    )
    .child((
        Box(Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                top: if config.top_bar.is_some() {
                    64.0 + itop
                } else {
                    0.0
                },
                bottom: if config.bottom_bar.is_some() {
                    80.0 + ibottom + iime
                } else {
                    ibottom + iime
                },
                ..Default::default()
            }))
        .child(content(content_padding)),
        if let Some(bar) = config.top_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(itop), Some(0.0), None))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(bar) = config.bottom_bar {
            Box(Modifier::new().absolute().offset(
                Some(0.0),
                None,
                Some(ibottom + iime),
                Some(0.0),
            ))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(fab) = config.floating_action_button {
            let mut fab_m = Modifier::new().absolute();
            match config.fab_position {
                FabPosition::End => {
                    fab_m = fab_m.offset(
                        None,
                        None,
                        Some(16.0 + ibottom + iime),
                        Some(16.0),
                    );
                }
                FabPosition::Center => {
                    fab_m = fab_m.fill_max_width().align_self(AlignSelf::CENTER).offset(
                        None,
                        None,
                        Some(16.0 + ibottom + iime),
                        None,
                    );
                }
            }
            Box(fab_m).child(fab)
        } else {
            Box(Modifier::new())
        },
        config.snackbar_host.unwrap_or_else(|| Box(Modifier::new())),
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
/// When [`TooltipConfig::enable_user_input`] is true (default), the tooltip is
/// shown on pointer hover and dismissed on leave.
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
    content: View,
    config: TooltipConfig,
) -> View {
    let text: Rc<str> = Rc::from(text.into());
    let th = theme();
    let spec = th.motion.overlay;
    let id = remember(|| TOOLTIP_COUNTER.fetch_add(1, Ordering::Relaxed));

    let alpha = animate_f32(
        format!("tooltip_alpha_{id}"),
        if state.is_visible() { 1.0 } else { 0.0 },
        spec,
    );

    let tooltip_visible = state.is_visible() || alpha > 0.01;
    let scale = 0.92 + 0.08 * alpha;

    let mut host = config
        .modifier
        .align_self(AlignSelf::FLEX_START)
        .flex_shrink(0.0);

    if config.enable_user_input {
        let enter = state.clone();
        let leave = state.clone();
        host = host.hoverable(
            move || enter.show(),
            move || leave.dismiss(),
        );
    }

    Box(host).child((
        content,
        if tooltip_visible {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(config.offset_y), Some(0.0), None)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER)
                .hit_passthrough()
                .render_z_index(10_000.0)
                .alpha(alpha))
            .child(
                Box(Modifier::new()
                    .background(config.container_color)
                    .clip_rounded(th.shapes.extra_small)
                    .padding_values(PaddingValues {
                        left: config.horizontal_padding,
                        right: config.horizontal_padding,
                        top: config.vertical_padding,
                        bottom: config.vertical_padding,
                    })
                    .max_width(config.max_width)
                    .flex_shrink(0.0)
                    .scale(scale)
                    .hit_passthrough()
                    .then({
                        let mut m = Modifier::new();
                        if config.shadow_elevation > 0.0 {
                            m = m.shadow(config.shadow_elevation, 0.0);
                        }
                        if config.tonal_elevation > 0.0 {
                            m = m.state_elevation(StateElevation {
                                default: config.tonal_elevation,
                                hovered: config.tonal_elevation,
                                pressed: config.tonal_elevation,
                                disabled: 0.0,
                            });
                        }
                        m
                    }))
                .child(
                    Text((*text).to_string())
                        .color(config.content_color)
                        .size(th.typography.label_medium),
                ),
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
        Box(Modifier::new()
            .fill_max_size()
            .background(config.content_color))
        .child(content),
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
        Box(Modifier::new()
            .fill_max_size()
            .background(config.content_color))
        .child(content),
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
        .child(
            Box(Modifier::new())
                .color(config.content_color)
                .child(drawer_content),
        ),
        Box(Modifier::new().flex_grow(1.0)).child(content),
    ))
}

/// A destination entry inside a NavigationDrawer.
#[derive(Clone)]
pub struct NavigationDrawerItemConfig {
    pub modifier: Modifier,
    pub icon: Option<View>,
    pub badge: Option<View>,
    pub enabled: bool,
    pub shape_radius: f32,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for NavigationDrawerItemConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            icon: None,
            badge: None,
            enabled: true,
            shape_radius: repose_core::locals::theme().shapes.large,
            interaction_source: None,
        }
    }
}

pub fn NavigationDrawerItem(
    label: View,
    selected: bool,
    on_click: impl Fn() + 'static,
    config: NavigationDrawerItemConfig,
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

    let nd_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

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
        .clip_rounded(config.shape_radius)
        .interaction_source(&*nd_source)
        .then(config.modifier);

    if config.enabled {
        m = m.clickable().on_click(move || on_click());
    }

    Box(m).child(with_content_color(fg, || {
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 24.0,
                top: 0.0,
                bottom: 0.0,
            }))
        .child((
            config
                .icon
                .unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
            Box(Modifier::new().width(12.0).height(1.0)),
            Box(Modifier::new().flex_grow(1.0)).child(label),
            config.badge.unwrap_or(Box(Modifier::new())),
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

const DDM_SCALE_FROM: f32 = 0.8;
const DDM_VERTICAL_PADDING: f32 = 8.0;
const DDM_ITEM_H_PAD: f32 = 12.0;
const DDM_ITEM_MIN_HEIGHT: f32 = 48.0;

/// Either a menu item or a divider.
#[derive(Clone)]
pub enum DropdownMenuEntry {
    Item(DropdownMenuItem),
    Divider,
}

/// M3 Dropdown Menu anchored to a trigger element.
///
/// Renders as a single overlay entry with a transparent full-screen scrim and
/// positioned card, matching Compose's Popup behavior. The card is bounded in
/// height so vertical_scroll activates when content overflows.
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
    let overlay_id = remember_with_key(format!("ddm_oid_{ddm_id}"), || signal(0u64));
    let trigger_rect = remember_state_with_key(format!("ddm_tr_{ddm_id}"), Rect::default);
    let scroll_state: Rc<ScrollState> =
        remember_with_key(format!("ddm_scroll_{ddm_id}"), ScrollState::new);

    let trigger = Box(Modifier::new().on_globally_positioned({
        let tr = trigger_rect.clone();
        move |rect| {
            *tr.borrow_mut() = rect;
        }
    }))
    .child(trigger);

    let anim = remember_state_with_key(format!("ddm_anim_{ddm_id}"), || {
        AnimatedValue::new(0.0, theme().motion.overlay)
    });
    let last_target = remember_state_with_key(format!("ddm_lt_{ddm_id}"), || f32::NAN);
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

    if menu_visible {
        if overlay_id.get() == 0 {
            let anim = anim.clone();
            let th = th.clone();
            let items = items.clone();
            let state = state.clone();
            let config = config.clone();
            let trigger_rect = trigger_rect.clone();
            let scroll_state = scroll_state.clone();

            let id = overlay.show_entry(
                Rc::new(move || {
                    let p = *anim.borrow().get();
                    let scale = DDM_SCALE_FROM + (1.0 - DDM_SCALE_FROM) * p;
                    let alpha = p;

                    let rect = *trigger_rect.borrow();
                    let win_h = get_window_container_height();
                    let hm = config.vertical_margin;

                    let space_below = (win_h - hm) - (rect.y + rect.h);
                    let space_above = rect.y - hm;
                    let place_below = space_below >= space_above;
                    let available_height = (if place_below { space_below } else { space_above }).max(48.0);

                    let popup_x = rect.x + config.offset_x;
                    let constrained_width = config.max_width;

                    let mut adjusted_config = config.clone();
                    adjusted_config.max_width = constrained_width;

                    let popup_y = if place_below {
                        rect.y + rect.h + config.offset_y
                    } else {
                        // Anchor bottom, so stays in the space above instead
                        // of growing down off-screen.
                        (rect.y - config.offset_y - available_height).max(hm)
                    };

                    let content = render_dropdown_menu_content(
                        &th,
                        &items,
                        state.clone(),
                        &adjusted_config,
                        scroll_state.clone(),
                        available_height,
                    );

                    let transform_origin_y = if place_below { 0.0 } else { 1.0 };

                    let menu = Box(
                        Modifier::new()
                            .absolute()
                            .offset(Some(popup_x), Some(popup_y), None, None)
                            .scale(scale)
                            .alpha(alpha)
                            .transform_origin(0.0, transform_origin_y),
                    )
                    .child(content);

                    let scrim = Box(Modifier::new().fill_max_size().on_pointer_down({
                        let s = state.clone();
                        move |_| s.dismiss()
                    }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, menu))
                }),
                901.0,
                false,
            );
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
    }

    Box(modifier).child(trigger)
}

fn render_dropdown_menu_content(
    th: &Theme,
    items: &[DropdownMenuEntry],
    state: Rc<MenuState>,
    config: &DropdownMenuConfig,
    scroll_state: Rc<ScrollState>,
    max_height: f32,
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
                    .min_height(config.item_height.max(DDM_ITEM_MIN_HEIGHT))
                    .padding_values(PaddingValues {
                        left: DDM_ITEM_H_PAD,
                        right: DDM_ITEM_H_PAD,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .align_items(AlignItems::CENTER);

                if item.enabled {
                    modifier = modifier
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        })
                        .clickable()
                        .on_click(move || {
                            on_click();
                            state.dismiss();
                        });
                }

                let mut row_children: Vec<View> = Vec::new();
                if let Some(icon) = item.leading_icon.clone() {
                    row_children.push(icon);
                    row_children.push(Box(Modifier::new().width(DDM_ITEM_H_PAD)));
                }
                row_children.push(
                    Box(Modifier::new().flex_grow(1.0)).child(
                        Text(item.text.clone())
                            .color(text_color)
                            .size(th.typography.label_large)
                            .single_line(),
                    ),
                );
                if let Some(icon) = item.trailing_icon.clone() {
                    row_children.push(Box(Modifier::new().width(DDM_ITEM_H_PAD)));
                    row_children.push(icon);
                }
                Row(modifier).child(row_children)
            }
            DropdownMenuEntry::Divider => Box(Modifier::new()
                .fill_max_width()
                .height(1.0)
                .margin(12.0)
                .background(config.divider_color)),
        })
        .collect();

    let binding = scroll_state.to_binding();
    let axis_binding = match &binding {
        ScrollBinding::Vertical(a) => a.clone(),
        _ => unreachable!(),
    };

    let items_column = Box(
        Modifier::new()
            .fill_max_width()
            .max_height((max_height - 2.0 * DDM_VERTICAL_PADDING).max(0.0))
            .vertical_scroll(axis_binding),
    )
    .child(Column(Modifier::new().fill_max_width()).with_children(children));

    let shadow_elevation = config
        .shadow_elevation
        .unwrap_or(th.elevation.level2);

    let mut card_modifier = Modifier::new()
        .shadow(shadow_elevation, 0.0)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .padding_values(PaddingValues {
            left: 0.0,
            right: 0.0,
            top: DDM_VERTICAL_PADDING,
            bottom: DDM_VERTICAL_PADDING,
        })
        .background(config.container_color)
        .clip_rounded(config.shape_radius.unwrap_or(th.shapes.extra_small));

    card_modifier = apply_tonal_elevation(card_modifier, config.tonal_elevation, config.container_color);

    if let Some((border_width, border_color, border_radius)) = config.border {
        card_modifier = card_modifier.border(border_width, border_color, border_radius);
    }

    Box(card_modifier).child(items_column)
}

/// Possible values of [`SearchBarState`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchBarValue {
    Collapsed,
    Expanded,
}

/// State for `SearchBar` -> manages expanded/collapsed progress, query text,
/// active state, and collapsed layout coordinates for popup anchoring.
pub struct SearchBarState {
    pub query: Signal<String>,
    pub expanded: Signal<bool>,
    pub active: Signal<bool>,
    /// Whether this search bar expands to full-screen (vs docked).
    /// Used by AppBarWithSearch to hide the collapsed bar when expanded.
    pub expands_to_full_screen: Signal<bool>,
    /// Container animation (shape, size, position)
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Content fade animation -> fades FIRST on collapse before container shrinks
    content_anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Tracked via `on_globally_positioned` on the collapsed bar.
    /// Used by expanded docked variants for popup placement.
    pub collapsed_layout_rect: Signal<(f32, f32, f32, f32)>,
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
            expands_to_full_screen: signal(false),
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            content_anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            collapsed_layout_rect: signal((0.0, 0.0, 0.0, 0.0)),
        }
    }

    pub fn query(&self) -> String {
        self.query.get()
    }

    pub fn set_query(&self, q: impl Into<String>) {
        self.query.set(q.into());
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn expand(&self) {
        self.expanded.set(true);
        self.anim.borrow_mut().set_target(1.0);
        self.content_anim.borrow_mut().set_target(1.0);
        request_frame();
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
        self.active.set(false);
        // Content fades first; container follows in progress()
        self.content_anim.borrow_mut().set_target(0.0);
        self.anim.borrow_mut().set_target(0.0);
        request_frame();
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn activate(&self) {
        self.active.set(true);
        self.expanded.set(true);
        self.anim.borrow_mut().set_target(1.0);
        self.content_anim.borrow_mut().set_target(1.0);
        request_frame();
    }

    pub fn deactivate(&self) {
        if self.expanded.get() {
            self.expanded.set(false);
            self.content_anim.borrow_mut().set_target(0.0);
            self.anim.borrow_mut().set_target(0.0);
        }
        self.active.set(false);
        FocusManager::new(vec![], None).clear_focus(false);
        request_frame();
    }

    /// Container animation progress: 0.0 = collapsed, 1.0 = expanded.
    /// Ticks the underlying AnimatedValue and requests frames while animating.
    pub fn progress(&self) -> f32 {
        let mut a = self.anim.borrow_mut();
        let still = a.update();
        if still {
            request_frame();
        }
        a.get().clamp(0.0, 1.0)
    }

    /// Content fade progress -> fades ahead of container on collapse.
    pub fn content_progress(&self) -> f32 {
        let mut a = self.content_anim.borrow_mut();
        let still = a.update();
        if still {
            request_frame();
        }
        a.get().clamp(0.0, 1.0)
    }

    /// Whether the animation is currently running.
    pub fn is_animating(&self) -> bool {
        self.anim.borrow().is_animating() || self.content_anim.borrow().is_animating()
    }

    /// Whether the search bar is currently expanded (with tolerance for spring overshoot).
    pub fn current_value(&self) -> SearchBarValue {
        if *self.anim.borrow().get() <= 0.02 {
            SearchBarValue::Collapsed
        } else {
            SearchBarValue::Expanded
        }
    }

    /// Snap the container progress to a specific fraction (0.0 = collapsed, 1.0 = expanded).
    pub fn snap_to(&self, fraction: f32) {
        self.anim.borrow_mut().snap_to(fraction.clamp(0.0, 1.0));
        request_frame();
    }
}

#[derive(Clone)]
pub struct SearchBarInputFieldConfig {
    pub state: Option<Rc<SearchBarState>>,
    pub on_search: Option<Rc<dyn Fn(String)>>,
    pub enabled: bool,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for SearchBarInputFieldConfig {
    fn default() -> Self {
        let th = theme();
        Self {
            state: None,
            on_search: None,
            enabled: true,
            text_color: th.on_surface,
            placeholder_color: th.on_surface_variant,
            leading_icon: None,
            trailing_icon: None,
            interaction_source: None,
        }
    }
}

/// Build a search bar input field with proper M3 SearchBar styling.
/// Equivalent to Compose Material3's `SearchBarDefaults.InputField`.
/// When `state` is provided, focus gain triggers expand and Escape triggers collapse.
/// Always renders a `UiTextField` (focusable even in collapsed state, matching CK).
pub fn SearchBarInputField(
    placeholder: String,
    query: String,
    on_query_change: Rc<dyn Fn(String)>,
    expanded: bool,
    config: SearchBarInputFieldConfig,
) -> View {
    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| Rc::new(MutableInteractionSource::new()));
    let focused = source.source().collect_is_focused();
    let state = config.state;
    let enabled = config.enabled;

    let mut input_m = Modifier::new()
        .flex_grow(1.0)
        .padding(4.0)
        .required_width_in(SearchBarDefaults::MIN_WIDTH, SearchBarDefaults::MAX_WIDTH)
        .required_height_in(SearchBarDefaults::HEIGHT, SearchBarDefaults::HEIGHT)
        .interaction_source(&*source)
        .semantics(Semantics {
            role: Role::TextField,
            label: Some("Search".into()),
            focused: expanded || focused,
            enabled,
            selectable_group: false,
        })
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    if let Some(ref s) = s {
                        if s.is_active() {
                            s.deactivate();
                        }
                    }
                    true
                } else if ev.key == Key::ArrowDown || ev.key == Key::ArrowUp {
                    if let Some(ref s) = s {
                        if !s.is_expanded() {
                            s.activate();
                        }
                    }
                    true
                } else {
                    false
                }
            }
        });
    if let Some(ref s) = state {
        let s2 = s.clone();
        input_m = input_m.on_focus_changed(move |focused| {
            if focused {
                s2.activate();
            }
        });
    }

    let on_qc = on_query_change.clone();
    let on_s = config.on_search.clone();

    // Always render the text field (focusable even when collapsed, matching CK).
    let read_only = !expanded;

    let display_color = if query.is_empty() {
        config.placeholder_color
    } else {
        config.text_color
    };

    let tf_state = remember_with_key("SearchBarInputField_tf_state", || {
        RefCell::new(TextFieldState::new())
    });
    if tf_state.borrow().text != query {
        tf_state.borrow_mut().text = query.clone();
    }

    // Build the row: [leading_icon] + text_field + [trailing_icon]
    let mut row_children: Vec<View> = Vec::new();
    if let Some(icon) = config.leading_icon {
        row_children.push(icon);
    }
    let on_qc2 = on_qc.clone();
    row_children.push(
        BasicTextField(
            tf_state.clone(),
            input_m,
            placeholder,
            repose_ui::TextFieldConfig {
                on_change: Some(Rc::new(move |text| on_qc2(text))),
                on_submit: on_s.clone(),
                enabled,
                read_only,
                line_limits: TextFieldLineLimits::SingleLine,
                keyboard_options: KeyboardOptions {
                    ime_action: ImeAction::Search,
                    ..KeyboardOptions::DEFAULT
                },
                ..Default::default()
            },
        )
        .color(display_color)
        .size(repose_core::locals::theme().typography.body_large),
    );
    if let Some(icon) = config.trailing_icon {
        row_children.push(icon);
    }

    if row_children.len() == 1 {
        row_children.into_iter().next().unwrap()
    } else {
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child(row_children)
    }
}

/// Apply tonal elevation as a translucent primary overlay when the container
/// color matches the surface color. This mirrors CK's Surface tonalElevation.
fn apply_tonal_elevation(m: Modifier, elevation: f32, container: Color) -> Modifier {
    if elevation > 0.0 {
        let th = theme();
        if container == th.colors.surface {
            let overlay_alpha = (elevation * 4.0 + 4.0).min(24.0) / 100.0;
            return m.background(th.colors.primary.with_alpha_f32(overlay_alpha));
        }
    }
    m
}

/// Record the collapsed bar's layout rect on the state. Returns a modifier
/// that should be applied to the collapsed bar.
fn track_collapsed_layout(state: &Rc<SearchBarState>) -> Modifier {
    let s = state.clone();
    Modifier::new().on_globally_positioned(move |rect| {
        s.collapsed_layout_rect
            .set((rect.x, rect.y, rect.w, rect.h));
    })
}


/// M3 Collapsed Search Bar -> renders ONLY the collapsed bar surface wrapping
/// the provided `input_field`. Does NOT manage expanded content.
///
/// Equivalent to CK's `SearchBar(state, inputField)` overload -> a passive
/// Surface that does NOT handle clicks or ripple. The click/focus→expand
/// behavior is managed by the `InputField` (via `SearchBarInputField`).
///
/// Pressing <kbd>Escape</kbd> deactivates the search bar (cross-platform back).
///
/// Use [`ExpandedFullScreenSearchBar`] / [`ExpandedDockedSearchBar`] for the
/// expanded state, or [`SearchBarWithContent`] for an all-in-one variant.
pub fn SearchBar(
    state: Rc<SearchBarState>,
    input_field: View,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: SearchBarConfig,
) -> View {
    let th = theme();
    let colors = config.colors;

    let mut bar_m = modifier
        .fill_max_width()
        .height(config.height)
        .state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            disabled: 0.0,
        })
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if ev.key == Key::Escape && s.is_active() {
                    s.deactivate();
                    true
                } else {
                    false
                }
            }
        })
        .on_focus_changed({
            let s = state.clone();
            move |focused| {
                if focused {
                    s.activate();
                }
            }
        })
        .semantics(Semantics {
            role: Role::TextField,
            label: Some("Search".into()),
            focused: state.is_active(),
            enabled: true,
            selectable_group: false,
        })
        .background(colors.container_color)
        .clip_rounded(config.shape_radius)
        .then(track_collapsed_layout(&state));

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, colors.container_color);

    Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(8.0).fill_max_height()),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    )
}


/// M3 Search Bar that manages expanded content with animated width and
/// suggestions dropdown. Equivalent to CK's
/// `SearchBar(inputField, expanded, onExpandedChange, ..., content)` overload.
///
/// The bar itself is a passive surface (no click handling) -> expansion is
/// driven by the `InputField`'s focus tracking inside `input_field`.
pub fn SearchBarWithContent(
    input_field: View,
    expanded: bool,
    on_expanded_change: Rc<dyn Fn(bool)>,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: SearchBarConfig,
    content: View,
) -> View {
    let th = theme();
    let width = animate_f32(
        "sbwc_w",
        if expanded {
            config.expanded_width
        } else {
            config.collapsed_width
        },
        theme().motion.expand,
    );

    let bar_bg = if expanded {
        config.colors.active_container_color
    } else {
        config.colors.container_color
    };
    let shape = if expanded {
        config.active_shape_radius
    } else {
        config.shape_radius
    };

    let mut bar_m = modifier
        .clone()
        .width(width)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .height(config.height)
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    cb(false);
                    true
                } else {
                    false
                }
            }
        })
        .background(bar_bg)
        .clip_rounded(shape);

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_bg);

    // Content fades with separate alpha so content can fade before collapse
    let content_alpha = animate_f32("sbwc_a", if expanded { 1.0 } else { 0.0 }, th.motion.color);

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(8.0).fill_max_height()),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    );

    let show_content = expanded || content_alpha > 0.01;
    if show_content || expanded {
        Column(modifier).child((
            bar,
            Box(Modifier::new()
                .width(width)
                .max_height(SearchBarDefaults::DOCKED_HEIGHT)
                .alpha(content_alpha)
                .background(config.colors.container_color)
                .clip_rounded(th.shapes.extra_small))
            .child(content),
        ))
    } else {
        bar
    }
}

/// M3 Docked Search Bar -> bounded-width variant with animated suggestions
/// dropdown (height + alpha).  Equivalent to CK's
/// `DockedSearchBar(inputField, expanded, onExpandedChange, ..., content)`.
/// The bar itself is a passive Surface -> expansion is driven by `InputField`.
pub fn DockedSearchBar(
    input_field: View,
    expanded: bool,
    on_expanded_change: Option<Rc<dyn Fn(bool)>>,
    modifier: Modifier,
    leading_icon: Option<View>,
    config: SearchBarConfig,
    content: View,
) -> View {
    let th = theme();
    let active = expanded;
    let colors = config.colors;

    let content_target = if expanded {
        get_window_container_height() * 2.0 / 3.0
    } else {
        0.0
    };
    let content_height = animate_f32("docked_sh", content_target, theme().motion.expand);
    let content_alpha = animate_f32(
        "docked_sa",
        if expanded { 1.0 } else { 0.0 },
        theme().motion.color,
    );
    let bar_bg = if active {
        colors.active_container_color
    } else {
        colors.container_color
    };

    let clear_btn = if active {
        Box(Modifier::new().size(24.0, 24.0).clickable().on_click({
            let cb = on_expanded_change.clone();
            move || {
                if let Some(ref cb) = cb {
                    cb(false);
                }
            }
        }))
        .child(Text("✕").size(16.0).color(colors.placeholder_color))
    } else {
        Box(Modifier::new())
    };

    let mut bar_m = modifier
        .z_index(1.0)
        .min_width(SearchBarDefaults::MIN_WIDTH)
        .height(config.height)
        .state_elevation(StateElevation {
            default: if active {
                th.elevation.level3
            } else {
                config.tonal_elevation
            },
            hovered: th.elevation.level2,
            pressed: th.elevation.level3,
            disabled: 0.0,
        })
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    if let Some(ref cb) = cb {
                        cb(false);
                    }
                    true
                } else {
                    false
                }
            }
        })
        .background(bar_bg)
        .clip_rounded(config.shape_radius);

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_bg);

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(12.0).fill_max_height()),
            input_field,
            clear_btn,
        )),
    );

    let show_content = expanded || content_height > 1.0;
    if show_content {
        Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
            bar,
            Box(Modifier::new()
                .min_width(SearchBarDefaults::MIN_WIDTH)
                .height(content_height)
                .alpha(content_alpha)
                .clip_rounded(th.shapes.small)
                .background(colors.container_color)
                .state_elevation(StateElevation {
                    default: th.elevation.level3,
                    hovered: th.elevation.level3,
                    pressed: th.elevation.level3,
                    disabled: 0.0,
                }))
            .child(
                Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
                    Box(Modifier::new()
                        .min_width(SearchBarDefaults::MIN_WIDTH)
                        .height(1.0)
                        .background(colors.divider_color)),
                    content,
                )),
            ),
        ))
    } else {
        bar
    }
}

/// Platform-agnostic window container height. On Skiko this would read
/// `LocalWindowInfo`, on Android `LocalConfiguration`. Defaults to 800 dp.
/// The `LayoutEngine` keeps this current from the physical viewport + density.
pub fn set_window_container_height(h: f32) {
    repose_core::locals::set_window_container_height(h);
}

fn get_window_container_height() -> f32 {
    repose_core::locals::get_window_container_height()
}

/// Set the window container width (in dp) used for dropdown constraints.
pub fn set_window_container_width(w: f32) {
    repose_core::locals::set_window_container_width(w);
}

fn get_window_container_width() -> f32 {
    repose_core::locals::get_window_container_width()
}

/// M3 Expanded Full‑Screen Search Bar -> rendered in an overlay covering the
/// entire window. Uses the state's own `progress()` for animation.
/// Equivalent to CK's `ExpandedFullScreenSearchBar(state, inputField, ...)`.
pub fn ExpandedFullScreenSearchBar(
    state: Rc<SearchBarState>,
    overlay: OverlayHandle,
    input_field: View,
    modifier: Modifier,
    config: ExpandedFullScreenSearchBarConfig,
    content: View,
) -> View {
    // Mark as full-screen so AppBarWithSearch can hide the collapsed bar
    state.expands_to_full_screen.set(true);

    let overlay_id = remember_with_key("efs_oid", || signal(0u64));
    let current_content = remember_state_with_key("efs_cc", || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    let progress = state.progress();
    let _content_alpha = state.content_progress();

    let expanded = state.is_expanded();
    let visible = expanded || progress > 0.01;

    if visible {
        if overlay_id.get() == 0 {
            let input_fr = FocusRequester::new();
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let modifier = modifier.clone();
                let input_field = input_field.clone();
                let current_content = current_content.clone();
                let config = config.clone();
                let input_fr = input_fr.clone();
                move || {
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();

                    // Wrap input with focus requester and request focus (CK parity: auto-focus on expand)
                    let inp = Box(Modifier::new().focus_requester(input_fr.clone()))
                        .child(input_field.clone());
                    input_fr.request_focus();

                    let header = Box(modifier
                        .clone()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .padding_values(PaddingValues {
                            left: 16.0,
                            right: 16.0,
                            top: 0.0,
                            bottom: 0.0,
                        })
                        .background(config.colors.container_color)
                        .alpha(alpha))
                    .child(inp);

                    let body = Box(Modifier::new()
                        .fill_max_width()
                        .flex_grow(1.0)
                        .alpha(c_alpha)
                        .background(th.surface))
                    .child(content);

                    let insets = config.window_insets;
                    let full = Column(Modifier::new().fill_max_size().padding_values(
                        PaddingValues {
                            left: insets.left,
                            right: insets.right,
                            top: insets.top,
                            bottom: insets.bottom,
                        },
                    ))
                    .child((header, body));

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.scrim_color.with_alpha((85.0 * alpha) as u8))
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, full))
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

/// M3 Expanded Docked Search Bar -> rendered as an overlay popup anchored below
/// the collapsed search bar using `collapsed_layout_rect`.
/// Equivalent to CK's `ExpandedDockedSearchBar(state, inputField, ...)`.
pub fn ExpandedDockedSearchBar(
    state: Rc<SearchBarState>,
    overlay: OverlayHandle,
    input_field: View,
    modifier: Modifier,
    config: ExpandedDockedSearchBarConfig,
    content: View,
) -> View {
    // Docked search bar does NOT expand to full-screen
    state.expands_to_full_screen.set(false);

    let overlay_id = remember_with_key("eds_oid", || signal(0u64));
    let current_content = remember_state_with_key("eds_cc", || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    let progress = state.progress();
    let _content_alpha = state.content_progress();
    let expanded = state.is_expanded();
    let visible = expanded || progress > 0.01;

    if visible {
        if overlay_id.get() == 0 {
            let input_fr = FocusRequester::new();
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let modifier = modifier.clone();
                let input_field = input_field.clone();
                let current_content = current_content.clone();
                let config = config.clone();
                let input_fr = input_fr.clone();
                move || {
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();
                    let (_cx, _cy, _cw, _ch) = state.collapsed_layout_rect.get();

                    let inp = Box(Modifier::new().focus_requester(input_fr.clone()))
                        .child(input_field.clone());
                    input_fr.request_focus();

                    let header = Box(modifier
                        .clone()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .alpha(alpha)
                        .background(config.colors.container_color)
                        .clip_rounded(config.shape_radius)
                        .state_elevation(StateElevation {
                            default: th.elevation.level3,
                            hovered: th.elevation.level2,
                            pressed: th.elevation.level3,
                            disabled: 0.0,
                        }))
                    .child(inp);

                    let dropdown = Box(Modifier::new()
                        .fill_max_width()
                        .max_height(get_window_container_height() * 2.0 / 3.0)
                        .alpha(c_alpha)
                        .clip_rounded(config.dropdown_shape_radius)
                        .background(config.colors.container_color)
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
                                .background(config.colors.divider_color)),
                            content,
                        )),
                    );

                    let col = Column(Modifier::new().fill_max_width().padding_values(
                        PaddingValues {
                            left: _cx.max(16.0),
                            right: 16.0,
                            top: _cy + _ch + config.dropdown_gap_size,
                            bottom: 0.0,
                        },
                    ))
                    .child((header, dropdown));

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.dropdown_scrim_color)
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, col))
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

/// M3 App Bar With Search -> integrates a search bar into a top app bar layout
/// with optional navigation icon, action buttons, scroll behavior, and window insets.
/// Wraps the internal `SearchBar` collapsed component.
pub fn AppBarWithSearch(
    state: Rc<SearchBarState>,
    input_field: View,
    navigation_icon: Option<View>,
    actions: Option<Vec<View>>,
    config: AppBarWithSearchConfig,
) -> View {
    let bg = config.colors.search_bar_container(config.scroll_fraction);
    let app_bar_bg = config.colors.app_bar_container(config.scroll_fraction);

    let insets = config.window_insets;

    // CK parity: when app bar container is transparent, disable tonal/shadow elevations
    let is_container_transparent = app_bar_bg.3 == 0;
    let tonal_elevation = if is_container_transparent {
        0.0
    } else {
        config.tonal_elevation
    };
    let shadow_elevation = if is_container_transparent {
        0.0
    } else {
        config.shadow_elevation
    };

    // Hide the collapsed bar when full-screen expanded (CK parity via expandsToFullScreen)
    let hide_collapsed = state.expands_to_full_screen.get() && state.is_expanded();
    let collapsed_alpha = if hide_collapsed { 0.0 } else { 1.0 };

    let bar_m = Modifier::new()
        .fill_max_width()
        .height(config.height + insets.top)
        .translate(0.0, config.scroll_offset)
        .background(app_bar_bg)
        .semantics(Semantics::new(Role::Container).with_selectable_group());

    let row = Row(Modifier::new()
        .fill_max_size()
        .align_items(AlignItems::CENTER)
        .padding_values(PaddingValues {
            left: config.content_padding.left + insets.left,
            right: config.content_padding.right + insets.right,
            top: insets.top,
            bottom: 0.0,
        }))
    .child({
        let mut children: Vec<View> = Vec::new();
        if let Some(nav) = navigation_icon {
            children.push(nav);
            children.push(Box(Modifier::new().width(4.0)));
        }
        // Wrap input_field in collapsed SearchBar (CK parity)
        let sb_colors = &config.colors.search_bar_colors;
        let collapsed_bar = SearchBar(
            state.clone(),
            input_field,
            Modifier::new().flex_grow(1.0).alpha(collapsed_alpha),
            None,
            None,
            SearchBarConfig {
                height: config.height - 8.0,
                shape_radius: config.shape_radius,
                colors: SearchBarColors {
                    container_color: bg,
                    active_container_color: bg,
                    divider_color: sb_colors.divider_color,
                    content_color: sb_colors.content_color,
                    placeholder_color: sb_colors.placeholder_color,
                    scrim_color: sb_colors.scrim_color,
                },
                tonal_elevation,
                shadow_elevation,
                ..Default::default()
            },
        );
        children.push(Box(Modifier::new().flex_grow(1.0)).child(collapsed_bar));
        if let Some(acts) = actions {
            children.push(Spacer());
            for a in acts {
                children.push(a);
            }
        }
        children
    });

    Box(bar_m.shadow(shadow_elevation, 0.0)).child(row)
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
                                .justify_content(JustifyContent::CENTER))
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
                        .justify_content(JustifyContent::CENTER)
                        .align_items(AlignItems::FLEX_END))
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
    Column(modifier.align_items(config.content_alignment)).child((
        if distance_fraction > 0.01 {
            Box(Modifier::new()
                .fill_max_width()
                .height(indicator_h)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER))
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

/// Colors for [`DatePicker`].
#[derive(Clone)]
pub struct DatePickerColors {
    pub container_color: Color,
    pub header_color: Color,
    pub weekday_color: Color,
    pub day_color: Color,
    pub selected_day_color: Color,
    pub selected_day_container_color: Color,
    pub today_content_color: Color,
    pub today_border_color: Color,
    pub navigation_color: Color,
    pub year_selected_container_color: Color,
    pub year_selected_content_color: Color,
    pub year_unselected_content_color: Color,
}

impl Default for DatePickerColors {
    fn default() -> Self {
        Self {
            container_color: DatePickerDefaults::container_color(),
            header_color: DatePickerDefaults::header_color(),
            weekday_color: DatePickerDefaults::weekday_color(),
            day_color: DatePickerDefaults::day_color(),
            selected_day_color: DatePickerDefaults::selected_day_color(),
            selected_day_container_color: DatePickerDefaults::selected_day_container_color(),
            today_content_color: DatePickerDefaults::today_content_color(),
            today_border_color: DatePickerDefaults::today_border_color(),
            navigation_color: DatePickerDefaults::header_color(),
            year_selected_container_color: DatePickerDefaults::year_selected_container_color(),
            year_selected_content_color: DatePickerDefaults::year_selected_content_color(),
            year_unselected_content_color: DatePickerDefaults::year_unselected_content_color(),
        }
    }
}

/// Configuration for [`DatePicker`].
#[derive(Clone)]
pub struct DatePickerConfig {
    pub modifier: Modifier,
    pub colors: DatePickerColors,
    pub show_mode_toggle: bool,
}

impl Default for DatePickerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: DatePickerColors::default(),
            show_mode_toggle: true,
        }
    }
}

/// M3 Date Picker dialog with month/year navigation, proper calendar grid,
/// today indicator, and confirm/cancel actions.
pub fn DatePicker(
    state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: DatePickerConfig,
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

    Column(config.modifier.padding(16.0)).child((
        // Month header
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child((
            IconButton(
                Box(Modifier::new())
                    .child(Text("◀").color(config.colors.navigation_color).size(16.0)),
                prev_month,
                IconButtonConfig::default(),
            ),
            Spacer(),
            Column(Modifier::new().align_items(AlignItems::CENTER)).child((
                Text(MONTH_NAMES[(month - 1) as usize].to_string())
                    .size(th.typography.title_medium)
                    .color(config.colors.header_color),
                Row(Modifier::new().gap(8.0).align_items(AlignItems::CENTER)).child((
                    IconButton(
                        Box(Modifier::new())
                            .child(Text("‹").color(config.colors.navigation_color).size(14.0)),
                        prev_year,
                        IconButtonConfig::default(),
                    ),
                    Text(year.to_string())
                        .size(th.typography.body_small)
                        .color(th.on_surface_variant),
                    IconButton(
                        Box(Modifier::new())
                            .child(Text("›").color(config.colors.navigation_color).size(14.0)),
                        next_year,
                        IconButtonConfig::default(),
                    ),
                )),
            )),
            Spacer(),
            IconButton(
                Box(Modifier::new())
                    .child(Text("▶").color(config.colors.navigation_color).size(16.0)),
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
                        .align_items(AlignItems::CENTER)
                        .justify_content(JustifyContent::CENTER))
                    .child(
                        Text(d.to_string())
                            .size(th.typography.label_small)
                            .color(config.colors.weekday_color),
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
                                        config.colors.selected_day_container_color
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .clip_rounded(20.0)
                                    .align_items(AlignItems::CENTER)
                                    .justify_content(JustifyContent::CENTER)
                                    .clickable()
                                    .on_click(move || {
                                        s.day.set(day_num as u32);
                                    }))
                                .child({
                                    let mut t = Text(day_num.to_string())
                                        .size(th.typography.body_medium)
                                        .color(if is_selected {
                                            config.colors.selected_day_color
                                        } else {
                                            config.colors.day_color
                                        });
                                    if is_today && !is_selected {
                                        t = t.modifier(Modifier::new().border(
                                            1.0,
                                            config.colors.today_border_color,
                                            10.0,
                                        ));
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
            .justify_content(JustifyContent::END)
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

/// Layout types for [`TimePicker`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum TimePickerLayoutType {
    Horizontal,
    Vertical,
}

/// Colors for [`TimePicker`].
#[derive(Clone)]
pub struct TimePickerColors {
    pub clock_dial_color: Color,
    pub clock_dial_selected_content_color: Color,
    pub clock_dial_unselected_content_color: Color,
    pub selector_color: Color,
    pub container_color: Color,
    pub period_selector_border_color: Color,
    pub period_selector_selected_container_color: Color,
    pub period_selector_unselected_container_color: Color,
    pub period_selector_selected_content_color: Color,
    pub period_selector_unselected_content_color: Color,
    pub time_selector_selected_container_color: Color,
    pub time_selector_unselected_container_color: Color,
    pub time_selector_selected_content_color: Color,
    pub time_selector_unselected_content_color: Color,
}

impl Default for TimePickerColors {
    fn default() -> Self {
        Self {
            clock_dial_color: TimePickerDefaults::clock_dial_color(),
            clock_dial_selected_content_color:
                TimePickerDefaults::clock_dial_selected_content_color(),
            clock_dial_unselected_content_color:
                TimePickerDefaults::clock_dial_unselected_content_color(),
            selector_color: TimePickerDefaults::selector_color(),
            container_color: TimePickerDefaults::container_color(),
            period_selector_border_color: TimePickerDefaults::period_selector_border_color(),
            period_selector_selected_container_color:
                TimePickerDefaults::period_selector_selected_container_color(),
            period_selector_unselected_container_color:
                TimePickerDefaults::period_selector_unselected_container_color(),
            period_selector_selected_content_color:
                TimePickerDefaults::period_selector_selected_content_color(),
            period_selector_unselected_content_color:
                TimePickerDefaults::period_selector_unselected_content_color(),
            time_selector_selected_container_color:
                TimePickerDefaults::time_selector_selected_container_color(),
            time_selector_unselected_container_color:
                TimePickerDefaults::time_selector_unselected_container_color(),
            time_selector_selected_content_color:
                TimePickerDefaults::time_selector_selected_content_color(),
            time_selector_unselected_content_color:
                TimePickerDefaults::time_selector_unselected_content_color(),
        }
    }
}

/// Configuration for [`TimePicker`].
#[derive(Clone)]
pub struct TimePickerConfig {
    pub modifier: Modifier,
    pub colors: TimePickerColors,
    pub layout_type: TimePickerLayoutType,
}

impl Default for TimePickerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: TimePickerColors::default(),
            layout_type: TimePickerLayoutType::Vertical,
        }
    }
}

/// M3 Time Picker - a simple time picker with hour/minute fields and AM/PM toggle.
pub fn TimePicker(
    state: Rc<TimePickerState>,
    on_confirm: Rc<dyn Fn(u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: TimePickerConfig,
) -> View {
    let th = theme();
    let hour = state.hour.get();
    let minute = state.minute.get();
    let is_am = state.is_am.get();

    let hour_str = format!("{:02}", hour);
    let min_str = format!("{:02}", minute);

    Column(
        config
            .modifier
            .width(256.0)
            .padding(24.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        // Time display
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
            Box(Modifier::new()
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || s.hour.set((s.hour.get() % 12) + 1)
                })
                .padding(8.0))
            .child(
                Text(hour_str)
                    .size(48.0)
                    .color(config.colors.clock_dial_unselected_content_color)
                    .single_line(),
            ),
            Text(":")
                .size(48.0)
                .color(config.colors.clock_dial_unselected_content_color)
                .single_line(),
            Box(Modifier::new()
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || s.minute.set((s.minute.get() + 1) % 60)
                })
                .padding(8.0))
            .child(
                Text(min_str)
                    .size(48.0)
                    .color(config.colors.clock_dial_unselected_content_color)
                    .single_line(),
            ),
        )),
        Box(Modifier::new().fill_max_width().height(16.0)),
        // AM/PM toggle
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 4.0,
                    bottom: 4.0,
                })
                .background(if is_am {
                    config.colors.period_selector_selected_container_color
                } else {
                    Color::TRANSPARENT
                })
                .clip_rounded(8.0)
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || {
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
                config.colors.period_selector_selected_content_color
            } else {
                config.colors.period_selector_unselected_content_color
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
                    config.colors.period_selector_selected_container_color
                } else {
                    Color::TRANSPARENT
                })
                .clip_rounded(8.0)
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || {
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
                config.colors.period_selector_selected_content_color
            } else {
                config.colors.period_selector_unselected_content_color
            })),
        )),
        Box(Modifier::new().fill_max_width().height(16.0)),
        Row(Modifier::new().fill_max_width()).child((
            Spacer(),
            Box(Modifier::new().padding(8.0).clickable().on_click({
                let on_dismiss = on_dismiss.clone();
                move || on_dismiss()
            }))
            .child(
                Text("Cancel")
                    .color(config.colors.selector_color)
                    .size(th.typography.label_large)
                    .single_line(),
            ),
            Box(Modifier::new().width(8.0).height(1.0)),
            Box(Modifier::new().padding(8.0).clickable().on_click({
                let on_confirm = on_confirm.clone();
                let state = state.clone();
                move || {
                    let (h, m) = state.selected_time();
                    on_confirm(h, m);
                }
            }))
            .child(
                Text("OK")
                    .color(config.colors.selector_color)
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
    pub enabled: bool,
    pub interaction_source: Option<MutableInteractionSource>,
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
    let default_effects = AnimationSpec::spring_crit(40.0);

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
                .align_self(AlignSelf::CENTER))
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
                .align_self(AlignSelf::CENTER))
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
        let is_enabled = item.enabled;

        let fg = animate_color(
            format!("nr_fg_{}_{}", id, i),
            if selected {
                config.selected_icon_color
            } else {
                config.unselected_icon_color
            },
            default_effects,
        );
        let fg_label = animate_color(
            format!("nr_fl_{}_{}", id, i),
            if selected {
                config.selected_text_color
            } else {
                config.unselected_text_color
            },
            default_effects,
        );
        let bg = animate_color(
            format!("nr_bg_{}_{}", id, i),
            if selected {
                config.indicator_color
            } else {
                Color::TRANSPARENT
            },
            default_effects,
        );

        let cb = item.on_click.clone();
        let nr_source: Rc<MutableInteractionSource> = item
            .interaction_source
            .clone()
            .map(Rc::new)
            .unwrap_or_else(|| remember(MutableInteractionSource::new));

        let mut item_m = Modifier::new()
            .fill_max_width()
            .padding_values(PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 4.0,
                bottom: 4.0,
            })
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .background(bg)
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08),
                pressed: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .clip_rounded(config.item_radius)
            .interaction_source(&*nr_source)
            .semantics(Semantics::new(Role::Tab).with_label(&item.label));

        if is_enabled {
            item_m = item_m.clickable().on_click({
                let cb = cb.clone();
                move || cb()
            });
        }

        item_views.push(
            Column(item_m).child((
                Column(Modifier::new()).child((
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
                    .color(fg_label)
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
            .align_items(AlignItems::CENTER)
            .semantics(Semantics::new(Role::Container).with_selectable_group())
            .then(config.modifier),
    )
    .child((
        Column(Modifier::new()).with_children(top_children),
        Box(Modifier::new().flex_grow(1.0)).child(
            Column(
                Modifier::new()
                    .fill_max_size()
                    .justify_content(JustifyContent::SPACE_BETWEEN)
                    .align_items(AlignItems::CENTER),
            )
            .with_children(item_views),
        ),
    ))
}

/// Direction for the dismiss action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DismissDirection {
    StartToEnd,
    EndToStart,
    Both,
}

/// Resolved state for swipe-to-dismiss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DismissValue {
    Default,
    DismissedToStart,
    DismissedToEnd,
}

/// State for `SwipeToDismiss` - backed by a generic `SwipeableState<DismissValue>`.
pub struct SwipeToDismissState {
    swipeable: repose_core::SwipeableState<DismissValue>,
    dismissed_offset: f32,
}

impl Default for SwipeToDismissState {
    fn default() -> Self {
        Self::new()
    }
}

impl SwipeToDismissState {
    pub fn new() -> Self {
        Self::with_config(SwipeToDismissConfig::default())
    }

    pub fn with_config(config: SwipeToDismissConfig) -> Self {
        let one_third = 1.0 / 3.0;
        let positional_threshold = (config.dismiss_threshold * one_third) / config.dismissed_offset;
        let mut anchors = vec![(0.0, DismissValue::Default)];
        if config.enable_dismiss_from_end_to_start {
            anchors.push((-config.dismissed_offset, DismissValue::DismissedToStart));
        }
        if config.enable_dismiss_from_start_to_end {
            anchors.push((config.dismissed_offset, DismissValue::DismissedToEnd));
        }
        // Sort by offset for correct clamp/nearest/next-anchor logic.
        anchors.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
        let swipeable = repose_core::SwipeableState::new(
            anchors,
            repose_core::SwipeableConfig {
                animation_spec: config.animation_spec.clone(),
                positional_threshold,
                ..Default::default()
            },
        );
        // Start at the default position (not anchors[0], which may be negative).
        swipeable.snap_to(0.0);
        Self {
            swipeable,
            dismissed_offset: config.dismissed_offset,
        }
    }

    /// Current animated offset in pixels.
    pub fn offset(&self) -> f32 {
        self.swipeable.offset()
    }

    /// Snap instantly to an offset (used during active drag).
    pub fn set_offset_instant(&self, off: f32) {
        self.swipeable.snap_to(off);
    }

    /// Whether the current position is past the dismiss threshold.
    pub fn is_dismissed(&self) -> bool {
        self.swipeable.current_value() != DismissValue::Default
    }

    /// Animate to the dismissed position.
    pub fn dismiss(&self) {
        self.swipeable.animate_to(&DismissValue::DismissedToStart);
    }

    /// Animate to the dismissed position with custom offset.
    pub fn dismiss_to(&self, offset: f32) {
        let value = if offset < 0.0 {
            DismissValue::DismissedToStart
        } else {
            DismissValue::DismissedToEnd
        };
        self.swipeable.animate_to(&value);
    }

    /// Animate back to origin.
    pub fn reset(&self) {
        self.swipeable.animate_to(&DismissValue::Default);
    }

    /// Fire the dismiss callback once when the spring settles past a given threshold.
    fn try_handle_dismiss_with_threshold(&self, on_dismiss: &Option<Rc<dyn Fn()>>, threshold: f32) {
        if !self.swipeable.is_animating() {
            let val = self.swipeable.current_value();
            if val != DismissValue::Default {
                if let Some(cb) = on_dismiss {
                    cb();
                }
            }
        }
    }
}

/// M3 SwipeToDismiss - wraps content that can be swiped to reveal
/// a `background` action view. On release past the threshold the content
/// springs to the dismissed position and `on_dismiss` fires **once**.
///
/// The gesture logic uses `SwipeableState<DismissValue>` internally, so it
/// supports both left and right dismiss directions based on the config.
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

    let s1 = state.swipeable.clone();
    let s2 = state.swipeable.clone();
    let s3 = state.swipeable.clone();
    let on_down = { move |e: PointerEvent| s1.on_pointer_down(e.position.x) };
    let on_move = { move |e: PointerEvent| s2.on_pointer_move(e.position.x) };
    let on_up = { move |_e: PointerEvent| s3.on_pointer_up() };

    let display_offset = offset
        .max(-config.dismissed_offset)
        .min(config.dismissed_offset);

    let content_modifier = {
        let mut m = Modifier::new()
            .fill_max_width()
            .translate(display_offset, 0.0);
        if config.gestures_enabled {
            m = m
                .on_pointer_down(on_down)
                .on_pointer_move(on_move)
                .on_pointer_up(on_up);
        }
        m
    };

    Column(modifier.fill_max_width()).child((
        Box(Modifier::new().fill_max_size().absolute()).child(background),
        Box(content_modifier).child(content),
    ))
}

/// M3 Carousel - a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
/// Configuration for [`Carousel`].
#[derive(Clone, Debug)]
pub struct CarouselConfig {
    pub modifier: Modifier,
}

impl Default for CarouselConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
        }
    }
}

/// M3 Carousel - a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
pub fn Carousel<T, F>(
    items: Vec<T>,
    item_width: f32,
    peek_amount: f32,
    state: Rc<LazyRowState>,
    item_builder: F,
    config: CarouselConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let padded_modifier = config.modifier.padding_values(PaddingValues {
        left: peek_amount,
        right: peek_amount,
        top: 0.0,
        bottom: 0.0,
    });

    LazyRow(
        items,
        item_width,
        item_builder,
        LazyRowConfig {
            state,
            modifier: padded_modifier,
            ..Default::default()
        },
    )
}
