#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{
    Box, Column, Row, Spacer, Text, TextStyle,
    ViewExt,
};

use super::*;

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
