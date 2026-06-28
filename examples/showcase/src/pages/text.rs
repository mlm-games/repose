use repose_core::{prelude::*, signal};
use repose_material::material3::{
    Button, ButtonConfig, OutlinedTextField, OutlinedTextFieldConfig,
};
use repose_material::{Icon, material_symbols};
use repose_ui::*;
use std::rc::Rc;

use crate::ui::{Caption, Hint, Page, Section, sp};

material_symbols! {
    home     : '\u{E88A}',
    favorite : '\u{E87D}',
    settings : '\u{E8B8}',
    search   : '\u{E8B6}',
}

fn field_status(change: String, submit: String) -> View {
    Column(Modifier::new().gap(2.0)).child((
        Caption(format!("last change: {change}")),
        Caption(format!("last submit: {submit}")),
    ))
}

fn symbol_cell(glyph: View, label: &'static str) -> View {
    Column(Modifier::new().gap(sp::XS)).child((glyph, Text(label).size(12.0)))
}

fn annotated_demos() -> Vec<(&'static str, View)> {
    let t = theme();
    vec![
        ("Annotated Text - Colored Spans", {
            let a = build_annotated_string(|b| {
                b.push_color("Red ", Color::from_rgba(0xE6, 0x1C, 0x1C, 255));
                b.push_color("Green ", Color::from_rgba(0x1C, 0xE6, 0x1C, 255));
                b.push_color("Blue ", Color::from_rgba(0x1C, 0x1C, 0xE6, 255));
                b.push("and ");
                b.push_color("Yellow", Color::from_rgba(0xE6, 0xE6, 0x1C, 255));
                b.push(" text spans.");
            });
            AnnotatedText(a).size(18.0)
        }),
        ("Annotated Text - Mixed Colors", {
            let a = build_annotated_string(|b| {
                b.push("This text has ");
                b.push_color("multiple", Color::from_rgba(0xE6, 0x1C, 0x1C, 255));
                b.push(" ");
                b.push_color("different", Color::from_rgba(0x1C, 0xE6, 0x1C, 255));
                b.push(" ");
                b.push_color("colors", Color::from_rgba(0x1C, 0x1C, 0xE6, 255));
                b.push(" in a single line.");
            });
            AnnotatedText(a).size(16.0)
        }),
        ("Annotated Text - Themed Colors", {
            let a = build_annotated_string(|b| {
                b.push_color("Primary ", t.primary);
                b.push_color("Secondary ", t.secondary);
                b.push_color("Tertiary ", t.tertiary);
                b.push_color("Error", t.error);
                b.push(" colored text.");
            });
            AnnotatedText(a).size(16.0)
        }),
        ("Annotated Text - Custom Font Size", {
            let a = build_annotated_string(|b| {
                b.push("Normal text ");
                b.push_with_style("Big text ", SpanStyle::default().font_size(24.0));
                b.push_with_style(
                    "Colored big ",
                    SpanStyle::default()
                        .color(Color::from_rgba(0xE6, 0x1C, 0x1C, 255))
                        .font_size(24.0),
                );
                b.push("back to normal.");
            });
            AnnotatedText(a).size(14.0)
        }),
        ("Annotated Text - Multi-line", {
            let a = build_annotated_string(|b| {
                b.push("This is a ");
                b.push_color("long paragraph", Color::from_rgba(0x1C, 0x8C, 0xE6, 255));
                b.push(" with styled spans that wraps across multiple lines when the text exceeds the available width. ");
                b.push_color("Each span", Color::from_rgba(0xE6, 0x1C, 0x8C, 255));
                b.push(" can have its own ");
                b.push_color("color", Color::from_rgba(0x8C, 0xE6, 0x1C, 255));
                b.push(" and the line breaking correctly preserves the styling per segment.");
            });
            AnnotatedText(a).size(16.0)
        }),
    ]
}

