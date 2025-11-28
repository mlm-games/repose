use repose_core::{TextDirection, prelude::*, with_text_direction};
use repose_navigation::{
    EntryScope, InstallBackHandler, NavDisplay, Navigator, remember_back_stack, renderer,
};
use repose_ui::*;
use serde::{Deserialize, Serialize};

use crate::{
    pages,
    ui::{LabeledSwitch, Page},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Key {
    Layout,
    Widgets,
    Text,
    Animation,
    Canvas,
    Scrolls,
    Errors,
}

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

    let stack = remember_back_stack(Key::Layout);

    let _bk = {
        let stack = stack.clone();
        effect(move || {
            InstallBackHandler((*stack).clone());
            on_unmount(|| {})
        })
    };

    // renderer: Key -> View
    let render = renderer(move |scope: &EntryScope<Key>| match scope.key() {
        Key::Layout => pages::layout::screen(),
        Key::Widgets => pages::widgets::screen(),
        Key::Text => pages::text::screen(),
        Key::Animation => pages::animation::screen(),
        Key::Canvas => pages::canvas::screen(),
        Key::Scrolls => pages::scrolls::screen(),
        Key::Errors => pages::errors::screen(),
    });

    // Tab spec: label -> route
    let tabs: [(&str, Key); 7] = [
        ("Layout", Key::Layout),
        ("Widgets", Key::Widgets),
        ("Text", Key::Text),
        ("Animation", Key::Animation),
        ("Canvas", Key::Canvas),
        ("Scrolls", Key::Scrolls),
        ("Errors", Key::Errors),
    ];
    let navigator = Navigator {
        stack: (*stack).clone(),
    };
    let current_key = stack.top().map(|(_, k, _)| k).unwrap_or(Key::Layout);

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
                                        LabeledSwitch(theme_dark.get(), "Dark", {
                                            let theme_dark = theme_dark.clone();
                                            move |v| theme_dark.set(v)
                                        })
                                        .modifier(Modifier::new().padding(8.0)),
                                        Text("RTL").modifier(Modifier::new().padding(8.0)),
                                        LabeledSwitch(rtl.get(), "RTL", {
                                            let rtl = rtl.clone();
                                            move |v| rtl.set(v)
                                        })
                                        .modifier(Modifier::new().padding(8.0)),
                                    )),
                                )),
                                // tab row. Selected uses current key from stack.
                                Row(Modifier::new().align_self_center().padding(8.0)).child(
                                    tabs.iter()
                                        .map(|(label, key)| {
                                            let is_selected = &current_key == key;
                                            let bg = if is_selected {
                                                theme().button_bg
                                            } else {
                                                theme().surface
                                            };
                                            let key_clone = key.clone();
                                            Button(Text(*label), {
                                                let nav = navigator.clone();
                                                move || nav.push(key_clone.clone())
                                            })
                                            .modifier(
                                                Modifier::new()
                                                    .padding(4.0)
                                                    .background(bg)
                                                    .clip_rounded(6.0),
                                            )
                                        })
                                        .collect::<Vec<_>>(),
                                ),
                                // Page content via NavDisplay
                                Page(Box(Modifier::new().fill_max_size().padding(16.0)).child(
                                    NavDisplay(
                                        stack.clone(),
                                        render.clone(),
                                        None, // back handled globally via InstallBackHandler
                                        Default::default(),
                                    ),
                                )),
                            )),
                        )
                    })
                },
            )
        })
    })
}
