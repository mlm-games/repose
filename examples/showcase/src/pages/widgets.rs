use std::rc::Rc;

use repose_core::{prelude::*, signal};
use repose_material::material3::{
    AssistChip, FilterChip, InputChip, M3RangeSlider, M3Slider, SuggestionChip, TextButton,
};
use repose_material::material3::{Checkbox, RadioButton, Switch};
use repose_material::{Icon, material_symbols};
use repose_ui::*;

use crate::ui::Section;

material_symbols! {
    add      : '\u{E145}',
    close    : '\u{E5CD}',
    favorite : '\u{E87D}',
    search   : '\u{E8B6}',
    send     : '\u{E163}',
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
    let input_selected = remember(|| signal(false));

    Column(Modifier::new().fill_max_width()).child((
        Section(
            "Switch / Checkbox / Radio",
            Column(Modifier::new().padding(12.0)).child((
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    Switch(sw.get(), {
                        let sw = sw.clone();
                        move |v| sw.set(v)
                    }),
                    Box(Modifier::new().width(10.0).height(1.0)),
                    Text("Switch"),
                )),
                Box(Modifier::new().height(10.0).width(1.0)),
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    Checkbox(cb.get(), {
                        let cb = cb.clone();
                        move |v| cb.set(v)
                    }),
                    Box(Modifier::new().width(10.0).height(1.0)),
                    Text("Checkbox"),
                )),
                Box(Modifier::new().height(10.0).width(1.0)),
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    RadioButton(radio.get() == 0, {
                        let r = radio.clone();
                        move || r.set(0)
                    }),
                    Box(Modifier::new().width(10.0).height(1.0)),
                    Text("Radio A"),
                )),
                Row(Modifier::new().align_items(AlignItems::Center)).child((
                    RadioButton(radio.get() == 1, {
                        let r = radio.clone();
                        move || r.set(1)
                    }),
                    Box(Modifier::new().width(10.0).height(1.0)),
                    Text("Radio B"),
                )),
            )),
        ),
        Section(
            "Sliders + Progress",
            Column(Modifier::new().padding(12.0)).child((
                M3Slider(s_val.get(), (0.0, 1.0), Some(0.01), {
                    let s = s_val.clone();
                    move |v| s.set(v)
                }),
                Box(Modifier::new().height(12.0).width(1.0)),
                M3RangeSlider(r_a.get(), r_b.get(), (0.0, 1.0), Some(0.01), {
                    let a = r_a.clone();
                    let b = r_b.clone();
                    move |x0, x1| {
                        a.set(x0);
                        b.set(x1);
                    }
                }),
                Box(Modifier::new().height(12.0).width(1.0)),
                ProgressBar(prog.get(), (0.0, 1.0)),
                Box(Modifier::new().height(12.0).width(1.0)),
                Row(Modifier::new()).child((
                    TextButton(
                        Modifier::new(),
                        {
                            let p = prog.clone();
                            move || p.update(|x| *x = (*x - 0.05).max(0.0))
                        },
                        || Text("Decrease"),
                    ),
                    Box(Modifier::new().width(12.0).height(1.0)),
                    TextButton(
                        Modifier::new(),
                        {
                            let p = prog.clone();
                            move || p.update(|x| *x = (*x + 0.05).min(1.0))
                        },
                        || Text("Increase"),
                    ),
                )),
            )),
        ),
        Section(
            "Spatial Focus (arrow keys)",
            Column(Modifier::new().padding(12.0)).child((
                Text("Navigate the 3×3 grid with arrow keys")
                    .size(14.0)
                    .color(theme().on_surface_variant),
                Box(Modifier::new().height(8.0).width(1.0)),
                Column(Modifier::new().gap(6.0).align_items(AlignItems::Center)).child(
                    (0..3)
                        .map(|row| {
                            Row(Modifier::new()
                                .gap(6.0)
                                .justify_content(JustifyContent::Center))
                            .child(
                                (0..3)
                                    .map(|col| {
                                        let idx = row * 3 + col;
                                        let item = Box(Modifier::new()
                                            .clickable()
                                            .padding(16.0)
                                            .background(theme().surface)
                                            .border(1.0, theme().outline, 8.0)
                                            .clip_rounded(8.0)
                                            .on_pointer_down(move |_| {
                                                log::info!("Clicked item {idx}");
                                            }))
                                        .child(
                                            Text(format!("{idx}"))
                                                .size(18.0)
                                                .color(theme().on_surface),
                                        );
                                        item
                                    })
                                    .collect::<Vec<_>>(),
                            )
                        })
                        .collect::<Vec<_>>(),
                ),
                Box(Modifier::new().height(8.0).width(1.0)),
                Text("Focus callback (check console):")
                    .size(14.0)
                    .color(theme().on_surface_variant),
                Box(Modifier::new().height(4.0).width(1.0)),
                Row(Modifier::new().gap(6.0)).child(
                    (0..3)
                        .map(|i| {
                            let lbl = format!("btn{i}");
                            let lbl2 = lbl.clone();
                            Box(Modifier::new()
                                .clickable()
                                .padding(12.0)
                                .background(theme().surface)
                                .border(1.0, theme().outline, 8.0)
                                .clip_rounded(8.0)
                                .on_focus_changed(move |focused| {
                                    log::warn!("{lbl} focus: {focused}");
                                }))
                            .child(Text(lbl2).size(14.0).color(theme().on_surface))
                        })
                        .collect::<Vec<_>>(),
                ),
            )),
        ),
        Section(
            "Chips",
            Column(Modifier::new().padding(12.0)).child((
                Column(Modifier::new()).child((
                    Text("AssistChip")
                        .size(14.0)
                        .color(theme().on_surface_variant),
                    Box(Modifier::new().height(8.0).width(1.0)),
                    Row(Modifier::new().gap(8.0)).child((
                        AssistChip(|| {}, Text("Basic"), None, None),
                        AssistChip(
                            || {},
                            Text("Leading"),
                            Some(Icon(Symbols::add).size(18.0)),
                            None,
                        ),
                        AssistChip(
                            || {},
                            Text("Both"),
                            Some(Icon(Symbols::search).size(18.0)),
                            Some(Icon(Symbols::close).size(18.0)),
                        ),
                    )),
                )),
                Box(Modifier::new().height(16.0).width(1.0)),
                Column(Modifier::new()).child((
                    Text("FilterChip (toggle)")
                        .size(14.0)
                        .color(theme().on_surface_variant),
                    Box(Modifier::new().height(8.0).width(1.0)),
                    Row(Modifier::new().gap(8.0)).child((
                        FilterChip(
                            filter_selected.get(),
                            {
                                let fs = filter_selected.clone();
                                move || fs.update(|x| *x = !*x)
                            },
                            Text("Toggle"),
                            None,
                            None,
                        ),
                        FilterChip(true, || {}, Text("Active"), None, None),
                        FilterChip(
                            true,
                            || {},
                            Text("Trailing"),
                            Some(Icon(Symbols::favorite).size(18.0)),
                            Some(Icon(Symbols::close).size(18.0)),
                        ),
                    )),
                )),
                Box(Modifier::new().height(16.0).width(1.0)),
            )),
        ),
    ))
}
