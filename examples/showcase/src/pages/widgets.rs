use std::rc::Rc;

use repose_core::{prelude::*, signal};
use repose_material::material3::{
    AssistChip, Badge, BadgeConfig, BadgedBox, BadgedBoxConfig, ButtonConfig, Checkbox,
    CheckboxConfig, ChipConfig, CircularProgressIndicator, DividerConfig, FilterChip,
    HorizontalDivider, LinearProgressIndicator, LinearProgressIndicatorConfig, RadioButton,
    RadioButtonConfig, RangeSlider, SearchBar, SearchBarConfig, SearchBarInputField,
    SearchBarInputFieldConfig, SearchBarState, Slider, SliderConfig, Switch, SwitchConfig, Tab,
    TabRow, TabRowConfig, TextButton, TooltipBox, TooltipConfig, TooltipState, VerticalDivider,
};
use repose_material::{Icon, material_symbols};
use repose_ui::*;

use crate::ui::{Hint, Labeled, Page, Section, sp};

material_symbols! {
    add      : '\u{E145}',
    close    : '\u{E5CD}',
    favorite : '\u{E87D}',
    search   : '\u{E8B6}',
    home     : '\u{E88A}',
    settings : '\u{E8B8}',
    info     : '\u{E88E}',
    star     : '\u{F09A}',
}

fn focus_cell(idx: i32) -> View {
    Box(Modifier::new()
        .clickable()
        .padding(sp::LG)
        .background(theme().surface)
        .border(1.0, theme().outline, 8.0)
        .clip_rounded(8.0)
        .on_pointer_down(move |_| log::info!("Clicked item {idx}")))
    .child(Text(format!("{idx}")).size(18.0).color(theme().on_surface))
}

