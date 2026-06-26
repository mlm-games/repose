use repose_core::{prelude::*, signal};
use repose_material::material3::{
    AssistChip, ButtonConfig, CardConfig, Checkbox, CheckboxConfig, ChipConfig, FilterChip,
    LinearProgressIndicator, RangeSlider, Slider, RadioButton, RadioButtonConfig, SliderConfig,
    Switch, SwitchConfig, TextButton,
};
use repose_material::{Icon, material_symbols};
use repose_ui::*;

use crate::ui::{Hint, Labeled, Page, Section, sp};

material_symbols! {
    add      : '\u{E145}',
    close    : '\u{E5CD}',
    favorite : '\u{E87D}',
    search   : '\u{E8B6}',
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
                LinearProgressIndicator(Some(prog.get()), Default::default()),
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
            "Spatial Focus (arrow keys)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                Hint("Navigate the 3×3 grid with arrow keys"),
                Column(Modifier::new().gap(6.0).align_items(AlignItems::Center)).child(
                    (0..3)
                        .map(|row| {
                            Row(Modifier::new()
                                .gap(6.0)
                                .justify_content(JustifyContent::Center))
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
    ])
}