pub fn screen() -> View {
    let single_text = remember_with_key("text_single_value", || signal(String::new()));
    let last_submit_single = remember_with_key("text_last_submit_single", || signal(String::new()));
    let last_change_single = remember_with_key("text_last_change_single", || signal(String::new()));
    let multi_text = remember_with_key("text_multi_value", || signal(String::new()));
    let last_submit_multi = remember_with_key("text_last_submit_multi", || signal(String::new()));
    let last_change_multi = remember_with_key("text_last_change_multi", || signal(String::new()));
    let toggle = remember(|| signal(false));

    let mut sections: Vec<View> = vec![
        Row(Modifier::new().fill_max_width().gap(sp::LG)).child((
            Section(
                "TextField (single-line)",
                Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                    OutlinedTextField(
                        Modifier::new().fill_max_width(),
                        single_text.get(),
                        {
                            let last_change = last_change_single.clone();
                            let t = single_text.clone();
                            move |v| {
                                t.set(v.clone());
                                last_change.set(v);
                            }
                        },
                        OutlinedTextFieldConfig {
                            label: Some("Type here".into()),
                            on_submit: Some(Rc::new({
                                let last_submit = last_submit_single.clone();
                                move |s| last_submit.set(s)
                            })),
                            ..Default::default()
                        },
                    ),
                    Hint("Single-line: Enter submits."),
                    field_status(last_change_single.get(), last_submit_single.get()),
                )),
            )
            .modifier(Modifier::new().flex_grow(1.0)),
            Section(
                "Material Symbols",
                Row(Modifier::new().padding(sp::MD).gap(sp::LG)).child((
                    symbol_cell(Icon(Symbols::home).size(32.0).color(theme().primary), "home"),
                    symbol_cell(Icon(Symbols::favorite).size(32.0).color(theme().error), "favorite"),
                    symbol_cell(Icon(Symbols::settings).size(32.0).color(theme().on_surface), "settings"),
                    symbol_cell(Icon(Symbols::search).size(32.0).color(theme().primary), "search"),
                )),
            )
            .modifier(Modifier::new().flex_grow(1.0)),
        )),
        Section("Password TextField", {
            let pw = remember_with_key("pw_value", || signal(String::new()));
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                BasicTextFieldEx::new("Password", pw.get(), Modifier::new().fill_max_width())
                    .password()
                    .on_change({
                        let p = pw.clone();
                        move |v| p.set(v)
                    })
                    .build(),
                Row(Modifier::new()).child((
                    Hint("(masked value: "),
                    Text(pw.get()).size(14.0).color(theme().primary),
                    Hint(")"),
                )),
            ))
        }),
        Section(
            "TextArea (multi-line)",
            Column(Modifier::new().padding(sp::MD).gap(sp::SM)).child((
                BasicTextField(
                    "Write notes…",
                    multi_text.get(),
                    Modifier::new()
                        .height(180.0)
                        .fill_max_width()
                        .background(theme().surface)
                        .border(1.0, theme().outline, 10.0)
                        .clip_rounded(10.0),
                    BasicTextFieldConfig {
                        single_line: false,
                        on_change: Some(Rc::new({
                            let t = multi_text.clone();
                            let last_change = last_change_multi.clone();
                            move |s: String| {
                                t.set(s.clone());
                                last_change.set(s);
                            }
                        })),
                        on_submit: Some(Rc::new({
                            let last_submit = last_submit_multi.clone();
                            move |s| last_submit.set(s)
                        })),
                        ..Default::default()
                    },
                ),
                Hint("Multi-line: Enter inserts newline. Cmd/Ctrl+Enter submits (if wired)."),
                field_status(last_change_multi.get(), last_submit_multi.get()),
            )),
        ),
        Section(
            "Wrapping + Ellipsis",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Text("Single-line label that ellipsizes when it runs out of space.")
                    .single_line()
                    .overflow_ellipsize()
                    .modifier(Modifier::new().fill_max_width()),
                Text("This paragraph demonstrates wrapping in a constrained box. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Vestibulum at arcu sed justo viverra posuere.")
                    .size(16.0)
                    .modifier(Modifier::new().width(420.0)),
            )),
        ),
    ];

    // Static annotated demos, data-driven.
    sections.extend(
        annotated_demos()
            .into_iter()
            .map(|(title, v)| Section(title, v)),
    );

    // Interactive annotated demo.
    sections.push(Section("Annotated Text - Toggle Example", {
        let annotated = if toggle.get() {
            build_annotated_string(|b| {
                b.push_color("ON", Color::from_rgba(0x1C, 0xE6, 0x1C, 255));
                b.push(" - The toggle is active");
            })
        } else {
            build_annotated_string(|b| {
                b.push_color("OFF", Color::from_rgba(0xE6, 0x1C, 0x1C, 255));
                b.push(" - The toggle is inactive");
            })
        };
        Column(Modifier::new().padding(sp::SM).gap(sp::SM)).child((
            AnnotatedText(annotated).size(18.0),
            Button(
                Modifier::new(),
                {
                    let t = toggle.clone();
                    move || t.update(|x| *x = !*x)
                },
                ButtonConfig::default(),
                || Text("Toggle"),
            ),
        ))
    }));

    sections.push(Section("Selectable Text", {
        let sel = remember_with_key("text_selectable_range", || signal("none".to_string()));
        let sel2 = sel.clone();
        Column(Modifier::new().padding(sp::SM).gap(sp::SM)).child((
            SelectableText(
                "Try clicking and dragging to select text in this paragraph.",
                16.0,
                move |range| {
                    sel2.set(match range {
                        Some((a, b)) if a != b => format!("{}..{}", a.min(b), a.max(b)),
                        Some((a, _)) => format!("caret at {a}"),
                        None => "cleared".into(),
                    });
                },
            ),
            Caption(format!("Selection: {}", sel.get())),
        ))
    }));

    Page(sections)
}