pub fn screen() -> View {
    let cb = remember(|| signal(true));
    let sw = remember(|| signal(false));
    let radio = remember(|| signal(0u8));
    let s_val = remember(|| signal(0.35f32));
    let r_a = remember(|| signal(0.2f32));
    let r_b = remember(|| signal(0.8f32));
    let prog = remember(|| signal(0.4f32));
    let filter_selected = remember(|| signal(false));
    let tab_index = remember(|| signal(0usize));
    let search_state = remember(SearchBarState::new);
    let tooltip_state = remember(TooltipState::new);
    let tooltip_state_inner = tooltip_state.clone();

    let th = theme();

    Page(vec![
        Section(
            "Switch / Checkbox / Radio",
            Column(Modifier::new().padding(sp::MD).gap(10.0)).child((
                Labeled(
                    Switch(
                        sw.get(),
                        {
                            let s = sw.clone();
                            move |v| s.set(v)
                        },
                        SwitchConfig::default(),
                    ),
                    "Switch",
                ),
                Labeled(
                    Checkbox(
                        cb.get(),
                        {
                            let s = cb.clone();
                            move |v| s.set(v)
                        },
                        CheckboxConfig::default(),
                    ),
                    "Checkbox",
                ),
                Labeled(
                    RadioButton(
                        radio.get() == 0,
                        {
                            let r = radio.clone();
                            move || r.set(0)
                        },
                        RadioButtonConfig::default(),
                    ),
                    "Radio A",
                ),
                Labeled(
                    RadioButton(
                        radio.get() == 1,
                        {
                            let r = radio.clone();
                            move || r.set(1)
                        },
                        RadioButtonConfig::default(),
                    ),
                    "Radio B",
                ),
            )),
        ),
        Section(
            "Sliders + Progress",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Slider(
                    s_val.get(),
                    (0.0, 1.0),
                    Some(0.01),
                    {
                        let s = s_val.clone();
                        move |v| s.set(v)
                    },
                    SliderConfig::default(),
                ),
                RangeSlider(
                    r_a.get(),
                    r_b.get(),
                    (0.0, 1.0),
                    Some(0.01),
                    {
                        let a = r_a.clone();
                        let b = r_b.clone();
                        move |x0, x1| {
                            a.set(x0);
                            b.set(x1);
                        }
                    },
                    SliderConfig::default(),
                ),
                Row(Modifier::new().fill_max_width().gap(sp::MD)).child((
                    Column(Modifier::new().gap(sp::SM).flex_grow(1.0)).child((
                        Text(format!("Linear: {:.0}%", prog.get() * 100.0))
                            .size(13.0)
                            .color(th.on_surface_variant),
                        LinearProgressIndicator(
                            Some(prog.get()),
                            LinearProgressIndicatorConfig::default(),
                        ),
                    )),
                    Column(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                        Text("Circular").size(13.0).color(th.on_surface_variant),
                        CircularProgressIndicator(Some(prog.get()), Default::default()),
                    )),
                    Column(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                        Text("Indeterminate")
                            .size(13.0)
                            .color(th.on_surface_variant),
                        CircularProgressIndicator(None, Default::default()),
                    )),
                )),
                Row(Modifier::new().gap(sp::MD)).child((
                    TextButton(
                        Modifier::new(),
                        {
                            let p = prog.clone();
                            move || p.update(|x| *x = (*x - 0.05).max(0.0))
                        },
                        ButtonConfig::default(),
                        || Text("Decrease"),
                    ),
                    TextButton(
                        Modifier::new(),
                        {
                            let p = prog.clone();
                            move || p.update(|x| *x = (*x + 0.05).min(1.0))
                        },
                        ButtonConfig::default(),
                        || Text("Increase"),
                    ),
                )),
            )),
        ),
        Section(
            "Search Bar",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                SearchBar(
                    search_state.clone(),
                    SearchBarInputField(
                        "Search...".to_string(),
                        search_state.query(),
                        Rc::new({
                            let s = search_state.clone();
                            move |q| s.set_query(q)
                        }),
                        search_state.is_expanded(),
                        SearchBarInputFieldConfig {
                            on_search: Some(Rc::new({
                                let s = search_state.clone();
                                move |query| {
                                    log::info!("Search submitted: {query}");
                                    s.deactivate();
                                }
                            })),
                            ..Default::default()
                        },
                    ),
                    Modifier::new(),
                    Some(Icon(Symbols::search).size(20.0)),
                    None,
                    SearchBarConfig::default(),
                ),
                Text(format!("Query: \"{}\"", search_state.query()))
                    .size(13.0)
                    .color(th.on_surface_variant),
            )),
        ),
        Section(
            "Tab Row",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                TabRow(
                    tab_index.get(),
                    vec![
                        Tab {
                            label: "Tab A".into(),
                            icon: Some(Icon(Symbols::home).size(18.0)),
                            on_click: Rc::new({
                                let t = tab_index.clone();
                                move || t.set(0)
                            }),
                            enabled: true,
                            interaction_source: None,
                        },
                        Tab {
                            label: "Tab B".into(),
                            icon: None,
                            on_click: Rc::new({
                                let t = tab_index.clone();
                                move || t.set(1)
                            }),
                            enabled: true,
                            interaction_source: None,
                        },
                        Tab {
                            label: "Tab C".into(),
                            icon: Some(Icon(Symbols::settings).size(18.0)),
                            on_click: Rc::new({
                                let t = tab_index.clone();
                                move || t.set(2)
                            }),
                            enabled: true,
                            interaction_source: None,
                        },
                    ],
                    TabRowConfig::default(),
                ),
                Text(format!("Selected tab: {}", tab_index.get()))
                    .size(14.0)
                    .color(th.on_surface),
            )),
        ),
        Section(
            "Badge / BadgedBox",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Row(Modifier::new().gap(sp::XL).align_items(AlignItems::CENTER)).child((
                    BadgedBox(
                        Badge(None, BadgeConfig::default()),
                        Icon(Symbols::info).size(24.0).color(th.on_surface),
                        BadgedBoxConfig {
                            has_content: false,
                            ..Default::default()
                        },
                    ),
                    BadgedBox(
                        Badge(
                            Some(Text("3").size(10.0).color(th.on_error)),
                            BadgeConfig::default(),
                        ),
                        Icon(Symbols::settings).size(24.0).color(th.on_surface),
                        BadgedBoxConfig {
                            has_content: true,
                            ..Default::default()
                        },
                    ),
                    BadgedBox(
                        Badge(
                            Some(Text("99+").size(9.0).color(th.on_error)),
                            BadgeConfig::default(),
                        ),
                        Icon(Symbols::favorite).size(24.0).color(th.error),
                        BadgedBoxConfig {
                            has_content: true,
                            ..Default::default()
                        },
                    ),
                )),
                Hint("Badges appear at the top-right of the wrapped content."),
            )),
        ),
        Section(
            "Tooltip",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                TooltipBox(
                    "This is a tooltip with the M3 rich style.",
                    tooltip_state_inner.clone(),
                    Modifier::new(),
                    Box(Modifier::new()
                        .padding(sp::MD)
                        .background(th.surface_container)
                        .border(1.0, th.outline_variant, 8.0)
                        .clip_rounded(8.0))
                    .child(
                        Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
                            Icon(Symbols::info).size(18.0).color(th.primary),
                            Text("Hover me").size(14.0).color(th.on_surface),
                        )),
                    ),
                    TooltipConfig::default(),
                ),
                Hint("Tooltips appear above the element on hover."),
            )),
        ),
        Section(
            "Spatial Focus (arrow keys)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Navigate the 3×3 grid with arrow keys"),
                Column(Modifier::new().gap(6.0).align_items(AlignItems::CENTER)).child(
                    (0..3)
                        .map(|row| {
                            Row(Modifier::new()
                                .gap(6.0)
                                .justify_content(JustifyContent::CENTER))
                            .child(
                                (0..3)
                                    .map(|col| focus_cell(row * 3 + col))
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
                Hint("Focus callback (check console):"),
                Row(Modifier::new().gap(6.0)).child(
                    (0..3)
                        .map(|i| {
                            let lbl = format!("btn{i}");
                            let lbl2 = lbl.clone();
                            Box(Modifier::new()
                                .clickable()
                                .padding(sp::MD)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 8.0)
                                .clip_rounded(8.0)
                                .on_focus_changed(move |focused| {
                                    log::warn!("{lbl} focus: {focused}")
                                }))
                            .child(Text(lbl2).size(14.0).color(theme().on_surface))
                        })
                        .collect::<Vec<_>>(),
                ),
            )),
        ),
        Section(
            "Chips",
            Column(Modifier::new().padding(sp::MD).gap(sp::LG)).child((
                Column(Modifier::new().gap(sp::SM)).child((
                    Hint("AssistChip"),
                    Row(Modifier::new().gap(sp::SM)).child((
                        AssistChip(|| {}, Text("Basic"), None, None, ChipConfig::default()),
                        AssistChip(
                            || {},
                            Text("Leading"),
                            Some(Icon(Symbols::add).size(18.0)),
                            None,
                            ChipConfig::default(),
                        ),
                        AssistChip(
                            || {},
                            Text("Both"),
                            Some(Icon(Symbols::search).size(18.0)),
                            Some(Icon(Symbols::close).size(18.0)),
                            ChipConfig::default(),
                        ),
                    )),
                )),
                Column(Modifier::new().gap(sp::SM)).child((
                    Hint("FilterChip (toggle)"),
                    Row(Modifier::new().gap(sp::SM)).child((
                        FilterChip(
                            filter_selected.get(),
                            {
                                let fs = filter_selected.clone();
                                move || fs.update(|x| *x = !*x)
                            },
                            Text("Toggle"),
                            None,
                            None,
                            ChipConfig::default(),
                        ),
                        FilterChip(
                            true,
                            || {},
                            Text("Active"),
                            None,
                            None,
                            ChipConfig::default(),
                        ),
                        FilterChip(
                            true,
                            || {},
                            Text("Trailing"),
                            Some(Icon(Symbols::favorite).size(18.0)),
                            Some(Icon(Symbols::close).size(18.0)),
                            ChipConfig::default(),
                        ),
                    )),
                )),
            )),
        ),
        Section(
            "Dividers",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                HorizontalDivider(DividerConfig::default()),
                Row(Modifier::new()
                    .height(40.0)
                    .gap(sp::MD)
                    .align_items(AlignItems::CENTER))
                .child((
                    Text("Left").size(14.0).color(th.on_surface),
                    VerticalDivider(DividerConfig::default()),
                    Text("Right").size(14.0).color(th.on_surface),
                )),
            )),
        ),
    ])
}
