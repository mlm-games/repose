use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::dialog::{Dialog, DialogState};
use repose_material::material3::{
    BottomSheet, DatePicker, DatePickerState, DropdownMenu, DropdownMenuEntry, DropdownMenuItem,
    FilledButton, MenuState, ModalBottomSheet, NavRailItem, NavigationRail, SheetState, TextButton,
    TimePicker, TimePickerState,
};
use repose_ui::{
    anim::{animate_f32, animate_keyframes},
    overlay::OverlayHandle,
    *,
};
use web_time::Duration;

use crate::ui::Section;

pub fn screen(overlay: OverlayHandle) -> View {
    // DropdownMenu state
    let menu_state = remember(MenuState::new);
    let menu_label = remember(|| signal("Choose…".to_string()));

    // BottomSheet state
    let sheet_state = remember(|| SheetState::new(200.0));
    let old_sheet_state = remember(|| signal(false));

    // DatePicker state
    let date_state = remember(|| DatePickerState::new(2026, 5, 29));
    let show_date_picker = remember(|| signal(false));
    let date_result = remember(|| signal("Not set".to_string()));

    // TimePicker state
    let time_state = remember(|| TimePickerState::new(14, 30));
    let show_time_picker = remember(|| signal(false));
    let time_result = remember(|| signal("Not set".to_string()));

    use repose_core::animation::KeyframesSpec;
    // Animation keyframes demo
    let kf_val = animate_keyframes(
        "m3_kf",
        KeyframesSpec::new(vec![
            (0.0, 20.0),
            (0.25, 120.0),
            (0.5, 60.0),
            (0.75, 140.0),
            (1.0, 100.0),
        ]),
        AnimationSpec::tween(Duration::from_millis(800), Easing::EaseOut)
            .repeated(RepeatableSpec::infinite()),
    );
    let anim_target = remember(|| signal(0.0f32));
    let anim_val = animate_f32(
        "m3_demo_anim",
        anim_target.get(),
        AnimationSpec::tween(Duration::from_millis(600), Easing::FastOutSlowIn),
    );

    // NavigationRail demo
    let rail_selected = remember(|| signal(0usize));

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
                    FilledButton(
                        Modifier::new(),
                        {
                            let s = menu_state.clone();
                            move || s.open()
                        },
                        || Text(menu_label.get().clone()).size(th.typography.body_large),
                    ),
                    menu_items.clone(),
                ),
            )),
        ),
        Section(
            "Modal Bottom Sheet",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(
                    Modifier::new(),
                    {
                        let s = sheet_state.clone();
                        move || s.show()
                    },
                    || Text("Open Bottom Sheet"),
                ),
                ModalBottomSheet(
                    sheet_state.clone(),
                    overlay.clone(),
                    Modifier::new(),
                    Column(Modifier::new().padding(24.0)).child((
                        Text("Sheet Content").color(th.on_surface).size(18.0),
                        Text("This is a modal bottom sheet with a drag handle.")
                            .color(th.on_surface_variant),
                        Box(Modifier::new().height(16.0).width(1.0)),
                        TextButton(
                            Modifier::new(),
                            {
                                let s = sheet_state.clone();
                                move || s.dismiss()
                            },
                            || Text("Dismiss"),
                        ),
                    )),
                ),
            )),
        ),
        Section(
            "Simple Bottom Sheet (animated)",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(
                    Modifier::new(),
                    {
                        let s = old_sheet_state.clone();
                        move || s.set(!s.get())
                    },
                    {
                        let s = old_sheet_state.clone();
                        move || Text(if s.get() { "Hide Sheet" } else { "Show Sheet" })
                    },
                ),
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
                FilledButton(
                    Modifier::new(),
                    {
                        let s = show_date_picker.clone();
                        move || s.set(true)
                    },
                    || Text("Pick Date"),
                ),
                Text(format!("Date: {}", date_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Box(Modifier::new().height(8.0).width(1.0)),
                FilledButton(
                    Modifier::new(),
                    {
                        let s = show_time_picker.clone();
                        move || s.set(true)
                    },
                    || Text("Pick Time"),
                ),
                Text(format!("Time: {}", time_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                // DatePicker dialog (overlay-based, never clipped by parents)
                {
                    let dp_state = remember(DialogState::new);
                    if show_date_picker.get() {
                        dp_state.show();
                        show_date_picker.set(false);
                    }
                    Dialog(
                        dp_state.clone(),
                        overlay.clone(),
                        Modifier::new(),
                        DatePicker(
                            date_state.clone(),
                            Rc::new({
                                let r = date_result.clone();
                                let dp_state = dp_state.clone();
                                move |y, m, d| {
                                    r.set(format!("{}-{:02}-{:02}", y, m, d));
                                    dp_state.dismiss();
                                }
                            }),
                            Rc::new({
                                let dp_state = dp_state.clone();
                                move || dp_state.dismiss()
                            }),
                        ),
                    )
                },
                Box(Modifier::new().height(8.0).width(1.0)),
                // TimePicker dialog (overlay-based)
                {
                    let tp_state = remember(DialogState::new);
                    if show_time_picker.get() {
                        tp_state.show();
                        show_time_picker.set(false);
                    }
                    Dialog(
                        tp_state.clone(),
                        overlay.clone(),
                        Modifier::new(),
                        TimePicker(
                            time_state.clone(),
                            Rc::new({
                                let r = time_result.clone();
                                let tp_state = tp_state.clone();
                                move |h, m| {
                                    r.set(format!("{:02}:{:02}", h, m));
                                    tp_state.dismiss();
                                }
                            }),
                            Rc::new({
                                let tp_state = tp_state.clone();
                                move || tp_state.dismiss()
                            }),
                        ),
                    )
                },
            )),
        ),
        Section(
            "NavigationRail",
            Column(Modifier::new().padding(12.0)).child((
                Text(format!("Selected: Item {}", rail_selected.get() + 1))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Box(Modifier::new().height(8.0).width(1.0)),
                Row(Modifier::new().height(400.0).border(1.0, th.outline, 8.0)).child({
                    NavigationRail(
                        rail_selected.get(),
                        vec![
                            NavRailItem {
                                icon: Text("★").size(20.0),
                                label: "Favorites".into(),
                                on_click: Rc::new({
                                    let s = rail_selected.clone();
                                    move || s.set(0)
                                }),
                                badge: None,
                            },
                            NavRailItem {
                                icon: Text("☁").size(20.0),
                                label: "Cloud".into(),
                                on_click: Rc::new({
                                    let s = rail_selected.clone();
                                    move || s.set(1)
                                }),
                                badge: None,
                            },
                            NavRailItem {
                                icon: Text("⚙").size(20.0),
                                label: "Settings".into(),
                                on_click: Rc::new({
                                    let s = rail_selected.clone();
                                    move || s.set(2)
                                }),
                                badge: Some(Box(Modifier::new()
                                    .size(8.0, 8.0)
                                    .background(th.error)
                                    .clip_rounded(4.0))),
                            },
                            NavRailItem {
                                icon: Text("🛒").size(20.0),
                                label: "Cart".into(),
                                on_click: Rc::new({
                                    let s = rail_selected.clone();
                                    move || s.set(3)
                                }),
                                badge: Some(
                                    Box(Modifier::new()
                                        .min_width(16.0)
                                        .height(16.0)
                                        .background(th.error)
                                        .clip_rounded(8.0)
                                        .align_items(AlignItems::Center)
                                        .justify_content(JustifyContent::Center))
                                    .child(Text("3").color(th.on_error).size(10.0).single_line()),
                                ),
                            },
                        ],
                        None, // header
                        None, // FAB
                    )
                }),
            )),
        ),
        Section(
            "Animation: Keyframes (animate_keyframes)",
            Column(Modifier::new().padding(12.0)).child((
                Text("Keyframe animation bouncing between sizes")
                    .size(14.0)
                    .color(th.on_surface_variant),
                Box(Modifier::new()
                    .size(kf_val, 24.0)
                    .background(th.primary)
                    .clip_rounded(4.0)),
            )),
        ),
        Section(
            "Animation: Tween (animate_f32)",
            Column(Modifier::new().padding(12.0)).child((
                FilledButton(
                    Modifier::new(),
                    {
                        let t = anim_target.clone();
                        move || t.set(rand_val())
                    },
                    || Text("Animate to random"),
                ),
                Box(Modifier::new()
                    .size(anim_val * 100.0 + 20.0, 20.0)
                    .background(th.tertiary)
                    .clip_rounded(4.0)),
            )),
        ),
    ))
}

fn rand_val() -> f32 {
    // Simple deterministic-ish pseudo-random for demo
    use web_time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    ((t % 1000) as f32) / 1000.0
}
