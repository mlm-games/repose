#![allow(non_snake_case)]

use repose_core::prelude::*;
use repose_material::material3::Card;
use repose_navigation::Navigator;
use repose_ui::*;

use crate::app::Route;

pub fn AppShell(
    current: Route,
    nav: Navigator<Route>,
    dark: bool,
    on_dark: impl Fn(bool) + 'static,
    rtl: bool,
    on_rtl: impl Fn(bool) + 'static,
    density: f32,
    on_density: impl Fn(f32) + 'static,
    text_scale: f32,
    on_text_scale: impl Fn(f32) + 'static,
    content: View,
) -> View {
    Surface(
        Modifier::new()
            .fill_max_size()
            .background(theme().background),
        Column(Modifier::new().fill_max_size()).child((
            TopBar(
                dark,
                on_dark,
                rtl,
                on_rtl,
                density,
                on_density,
                text_scale,
                on_text_scale,
            ),
            Row(Modifier::new().fill_max_size()).child((
                NavRail(current, nav),
                // Page container
                Box(Modifier::new().fill_max_size().padding(16.0)).child(content),
            )),
        )),
    )
}

pub fn TopBar(
    dark: bool,
    on_dark: impl Fn(bool) + 'static,
    rtl: bool,
    on_rtl: impl Fn(bool) + 'static,
    density: f32,
    on_density: impl Fn(f32) + 'static,
    text_scale: f32,
    on_text_scale: impl Fn(f32) + 'static,
) -> View {
    let th = theme();

    Row(Modifier::new()
        .padding(12.0)
        .background(th.surface)
        .border(1.0, th.outline, 0.0))
    .child((
        Text("Repose Showcase").size(18.0).color(th.on_surface),
        Spacer(),
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            LabeledSwitch("Dark", dark, on_dark),
            Box(Modifier::new().width(12.0).height(1.0)),
            LabeledSwitch("RTL", rtl, on_rtl),
            Box(Modifier::new().width(16.0).height(1.0)),
            LabeledSlider("Density", density, (0.75, 2.0), Some(0.05), on_density)
                .modifier(Modifier::new().width(220.0)),
            Box(Modifier::new().width(16.0).height(1.0)),
            LabeledSlider("Text", text_scale, (0.75, 2.0), Some(0.05), on_text_scale)
                .modifier(Modifier::new().width(220.0)),
        )),
    ))
}

pub fn NavRail(current: Route, nav: Navigator<Route>) -> View {
    let th = theme();

    let routes: [Route; 9] = [
        Route::Home,
        Route::Layout,
        Route::Widgets,
        Route::Text,
        Route::Scroll,
        Route::Canvas,
        Route::Lists,
        Route::Animation,
        Route::Errors,
    ];

    // A simple left rail: Card for a consistent surface.
    Card(
        Modifier::new()
            .width(220.0)
            .fill_max_height()
            .background(th.surface)
            .border(1.0, th.outline, 12.0)
            .clip_rounded(12.0)
            .padding(8.0),
        true,
        Column(Modifier::new().fill_max_size()).child((
            Text("Navigation")
                .size(14.0)
                .color(Color::from_hex("#999999"))
                .modifier(Modifier::new().padding(8.0)),
            Column(Modifier::new().fill_max_size()).child(
                routes
                    .iter()
                    .map(|&r| {
                        NavItem(r, r == current, {
                            let nav = nav.clone();
                            move || nav.push(r)
                        })
                    })
                    .collect::<Vec<_>>(),
            ),
        )),
    )
}

fn NavItem(route: Route, selected: bool, on_click: impl Fn() + 'static) -> View {
    let th = theme();

    let bg = if selected {
        // selected state: use primary with some transparency
        Color(th.primary.0, th.primary.1, th.primary.2, 48)
    } else {
        th.surface
    };

    let fg = if selected { th.primary } else { th.on_surface };

    Button(Text(route.title()).size(16.0).color(fg), on_click).modifier(
        Modifier::new()
            .key(route.id()) // stable identity for nav items
            .fill_max_width()
            .padding(6.0)
            .background(bg)
            .clip_rounded(8.0),
    )
}

pub fn Section(title: &str, body: View) -> View {
    Column(Modifier::new().padding(8.0)).child((
        Text(title)
            .size(18.0)
            .color(theme().on_surface)
            .modifier(Modifier::new().padding(8.0)),
        Card(Modifier::new().fill_max_width(), true, body),
    ))
}

pub fn LabeledSwitch(label: &str, checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    Row(Modifier::new().align_items(AlignItems::Center)).child((
        Text(label).size(14.0).color(Color::from_hex("#999999")),
        Box(Modifier::new().width(8.0).height(1.0)),
        Switch(checked, on_change),
    ))
}

pub fn LabeledSlider(
    label: &str,
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    Column(Modifier::new().align_items(AlignItems::Stretch)).child((
        Text(format!("{label}: {:.2}", value))
            .size(14.0)
            .color(Color::from_hex("#999999")),
        Box(Modifier::new().height(6.0).width(1.0)),
        Slider(value, range, step, on_change),
    ))
}
