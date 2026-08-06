#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Icon, Symbol};
use repose_core::*;
use repose_ui::{
    Box, TextStyle, ViewExt,
    anim::{animate_color, animate_f32},
};

use super::*;

/// Configuration for [`Checkbox`].
#[derive(Clone, Debug)]
pub struct CheckboxConfig {
    pub modifier: Modifier,
    /// When false, the checkbox renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub checked_color: Color,
    pub unchecked_color: Color,
    pub checkmark_color: Color,
    /// Border color when checked. Default: same as `checked_color`.
    pub checked_border_color: Color,
    /// Border color when unchecked. Default: same as `unchecked_color`.
    pub unchecked_border_color: Color,
    pub disabled_checked_box_color: Color,
    pub disabled_unchecked_box_color: Color,
    pub disabled_indeterminate_box_color: Color,
    pub disabled_checkmark_color: Color,
    pub disabled_checked_border_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_indeterminate_border_color: Color,
    pub state_colors: StateColors,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for CheckboxConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            checked_color: CheckboxDefaults::checked_color(),
            unchecked_color: CheckboxDefaults::unchecked_color(),
            checkmark_color: CheckboxDefaults::checkmark_color(),
            checked_border_color: CheckboxDefaults::checked_color(),
            unchecked_border_color: CheckboxDefaults::unchecked_color(),
            disabled_checked_box_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_unchecked_box_color: Color::TRANSPARENT,
            disabled_indeterminate_box_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_checkmark_color: CheckboxDefaults::disabled_checkmark_color(),
            disabled_checked_border_color: CheckboxDefaults::disabled_checked_box_color(),
            disabled_unchecked_border_color: CheckboxDefaults::disabled_unchecked_border_color(),
            disabled_indeterminate_border_color: CheckboxDefaults::disabled_checked_box_color(),
            state_colors: CheckboxDefaults::state_colors_default(),
            interaction_source: None,
        }
    }
}

