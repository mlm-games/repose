#![cfg(target_os = "android")]
use log::LevelFilter;
use repose_core::prelude::*;
use repose_platform::android::run_android_app;
use repose_ui::*;
use winit::platform::android::activity::AndroidApp;

mod ui;
mod pages {
    pub mod layout;
    pub mod lists;
    pub mod text;
    pub mod widgets;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Layout,
    Widgets,
    Text,
    Lists,
}

fn app(_s: &mut Scheduler) -> View {
    let page = remember(|| signal(Page::Layout));
    // App-level toggles
    let rtl = remember(|| signal(false));
    let theme_dark = remember(|| signal(true));

    // Theme switcher
    let theme_light = Theme {
        background: Color::from_hex("#FAFAFA"),
        surface: Color::from_hex("#FFFFFF"),
        on_surface: Color::from_hex("#222222"),
        primary: Color::from_hex("#3B82F6"),
        on_primary: Color::WHITE,
    };
    let theme_dark_v = Theme::default();
    let use_theme = if theme_dark.get() {
        theme_dark_v
    } else {
        theme_light
    };
    let dir = if rtl.get() {
        repose_core::locals::TextDirection::Rtl
    } else {
        repose_core::locals::TextDirection::Ltr
    };

    repose_core::with_text_direction(dir, || {
        repose_core::with_theme(use_theme, || {
            let content = match page.get() {
                Page::Layout => pages::layout::screen(),
                Page::Widgets => pages::widgets::screen(),
                Page::Text => pages::text::screen(),
                Page::Lists => pages::lists::screen(),
            };

            Surface(
                Modifier::new()
                    .fill_max_size()
                    .background(theme().background),
                Column(Modifier::new()).child((
                    // Top bar
                    ui::TopBar().child((
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
                    // Tabs
                    ui::Tabs(
                        &[
                            ("Layout", Page::Layout),
                            ("Widgets", Page::Widgets),
                            ("Text", Page::Text),
                            ("Lists", Page::Lists),
                        ],
                        page.as_ref().clone(),
                    ),
                    // Page content
                    Box(Modifier::new().fill_max_size().padding(16.0)).child(content),
                )),
            )
        })
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));
    let _ = run_android_app(android_app, app as fn(&mut Scheduler) -> View);
}
