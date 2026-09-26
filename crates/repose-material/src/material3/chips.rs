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
    pub default: Dp,
    pub hovered: Dp,
    pub focused: Dp,
    pub pressed: Dp,
    pub dragged: Dp,
    pub disabled: Dp,
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
    pub border_width: Dp,
    pub border_color: Color,
    pub selected_border_color: Color,
    pub disabled_border_color: Color,
    pub disabled_selected_border_color: Color,
    pub shape_radius: Dp,
    pub horizontal_padding: Dp,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChipStyle {
    /// Outlined variant: draws a border, no elevation.
    Bordered,
    /// Filled variant: draws elevation, no border.
    Elevated,
}

/// Selectable chips tween their colors, keyed by a per-instance id drawn from
/// [`FILTERCHIP_COUNTER`]. Non-selectable chips pass `None` and resolve colors
/// directly. The `id` is allocated by each public wrapper (not here) so that
/// chips of different kinds rendered in the same scope keep distinct slots.
#[derive(Clone, Copy)]
struct ChipAnim {
    prefix: &'static str,
    id: u64,
}

/// The label plus its optional leading/trailing adornments.
struct ChipContent {
    label: View,
    leading: Option<View>,
    trailing: Option<View>,
}

fn chip_icon_slot(view: View, color: Color, gap: Dp, leading: bool) -> View {
    Box(Modifier::new().padding_values(PaddingValues {
        left: if leading { Dp(0.0) } else { gap },
        right: if leading { gap } else { Dp(0.0) },
        top: Dp(0.0),
        bottom: Dp(0.0),
    }))
    .child(with_content_color(color, move || view))
}

fn chip_impl(
    style: ChipStyle,
    selected: bool,
    anim: Option<ChipAnim>,
    on_click: impl Fn() + 'static,
    content: ChipContent,
    config: ChipConfig,
) -> View {
    let ChipContent {
        label,
        leading: leading_icon,
        trailing: trailing_icon,
    } = content;
    let th = theme();
    let spec = th.motion.color;
    let is_enabled = config.enabled;
    let colors = &config.colors;

    let (bg, label_color, leading_color, trailing_color) = match anim {
        Some(ChipAnim { prefix, id }) => (
            animate_color(
                format!("{prefix}_bg_{id}"),
                colors.container(is_enabled, selected),
                spec,
            ),
            animate_color(
                format!("{prefix}_lc_{id}"),
                colors.label(is_enabled, selected),
                spec,
            ),
            animate_color(
                format!("{prefix}_lic_{id}"),
                colors.leading_icon(is_enabled, selected),
                spec,
            ),
            animate_color(
                format!("{prefix}_tic_{id}"),
                colors.trailing_icon(is_enabled, selected),
                spec,
            ),
        ),
        None => (
            colors.container(is_enabled, selected),
            colors.label(is_enabled, selected),
            colors.leading_icon(is_enabled, selected),
            colors.trailing_icon(is_enabled, selected),
        ),
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
        });

    if style == ChipStyle::Elevated {
        m = m.state_elevation(config.elevation.to_state_elevation());
    }

    m = m
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.horizontal_padding,
            top: Dp(0.0),
            bottom: Dp(0.0),
        })
        .background(bg)
        .clip_rounded(shape)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .then(config.modifier);

    if style == ChipStyle::Bordered {
        let border = match (is_enabled, selected) {
            (true, true) => config.selected_border_color,
            (true, false) => config.border_color,
            (false, true) => config.disabled_selected_border_color,
            (false, false) => config.disabled_border_color,
        };
        if config.border_width.0 > 0.0 && border != Color::TRANSPARENT {
            m = m.border(config.border_width, border, shape);
        }
    }

    m = apply_m3_clickable(m, &ch_source, label_color, is_enabled, on_click);
    m = with_button_semantics(m, is_enabled);

    let lead = leading_icon
        .map(|v| chip_icon_slot(v, leading_color, Dp(8.0), true))
        .unwrap_or_else(|| Box(Modifier::new()));
    let row = Row(Modifier::new().align_items(AlignItems::CENTER));

    match trailing_icon {
        Some(v) => {
            let trail = chip_icon_slot(v, trailing_color, Dp(8.0), false);
            Box(m).child(row.child((lead, with_content_color(label_color, move || label), trail)))
        }
        None => Box(m).child(row.child((lead, with_content_color(label_color, move || label)))),
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
    chip_impl(
        ChipStyle::Bordered,
        false,
        None,
        on_click,
        ChipContent {
            label,
            leading: leading_icon,
            trailing: trailing_icon,
        },
        config,
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
    chip_impl(
        ChipStyle::Elevated,
        false,
        None,
        on_click,
        ChipContent {
            label,
            leading: leading_icon,
            trailing: trailing_icon,
        },
        config,
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
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    chip_impl(
        ChipStyle::Bordered,
        selected,
        Some(ChipAnim {
            prefix: "fc",
            id: *id,
        }),
        on_click,
        ChipContent {
            label,
            leading: leading_icon,
            trailing: trailing_icon,
        },
        config,
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
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    chip_impl(
        ChipStyle::Elevated,
        selected,
        Some(ChipAnim {
            prefix: "efc",
            id: *id,
        }),
        on_click,
        ChipContent {
            label,
            leading: leading_icon,
            trailing: trailing_icon,
        },
        config,
    )
}

pub fn SuggestionChip(
    on_click: impl Fn() + 'static,
    label: View,
    icon: Option<View>,
    config: ChipConfig,
) -> View {
    chip_impl(
        ChipStyle::Bordered,
        false,
        None,
        on_click,
        ChipContent {
            label,
            leading: icon,
            trailing: None,
        },
        config,
    )
}

/// M3 Elevated Suggestion Chip - like [`SuggestionChip`] but with elevation and filled bg.
pub fn ElevatedSuggestionChip(
    on_click: impl Fn() + 'static,
    label: View,
    icon: Option<View>,
    config: ChipConfig,
) -> View {
    chip_impl(
        ChipStyle::Elevated,
        false,
        None,
        on_click,
        ChipContent {
            label,
            leading: icon,
            trailing: None,
        },
        config,
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
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    chip_impl(
        ChipStyle::Bordered,
        selected,
        Some(ChipAnim {
            prefix: "ic",
            id: *id,
        }),
        on_click,
        ChipContent {
            label,
            leading: avatar.or(leading_icon),
            trailing: trailing_icon,
        },
        config,
    )
}

/// Shared layout for the M3 chip group composables: a full-width wrapping
/// `FlowRow` whose chips keep their intrinsic width (via `flex_shrink(0)`)
/// instead of shrinking under constrained/centered parents.
pub fn chip_group_flow(modifier: Modifier, children: impl repose_ui::IntoChildren) -> View {
    FlowRow(
        Modifier::new()
            .fill_max_width()
            .gap(Dp(8.0))
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
