use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::dialog::{DatePickerDialog, DialogState, TimePickerDialog};
use repose_material::material3::{
    BottomSheet, BottomSheetConfig, Button, ButtonConfig, ButtonGroup, ButtonGroupScope,
    DatePickerDialogConfig, DatePickerState, DropdownMenu, DropdownMenuConfig, DropdownMenuEntry,
    DropdownMenuItem, MenuState, ModalBottomSheet, NavRailItem, NavigationRail,
    NavigationRailConfig, SegmentConfig, SegmentedButton, SegmentedButtonConfig, SheetState,
    SplitButtonConfig, SplitButtonLayout, SplitButtonLeadingButton, SplitButtonTonalLeadingButton,
    SplitButtonTonalTrailingToggleButton, SplitButtonTrailingButton,
    SplitButtonTrailingToggleButton, TextButton, TimePickerDialogConfig, TimePickerState,
};
use repose_material::{Icon, material_symbols};

material_symbols! {
    cloud          : '\u{F15C}',
    settings       : '\u{E8B8}',
    shopping_cart  : '\u{E8CC}',
    star           : '\u{F09A}',
    expand_more    : '\u{E5CF}',
}
use repose_ui::{
    anim::{animate_f32, animate_keyframes},
    overlay::OverlayHandle,
    *,
};
use web_time::Duration;

use crate::ui::{Page, Section, sp};

fn rail_item(
    label: &str,
    icon: View,
    sel: impl Fn() + 'static,
    badge: Option<View>,
) -> NavRailItem {
    NavRailItem {
        icon,
        label: label.into(),
        on_click: Rc::new(sel),
        badge,
        enabled: true,
        interaction_source: None,
    }
}

