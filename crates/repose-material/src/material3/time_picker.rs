#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt};

use super::*;

struct TimeParts {
    hour: Signal<u32>,
}

fn install_hour_normalizer(parts: &Rc<TimeParts>) {
    let weak = Rc::downgrade(parts);
    parts.hour.subscribe(move |hour| {
        let Some(parts) = weak.upgrade() else { return };
        let normalized = display_hour(*hour);
        if *hour != normalized {
            parts.hour.set_neq(normalized);
        }
    });
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TimePickerField {
    Hour,
    Minute,
}

/// State for `TimePicker` - manages selected hour and minute.
pub struct TimePickerState {
    pub hour: Signal<u32>,
    pub minute: Signal<u32>,
    pub is_am: Signal<bool>,
    pub selected_field: Signal<TimePickerField>,
    _parts: Rc<TimeParts>,
}

impl TimePickerState {
    pub fn new(hour: u32, minute: u32) -> Self {
        let hour = hour % 24;
        let is_am = hour < 12;
        let hour = signal(display_hour(hour));
        let parts = Rc::new(TimeParts { hour: hour.clone() });
        install_hour_normalizer(&parts);
        Self {
            hour,
            minute: signal(minute.min(59)),
            is_am: signal(is_am),
            selected_field: signal(TimePickerField::Hour),
            _parts: parts,
        }
    }

    pub fn set_hour(&self, hour: u32) {
        repose_core::reactive::batch(|| {
            self.hour.set_neq(display_hour(hour));
            self.selected_field.set_neq(TimePickerField::Hour);
        });
    }

    pub fn set_minute(&self, minute: u32) {
        repose_core::reactive::batch(|| {
            self.minute.set_neq(minute.min(59));
            self.selected_field.set_neq(TimePickerField::Minute);
        });
    }

    pub fn set_period(&self, is_am: bool) {
        self.is_am.set_neq(is_am);
    }

    pub fn display_hour(&self) -> u32 {
        display_hour(self.hour.get())
    }

    pub fn selected_time(&self) -> (u32, u32) {
        let h = self.display_hour();
        let hour = if self.is_am.get() {
            if h == 12 { 0 } else { h }
        } else if h == 12 {
            12
        } else {
            h + 12
        };
        (hour, self.minute.get().min(59))
    }
}

fn display_hour(hour: u32) -> u32 {
    let hour = hour % 12;
    if hour == 0 { 12 } else { hour }
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
    pub confirm_label: String,
    pub dismiss_label: String,
}

impl Default for TimePickerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: TimePickerColors::default(),
            layout_type: TimePickerLayoutType::Vertical,
            confirm_label: TimePickerDefaults::CONFIRM_LABEL.to_string(),
            dismiss_label: TimePickerDefaults::DISMISS_LABEL.to_string(),
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
    let hour = state.display_hour();
    let minute = state.minute.get().min(59);
    let is_am = state.is_am.get();
    let selected_field = state.selected_field.get();
    let dismiss_label = config.dismiss_label.clone();
    let confirm_label = config.confirm_label.clone();

    let hour_str = format!("{hour:02}");
    let min_str = format!("{minute:02}");
    let selector_content = config.colors.time_selector_selected_content_color;
    let dial_content = config.colors.clock_dial_selected_content_color;
    let selected_period_content = match config.layout_type {
        TimePickerLayoutType::Horizontal => selector_content,
        TimePickerLayoutType::Vertical => config.colors.period_selector_selected_content_color,
    };
    let picker_width = match config.layout_type {
        TimePickerLayoutType::Horizontal => Dp(360.0),
        TimePickerLayoutType::Vertical => Dp(TimePickerDefaults::CLOCK_DIAL_CONTAINER_SIZE.0),
    };

    Column(
        config
            .modifier
            .width(picker_width)
            .padding(Dp(24.0))
            .align_items(AlignItems::CENTER)
            .background(config.colors.container_color)
            .semantics(Semantics {
                role: Role::Container,
                label: Some("Time picker".into()),
                value: Some(format!(
                    "{hour_str}:{min_str} {}",
                    if is_am { "AM" } else { "PM" }
                )),
                ..Default::default()
            }),
    )
    .child((
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .background(config.colors.time_selector_unselected_container_color)
            .border(
                Dp(1.0),
                config.colors.clock_dial_color,
                TimePickerDefaults::CLOCK_DIAL_MIN_CONTAINER_SIZE * 0.5,
            )
            .clip_rounded(TimePickerDefaults::CLOCK_DIAL_MIN_CONTAINER_SIZE * 0.5))
        .child((
            Box(Modifier::new()
                .background(config.colors.time_selector_selected_container_color)
                .padding(Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || {
                        let next = display_hour(s.hour.get()) % 12 + 1;
                        s.set_hour(next);
                    }
                })
                .semantics(Semantics {
                    role: Role::Button,
                    label: Some("Hour".into()),
                    enabled: true,
                    selected: Some(selected_field == TimePickerField::Hour),
                    value: Some(hour_str.clone()),
                    ..Default::default()
                }))
            .child(
                Text(hour_str)
                    .size(Sp(48.0))
                    .color(dial_content)
                    .single_line(),
            ),
            Text(":")
                .size(Sp(48.0))
                .color(config.colors.clock_dial_unselected_content_color)
                .single_line(),
            Box(Modifier::new()
                .background(config.colors.time_selector_selected_container_color)
                .padding(Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || s.set_minute((s.minute.get() + 1) % 60)
                })
                .semantics(Semantics {
                    role: Role::Button,
                    label: Some("Minute".into()),
                    enabled: true,
                    selected: Some(selected_field == TimePickerField::Minute),
                    value: Some(min_str.clone()),
                    ..Default::default()
                }))
            .child(
                Text(min_str)
                    .size(Sp(48.0))
                    .color(dial_content)
                    .single_line(),
            ),
        )),
        Box(Modifier::new().fill_max_width().height(Dp(16.0))),
        // AM/PM toggle
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: Dp(12.0),
                    right: Dp(12.0),
                    top: Dp(4.0),
                    bottom: Dp(4.0),
                })
                .background(if is_am {
                    config.colors.period_selector_selected_container_color
                } else {
                    config.colors.period_selector_unselected_container_color
                })
                .clip_rounded(Dp(8.0))
                .border(Dp(1.0), config.colors.period_selector_border_color, Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || {
                        if !s.is_am.get() {
                            s.set_period(true);
                        }
                    }
                })
                .semantics(Semantics {
                    role: Role::RadioButton,
                    label: Some("AM".into()),
                    enabled: true,
                    selected: Some(is_am),
                    checked: Some(is_am),
                    ..Default::default()
                }))
            .child(Text("AM").size(th.typography.label_large).color(if is_am {
                selected_period_content
            } else {
                config.colors.period_selector_unselected_content_color
            })),
            Box(Modifier::new().width(Dp(8.0)).height(Dp(1.0))),
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: Dp(12.0),
                    right: Dp(12.0),
                    top: Dp(4.0),
                    bottom: Dp(4.0),
                })
                .background(if !is_am {
                    config.colors.period_selector_selected_container_color
                } else {
                    config.colors.period_selector_unselected_container_color
                })
                .clip_rounded(Dp(8.0))
                .border(Dp(1.0), config.colors.period_selector_border_color, Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let s = state.clone();
                    move || {
                        if s.is_am.get() {
                            s.set_period(false);
                        }
                    }
                })
                .semantics(Semantics {
                    role: Role::RadioButton,
                    label: Some("PM".into()),
                    enabled: true,
                    selected: Some(!is_am),
                    checked: Some(!is_am),
                    ..Default::default()
                }))
            .child(Text("PM").size(th.typography.label_large).color(if !is_am {
                selected_period_content
            } else {
                config.colors.period_selector_unselected_content_color
            })),
        )),
        Box(Modifier::new().fill_max_width().height(Dp(16.0))),
        Row(Modifier::new().fill_max_width()).child((
            Spacer(),
            Box(Modifier::new()
                .padding(Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let on_dismiss = on_dismiss.clone();
                    move || on_dismiss()
                })
                .semantics(Semantics {
                    role: Role::Button,
                    label: Some(dismiss_label.clone()),
                    enabled: true,
                    ..Default::default()
                }))
            .child(
                Text(dismiss_label)
                    .color(config.colors.selector_color)
                    .size(th.typography.label_large)
                    .single_line(),
            ),
            Box(Modifier::new().width(Dp(8.0)).height(Dp(1.0))),
            Box(Modifier::new()
                .padding(Dp(8.0))
                .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                    color: Some(th.on_surface),
                    bounded: true,
                    ..Default::default()
                }))
                .clickable()
                .on_click({
                    let on_confirm = on_confirm.clone();
                    let state = state.clone();
                    move || {
                        let (h, m) = state.selected_time();
                        on_confirm(h, m);
                    }
                })
                .semantics(Semantics {
                    role: Role::Button,
                    label: Some(confirm_label.clone()),
                    enabled: true,
                    ..Default::default()
                }))
            .child(
                Text(confirm_label)
                    .color(config.colors.selector_color)
                    .size(th.typography.label_large)
                    .single_line(),
            ),
        )),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn period_toggle_keeps_display_hour_in_range() {
        let state = TimePickerState::new(11, 7);
        state.set_period(false);
        assert_eq!(state.display_hour(), 11);
        assert_eq!(state.selected_time(), (23, 7));
        state.set_period(true);
        assert_eq!(state.selected_time(), (11, 7));
    }

    #[test]
    fn public_hour_updates_keep_twelve_hour_values() {
        let state = TimePickerState::new(0, 0);
        state.hour.set(0);
        assert_eq!(state.hour.get(), 12);
        state.set_hour(24);
        assert_eq!(state.display_hour(), 12);
        state.set_minute(75);
        assert_eq!(state.selected_time(), (0, 59));
        assert_eq!(state.selected_field.get(), TimePickerField::Minute);
    }
}
