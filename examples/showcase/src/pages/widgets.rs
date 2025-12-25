use repose_core::{prelude::*, signal};
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    let cb = remember(|| signal(true));
    let sw = remember(|| signal(false));
    let radio = remember(|| signal(0u8));
    let s_val = remember(|| signal(0.35f32));
    let r_a = remember(|| signal(0.2f32));
    let r_b = remember(|| signal(0.8f32));
    let prog = remember(|| signal(0.4f32));

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
                Slider(s_val.get(), (0.0, 1.0), Some(0.01), {
                    let s = s_val.clone();
                    move |v| s.set(v)
                }),
                Box(Modifier::new().height(12.0).width(1.0)),
                RangeSlider(r_a.get(), r_b.get(), (0.0, 1.0), Some(0.01), {
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
                    Button(Text("Decrease"), {
                        let p = prog.clone();
                        move || p.update(|x| *x = (*x - 0.05).max(0.0))
                    }),
                    Box(Modifier::new().width(12.0).height(1.0)),
                    Button(Text("Increase"), {
                        let p = prog.clone();
                        move || p.update(|x| *x = (*x + 0.05).min(1.0))
                    }),
                )),
            )),
        ),
    ))
}