pub fn screen(overlay: OverlayHandle) -> View {
    // DropdownMenu state
    let menu_state = remember(MenuState::new);
    let menu_label = remember(|| signal("Choose…".to_string()));

    // BottomSheet state
    let sheet_state = remember(|| SheetState::new(200.0));
    let old_sheet_state = remember(|| signal(false));

    // Date / time picker state
    let date_state = remember(|| DatePickerState::new(2026, 5, 29));
    let date_dialog_state = remember(DialogState::new);
    let date_result = remember(|| signal("Not set".to_string()));
    let time_state = remember(|| TimePickerState::new(14, 30));
    let time_dialog_state = remember(DialogState::new);
    let time_result = remember(|| signal("Not set".to_string()));

    use repose_core::animation::KeyframesSpec;
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

    let rail_selected = remember(|| signal(0usize));
    let th = theme();

    let menu_items: Vec<DropdownMenuEntry> = vec![
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 1", {
            let l = menu_label.clone();
            move || l.set("Item 1".to_string())
        })),
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 2", {
            let l = menu_label.clone();
            move || l.set("Item 2".to_string())
        })),
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 3 (disabled)", || {}).disabled()),
        DropdownMenuEntry::Divider,
        DropdownMenuEntry::Item(DropdownMenuItem::new("Item 4", {
            let l = menu_label.clone();
            move || l.set("Item 4".to_string())
        })),
    ];

    Page(vec![
        Section(
            "DropdownMenu",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Text(format!("Selected: {}", menu_label.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                DropdownMenu(
                    menu_state.clone(),
                    overlay.clone(),
                    Modifier::new().fill_max_width(),
                    Button(
                        Modifier::new(),
                        {
                            let s = menu_state.clone();
                            move || s.open()
                        },
                        ButtonConfig::default(),
                        || Text(menu_label.get().clone()).size(th.typography.body_large),
                    ),
                    menu_items.clone(),
                    DropdownMenuConfig::default(),
                ),
            )),
        ),
        Section(
            "Modal Bottom Sheet",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Button(
                    Modifier::new(),
                    {
                        let s = sheet_state.clone();
                        move || s.show()
                    },
                    ButtonConfig::default(),
                    || Text("Open Bottom Sheet"),
                ),
                ModalBottomSheet(
                    sheet_state.clone(),
                    overlay.clone(),
                    Modifier::new(),
                    Column(Modifier::new().padding(sp::XL).gap(sp::SM)).child((
                        Text("Sheet Content").color(th.on_surface).size(18.0),
                        Text("This is a modal bottom sheet with a drag handle.")
                            .color(th.on_surface_variant),
                        TextButton(
                            Modifier::new(),
                            {
                                let s = sheet_state.clone();
                                move || s.dismiss()
                            },
                            ButtonConfig::default(),
                            || Text("Dismiss"),
                        ),
                    )),
                    BottomSheetConfig::default(),
                ),
            )),
        ),
        Section(
            "Simple Bottom Sheet (animated)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Button(
                    Modifier::new(),
                    {
                        let s = old_sheet_state.clone();
                        move || s.set(!s.get())
                    },
                    ButtonConfig::default(),
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
                        .padding(sp::XL),
                    Text("This is a simple animated bottom sheet."),
                    BottomSheetConfig::default(),
                ),
            )),
        ),
        Section(
            "DatePicker + TimePicker",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Button(
                    Modifier::new(),
                    {
                        let s = date_dialog_state.clone();
                        move || s.show()
                    },
                    ButtonConfig::default(),
                    || Text("Pick Date"),
                ),
                Text(format!("Date: {}", date_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Button(
                    Modifier::new(),
                    {
                        let s = time_dialog_state.clone();
                        move || s.show()
                    },
                    ButtonConfig::default(),
                    || Text("Pick Time"),
                ),
                Text(format!("Time: {}", time_result.get()))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                DatePickerDialog(
                    date_dialog_state.clone(),
                    overlay.clone(),
                    date_state.clone(),
                    Rc::new({
                        let r = date_result.clone();
                        let s = date_dialog_state.clone();
                        move |y, m, d| {
                            r.set(format!("{}-{:02}-{:02}", y, m, d));
                            s.dismiss();
                        }
                    }),
                    Rc::new({
                        let s = date_dialog_state.clone();
                        move || s.dismiss()
                    }),
                    DatePickerDialogConfig::default(),
                ),
                TimePickerDialog(
                    time_dialog_state.clone(),
                    overlay.clone(),
                    time_state.clone(),
                    Rc::new({
                        let r = time_result.clone();
                        let s = time_dialog_state.clone();
                        move |h, m| {
                            r.set(format!("{:02}:{:02}", h, m));
                            s.dismiss();
                        }
                    }),
                    Rc::new({
                        let s = time_dialog_state.clone();
                        move || s.dismiss()
                    }),
                    TimePickerDialogConfig::default(),
                ),
            )),
        ),
        Section(
            "NavigationRail",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Text(format!("Selected: Item {}", rail_selected.get() + 1))
                    .color(th.on_surface)
                    .size(th.typography.body_medium),
                Row(Modifier::new().height(400.0).border(1.0, th.outline, 8.0)).child(
                    NavigationRail(
                        rail_selected.get(),
                        vec![
                            rail_item(
                                "Favorites",
                                Icon(Symbols::star).size(20.0),
                                {
                                    let s = rail_selected.clone();
                                    move || s.set(0)
                                },
                                None,
                            ),
                            rail_item(
                                "Cloud",
                                Icon(Symbols::cloud).size(20.0),
                                {
                                    let s = rail_selected.clone();
                                    move || s.set(1)
                                },
                                None,
                            ),
                            rail_item(
                                "Settings",
                                Icon(Symbols::settings).size(20.0),
                                {
                                    let s = rail_selected.clone();
                                    move || s.set(2)
                                },
                                Some(Box(Modifier::new()
                                    .size(8.0, 8.0)
                                    .background(th.error)
                                    .clip_rounded(4.0))),
                            ),
                            rail_item(
                                "Cart",
                                Icon(Symbols::shopping_cart).size(20.0),
                                {
                                    let s = rail_selected.clone();
                                    move || s.set(3)
                                },
                                Some(
                                    Box(Modifier::new()
                                        .min_width(16.0)
                                        .height(16.0)
                                        .background(th.error)
                                        .clip_rounded(8.0)
                                        .align_items(AlignItems::Center)
                                        .justify_content(JustifyContent::Center))
                                    .child(Text("3").color(th.on_error).size(10.0).single_line()),
                                ),
                            ),
                        ],
                        None,
                        None,
                        NavigationRailConfig::default(),
                    ),
                ),
            )),
        ),
        Section(
            "SegmentedButton",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child({
                let seg_sel = remember(|| signal(0usize));
                SegmentedButton(
                    &[seg_sel.get()],
                    vec![
                        SegmentConfig {
                            label: "Day".into(),
                            icon: None,
                            on_click: Rc::new({
                                let s = seg_sel.clone();
                                move || s.set(0)
                            }),
                            enabled: true,
                            ..Default::default()
                        },
                        SegmentConfig {
                            label: "Week".into(),
                            icon: None,
                            on_click: Rc::new({
                                let s = seg_sel.clone();
                                move || s.set(1)
                            }),
                            enabled: true,
                            ..Default::default()
                        },
                        SegmentConfig {
                            label: "Month".into(),
                            icon: None,
                            on_click: Rc::new({
                                let s = seg_sel.clone();
                                move || s.set(2)
                            }),
                            enabled: true,
                            ..Default::default()
                        },
                    ],
                    SegmentedButtonConfig::default(),
                )
            }),
        ),
        Section(
            "SplitButtonLayout",
            Column(Modifier::new().padding(sp::MD).gap(sp::LG)).child((
                Column(Modifier::new().gap(sp::SM)).child((
                    Text("Filled SplitButton (with toggleable trailing)")
                        .size(14.0)
                        .color(th.on_surface_variant),
                    {
                        let checked = remember(|| signal(false));
                        SplitButtonLayout(
                            SplitButtonLeadingButton(
                                Modifier::new(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Action"),
                            ),
                            SplitButtonTrailingToggleButton(
                                checked.get(),
                                {
                                    let c = checked.clone();
                                    move |v| c.set(v)
                                },
                                Modifier::new(),
                                Default::default(),
                                move |chk| {
                                    Icon(Symbols::settings).size(if chk { 22.0 } else { 18.0 })
                                },
                            ),
                            SplitButtonConfig::default(),
                        )
                    },
                )),
                Column(Modifier::new().gap(sp::SM)).child((
                    Text("Tonal SplitButton")
                        .size(14.0)
                        .color(th.on_surface_variant),
                    {
                        let checked = remember(|| signal(false));
                        SplitButtonLayout(
                            SplitButtonTonalLeadingButton(
                                Modifier::new(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Filter"),
                            ),
                            SplitButtonTonalTrailingToggleButton(
                                checked.get(),
                                {
                                    let c = checked.clone();
                                    move |v| c.set(v)
                                },
                                Modifier::new(),
                                Default::default(),
                                move |chk| {
                                    Icon(Symbols::settings).size(if chk { 22.0 } else { 18.0 })
                                },
                            ),
                            SplitButtonConfig::default(),
                        )
                    },
                )),
                Column(Modifier::new().gap(sp::SM)).child((
                    Text("SplitButton with DropdownMenu")
                        .size(14.0)
                        .color(th.on_surface_variant),
                    {
                        let split_menu = remember(MenuState::new);
                        let split_menu_items: Vec<DropdownMenuEntry> = vec![
                            DropdownMenuEntry::Item(DropdownMenuItem::new("Save as PDF", {
                                let m = split_menu.clone();
                                move || m.dismiss()
                            })),
                            DropdownMenuEntry::Item(DropdownMenuItem::new("Save as DOCX", {
                                let m = split_menu.clone();
                                move || m.dismiss()
                            })),
                            DropdownMenuEntry::Divider,
                            DropdownMenuEntry::Item(DropdownMenuItem::new("Export as CSV", {
                                let m = split_menu.clone();
                                move || m.dismiss()
                            })),
                        ];
                        SplitButtonLayout(
                            SplitButtonLeadingButton(
                                Modifier::new(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Save"),
                            ),
                            DropdownMenu(
                                split_menu.clone(),
                                overlay.clone(),
                                Modifier::new(),
                                SplitButtonTrailingButton(
                                    Modifier::new(),
                                    move || split_menu.open(),
                                    ButtonConfig::default(),
                                    || Icon(Symbols::expand_more).size(18.0),
                                ),
                                split_menu_items.clone(),
                                DropdownMenuConfig::default(),
                            ),
                            SplitButtonConfig::default(),
                        )
                    },
                )),
            )),
        ),
        Section(
            "ButtonGroup",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child({
                let sel = remember(|| signal(0usize));
                ButtonGroup(
                    Modifier::new().fill_max_width(),
                    8.0,
                    move |scope: &mut ButtonGroupScope| {
                        let s = sel.clone();
                        scope.clickable_item(move || s.set(0), "First".into(), None);
                        let s = sel.clone();
                        scope.clickable_item(move || s.set(1), "Second".into(), None);
                        let s = sel.clone();
                        scope.clickable_item(move || s.set(2), "Third".into(), None);
                    },
                )
            }),
        ),
        Section(
            "Animation: Keyframes (animate_keyframes)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
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
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Button(
                    Modifier::new(),
                    {
                        let t = anim_target.clone();
                        move || t.set(rand_val())
                    },
                    ButtonConfig::default(),
                    || Text("Animate to random"),
                ),
                Box(Modifier::new()
                    .size(anim_val * 100.0 + 20.0, 20.0)
                    .background(th.tertiary)
                    .clip_rounded(4.0)),
            )),
        ),
    ])
}

fn rand_val() -> f32 {
    use web_time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    ((t % 1000) as f32) / 1000.0
}
