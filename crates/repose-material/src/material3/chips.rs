#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::Ordering;

use repose_core::*;
use repose_ui::{Box, FlowRow, FlowRowConfig, Row, ViewExt, anim::animate_color};

use super::util::{FILTERCHIP_COUNTER, apply_m3_clickable, with_button_semantics};
use super::*;

/// Color slots for chips (both non-selectable and selectable).
#[derive(Clone, Copy, Debug)]
pub struct ChipColors {
    pub container_color: Color,
    pub label_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,
    pub disabled_container_color: Color,
    pub disabled_label_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,
    pub selected_container_color: Color,
    pub selected_label_color: Color,
    pub selected_leading_icon_color: Color,
    pub selected_trailing_icon_color: Color,
    pub disabled_selected_container_color: Color,
}

impl ChipColors {
    pub fn container(&self, enabled: bool, selected: bool) -> Color {
        match (enabled, selected) {
            (true, true) => self.selected_container_color,
            (true, false) => self.container_color,
            (false, true) => self.disabled_selected_container_color,
            (false, false) => self.disabled_container_color,
        }
    }
    pub fn label(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_label_color
        } else if selected {
            self.selected_label_color
        } else {
            self.label_color
        }
    }
    pub fn leading_icon(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if selected {
            self.selected_leading_icon_color
        } else {
            self.leading_icon_color
        }
    }
    pub fn trailing_icon(&self, enabled: bool, selected: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if selected {
            self.selected_trailing_icon_color
        } else {
            self.trailing_icon_color
        }
    }
}

/// Elevation levels for chips.
#[derive(Clone, Copy, Debug)]
pub struct ChipElevation {
    pub default: f32,
    pub hovered: f32,
    pub focused: f32,
    pub pressed: f32,
    pub dragged: f32,
    pub disabled: f32,
}

impl ChipElevation {
    pub fn to_state_elevation(&self) -> StateElevation {
        StateElevation {
            default: self.default,
            hovered: self.hovered,
            focused: self.focused,
            pressed: self.pressed,
            dragged: self.dragged,
            disabled: self.disabled,
        }
    }
}

impl Default for ChipElevation {
    fn default() -> Self {
        Self {
            default: ChipDefaults::elevation_default(),
            hovered: ChipDefaults::elevation_hovered(),
            focused: ChipDefaults::elevation_focused(),
            pressed: ChipDefaults::elevation_pressed(),
            dragged: ChipDefaults::elevation_dragged(),
            disabled: ChipDefaults::elevation_disabled(),
        }
    }
}

/// Configuration for chips.
#[derive(Clone, Debug)]
pub struct ChipConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub colors: ChipColors,
    pub elevation: ChipElevation,
    pub border_width: f32,
    pub border_color: Color,
    pub selected_border_color: Color,
    pub disabled_border_color: Color,
    pub disabled_selected_border_color: Color,
    pub shape_radius: f32,
    pub horizontal_padding: f32,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for ChipConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            colors: ChipColors {
                container_color: ChipDefaults::container_color(),
                label_color: ChipDefaults::label_color(),
                leading_icon_color: ChipDefaults::leading_icon_color(),
                trailing_icon_color: ChipDefaults::trailing_icon_color(),
                disabled_container_color: ChipDefaults::disabled_container_color(),
                disabled_label_color: ChipDefaults::disabled_label_color(),
                disabled_leading_icon_color: ChipDefaults::disabled_leading_icon_color(),
                disabled_trailing_icon_color: ChipDefaults::disabled_trailing_icon_color(),
                selected_container_color: ChipDefaults::selected_container_color(),
                selected_label_color: ChipDefaults::selected_label_color(),
                selected_leading_icon_color: ChipDefaults::selected_leading_icon_color(),
                selected_trailing_icon_color: ChipDefaults::selected_trailing_icon_color(),
                disabled_selected_container_color: ChipDefaults::disabled_selected_container_color(
                ),
            },
            elevation: ChipElevation::default(),
            border_width: ChipDefaults::BORDER_WIDTH,
            border_color: ChipDefaults::border_color(),
            selected_border_color: ChipDefaults::selected_border_color(),
            disabled_border_color: ChipDefaults::disabled_border_color(),
            disabled_selected_border_color: ChipDefaults::disabled_selected_border_color(),
            shape_radius: ChipDefaults::SHAPE_RADIUS,
            horizontal_padding: ChipDefaults::HORIZONTAL_PADDING,
            interaction_source: None,
        }
    }
}

/// M3 Assist Chip - a chip for triggering actions.
pub fn AssistChip(
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let trailing_color = colors.trailing_icon(is_enabled, false);
    let border = if is_enabled {
        config.border_color
    } else {
        config.disabled_border_color
    };
    let shape = config.shape_radius;
    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }

    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

/// M3 Elevated Assist Chip - like [`AssistChip`] but with elevated container.
pub fn ElevatedAssistChip(
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: ChipConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let colors = &config.colors;
    let bg = colors.container(is_enabled, false);
    let label_color = colors.label(is_enabled, false);
    let leading_color = colors.leading_icon(is_enabled, false);
    let trailing_color = colors.trailing_icon(is_enabled, false);
    let shape = config.shape_radius;
    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .state_elevation(config.elevation.to_state_elevation())
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

    let ch_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .flex_shrink(0.0)
        .min_height(ChipDefaults::HEIGHT)
        .height(ChipDefaults::HEIGHT)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: 0.0,
            bottom: 0.0,
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    if config.border_width > 0.0 && border != Color::TRANSPARENT {
        m = m.border(config.border_width, border, shape);
    }
    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

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

/// Shared layout for the M3 chip group composables: a full-width wrapping
/// `FlowRow` whose chips keep their intrinsic width (via `flex_shrink(0)`)
/// instead of shrinking under constrained/centered parents.
pub fn chip_group_flow(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    FlowRow(
        Modifier::new()
            .fill_max_width()
            .gap(8.0)
            .align_items(AlignItems::CENTER)
            .then(modifier),
        FlowRowConfig::default(),
    )
    .child(children)
}

/// M3 Filter Chip Group.
pub fn FilterChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Elevated Filter Chip Group.
pub fn ElevatedFilterChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Assist Chip Group.
pub fn AssistChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Elevated Assist Chip Group.
/// [`ElevatedAssistChip`]s.
pub fn ElevatedAssistChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Suggestion Chip Group.
pub fn SuggestionChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Elevated Suggestion Chip Group.
pub fn ElevatedSuggestionChipGroup(
    modifier: Modifier,
    children: impl repose_ui::IntoChildren,
) -> View {
    chip_group_flow(modifier, children)
}

/// M3 Input Chip Group.
pub fn InputChipGroup(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    chip_group_flow(modifier, children)
}
