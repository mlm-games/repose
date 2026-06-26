#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::dialog::{Dialog, DialogState};
use repose_material::material3::{
    ElevatedCard, FilledCard, IconButton, IconButtonConfig, M3Slider, SliderConfig, Switch, SwitchConfig,
};
use repose_material::{Icon, material_symbols};
use repose_navigation::Navigator;
use repose_ui::overlay::OverlayHandle;
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::*;

use crate::app::Route;

material_symbols! {
    settings : '\u{E8B8}',
}

pub mod sp {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
}

pub mod radius {
    pub const SM: f32 = 6.0;
    pub const MD: f32 = 10.0;
    pub const LG: f32 = 12.0;
}

pub fn VSpace(h: f32) -> View {
    Box(Modifier::new().height(h).width(1.0))
}

pub fn HSpace(w: f32) -> View {
    Box(Modifier::new().width(w).height(1.0))
}

/// Secondary descriptive text, which replaces the repeated
/// `.size(13/14).color(on_surface_variant)` pattern.
pub fn Hint(text: impl Into<String>) -> View {
    Text(text.into())
        .size(13.0)
        .color(theme().on_surface_variant)
}

pub fn Caption(text: impl Into<String>) -> View {
    Text(text.into())
        .size(12.0)
        .color(theme().on_surface_variant)
}

/// Standard page container with consistent section rhythm.
pub fn Page(children: Vec<View>) -> View {
    Column(Modifier::new().fill_max_width().gap(sp::LG)).with_children(children)
}

/// Card-backed demo cell shared by grid / staggered-grid pages.
pub fn DemoTile(
    title: impl Into<String>,
    subtitle: impl Into<String>,
    bg: Color,
    fg: Color,
    height: f32,
) -> View {
    Box(Modifier::new()
        .fill_max_width()
        .height(height)
        .background(bg)
        .clip_rounded(radius::LG))
    .child(
        Column(
            Modifier::new()
                .fill_max_size()
                .justify_content(JustifyContent::Center)
                .align_items(AlignItems::Center),
        )
        .child((
            Text(title.into()).size(20.0).color(fg),
            Text(subtitle.into()).size(12.0).color(fg.with_alpha(180)),
        )),
    )
}

pub fn Section(title: &str, body: View) -> View {
    SectionWith(title, None, body)
}

pub fn SectionWith(title: &str, subtitle: Option<&str>, body: View) -> View {
    let th = theme();
    let mut header: Vec<View> = vec![Text(title).size(18.0).color(th.on_surface)];
    if let Some(s) = subtitle {
        header.push(Hint(s));
    }
    Column(Modifier::new().padding(sp::SM).gap(sp::SM)).child((
        Column(Modifier::new().padding(sp::SM).gap(2.0)).with_children(header),
        ElevatedCard(Modifier::new().fill_max_width().padding(sp::LG), body),
    ))
}

#[derive(Clone)]
pub struct SettingsVm {
    pub dark: bool,
    pub on_dark: Rc<dyn Fn(bool)>,
    pub rtl: bool,
    pub on_rtl: Rc<dyn Fn(bool)>,
    pub density: f32,
    pub on_density: Rc<dyn Fn(f32)>,
    pub text_scale: f32,
    pub on_text_scale: Rc<dyn Fn(f32)>,
}

pub fn AppShell(
    current: Route,
    nav: Navigator<Route>,
    overlay: OverlayHandle,
    settings: SettingsVm,
    content: View,
) -> View {
    Box(Modifier::new()
        .fill_max_size()
        .background(theme().background))
    .child(Column(Modifier::new().fill_max_size()).child((
        TopBar(overlay, settings),
        Row(Modifier::new().fill_max_size()).child((
            NavRail(current, nav),
            ScrollArea(
                Modifier::new().fill_max_size().padding(sp::LG),
                {
                    let st = remember_scroll_state("page_scroll");
                    st.set_show_scrollbar(false);
                    st
                },
                content,
            ),
        )),
    )))
}

