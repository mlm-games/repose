#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt, ZStack};

use super::*;

struct TimeParts {
    hour: Signal<u32>,
    hour_normalizer: RefCell<Option<SubGuard<u32>>>,
}

fn install_hour_normalizer(parts: &Rc<TimeParts>) {
    let weak = Rc::downgrade(parts);
    let hour_normalizer = parts.hour.subscribe_guard(move |hour| {
        let Some(parts) = weak.upgrade() else { return };
        let normalized = display_hour(*hour);
        if *hour != normalized {
            parts.hour.set_neq(normalized);
        }
    });
    *parts.hour_normalizer.borrow_mut() = Some(hour_normalizer);
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
        let parts = Rc::new(TimeParts {
            hour: hour.clone(),
            hour_normalizer: RefCell::new(None),
        });
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

/// Material 3 clock face for a 24-hour selection.
pub fn TimePicker24Hour(state: Rc<TimePickerState>, layout: TimePickerLayoutType) -> View {
    const DIAL: f32 = 256.0;
    const OUTER: f32 = 101.0;
    const INNER: f32 = 69.0;
    let colors = TimePickerColors::default();
    let (hour, minute) = state.selected_time();
    let field = state.selected_field.get();
    let selecting_hour = field == TimePickerField::Hour;
    let selected = if selecting_hour { hour } else { minute };
    let divisions = if selecting_hour { 12 } else { 60 };
    let angle = |value: u32| (value % divisions) as f32 * std::f32::consts::TAU / divisions as f32;
    let point = |value: u32, radius: f32| {
        let (x, y) = angle(value).sin_cos();
        (x * radius, -y * radius)
    };
    let radius = if selecting_hour && hour >= 12 {
        INNER
    } else {
        OUTER
    };
    let (x, y) = point(selected, radius);
    let circle = |diameter: f32, x: f32, y: f32, color: Color| {
        Modifier::new()
            .size(Dp(diameter), Dp(diameter))
            .offset(Dp(x), Dp(y))
            .clip_rounded(Dp(diameter / 2.0))
            .background(color)
    };
    let center = DIAL / 2.0;
    let (normal_x, normal_y) = (-y / radius, x / radius);
    let track = ShapePath::new()
        .move_to(Dp(center + normal_x), Dp(center + normal_y))
        .line_to(Dp(center + x + normal_x), Dp(center + y + normal_y))
        .line_to(Dp(center + x - normal_x), Dp(center + y - normal_y))
        .line_to(Dp(center - normal_x), Dp(center - normal_y))
        .close();
    let mut face = vec![
        Box(Modifier::new()
            .size(Dp(DIAL), Dp(DIAL))
            .background_shape_color(colors.selector_color, Shape::path(track))),
        Box(circle(48.0, x, y, colors.selector_color)),
        Box(circle(8.0, 0.0, 0.0, colors.selector_color)),
    ];
    if !selecting_hour && minute % 5 != 0 {
        face.push(Box(circle(
            4.0,
            x,
            y,
            colors.clock_dial_selected_content_color,
        )));
    }

    let values: Vec<u32> = if selecting_hour {
        (0..24).collect()
    } else {
        (0..60).step_by(5).collect()
    };
    for value in values {
        let (x, y) = point(
            value,
            if selecting_hour && value >= 12 {
                INNER
            } else {
                OUTER
            },
        );
        let label = if selecting_hour {
            value.to_string()
        } else {
            format!("{value:02}")
        };
        let state = state.clone();
        face.push(
            Box(Modifier::new()
                .size(Dp(32.0), Dp(32.0))
                .offset(Dp(x), Dp(y))
                .content_alignment(Alignment::Center)
                .clickable()
                .on_click(move || {
                    if selecting_hour {
                        state.set_period(value < 12);
                        state.set_hour(value);
                        state.selected_field.set_neq(TimePickerField::Minute);
                    } else {
                        state.set_minute(value);
                    }
                })
                .semantics(Semantics {
                    role: Role::Button,
                    label: Some(label.clone()),
                    selected: Some(value == selected),
                    enabled: true,
                    ..Default::default()
                }))
            .child(Text(label).size(theme().typography.body_large).color(
                if value == selected {
                    colors.clock_dial_selected_content_color
                } else {
                    colors.clock_dial_unselected_content_color
                },
            )),
        );
    }

    let selector = |part, value| {
        let active = field == part;
        let state = state.clone();
        Box(Modifier::new()
            .size(Dp(96.0), Dp(80.0))
            .content_alignment(Alignment::Center)
            .clip_rounded(Dp(8.0))
            .background(if active {
                colors.time_selector_selected_container_color
            } else {
                colors.time_selector_unselected_container_color
            })
            .clickable()
            .on_click(move || state.selected_field.set_neq(part))
            .semantics(Semantics {
                role: Role::Button,
                label: Some(match part {
                    TimePickerField::Hour => "Hour".into(),
                    TimePickerField::Minute => "Minute".into(),
                }),
                value: Some(format!("{value:02}")),
                selected: Some(active),
                enabled: true,
                ..Default::default()
            }))
        .child(Text(format!("{value:02}")).size(Sp(57.0)).color(if active {
            colors.time_selector_selected_content_color
        } else {
            colors.time_selector_unselected_content_color
        }))
    };
    let display = Row(Modifier::new().align_items(AlignItems::CENTER)).child((
        selector(TimePickerField::Hour, hour),
        Box(Modifier::new()
            .width(Dp(24.0))
            .content_alignment(Alignment::Center))
        .child(Text(":").size(Sp(57.0))),
        selector(TimePickerField::Minute, minute),
    ));

    let active_pointer = remember(|| Cell::new(None::<(PointerId, Vec2, bool)>));
    let center = Dp(DIAL / 2.0).to_px().0;
    let drag_slop = Dp(4.0).to_px().0;
    let inner_threshold = Dp(74.0).to_px().0;
    let select = {
        let state = state.clone();
        move |position: Vec2, dragged: bool, finished: bool| {
            let x = position.x - center;
            let y = position.y - center;
            let turns = x.atan2(-y) / std::f32::consts::TAU;
            if selecting_hour {
                let hour = (turns * 12.0).round() as i32;
                let inner = x.hypot(y) < inner_threshold;
                batch(|| {
                    state.set_period(!inner);
                    state.set_hour(hour.rem_euclid(12) as u32);
                    if finished {
                        state.selected_field.set_neq(TimePickerField::Minute);
                    }
                });
            } else {
                let step = if dragged { 1 } else { 5 };
                let tick = (turns * (60 / step) as f32).round() as i32;
                state.set_minute((tick * step).rem_euclid(60) as u32);
            }
        }
    };
    let dial = ZStack(
        Modifier::new()
            .size(Dp(DIAL), Dp(DIAL))
            .clip_rounded(Dp(DIAL / 2.0))
            .background(colors.clock_dial_color)
            .on_pointer_down({
                let active_pointer = active_pointer.clone();
                move |event| {
                    if active_pointer.get().is_none()
                        && matches!(event.event, PointerEventKind::Down(PointerButton::Primary))
                    {
                        active_pointer.set(Some((event.id, event.position, false)));
                    }
                }
            })
            .on_pointer_move({
                let active_pointer = active_pointer.clone();
                let select = select.clone();
                move |event| {
                    if let Some((id, origin, dragged)) = active_pointer.get()
                        && id == event.id
                        && (dragged
                            || (event.position.x - origin.x).hypot(event.position.y - origin.y)
                                > drag_slop)
                    {
                        active_pointer.set(Some((id, origin, true)));
                        select(event.position, true, false);
                        event.consume();
                    }
                }
            })
            .on_pointer_up({
                let active_pointer = active_pointer.clone();
                move |event| {
                    if let Some((id, _, dragged)) = active_pointer.get()
                        && id == event.id
                        && matches!(event.event, PointerEventKind::Up(PointerButton::Primary))
                    {
                        active_pointer.set(None);
                        select(event.position, dragged, true);
                        event.consume();
                    }
                }
            })
            .on_pointer_cancel(move |_| active_pointer.set(None)),
    )
    .with_children(face);

    match layout {
        TimePickerLayoutType::Horizontal => Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .gap(Dp(36.0)))
        .child((display, dial)),
        TimePickerLayoutType::Vertical => Column(
            Modifier::new()
                .width(Dp(DIAL))
                .align_items(AlignItems::CENTER)
                .gap(Dp(36.0)),
        )
        .child((display, dial)),
    }
}

