use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::{
    BottomSheet, DatePicker, DatePickerState, DropdownMenu, DropdownMenuEntry, DropdownMenuItem,
    FilledButton, MenuState, ModalBottomSheet, PullToRefresh, PullToRefreshState, SearchBar,
    SearchBarState, SheetState, TextButton, TimePicker, TimePickerState,
};
use repose_ui::{anim::animate_f32, overlay::OverlayHandle, *};
use web_time::Duration;

use crate::ui::Section;

pub fn screen(overlay: OverlayHandle) -> View {
    // DropdownMenu state
    let menu_state = remember(|| MenuState::new());
    let menu_label = remember(|| signal("Choose…".to_string()));

    // SearchBar state
    let search_state = remember(|| SearchBarState::new());

    // BottomSheet state
    let sheet_state = remember(|| SheetState::new(200.0));
    let old_sheet_state = remember(|| signal(false));

    // PullToRefresh state
    let ptr_state = remember(|| PullToRefreshState::new());

    // DatePicker state
    let date_state = remember(|| DatePickerState::new(2026, 5, 29));
    let show_date_picker = remember(|| signal(false));
    let date_result = remember(|| signal("Not set".to_string()));

    // TimePicker state
    let time_state = remember(|| TimePickerState::new(14, 30));
    let show_time_picker = remember(|| signal(false));
    let time_result = remember(|| signal("Not set".to_string()));

    // Animation keyframes demo
    let anim_target = remember(|| signal(0.0f32));
    let anim_val = animate_f32(
        "m3_demo_anim",
        anim_target.get(),
        AnimationSpec::tween(Duration::from_millis(600), Easing::FastOutSlowIn),
    );

    let th = theme();

    // DropdownMenu items
    let menu_items: Vec<DropdownMenuEntry> = vec![
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 1", {
            let label = menu_label.clone();
            move || label.set("Item 1".to_string())
        })),
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 2", {
            let label = menu_label.clone();
            move || label.set("Item 2".to_string())
        })),
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 3 (disabled)", || {}).disabled()),
        DropdownMenuEntry::Divider,
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 4", {
            let label = menu_label.clone();
            move || label.set("Item 4".to_string())
        })),
    ];

    Column(Modifier::new().fill_max_width()).child((
        Section(
            "DropdownMenu",
            Column(Modifier::new().padding(12.0)).child((
                Text(format!("Selected: {}", menu_label.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Box(Modifier::new().height(8.0).width(1.0)),
                DropdownMenu(
                    menu_state.clone(),
                    overlay.clone(),
                    Modifier::new().fill_max_width(),
                    FilledButton(Modifier::new(), {
                        let s = menu_state.clone();
                        move || s.open()
                    }, || Text(menu_label.get().clone())
                        .size(th.typography.body_large)),
                    menu_items.clone(),
                ),
            )),
        ),
        Section(
            "Modal Bottom Sheet",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), {
                    let s = sheet_state.clone();
                    move || s.show()
                }, || Text("Open Bottom Sheet")),
                ModalBottomSheet(
                    sheet_state.clone(),
                    overlay.clone(),
                    Modifier::new(),
                    Column(Modifier::new().padding(24.0)).child((
                        Text("Sheet Content").color(th.on_surface).size(18.0),
                        Text("This is a modal bottom sheet with a drag handle.")
                            .color(th.on_surface_variant),
                        Box(Modifier::new().height(16.0).width(1.0)),
                        TextButton(Modifier::new(), {
                            let s = sheet_state.clone();
                            move || s.dismiss()
                        }, || Text("Dismiss")),
                    )),
                ),
            )),
        ),
        Section(
            "Simple Bottom Sheet (animated)",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), {
                    let s = old_sheet_state.clone();
                    move || s.set(!s.get())
                }, {
                    let s = old_sheet_state.clone();
                    move || Text(if s.get() { "Hide Sheet" } else { "Show Sheet" })
                }),
                BottomSheet(
                    old_sheet_state.get(),
                    {
                        let s = old_sheet_state.clone();
                        move || s.set(false)
                    },
                    Modifier::new()
                        .fill_max_width()
                        .background(th.surface_container_low)
                        .padding(24.0),
                    Text("This is a simple animated bottom sheet."),
                ),
            )),
        ),
        Section(
            "DatePicker + TimePicker",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), {
                    let s = show_date_picker.clone();
                    move || s.set(true)
                }, || Text("Pick Date")),
                Text(format!("Date: {}", date_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Box(Modifier::new().height(8.0).width(1.0)),
                FilledButton(Modifier::new(), {
                    let s = show_time_picker.clone();
                    move || s.set(true)
                }, || Text("Pick Time")),
                Text(format!("Time: {}", time_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                // DatePicker popup
                if show_date_picker.get() {
                    Stack(Modifier::new().fill_max_size()).child((
                        Box(Modifier::new()
                            .fill_max_size()
                            .background(th.scrim.with_alpha(170))
                            .clickable()
                            .on_pointer_down({
                                let s = show_date_picker.clone();
                                move |_| s.set(false)
                            })),
                        DatePicker(
                            date_state.clone(),
                            Rc::new({
                                let r = date_result.clone();
                                let s = show_date_picker.clone();
                                move |y, m, d| {
                                    r.set(format!("{}-{:02}-{:02}", y, m, d));
                                    s.set(false);
                                }
                            }),
                            Rc::new({
                                let s = show_date_picker.clone();
                                move || s.set(false)
                            }),
                        ),
                    ))
                } else {
                    Box(Modifier::new())
                },
                // TimePicker popup
                if show_time_picker.get() {
                    Stack(Modifier::new().fill_max_size()).child((
                        Box(Modifier::new()
                            .fill_max_size()
                            .background(th.scrim.with_alpha(170))
                            .clickable()
                            .on_pointer_down({
                                let s = show_time_picker.clone();
                                move |_| s.set(false)
                            })),
                        TimePicker(
                            time_state.clone(),
                            Rc::new({
                                let r = time_result.clone();
                                let s = show_time_picker.clone();
                                move |h, m| {
                                    r.set(format!("{:02}:{:02}", h, m));
                                    s.set(false);
                                }
                            }),
                            Rc::new({
                                let s = show_time_picker.clone();
                                move || s.set(false)
                            }),
                        ),
                    ))
                } else {
                    Box(Modifier::new())
                },
            )),
        ),
        Section(
            "Animation: Keyframes (via animate_f32)",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(Modifier::new(), {
                    let t = anim_target.clone();
                    move || t.set(rand_val())
                }, || Text("Animate to random")),
                Box(Modifier::new()
                    .size(anim_val * 100.0 + 20.0, 20.0)
                    .background(th.primary)
                    .clip_rounded(4.0)),
            )),
        ),
    ))
}

fn rand_val() -> f32 {
    // Simple deterministic-ish pseudo-random for demo
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    ((t % 1000) as f32) / 1000.0
}
