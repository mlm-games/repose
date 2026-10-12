#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt};

use super::*;

struct DateParts {
    year: Signal<i32>,
    month: Signal<u32>,
    day: Signal<u32>,
    year_normalizer: RefCell<Option<SubGuard<i32>>>,
    month_normalizer: RefCell<Option<SubGuard<u32>>>,
}

fn install_date_normalizers(parts: &Rc<DateParts>) {
    let weak = Rc::downgrade(parts);
    let year_normalizer = parts.year.subscribe_guard(move |year| {
        let Some(parts) = weak.upgrade() else { return };
        let current_month = parts.month.get();
        let month = current_month.clamp(1, 12);
        let current_day = parts.day.get();
        let day = current_day.clamp(1, days_in_month(*year, month));
        if current_month != month {
            parts.month.set_neq(month);
        }
        if current_day != day {
            parts.day.set_neq(day);
        }
    });
    *parts.year_normalizer.borrow_mut() = Some(year_normalizer);

    let weak = Rc::downgrade(parts);
    let month_normalizer = parts.month.subscribe_guard(move |_| {
        let Some(parts) = weak.upgrade() else { return };
        let year = parts.year.get();
        let current_month = parts.month.get();
        let month = current_month.clamp(1, 12);
        let current_day = parts.day.get();
        let day = current_day.clamp(1, days_in_month(year, month));
        if current_month != month {
            parts.month.set_neq(month);
        }
        if current_day != day {
            parts.day.set_neq(day);
        }
    });
    *parts.month_normalizer.borrow_mut() = Some(month_normalizer);
}

/// State for `DatePicker` - manages selected date.
pub struct DatePickerState {
    pub year: Signal<i32>,
    pub month: Signal<u32>,
    pub day: Signal<u32>,
    id: u64,
    _parts: Rc<DateParts>,
}

impl DatePickerState {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        let month = month.clamp(1, 12);
        let day = day.clamp(1, days_in_month(year, month));
        Self::from_signals(signal(year), signal(month), signal(day))
    }

    fn from_signals(year: Signal<i32>, month: Signal<u32>, day: Signal<u32>) -> Self {
        let parts = Rc::new(DateParts {
            year: year.clone(),
            month: month.clone(),
            day: day.clone(),
            year_normalizer: RefCell::new(None),
            month_normalizer: RefCell::new(None),
        });
        install_date_normalizers(&parts);
        Self {
            year,
            month,
            day,
            id: unique_component_id(),
            _parts: parts,
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("date-picker:{}_{}", self.id, suffix)
    }

    pub fn try_new(year: i32, month: u32, day: u32) -> Option<Self> {
        is_valid_date(year, month, day)
            .then(|| Self::from_signals(signal(year), signal(month), signal(day)))
    }

    pub fn is_valid(year: i32, month: u32, day: u32) -> bool {
        is_valid_date(year, month, day)
    }

    pub fn is_valid_date(year: i32, month: u32, day: u32) -> bool {
        is_valid_date(year, month, day)
    }

    pub fn set_year(&self, year: i32) {
        let month = self.month.get().clamp(1, 12);
        let day = self.day.get().clamp(1, days_in_month(year, month));
        repose_core::reactive::batch(|| {
            self.year.set_neq(year);
            self.month.set_neq(month);
            self.day.set_neq(day);
        });
    }

    pub fn set_month(&self, month: u32) {
        let month = month.clamp(1, 12);
        let year = self.year.get();
        let day = self.day.get().clamp(1, days_in_month(year, month));
        repose_core::reactive::batch(|| {
            self.year.set_neq(year);
            self.month.set_neq(month);
            self.day.set_neq(day);
        });
    }

    pub fn set_day(&self, day: u32) {
        let year = self.year.get();
        let month = self.month.get().clamp(1, 12);
        let day = day.clamp(1, days_in_month(year, month));
        repose_core::reactive::batch(|| {
            self.year.set_neq(year);
            self.month.set_neq(month);
            self.day.set_neq(day);
        });
    }

    pub fn set_date(&self, year: i32, month: u32, day: u32) -> bool {
        if !is_valid_date(year, month, day) {
            return false;
        }
        repose_core::reactive::batch(|| {
            self.year.set_neq(year);
            self.month.set_neq(month);
            self.day.set_neq(day);
        });
        true
    }

    pub fn selected_date(&self) -> (i32, u32, u32) {
        let year = self.year.get();
        let month = self.month.get().clamp(1, 12);
        let day = self.day.get().clamp(1, days_in_month(year, month));
        (year, month, day)
    }

    pub fn current_date(&self) -> (i32, u32, u32) {
        self.selected_date()
    }
}