/// A Material 3 time picker dialog with a 24-hour clock face.
pub fn TimePicker24HourDialog(
    dialog: Rc<DialogState>,
    time: Rc<TimePickerState>,
    title: View,
    dismiss_button: View,
    confirm_button: View,
) -> View {
    let width = get_window_container_width();
    let height = get_window_container_height();
    let landscape = width > height && width >= 556.0 && height >= 340.0;
    let layout = if landscape {
        TimePickerLayoutType::Horizontal
    } else {
        TimePickerLayoutType::Vertical
    };
    let panel_width = Dp(if landscape { 556.0 } else { 304.0 });
    let picker = TimePicker24Hour(time, layout);
    let actions = Row(Modifier::new()
        .fill_max_width()
        .justify_content(JustifyContent::END)
        .gap(Dp(8.0)))
    .child((dismiss_button, confirm_button));
    let content = if landscape {
        let bottom = if height >= 384.0 { 24.0 } else { 8.0 };
        let picker_top = (height - 256.0) / 2.0 - 22.0;
        Column(
            Modifier::new()
                .fill_max_width()
                .height(Dp(height))
                .padding_values(PaddingValues {
                    left: Dp(24.0),
                    right: Dp(24.0),
                    top: Dp(24.0),
                    bottom: Dp(bottom),
                }),
        )
        .child((
            Box(Modifier::new().height(Dp(24.0))).child(title),
            Box(Modifier::new().height(Dp((picker_top - 48.0).max(0.0)))),
            picker,
            Box(Modifier::new().height(Dp((height - bottom - 48.0 - picker_top - 256.0).max(0.0)))),
            actions,
        ))
    } else {
        Column(Modifier::new().fill_max_width().padding(Dp(24.0))).child((
            title,
            Box(Modifier::new().height(Dp(20.0))),
            picker,
            Box(Modifier::new().height(Dp(24.0))),
            actions,
        ))
    };
    Dialog(
        dialog,
        Modifier::new().width(panel_width),
        DialogProperties {
            use_platform_default_width: false,
            use_platform_insets: false,
            ..Default::default()
        },
        content,
    )
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

#[derive(Clone, Debug)]
pub struct TimeInputConfig {
    pub modifier: Modifier,
    pub hour_label: String,
    pub minute_label: String,
}

impl Default for TimeInputConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            hour_label: "Hour".into(),
            minute_label: "Minute".into(),
        }
    }
}

