#![allow(non_snake_case)]

use repose_core::prelude::*;
use repose_material::material3::dialog::{Dialog, DialogState};
use repose_material::material3::{Card, ElevatedCard, IconButton, M3Slider, Switch};
use repose_material::{Icon, material_symbols};
use repose_navigation::Navigator;
use repose_ui::overlay::OverlayHandle;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;

use crate::app::Route;

material_symbols! {
    settings : '\u{E8B8}',
}

pub fn AppShell(
    current: Route,
    nav: Navigator<Route>,
    overlay: OverlayHandle,
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
                overlay,
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
                ScrollArea(
                    Modifier::new().fill_max_size().padding(16.0),
                    {
                        let st = remember_scroll_state("page_scroll");
                        st.set_show_scrollbar(false);
                        st
                    },
                    content,
                ),
            )),
        )),
    )
}

pub fn TopBar(
    overlay: OverlayHandle,
    dark: bool,
    on_dark: impl Fn(bool) + 'static,
    rtl: bool,
    on_rtl: impl Fn(bool) + 'static,
    density: f32,
    on_density: impl Fn(f32) + 'static,
    text_scale: f32,
    on_text_scale: impl Fn(f32) + 'static,
) -> View {
    let settings_state = remember(|| DialogState::new());
    let th = theme();

    Row(Modifier::new()
        .padding(12.0)
        .background(th.surface)
        .border(1.0, th.outline, 0.0)
        .align_items(AlignItems::Center))
    .child((
        Text("Repose Showcase").size(18.0).color(th.on_surface),
        Spacer(),
        IconButton(
            Icon(Symbols::settings)
                .size(20.0)
                .color(th.on_surface_variant),
            {
                let s = settings_state.clone();
                move || s.show()
            },
        ),
        // Overlay-based settings dialog (renders nothing in the layout)
        Dialog(
            settings_state.clone(),
            overlay.clone(),
            Modifier::new(),
            Column(Modifier::new().padding(24.0).min_width(320.0)).with_children(vec![
                Text("Settings").size(20.0).color(th.on_surface),
                Box(Modifier::new().size(1.0, 16.0)),
                LabeledSwitch("Dark Mode", dark, on_dark),
                Box(Modifier::new().size(1.0, 12.0)),
                LabeledSwitch("RTL Layout", rtl, on_rtl),
                Box(Modifier::new().size(1.0, 12.0)),
                LabeledSlider("Density", density, (0.75, 2.0), Some(0.05), on_density),
                Box(Modifier::new().size(1.0, 12.0)),
                LabeledSlider(
                    "Text Scale",
                    text_scale,
                    (0.75, 2.0),
                    Some(0.05),
                    on_text_scale,
                ),
            ]),
        ),
    ))
}

pub fn NavRail(current: Route, nav: Navigator<Route>) -> View {
    let th = theme();

    let routes: [Route; 18] = [
        Route::Home,
        Route::Layout,
        Route::Widgets,
        Route::Text,
        Route::Scroll,
        Route::ScrollFeatures,
        Route::Canvas,
        Route::Lists,
        Route::Grid,
        Route::StaggeredGrid,
        Route::Pager,
        Route::Animation,
        Route::Dnd,
        Route::Docking,
        Route::Errors,
        Route::Windows,
        Route::M3,
        Route::Adaptive,
    ];

    Card(
        Modifier::new().width(220.0).fill_max_height().padding(8.0),
        Column(Modifier::new().fill_max_size()).child((
            Text("Navigation")
                .size(14.0)
                .color(th.on_surface_variant)
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
        th.primary.with_alpha(48)
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
        ElevatedCard(Modifier::new().fill_max_width().padding(16.0), body),
    ))
}

pub fn LabeledSwitch(label: &str, checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    let th = theme();
    Row(Modifier::new().align_items(AlignItems::Center)).child((
        Text(label).size(14.0).color(th.on_surface_variant),
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
    let th = theme();
    Column(Modifier::new().align_items(AlignItems::Stretch)).child((
        Text(format!("{label}: {:.2}", value))
            .size(14.0)
            .color(th.on_surface_variant),
        Box(Modifier::new().height(6.0).width(1.0)),
        M3Slider(value, range, step, on_change),
    ))
}