pub fn is_valid_date(year: i32, month: u32, day: u32) -> bool {
    (1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month)
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
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

pub fn today_date() -> (i32, u32, u32) {
    let today = ReposeDate::now();
    (today.year, today.month, today.day)
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
    pub confirm_label: String,
    pub dismiss_label: String,
}

impl Default for DatePickerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: DatePickerColors::default(),
            show_mode_toggle: true,
            confirm_label: DatePickerDefaults::CONFIRM_LABEL.to_string(),
            dismiss_label: DatePickerDefaults::DISMISS_LABEL.to_string(),
        }
    }
}

fn navigation_button(
    instance_key: &str,
    label: &'static str,
    glyph: &str,
    color: Color,
    on_click: impl Fn() + 'static,
) -> View {
    let source = remember_with_key(
        format!("{instance_key}:navigation:{label}"),
        MutableInteractionSource::new,
    );
    let modifier = Modifier::new()
        .size(
            DatePickerDefaults::DATE_CELL_SIZE,
            DatePickerDefaults::DATE_CELL_SIZE,
        )
        .semantics(Semantics {
            role: Role::Button,
            label: Some(label.into()),
            enabled: true,
            ..Default::default()
        });
    let modifier = super::util::apply_m3_clickable(modifier, &source, color, true, on_click);
    Box(modifier
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER))
    .child(Text(glyph.to_string()).color(color).size(Sp(16.0)))
}

