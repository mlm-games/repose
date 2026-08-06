#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt};

use super::*;

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
