use repose_core::{TextDirection, prelude::*, with_text_direction};
use repose_ui::{
    navigation::{NavController, NavHost},
    *,
};
use std::collections::HashMap;

use crate::{pages, ui::Page};

pub fn app(_s: &mut Scheduler) -> View {
    // App state
    let theme_dark = remember(|| signal(true));
    let rtl = remember(|| signal(false));
    let density = remember(|| signal(1.0f32)); // dp scale
    let text_scale = remember(|| signal(1.0f32)); // font scale

    // Themes
    let theme_light = {
        let mut t = Theme::default();
        t.background = Color::from_hex("#FAFAFA");
        t.surface = Color::from_hex("#FFFFFF");
        t.on_surface = Color::from_hex("#222222");
        t.primary = Color::from_hex("#3B82F6");
        t.on_primary = Color::WHITE;
        t
    };
    let theme_dark_v = Theme::default();
    let use_theme = if theme_dark.get() {
        theme_dark_v
    } else {
        theme_light
    };

    let dir = if rtl.get() {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    };

    // Navigation
    let nav = remember_with_key("nav", || NavController::new("layout"));
    let nav = nav.as_ref().clone();

    let mut routes: HashMap<String, Box<dyn Fn() -> View>> = HashMap::new();
    routes.insert("layout".into(), Box::new(|| pages::layout::screen()));
    routes.insert("widgets".into(), Box::new(|| pages::widgets::screen()));
    routes.insert("text".into(), Box::new(|| pages::text::screen()));
    // routes.insert("lists".into(), Box::new(|| pages::lists::screen()));
    routes.insert("animation".into(), Box::new(|| pages::animation::screen()));
    routes.insert("canvas".into(), Box::new(|| pages::canvas::screen()));
    routes.insert("scrolls".into(), Box::new(|| pages::scrolls::screen()));
    routes.insert("errors".into(), Box::new(|| pages::errors::screen()));

    // Tab spec: label -> route
    let tabs: [(&str, &str); 7] = [
        ("Layout", "layout"),
        ("Widgets", "widgets"),
        ("Text", "text"),
        // ("Lists", "lists"),
        ("Animation", "animation"),
        ("Canvas", "canvas"),
        ("Scrolls", "scrolls"),
        ("Errors", "errors"),
    ];

    with_text_direction(dir, || {
        with_theme(use_theme, || {
            with_density(
                Density {
                    scale: density.get(),
                },
                || {
                    with_text_scale(TextScale(text_scale.get()), || {
                        Surface(
                            Modifier::new()
                                .fill_max_size()
                                .background(theme().background),
                            Column(Modifier::new().fill_max_size()).child((
                                // Top bar + toggles
                                crate::ui::TopBar().child((
                                    Text("Repose Showcase").modifier(Modifier::new().padding(8.0)),
                                    Spacer(),
                                    Row(Modifier::new()).child((
                                        Text("Theme").modifier(Modifier::new().padding(8.0)),
                                        Switch(theme_dark.get(), "Dark", {
                                            let theme_dark = theme_dark.clone();
                                            move |v| theme_dark.set(v)
                                        })
                                        .modifier(Modifier::new().padding(8.0)),
                                        Text("RTL").modifier(Modifier::new().padding(8.0)),
                                        Switch(rtl.get(), "RTL", {
                                            let rtl = rtl.clone();
                                            move |v| rtl.set(v)
                                        })
                                        .modifier(Modifier::new().padding(8.0)),
                                    )),
                                )),
                                // tab row. Selected uses nav.current.
                                {
                                    let current = nav.current.get();
                                    Row(Modifier::new().align_self_center().padding(8.0)).child(
                                        tabs.iter()
                                            .map(|(label, route)| {
                                                let is_selected = &current == *route;
                                                let bg = if is_selected {
                                                    theme().button_bg
                                                } else {
                                                    theme().surface
                                                };
                                                Button(Text(*label), {
                                                    let nav = nav.clone();
                                                    let r = (*route).to_string();
                                                    move || nav.navigate(r.clone())
                                                })
                                                .modifier(
                                                    Modifier::new()
                                                        .padding(4.0)
                                                        .background(bg)
                                                        .clip_rounded(6.0),
                                                )
                                            })
                                            .collect::<Vec<_>>(),
                                    )
                                },
                                // Page content via NavHost
                                Page(
                                    Box(Modifier::new().fill_max_size().padding(16.0))
                                        .child(NavHost(nav.clone(), routes)),
                                ),
                            )),
                        )
                    })
                },
            )
        })
    })
}