/// M3 Date Picker with month/year navigation, proper calendar grid,
/// today indicator, and confirm/cancel actions.
pub fn DatePicker(
    state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: DatePickerConfig,
) -> View {
    let th = theme();
    let instance_key = state.key("navigation");
    let (year, month, day) = state.selected_date();
    let dim = days_in_month(year, month);
    let start_dow = first_day_of_month(year, month);

    let prev_year = {
        let s = state.clone();
        move || {
            let next = s.year.get().saturating_sub(1);
            s.set_year(next);
        }
    };
    let next_year = {
        let s = state.clone();
        move || {
            let next = s.year.get().saturating_add(1);
            s.set_year(next);
        }
    };

    let prev_month = {
        let s = state.clone();
        move || {
            let (y, m, d) = s.selected_date();
            let (next_y, next_m) = if m == 1 {
                (y.saturating_sub(1), 12)
            } else {
                (y, m - 1)
            };
            s.set_date(next_y, next_m, d.min(days_in_month(next_y, next_m)));
        }
    };

    let next_month = {
        let s = state.clone();
        move || {
            let (y, m, d) = s.selected_date();
            let (next_y, next_m) = if m == 12 {
                (y.saturating_add(1), 1)
            } else {
                (y, m + 1)
            };
            s.set_date(next_y, next_m, d.min(days_in_month(next_y, next_m)));
        }
    };

    let today = today_date();
    let selected_value = format!("{year:04}-{month:02}-{day:02}");
    let dismiss_label = config.dismiss_label.clone();
    let confirm_label = config.confirm_label.clone();
    let dismiss_semantics_label = dismiss_label.clone();
    let confirm_semantics_label = confirm_label.clone();

    let year_controls: View = if config.show_mode_toggle {
        Row(Modifier::new().gap(Dp(8.0)).align_items(AlignItems::CENTER)).child((
            navigation_button(
                &instance_key,
                "Previous year",
                "‹",
                config.colors.navigation_color,
                prev_year,
            ),
            Box(Modifier::new()
                .background(config.colors.year_selected_container_color)
                .clip_rounded(Dp(4.0)))
            .child(
                Text(year.to_string())
                    .size(th.typography.body_small)
                    .color(config.colors.year_selected_content_color),
            ),
            navigation_button(
                &instance_key,
                "Next year",
                "›",
                config.colors.navigation_color,
                next_year,
            ),
        ))
    } else {
        Box(Modifier::new()).child(
            Text(year.to_string())
                .size(th.typography.body_small)
                .color(config.colors.year_unselected_content_color),
        )
    };

    Column(
        config
            .modifier
            .background(config.colors.container_color)
            .padding(DatePickerDefaults::HORIZONTAL_PADDING)
            .semantics(Semantics {
                role: Role::Container,
                label: Some("Date picker".into()),
                value: Some(selected_value),
                ..Default::default()
            }),
    )
    .child((
        // Month header
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child((
            navigation_button(
                &instance_key,
                "Previous month",
                "◀",
                config.colors.navigation_color,
                prev_month,
            ),
            Spacer(),
            Column(Modifier::new().align_items(AlignItems::CENTER)).child((
                Text(MONTH_NAMES[(month - 1) as usize].to_string())
                    .size(th.typography.title_medium)
                    .color(config.colors.header_color),
                year_controls,
            )),
            Spacer(),
            navigation_button(
                &instance_key,
                "Next month",
                "▶",
                config.colors.navigation_color,
                next_month,
            ),
        )),
        Box(Modifier::new().fill_max_width().height(Dp(12.0))),
        // Day grid
        Column(Modifier::new()).child({
            let mut rows: Vec<View> = Vec::new();
            // Day-of-week headers
            let dow_headers: Vec<View> = DOW_HEADERS
                .iter()
                .map(|d| {
                    Box(Modifier::new()
                        .width(DatePickerDefaults::DATE_CELL_SIZE)
                        .height(DatePickerDefaults::DATE_CELL_SIZE)
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
                        week.push(Box(Modifier::new()
                            .width(DatePickerDefaults::DATE_CELL_SIZE)
                            .height(DatePickerDefaults::DATE_CELL_SIZE)));
                    } else {
                        let day_num = (cell_idx - start_dow + 1) as i32;
                        if day_num <= dim as i32 {
                            let is_selected = day_num == day as i32;
                            let is_today =
                                today.0 == year && today.1 == month && today.2 == day_num as u32;
                            let s = state.clone();
                            let day_value = format!("{year:04}-{month:02}-{day_num:02}");
                            let day_label = format!("{day_num}");
                            let day_color = if is_selected {
                                config.colors.selected_day_color
                            } else if is_today {
                                config.colors.today_content_color
                            } else {
                                config.colors.day_color
                            };
                            week.push(
                                Box(Modifier::new()
                                    .width(DatePickerDefaults::DATE_CELL_SIZE)
                                    .height(DatePickerDefaults::DATE_CELL_SIZE)
                                    .background(if is_selected {
                                        config.colors.selected_day_container_color
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .clip_rounded(DatePickerDefaults::DATE_CELL_SIZE * 0.5)
                                    .indication(crate::ripple::ripple(
                                        crate::ripple::RippleConfig {
                                            color: Some(theme().on_surface),
                                            bounded: true,
                                            ..Default::default()
                                        },
                                    ))
                                    .align_items(AlignItems::CENTER)
                                    .justify_content(JustifyContent::CENTER)
                                    .clickable()
                                    .on_click(move || {
                                        let (y, m, _) = s.selected_date();
                                        s.set_date(y, m, day_num as u32);
                                    })
                                    .semantics(Semantics {
                                        role: Role::Button,
                                        label: Some(day_label),
                                        enabled: true,
                                        selected: Some(is_selected),
                                        value: Some(day_value),
                                        ..Default::default()
                                    }))
                                .child({
                                    let mut t = Text(day_num.to_string())
                                        .size(th.typography.body_medium)
                                        .color(day_color);
                                    if is_today && !is_selected {
                                        t = t.modifier(Modifier::new().border(
                                            DatePickerDefaults::TODAY_BORDER_WIDTH,
                                            config.colors.today_border_color,
                                            DatePickerDefaults::DATE_CELL_SIZE * 0.5,
                                        ));
                                    }
                                    t
                                }),
                            );
                        } else {
                            week.push(Box(Modifier::new()
                                .width(DatePickerDefaults::DATE_CELL_SIZE)
                                .height(DatePickerDefaults::DATE_CELL_SIZE)));
                        }
                    }
                }
                rows.push(Row(Modifier::new()).with_children(week));
            }
            rows
        }),
        Box(Modifier::new().fill_max_width().height(Dp(12.0))),
        // Cancel / Confirm
        Row(Modifier::new()
            .fill_max_width()
            .justify_content(JustifyContent::END)
            .gap(Dp(8.0)))
        .child((
            TextButton(
                Modifier::new(),
                {
                    let on_dismiss = on_dismiss.clone();
                    move || (on_dismiss)()
                },
                ButtonConfig::default(),
                move || Text(dismiss_label.clone()).size(Sp(14.0)),
            )
            .semantics(Semantics {
                role: Role::Button,
                label: Some(dismiss_semantics_label),
                enabled: true,
                ..Default::default()
            }),
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
                move || Text(confirm_label.clone()).size(Sp(14.0)),
            )
            .semantics(Semantics {
                role: Role::Button,
                label: Some(confirm_semantics_label),
                enabled: true,
                ..Default::default()
            }),
        )),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_dates_are_not_exposed() {
        assert!(DatePickerState::try_new(2024, 2, 30).is_none());
        assert_eq!(
            DatePickerState::new(2024, 2, 30).selected_date(),
            (2024, 2, 29)
        );
        let state = DatePickerState::new(2024, 1, 31);
        assert!(!state.set_date(2023, 2, 29));
        assert_eq!(state.selected_date(), (2024, 1, 31));
        state.month.set(2);
        assert_eq!(state.day.get(), 29);
        state.year.set(2023);
        assert_eq!(state.day.get(), 28);
    }

    #[test]
    fn date_range_selection_orders_and_restarts() {
        let state = DateRangePickerState::new();
        assert_eq!(state.range(), None);
        state.select((2024, 5, 10));
        assert_eq!(state.range(), None);
        state.select((2024, 5, 3));
        assert_eq!(state.range(), None);
        assert_eq!(state.start.get(), Some((2024, 5, 3)));
        state.select((2024, 5, 15));
        assert_eq!(state.range(), Some(((2024, 5, 3), (2024, 5, 15))));
        state.select((2024, 6, 1));
        assert_eq!(state.range(), None);
        assert_eq!(state.start.get(), Some((2024, 6, 1)));
    }
}

pub type CalendarDate = (i32, u32, u32);

pub struct DateRangePickerState {
    pub start: Signal<Option<CalendarDate>>,
    pub end: Signal<Option<CalendarDate>>,
    pub selecting_end: Signal<bool>,
    pub year: Signal<i32>,
    pub month: Signal<u32>,
    id: u64,
}

impl DateRangePickerState {
    pub fn new() -> Self {
        let (y, m, _) = today_date();
        Self {
            start: signal(None),
            end: signal(None),
            selecting_end: signal(false),
            year: signal(y),
            month: signal(m),
            id: unique_component_id(),
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("date-range-picker:{}_{}", self.id, suffix)
    }

    pub fn range(&self) -> Option<(CalendarDate, CalendarDate)> {
        match (self.start.get(), self.end.get()) {
            (Some(s), Some(e)) if s <= e => Some((s, e)),
            (Some(s), Some(e)) => Some((e, s)),
            _ => None,
        }
    }

    pub fn select(&self, date: CalendarDate) {
        if !self.selecting_end.get() || self.start.get().is_none() {
            self.start.set(Some(date));
            self.end.set(None);
            self.selecting_end.set(true);
        } else if self.start.get().is_some_and(|s| date < s) {
            self.start.set(Some(date));
            self.end.set(None);
        } else {
            self.end.set(Some(date));
            self.selecting_end.set(false);
        }
    }

    pub fn clear(&self) {
        self.start.set(None);
        self.end.set(None);
        self.selecting_end.set(false);
    }
}

impl Default for DateRangePickerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct DateRangePickerConfig {
    pub modifier: Modifier,
    pub colors: DatePickerColors,
    pub confirm_label: String,
    pub dismiss_label: String,
}

impl Default for DateRangePickerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: DatePickerColors::default(),
            confirm_label: DatePickerDefaults::CONFIRM_LABEL.to_string(),
            dismiss_label: DatePickerDefaults::DISMISS_LABEL.to_string(),
        }
    }
}

pub fn DateRangePicker(
    state: Rc<DateRangePickerState>,
    on_confirm: Rc<dyn Fn(CalendarDate, CalendarDate)>,
    on_dismiss: Rc<dyn Fn()>,
    config: DateRangePickerConfig,
) -> View {
    let th = theme();
    let instance_key = state.key("navigation");
    let year = state.year.get();
    let month = state.month.get().clamp(1, 12);
    let dim = days_in_month(year, month);
    let start_dow = first_day_of_month(year, month);
    let today = today_date();
    let (start, end) = (state.start.get(), state.end.get());
    let (lo, hi) = match (start, end) {
        (Some(s), Some(e)) if s <= e => (Some(s), Some(e)),
        (Some(s), Some(e)) => (Some(e), Some(s)),
        (s, None) => (s, s),
        (None, _) => (None, None),
    };

    let prev_month = {
        let s = state.clone();
        move || {
            let (y, m) = (s.year.get(), s.month.get().clamp(1, 12));
            let (next_y, next_m) = if m == 1 {
                (y.saturating_sub(1), 12)
            } else {
                (y, m - 1)
            };
            s.year.set_neq(next_y);
            s.month.set_neq(next_m);
        }
    };
    let next_month = {
        let s = state.clone();
        move || {
            let (y, m) = (s.year.get(), s.month.get().clamp(1, 12));
            let (next_y, next_m) = if m == 12 {
                (y.saturating_add(1), 1)
            } else {
                (y, m + 1)
            };
            s.year.set_neq(next_y);
            s.month.set_neq(next_m);
        }
    };

    let fmt = |d: CalendarDate| format!("{:04}-{:02}-{:02}", d.0, d.1, d.2);
    let value = match (lo, hi) {
        (Some(l), Some(h)) => format!("{} – {}", fmt(l), fmt(h)),
        _ => "No range selected".into(),
    };
    let dismiss_label = config.dismiss_label.clone();
    let confirm_label = config.confirm_label.clone();

    Column(
        config
            .modifier
            .background(config.colors.container_color)
            .padding(DatePickerDefaults::HORIZONTAL_PADDING)
            .semantics(Semantics {
                role: Role::Container,
                label: Some("Date range picker".into()),
                value: Some(value),
                ..Default::default()
            }),
    )
    .child((
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child((
            navigation_button(
                &instance_key,
                "Previous month",
                "◀",
                config.colors.navigation_color,
                prev_month,
            ),
            Spacer(),
            Text(format!("{} {}", MONTH_NAMES[(month - 1) as usize], year))
                .size(th.typography.title_medium)
                .color(config.colors.header_color),
            Spacer(),
            navigation_button(
                &instance_key,
                "Next month",
                "▶",
                config.colors.navigation_color,
                next_month,
            ),
        )),
        Box(Modifier::new().fill_max_width().height(Dp(12.0))),
        Column(Modifier::new()).child({
            let mut rows: Vec<View> = Vec::new();
            rows.push(
                Row(Modifier::new()).with_children(
                    DOW_HEADERS
                        .iter()
                        .map(|d| {
                            Box(Modifier::new()
                                .width(DatePickerDefaults::DATE_CELL_SIZE)
                                .height(DatePickerDefaults::DATE_CELL_SIZE)
                                .align_items(AlignItems::CENTER)
                                .justify_content(JustifyContent::CENTER))
                            .child(
                                Text(d.to_string())
                                    .size(th.typography.label_small)
                                    .color(config.colors.weekday_color),
                            )
                        })
                        .collect(),
                ),
            );
            let total_cells = start_dow + dim;
            let num_rows = total_cells.div_ceil(7).min(6);
            for w in 0..num_rows {
                let mut week: Vec<View> = Vec::new();
                for d in 0..7 {
                    let cell_idx = w * 7 + d;
                    if cell_idx < start_dow {
                        week.push(Box(Modifier::new()
                            .width(DatePickerDefaults::DATE_CELL_SIZE)
                            .height(DatePickerDefaults::DATE_CELL_SIZE)));
                    } else {
                        let day_num = (cell_idx - start_dow + 1) as i32;
                        if day_num <= dim as i32 {
                            let date = (year, month, day_num as u32);
                            let is_endpoint = lo == Some(date) || hi == Some(date) && lo != hi;
                            let in_range = match (lo, hi) {
                                (Some(l), Some(h)) => date > l && date < h,
                                _ => false,
                            };
                            let is_today = today == date;
                            let s = state.clone();
                            let day_color = if is_endpoint {
                                config.colors.selected_day_color
                            } else if is_today {
                                config.colors.today_content_color
                            } else {
                                config.colors.day_color
                            };
                            week.push(
                                Box(Modifier::new()
                                    .width(DatePickerDefaults::DATE_CELL_SIZE)
                                    .height(DatePickerDefaults::DATE_CELL_SIZE)
                                    .background(if is_endpoint {
                                        config.colors.selected_day_container_color
                                    } else if in_range {
                                        config
                                            .colors
                                            .selected_day_container_color
                                            .with_alpha_f32(0.35)
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .clip_rounded(DatePickerDefaults::DATE_CELL_SIZE * 0.5)
                                    .align_items(AlignItems::CENTER)
                                    .justify_content(JustifyContent::CENTER)
                                    .clickable()
                                    .on_click(move || s.select(date))
                                    .semantics(Semantics {
                                        role: Role::Button,
                                        label: Some(day_num.to_string()),
                                        enabled: true,
                                        selected: Some(is_endpoint || in_range),
                                        value: Some(fmt(date)),
                                        ..Default::default()
                                    }))
                                .child(
                                    Text(day_num.to_string())
                                        .size(th.typography.body_medium)
                                        .color(day_color),
                                ),
                            );
                        } else {
                            week.push(Box(Modifier::new()
                                .width(DatePickerDefaults::DATE_CELL_SIZE)
                                .height(DatePickerDefaults::DATE_CELL_SIZE)));
                        }
                    }
                }
                rows.push(Row(Modifier::new()).with_children(week));
            }
            rows
        }),
        Box(Modifier::new().fill_max_width().height(Dp(12.0))),
        Row(Modifier::new()
            .fill_max_width()
            .justify_content(JustifyContent::END)
            .gap(Dp(8.0)))
        .child((
            TextButton(
                Modifier::new(),
                {
                    let on_dismiss = on_dismiss.clone();
                    move || (on_dismiss)()
                },
                ButtonConfig::default(),
                move || Text(dismiss_label.clone()).size(Sp(14.0)),
            ),
            TextButton(
                Modifier::new(),
                {
                    let on_confirm = on_confirm.clone();
                    let state = state.clone();
                    move || {
                        if let Some((s, e)) = state.range() {
                            on_confirm(s, e);
                        }
                    }
                },
                ButtonConfig::default(),
                move || Text(confirm_label.clone()).size(Sp(14.0)),
            ),
        )),
    ))
}

fn parse_calendar_date(text: &str) -> Option<CalendarDate> {
    let mut parts = text.trim().split('-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (y, m, d) = (
        y.parse::<i32>().ok()?,
        m.parse::<u32>().ok()?,
        d.parse::<u32>().ok()?,
    );
    is_valid_date(y, m, d).then_some((y, m, d))
}

pub fn DateInput(state: Rc<DatePickerState>, modifier: Modifier, label: Option<String>) -> View {
    let (y, m, d) = state.selected_date();
    let text = format!("{y:04}-{m:02}-{d:02}");
    let invalid = remember_state_with_key(state.key("input-error"), || false);
    let invalid_get = || *invalid.borrow();
    let mut config = OutlinedTextFieldConfig {
        label,
        single_line: true,
        is_error: invalid_get(),
        ..Default::default()
    };
    config.placeholder = Some("YYYY-MM-DD".into());
    OutlinedTextField(
        modifier,
        text,
        move |v| {
            if v.trim().is_empty() {
                *invalid.borrow_mut() = false;
                return;
            }
            match parse_calendar_date(&v) {
                Some((y, m, d)) => {
                    *invalid.borrow_mut() = false;
                    state.set_date(y, m, d);
                }
                None => *invalid.borrow_mut() = true,
            }
        },
        config,
    )
}

pub fn DateRangeInput(
    state: Rc<DateRangePickerState>,
    modifier: Modifier,
    start_label: Option<String>,
    end_label: Option<String>,
) -> View {
    let fmt = |d: CalendarDate| format!("{:04}-{:02}-{:02}", d.0, d.1, d.2);
    let start_text = state.start.get().map(&fmt).unwrap_or_default();
    let end_text = state.end.get().map(&fmt).unwrap_or_default();
    let st_start = state.clone();
    let st_end = state.clone();
    Row(modifier.align_items(AlignItems::CENTER).gap(Dp(8.0))).child((
        OutlinedTextField(
            Modifier::new().flex_grow(1.0),
            start_text,
            move |v| {
                if let Some(date) = parse_calendar_date(&v) {
                    st_start.start.set(Some(date));
                    st_start.selecting_end.set(true);
                }
            },
            OutlinedTextFieldConfig {
                label: start_label,
                single_line: true,
                placeholder: Some("YYYY-MM-DD".into()),
                ..Default::default()
            },
        ),
        Text("–").size(theme().typography.title_medium),
        OutlinedTextField(
            Modifier::new().flex_grow(1.0),
            end_text,
            move |v| {
                if let Some(date) = parse_calendar_date(&v) {
                    st_end.end.set(Some(date));
                    st_end.selecting_end.set(false);
                }
            },
            OutlinedTextFieldConfig {
                label: end_label,
                single_line: true,
                placeholder: Some("YYYY-MM-DD".into()),
                ..Default::default()
            },
        ),
    ))
}
