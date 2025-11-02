use compose_core::prelude::*;
use compose_platform::run_desktop_app;
use compose_ui::*;
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

    let content = {
        match page.get() {
            Page::Layout => pages::layout::screen(),
            Page::Widgets => pages::widgets::screen(),
            Page::Text => pages::text::screen(),
            Page::Lists => pages::lists::screen(),
        }
    };

    let root = Surface(
        Modifier::new()
            .fill_max_size()
            .background(theme().background),
        Column(Modifier::new()).child((
            // Top bar
            ui::TopBar().child((
                Text("Repose Showcase").modifier(Modifier::new().padding(8.0)),
                Spacer(),
                // Theme toggle
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
    );

    // Apply theme and direction
    let themed = compose_core::with_theme(
        if theme_dark.get() {
            theme_dark_v
        } else {
            theme_light
        },
        || root,
    );
    if rtl.get() {
        compose_core::with_text_direction(compose_core::locals::TextDirection::Rtl, || themed)
    } else {
        themed
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app)
}