/// M3 Checkbox.
/// Renders a 40dp touch-target with an 18dp check box inside.
/// Fill, border, and check mark animate with 100ms FastOutSlowIn.
static CHECKBOX_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Checkbox(checked: bool, on_change: impl Fn(bool) + 'static, config: CheckboxConfig) -> View {
    let th = theme();
    let sz = CheckboxDefaults::BOX_SIZE;

    let id = remember(|| CHECKBOX_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color_fast;

    let is_enabled = config.enabled;

    let fill = animate_color(
        format!("cb_fill_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_box_color
            } else {
                config.disabled_unchecked_box_color
            }
        } else if checked {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("cb_bw_{}", id),
        if !is_enabled && checked {
            0.0
        } else if !is_enabled {
            CheckboxDefaults::STROKE_WIDTH
        } else if checked {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("cb_bd_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if checked {
            Color::TRANSPARENT
        } else {
            config.unchecked_border_color
        },
        spec,
    );
    let check_alpha = animate_f32(
        format!("cb_ca_{}", id),
        if checked { 1.0 } else { 0.0 },
        spec,
    );
    let check_col = if !is_enabled {
        config.disabled_checkmark_color
    } else {
        config.checkmark_color
    };

    let cb = move || {
        if config.enabled {
            on_change(!checked)
        }
    };

    let cb_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    Box(Modifier::new()
        .width(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .height(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .state_colors(config.state_colors)
        .interaction_source(&*cb_source)
        .clickable()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .on_click(cb)
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(sz, sz)
            .background(fill)
            .border(bd_w, bd, CheckboxDefaults::CORNER_RADIUS)
            .clip_rounded(CheckboxDefaults::CORNER_RADIUS)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER))
        .child(if check_alpha > 0.01 {
            Box(Modifier::new().alpha(check_alpha)).child(
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(check_col)
                    .size(CheckboxDefaults::CHECK_ICON_SIZE),
            )
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Three-state value for [`TriStateCheckbox`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriState {
    Checked,
    Unchecked,
    Indeterminate,
}

/// M3 Tri-State Checkbox - cycles through Checked -> Indeterminate -> Unchecked.
/// Indeterminate shows a dash instead of a checkmark.
pub fn TriStateCheckbox(
    state: TriState,
    on_change: impl Fn(TriState) + 'static,
    config: CheckboxConfig,
) -> View {
    let th = theme();
    let sz = CheckboxDefaults::BOX_SIZE;

    let id = remember(|| CHECKBOX_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color_fast;

    let is_checked = state == TriState::Checked;
    let is_indeterminate = state == TriState::Indeterminate;
    let has_fill = is_checked || is_indeterminate;
    let is_enabled = config.enabled;

    let fill = animate_color(
        format!("tc_fill_{}", id),
        if !is_enabled {
            if has_fill {
                config.disabled_indeterminate_box_color
            } else {
                config.disabled_unchecked_box_color
            }
        } else if has_fill {
            config.checked_color
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let bd_w = animate_f32(
        format!("tc_bw_{}", id),
        if !is_enabled {
            if has_fill {
                0.0
            } else {
                CheckboxDefaults::STROKE_WIDTH
            }
        } else if has_fill {
            0.0
        } else {
            CheckboxDefaults::STROKE_WIDTH
        },
        spec,
    );
    let bd = animate_color(
        format!("tc_bd_{}", id),
        if !is_enabled {
            if has_fill {
                config.disabled_indeterminate_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if has_fill {
            Color::TRANSPARENT
        } else {
            config.unchecked_border_color
        },
        spec,
    );
    let symbol_alpha = animate_f32(
        format!("tc_sa_{}", id),
        if has_fill { 1.0 } else { 0.0 },
        spec,
    );
    let symbol_col = if !is_enabled {
        config.disabled_checkmark_color
    } else {
        config.checkmark_color
    };

    Box(Modifier::new()
        .width(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .height(CheckboxDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .clickable()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .on_click(move || {
            if is_enabled {
                on_change(match state {
                    TriState::Checked => TriState::Unchecked,
                    TriState::Indeterminate => TriState::Checked,
                    TriState::Unchecked => TriState::Checked,
                })
            }
        })
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(sz, sz)
            .background(fill)
            .border(bd_w, bd, CheckboxDefaults::CORNER_RADIUS)
            .clip_rounded(CheckboxDefaults::CORNER_RADIUS)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER))
        .child(if symbol_alpha > 0.01 {
            Box(Modifier::new().alpha(symbol_alpha)).child(if is_indeterminate {
                // Dash for indeterminate
                Box(Modifier::new()
                    .width(10.0)
                    .height(2.0)
                    .background(symbol_col)
                    .clip_rounded(1.0))
            } else {
                Icon(Symbol::new("done", '\u{E876}'))
                    .color(symbol_col)
                    .size(CheckboxDefaults::CHECK_ICON_SIZE)
            })
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Configuration for [`RadioButton`].
#[derive(Clone, Debug)]
pub struct RadioButtonConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub selected_color: Color,
    pub unselected_color: Color,
    pub disabled_selected_color: Color,
    pub disabled_unselected_color: Color,
    pub state_colors: StateColors,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for RadioButtonConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            selected_color: RadioButtonDefaults::selected_color(),
            unselected_color: RadioButtonDefaults::unselected_color(),
            disabled_selected_color: RadioButtonDefaults::disabled_selected_color(),
            disabled_unselected_color: RadioButtonDefaults::disabled_unselected_color(),
            state_colors: RadioButtonDefaults::state_colors_default(),
            interaction_source: None,
        }
    }
}

/// M3 RadioButton.
/// Renders a 40dp touch-target with a 20dp outer circle + inner dot.
/// Ring color animates with 100ms FastOutSlowIn; dot size animates with spring.
static RADIO_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn RadioButton(
    selected: bool,
    on_select: impl Fn() + 'static,
    config: RadioButtonConfig,
) -> View {
    let th = theme();
    let d = RadioButtonDefaults::OUTER_RADIUS * 2.0;

    let id = remember(|| RADIO_COUNTER.fetch_add(1, Ordering::Relaxed));
    let color_spec = th.motion.color_fast;
    let spring = th.motion.spring;

    let ring_col = animate_color(
        format!("rb_ring_{}", id),
        if !config.enabled {
            if selected {
                config.disabled_selected_color
            } else {
                config.disabled_unselected_color
            }
        } else if selected {
            config.selected_color
        } else {
            config.unselected_color
        },
        color_spec,
    );
    let dot_size = animate_f32(
        format!("rb_dot_{}", id),
        if selected {
            RadioButtonDefaults::DOT_RADIUS * 2.0
        } else {
            0.0
        },
        spring,
    );
    let dot_col = if !config.enabled {
        config.disabled_selected_color
    } else {
        config.selected_color
    };

    let cb = move || {
        if config.enabled {
            on_select()
        }
    };

    let rb_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    Box(Modifier::new()
        .width(RadioButtonDefaults::TOUCH_TARGET_SIZE)
        .height(RadioButtonDefaults::TOUCH_TARGET_SIZE)
        .padding(0.0)
        .clip_rounded(20.0)
        .background(Color::TRANSPARENT)
        .state_colors(config.state_colors)
        .interaction_source(&*rb_source)
        .clickable()
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .on_click(cb)
        .then(config.modifier))
    .child(
        Box(Modifier::new()
            .size(d, d)
            .border(RadioButtonDefaults::STROKE_WIDTH, ring_col, d * 0.5)
            .clip_rounded(d * 0.5)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER))
        .child(if dot_size > 0.5 {
            Box(Modifier::new()
                .size(dot_size, dot_size)
                .background(dot_col)
                .clip_rounded(dot_size * 0.5))
        } else {
            Box(Modifier::new())
        }),
    )
}

/// Configuration for [`Switch`].
#[derive(Clone, Debug)]
pub struct SwitchConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to clicks.
    pub enabled: bool,
    pub checked_track_color: Color,
    pub unchecked_track_color: Color,
    pub checked_thumb_color: Color,
    pub unchecked_thumb_color: Color,
    /// Icon color for the thumb content when checked. Default: `on_primary`.
    pub checked_icon_color: Color,
    /// Icon color for the thumb content when unchecked. Default: `outline`.
    pub unchecked_icon_color: Color,
    /// Border color when checked. Default: transparent.
    pub checked_border_color: Color,
    /// Border color when unchecked.
    pub unchecked_border_color: Color,
    pub disabled_checked_thumb_color: Color,
    pub disabled_checked_track_color: Color,
    pub disabled_checked_border_color: Color,
    pub disabled_checked_icon_color: Color,
    pub disabled_unchecked_thumb_color: Color,
    pub disabled_unchecked_track_color: Color,
    pub disabled_unchecked_border_color: Color,
    pub disabled_unchecked_icon_color: Color,
    pub state_colors: StateColors,
    pub thumb_content: Option<View>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for SwitchConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            checked_track_color: SwitchDefaults::checked_track_color(),
            unchecked_track_color: SwitchDefaults::unchecked_track_color(),
            checked_thumb_color: SwitchDefaults::checked_thumb_color(),
            unchecked_thumb_color: SwitchDefaults::unchecked_thumb_color(),
            checked_icon_color: SwitchDefaults::checked_icon_color(),
            unchecked_icon_color: SwitchDefaults::unchecked_icon_color(),
            checked_border_color: Color::TRANSPARENT,
            unchecked_border_color: SwitchDefaults::unchecked_border_color(),
            disabled_checked_thumb_color: SwitchDefaults::disabled_checked_thumb_color(),
            disabled_checked_track_color: SwitchDefaults::disabled_checked_track_color(),
            disabled_checked_border_color: Color::TRANSPARENT,
            disabled_checked_icon_color: SwitchDefaults::disabled_checked_icon_color(),
            disabled_unchecked_thumb_color: SwitchDefaults::disabled_unchecked_thumb_color(),
            disabled_unchecked_track_color: SwitchDefaults::disabled_unchecked_track_color(),
            disabled_unchecked_border_color: SwitchDefaults::disabled_unchecked_border_color(),
            disabled_unchecked_icon_color: SwitchDefaults::disabled_unchecked_icon_color(),
            state_colors: SwitchDefaults::state_colors_default(),
            thumb_content: None,
            interaction_source: None,
        }
    }
}

/// M3 Switch.
/// Renders a pill track with an animated thumb knob.
/// Thumb position, size, and colors animate with spring/tween physics.
static SWITCH_COUNTER: AtomicU64 = AtomicU64::new(0);
pub fn Switch(checked: bool, on_change: impl Fn(bool) + 'static, config: SwitchConfig) -> View {
    let th = theme();
    let track_w = SwitchDefaults::TRACK_WIDTH;
    let track_h = SwitchDefaults::TRACK_HEIGHT;

    let id = remember(|| SWITCH_COUNTER.fetch_add(1, Ordering::Relaxed));

    let hovered = remember(|| Signal::new(false));
    let pressed = remember(|| Signal::new(false));

    // Thumb: spring-animated position and size
    let thumb_target_pos = if checked {
        track_w - SwitchDefaults::THUMB_CHECKED_SIZE - 4.0
    } else {
        8.0
    };
    let thumb_target_d = if checked {
        SwitchDefaults::THUMB_CHECKED_SIZE
    } else {
        SwitchDefaults::THUMB_UNCHECKED_SIZE
    };
    let spring = th.motion.spring;

    let thumb_left = animate_f32(format!("sw_pos_{}", id), thumb_target_pos, spring);
    let thumb_d = animate_f32(format!("sw_d_{}", id), thumb_target_d, spring);
    let thumb_top = (track_h - thumb_d) * 0.5;

    let color_spec = th.motion.color_fast;
    let is_enabled = config.enabled;

    let track_bg = animate_color(
        format!("sw_tbg_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_track_color
            } else {
                config.disabled_unchecked_track_color
            }
        } else if checked {
            config.checked_track_color
        } else {
            config.unchecked_track_color
        },
        color_spec,
    );
    let thumb_bg = animate_color(
        format!("sw_tmbg_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_thumb_color
            } else {
                config.disabled_unchecked_thumb_color
            }
        } else if checked {
            config.checked_thumb_color
        } else {
            config.unchecked_thumb_color
        },
        color_spec,
    );
    let track_border = animate_f32(
        format!("sw_tb_{}", id),
        if !is_enabled {
            if checked { 0.0 } else { 2.0 }
        } else if checked {
            0.0
        } else {
            2.0
        },
        color_spec,
    );
    let border_color = animate_color(
        format!("sw_bc_{}", id),
        if !is_enabled {
            if checked {
                config.disabled_checked_border_color
            } else {
                config.disabled_unchecked_border_color
            }
        } else if checked {
            config.checked_border_color
        } else {
            config.unchecked_border_color
        },
        color_spec,
    );

    let state_overlay = animate_color(
        format!("sw_ol_{}", id),
        if !is_enabled {
            Color::TRANSPARENT
        } else if pressed.get() {
            config.state_colors.pressed
        } else if hovered.get() {
            config.state_colors.hovered
        } else {
            config.state_colors.default
        },
        color_spec,
    );

    let sw_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    Box(Modifier::new()
        .size(track_w, track_h)
        .padding(0.0)
        .clip_rounded(track_h * 0.5)
        .background(track_bg)
        .border(track_border, border_color, track_h * 0.5)
        .interaction_source(&*sw_source)
        .clickable()
        .on_pointer_enter({
            let h = hovered.clone();
            move |_| h.set(true)
        })
        .on_pointer_leave({
            let h = hovered.clone();
            let p = pressed.clone();
            move |_| {
                h.set(false);
                p.set(false);
            }
        })
        .on_pointer_down({
            let p = pressed.clone();
            move |_| p.set(true)
        })
        .on_click({
            let cb = on_change;
            move || cb(!checked)
        })
        .on_pointer_up({
            let p = pressed.clone();
            move |_| p.set(false)
        })
        .then(config.modifier))
    .child((
        Box(Modifier::new()
            .size(thumb_d, thumb_d)
            .background(thumb_bg)
            .clip_rounded(thumb_d * 0.5)
            .hit_passthrough()
            .absolute()
            .offset(Some(thumb_left), Some(thumb_top), None, None)),
        Box(Modifier::new()
            .size(40.0, 40.0)
            .clip_rounded(20.0)
            .background(state_overlay)
            .hit_passthrough()
            .absolute()
            .offset(
                Some(thumb_left + thumb_d * 0.5 - 20.0),
                Some(track_h * 0.5 - 20.0),
                None,
                None,
            )),
    ))
}