pub fn TopBar(overlay: OverlayHandle, vm: SettingsVm) -> View {
    let settings_state = remember(DialogState::new);
    let th = theme();

    Row(Modifier::new()
        .padding(sp::MD)
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
            IconButtonConfig::default(),
        ),
        Dialog(
            settings_state.clone(),
            overlay,
            Modifier::new(),
            Column(Modifier::new().padding(sp::XL).min_width(320.0).gap(sp::MD)).child((
                Text("Settings").size(20.0).color(th.on_surface),
                LabeledSwitch("Dark Mode", vm.dark, {
                    let f = vm.on_dark.clone();
                    move |v| f(v)
                }),
                LabeledSwitch("RTL Layout", vm.rtl, {
                    let f = vm.on_rtl.clone();
                    move |v| f(v)
                }),
                LabeledSlider("Density", vm.density, (0.75, 2.0), Some(0.05), {
                    let f = vm.on_density.clone();
                    move |v| f(v)
                }),
                LabeledSlider("Text Scale", vm.text_scale, (0.75, 2.0), Some(0.05), {
                    let f = vm.on_text_scale.clone();
                    move |v| f(v)
                }),
            )),
        ),
    ))
}

pub fn NavRail(current: Route, nav: Navigator<Route>) -> View {
    let th = theme();
    FilledCard(
        Modifier::new()
            .width(220.0)
            .fill_max_height()
            .padding(sp::SM),
        Column(Modifier::new().fill_max_size().gap(2.0)).child(
            std::iter::once(
                Text("Navigation")
                    .size(13.0)
                    .color(th.on_surface_variant)
                    .modifier(Modifier::new().padding(sp::SM)),
            )
            .chain(Route::ALL.iter().map(|&r| {
                NavItem(r, r == current, {
                    let nav = nav.clone();
                    move || nav.push(r)
                })
            }))
            .collect::<Vec<_>>(),
        ),
    )
}

fn NavItem(route: Route, selected: bool, on_click: impl Fn() + 'static) -> View {
    let th = theme();
    let (bg, fg) = if selected {
        (th.secondary_container, th.on_secondary_container)
    } else {
        (th.surface, th.on_surface)
    };
    let indicator = if selected { th.primary } else { bg };

    Box(Modifier::new()
        .key(route.id())
        .fill_max_width()
        .padding(6.0)
        .background(bg)
        .clip_rounded(radius::MD)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(
        Row(Modifier::new().align_items(AlignItems::Center).gap(sp::SM)).child((
            Box(Modifier::new()
                .size(3.0, 16.0)
                .background(indicator)
                .clip_rounded(2.0)),
            Text(route.title()).size(15.0).color(fg),
        )),
    )
}

pub fn LabeledSwitch(label: &str, checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    Row(Modifier::new().align_items(AlignItems::Center).gap(sp::SM)).child((
        Text(label).size(14.0).color(theme().on_surface_variant),
        Switch(checked, on_change, SwitchConfig::default()),
    ))
}

pub fn LabeledSlider(
    label: &str,
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    Column(Modifier::new().align_items(AlignItems::Stretch).gap(6.0)).child((
        Text(format!("{label}: {value:.2}"))
            .size(14.0)
            .color(theme().on_surface_variant),
        M3Slider(value, range, step, on_change, SliderConfig::default()),
    ))
}

/// Control + trailing label row (kills the Row/HSpace/Text triplets in widgets.rs).
pub fn Labeled(control: View, label: &str) -> View {
    Row(Modifier::new().align_items(AlignItems::Center).gap(10.0)).child((control, Text(label)))
}

pub fn ShortcutHud(note: String, fired: bool) -> View {
    let th = theme();
    Box(Modifier::new()
        .absolute()
        .offset(None, None, Some(16.0), Some(16.0))
        .padding(10.0)
        .background(th.surface.with_alpha(204))
        .clip_rounded(radius::LG)
        .hit_passthrough()
        .render_z_index(1000.0))
    .child(
        Column(Modifier::new().gap(sp::XS)).child((
            Text("Shortcut Overrides")
                .size(12.0)
                .color(th.on_surface.with_alpha(204)),
            Text(note).size(12.0).color(th.on_surface.with_alpha(153)),
            if fired {
                Text("Snackbar triggered").size(12.0).color(th.primary)
            } else {
                Text("Snackbar idle")
                    .size(12.0)
                    .color(th.on_surface.with_alpha(102))
            },
        )),
    )
}