pub fn TimeInput(state: Rc<TimePickerState>, config: TimeInputConfig) -> View {
    let hour_text = format!("{:02}", state.hour.get().min(23));
    let minute_text = format!("{:02}", state.minute.get().min(59));
    let is_am = state.is_am.get();
    let st_hour = state.clone();
    let st_minute = state.clone();
    let st_period = state.clone();
    Row(config.modifier.align_items(AlignItems::CENTER)).child((
        OutlinedTextField(
            Modifier::new().width(Dp(96.0)),
            hour_text,
            move |v| {
                if let Ok(h) = v.trim().parse::<u32>() {
                    st_hour.set_hour(h.min(23));
                }
            },
            OutlinedTextFieldConfig {
                label: Some(config.hour_label.clone()),
                single_line: true,
                ..Default::default()
            },
        ),
        Text(":").size(theme().typography.display_medium),
        OutlinedTextField(
            Modifier::new().width(Dp(96.0)),
            minute_text,
            move |v| {
                if let Ok(m) = v.trim().parse::<u32>() {
                    st_minute.set_minute(m.min(59));
                }
            },
            OutlinedTextFieldConfig {
                label: Some(config.minute_label.clone()),
                single_line: true,
                ..Default::default()
            },
        ),
        Spacer(),
        TextButton(
            Modifier::new(),
            {
                let st_period = st_period.clone();
                move || st_period.is_am.set_neq(true)
            },
            ButtonConfig {
                enabled: !is_am,
                ..Default::default()
            },
            || Text("AM").single_line(),
        ),
        TextButton(
            Modifier::new(),
            move || st_period.is_am.set_neq(false),
            ButtonConfig {
                enabled: is_am,
                ..Default::default()
            },
            || Text("PM").single_line(),
        ),
    ))
}
