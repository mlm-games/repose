use crate::ui::{
    LabeledCheckbox, LabeledLinearProgress, LabeledRadioButton, LabeledRangeSlider, LabeledSlider,
    LabeledSwitch, Section,
};
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
    let cb = remember(|| signal(true));
    let radio = remember(|| signal("A".to_string()));
    let sw = remember(|| signal(false));
    let s_val = remember(|| signal(0.35f32));
    let r_a = remember(|| signal(0.2f32));
    let r_b = remember(|| signal(0.8f32));
    let prog = remember(|| signal(0.4f32));

    Column(Modifier::new().padding(8.0)).child((
        Section(
            "Switch / Checkbox / Radio",
            Column(Modifier::new().padding(8.0)).child((
                LabeledSwitch(sw.get(), "Master switch", {
                    let sw = sw.clone();
                    move |v| sw.set(v)
                })
                .modifier(Modifier::new().padding(6.0)),
                LabeledCheckbox(cb.get(), "Enable feature X", {
                    let cb = cb.clone();
                    move |v| cb.set(v)
                })
                .modifier(Modifier::new().padding(6.0)),
                Row(Modifier::new()).child((
                    LabeledRadioButton(radio.get() == "A", "Option A", {
                        let radio = radio.clone();
                        move || radio.set("A".into())
                    })
                    .modifier(Modifier::new().padding(6.0)),
                    LabeledRadioButton(radio.get() == "B", "Option B", {
                        let radio = radio.clone();
                        move || radio.set("B".into())
                    })
                    .modifier(Modifier::new().padding(6.0)),
                )),
            )),
        ),
        Section(
            "Slider / RangeSlider / Progress",
            Column(Modifier::new().padding(8.0)).child((
                LabeledSlider(s_val.get(), (0.0, 1.0), Some(0.01), "Volume", {
                    let s_val = s_val.clone();
                    move |v| s_val.set(v)
                })
                .modifier(Modifier::new().padding(6.0)),
                LabeledRangeSlider(r_a.get(), r_b.get(), (0.0, 1.0), Some(0.01), "Window", {
                    let a = r_a.clone();
                    let b = r_b.clone();
                    move |x0, x1| {
                        a.set(x0);
                        b.set(x1);
                    }
                })
                .modifier(Modifier::new().padding(6.0)),
                LabeledLinearProgress(Some(prog.get()), "Progress")
                    .modifier(Modifier::new().padding(6.0)),
                Row(Modifier::new().padding(8.0)).child((
                    Button(Text("⬆️"), {
                        let p = prog.clone();
                        move || p.update(|x| *x = (*x + 0.05).min(1.0))
                    }),
                    Button(Text("⬇️"), {
                        let p = prog.clone();
                        move || p.update(|x| *x = (*x - 0.05).max(0.0))
                    }),
                )),
            )),
        ),
    ))
}
