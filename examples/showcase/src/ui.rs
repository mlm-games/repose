#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::dialog::{Dialog, DialogProperties, DialogState};
use repose_material::material3::{
    Card, CardConfig, ElevatedCard, IconButton, IconButtonConfig, Slider, SliderConfig, Switch,
    SwitchConfig,
};
use repose_material::{Icon, material_symbols};
use repose_navigation::Navigator;
use repose_ui::overlay::OverlayHandle;
use repose_ui::scroll::{
    HorizontalScrollArea, ScrollArea, remember_horizontal_scroll_state, remember_scroll_state,
};
use repose_ui::*;

use crate::app::{Route, RouteGroup};

material_symbols! {
    settings : '\u{E8B8}',
}

pub mod sp {
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

pub mod radius {
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 18.0;
    pub const XL: f32 = 28.0;
}

pub fn VSpace(h: f32) -> View {
    Box(Modifier::new().height(h).width(1.0))
}

pub fn HSpace(w: f32) -> View {
    Box(Modifier::new().width(w).height(1.0))
}

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

pub fn Pill(label: impl Into<String>, bg: Color, fg: Color) -> View {
    Box(Modifier::new()
        .padding(8.0)
        .background(bg)
        .clip_rounded(999.0))
    .child(Text(label.into()).size(12.0).color(fg).single_line())
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
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER),
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

    let mut header: Vec<View> = vec![
        Text(title).size(18.0).color(th.on_surface),
    ];

    if let Some(s) = subtitle {
        header.push(Hint(s));
    }

    Column(Modifier::new().gap(sp::SM)).child((
        Column(Modifier::new().gap(2.0).padding(sp::XS)).with_children(header),
        ElevatedCard(
            CardConfig {
                modifier: Modifier::new()
                    .fill_max_width()
                    .padding(sp::LG),
                ..Default::default()
            },
            || Column(Modifier::new().fill_max_size()).child(body),
        ),
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
    let class = window_size_class();
    let compact = !class.is_at_least_medium_width();

    let shell = if compact {
        Column(Modifier::new().fill_max_size()).child((
            TopBar(current, overlay, settings, true),
            CompactNav(current, nav),
            PageViewport(current, content, true),
        ))
    } else {
        Column(Modifier::new().fill_max_size()).child((
            TopBar(current, overlay, settings, false),
            Row(Modifier::new().fill_max_size()).child((
                NavRail(current, nav),
                PageViewport(current, content, false),
            )),
        ))
    };

    Box(Modifier::new()
        .fill_max_size()
        .background(theme().background))
    .child(shell)
}

fn PageViewport(current: Route, content: View, compact: bool) -> View {
    let scroll = remember_scroll_state(format!("shell:page-scroll:{:?}", current));
    scroll.set_show_scrollbar(false);

    let mut children: Vec<View> = Vec::new();

    if current != Route::Home {
        children.push(PageHero(current, compact));
    }

    children.push(content);

    ScrollArea(
        Modifier::new()
            .fill_max_size()
            .padding(if compact { sp::MD } else { sp::XL }),
        scroll,
        Column(Modifier::new()
            .fill_max_width()
            .max_width(1180.0)
            .gap(if compact { sp::MD } else { sp::XL }))
        .with_children(children),
    )
}

fn PageHero(route: Route, compact: bool) -> View {
    let th = theme();

    let title_block = Column(Modifier::new().gap(sp::SM)).child((
        Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::SM)).child((
            RouteBadge(route, true),
            Pill(
                route.group().title(),
                th.primary.with_alpha(24),
                th.primary,
            ),
        )),
        Text(route.title())
            .size(if compact { 28.0 } else { 36.0 })
            .color(th.on_surface),
        Text(route.description())
            .size(if compact { 14.0 } else { 15.0 })
            .color(th.on_surface_variant),
    ));

    Box(Modifier::new()
        .fill_max_width()
        .background(th.surface_container_low)
        .border(1.0, th.outline_variant, radius::XL)
        .clip_rounded(radius::XL)
        .padding(if compact { sp::LG } else { sp::XL }))
    .child(title_block)
}

pub fn TopBar(
    current: Route,
    overlay: OverlayHandle,
    vm: SettingsVm,
    compact: bool,
) -> View {
    let settings_state = remember(DialogState::new);
    let th = theme();

    Box(Modifier::new()
        .fill_max_width()
        .padding(if compact { sp::SM } else { sp::MD }))
    .child(
        Row(Modifier::new()
            .fill_max_width()
            .height(64.0)
            .padding(sp::MD)
            .background(th.surface_container)
            .border(1.0, th.outline_variant, radius::XL)
            .clip_rounded(radius::XL)
            .align_items(AlignItems::CENTER)
            .gap(sp::MD))
        .child((
            BrandBlock(current, compact),
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
                DialogProperties {
                    ..Default::default()
                },
                SettingsPanel(vm),
            ),
        )),
    )
}

fn BrandBlock(current: Route, compact: bool) -> View {
    let th = theme();

    let text_children = if compact {
        vec![
            Text(current.title()).size(17.0).color(th.on_surface),
            Text("Repose Showcase")
                .size(12.0)
                .color(th.on_surface_variant),
        ]
    } else {
        vec![
            Text("Repose UI").size(18.0).color(th.on_surface),
            Text("M3 multiplatform showcase")
                .size(12.0)
                .color(th.on_surface_variant),
        ]
    };

    Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::MD)).child((
        Box(Modifier::new()
            .size(40.0, 40.0)
            .background(th.primary)
            .clip_rounded(14.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER))
        .child(Text("R").size(18.0).color(th.on_primary)),
        Column(Modifier::new().gap(1.0)).with_children(text_children),
    ))
}

fn SettingsPanel(vm: SettingsVm) -> View {
    let th = theme();

    Column(Modifier::new()
        .padding(sp::XL)
        .min_width(340.0)
        .max_width(440.0)
        .gap(sp::LG))
    .child((
        Text("Settings").size(22.0).color(th.on_surface),
        Column(Modifier::new().gap(sp::MD)).child((
            LabeledSwitch("Dark mode", vm.dark, {
                let f = vm.on_dark.clone();
                move |v| f(v)
            }),
            LabeledSwitch("RTL layout", vm.rtl, {
                let f = vm.on_rtl.clone();
                move |v| f(v)
            }),
            LabeledSlider("Density", vm.density, (0.75, 2.0), Some(0.05), {
                let f = vm.on_density.clone();
                move |v| f(v)
            }),
            LabeledSlider("Text scale", vm.text_scale, (0.75, 2.0), Some(0.05), {
                let f = vm.on_text_scale.clone();
                move |v| f(v)
            }),
        )),
    ))
}

pub fn NavRail(current: Route, nav: Navigator<Route>) -> View {
    let th = theme();
    let scroll = remember_scroll_state("shell:nav-rail");
    scroll.set_show_scrollbar(false);

    let mut items: Vec<View> = Vec::new();

    for group in RouteGroup::ALL {
        items.push(NavGroupLabel(group));
        for r in Route::ALL.iter().copied().filter(|r| r.group() == group) {
            items.push(NavItem(r, r == current, {
                let nav = nav.clone();
                move || nav.push(r)
            }));
        }
    }

    Card(
        CardConfig {
            modifier: Modifier::new()
                .width(292.0)
                .fill_max_height()
                .padding(sp::SM),
            ..Default::default()
        },
        || {
            ScrollArea(
                Modifier::new().fill_max_size(),
                scroll,
                Column(Modifier::new().fill_max_width().gap(2.0)).with_children(items),
            )
        },
    )
}

fn NavGroupLabel(group: RouteGroup) -> View {
    let th = theme();

    Column(Modifier::new()
        .fill_max_width()
        .padding(sp::MD)
        .gap(1.0))
    .child((
        Text(group.title())
            .size(12.0)
            .color(th.primary),
        Text(group.subtitle())
            .size(11.0)
            .color(th.on_surface_variant),
    ))
}

fn NavItem(route: Route, selected: bool, on_click: impl Fn() + 'static) -> View {
    let th = theme();

    let (bg, fg, sub, stroke) = if selected {
        (
            th.secondary_container,
            th.on_secondary_container,
            th.on_secondary_container.with_alpha(190),
            th.primary,
        )
    } else {
        (
            th.surface,
            th.on_surface,
            th.on_surface_variant,
            th.outline_variant.with_alpha(0),
        )
    };

    Box(Modifier::new()
        .key(route.id())
        .fill_max_width()
        .padding(sp::SM)
        .background(bg)
        .border(1.0, stroke, radius::LG)
        .clip_rounded(radius::LG)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .gap(sp::MD))
        .child((
            RouteBadge(route, selected),
            Column(Modifier::new().gap(2.0).flex_grow(1.0)).child((
                Text(route.title())
                    .size(14.0)
                    .color(fg)
                    .single_line()
                    .overflow_ellipsize(),
                Text(route.description())
                    .size(11.0)
                    .color(sub)
                    .single_line()
                    .overflow_ellipsize(),
            )),
        )),
    )
}

fn RouteBadge(route: Route, selected: bool) -> View {
    let th = theme();
    let bg = if selected {
        th.primary
    } else {
        th.surface_container_high
    };
    let fg = if selected {
        th.on_primary
    } else {
        th.on_surface_variant
    };

    Box(Modifier::new()
        .size(38.0, 38.0)
        .background(bg)
        .clip_rounded(14.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER)
        .flex_shrink(0.0))
    .child(Text(route.badge()).size(13.0).color(fg))
}

fn CompactNav(current: Route, nav: Navigator<Route>) -> View {
    let scroll = remember_horizontal_scroll_state("shell:compact-nav");

    HorizontalScrollArea(
        Modifier::new()
            .fill_max_width()
            .height(64.0),
        scroll,
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .gap(sp::SM)
            .padding(sp::SM))
        .child(
            Route::ALL
                .iter()
                .copied()
                .map(|r| {
                    CompactNavChip(r, r == current, {
                        let nav = nav.clone();
                        move || nav.push(r)
                    })
                })
                .collect::<Vec<_>>(),
        ),
    )
}

fn CompactNavChip(route: Route, selected: bool, on_click: impl Fn() + 'static) -> View {
    let th = theme();
    let bg = if selected {
        th.primary
    } else {
        th.surface_container_high
    };
    let fg = if selected {
        th.on_primary
    } else {
        th.on_surface_variant
    };

    Box(Modifier::new()
        .padding(sp::MD)
        .background(bg)
        .clip_rounded(999.0)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .gap(sp::SM))
        .child((
            Text(route.badge()).size(12.0).color(fg),
            Text(route.title()).size(13.0).color(fg).single_line(),
        )),
    )
}

pub fn LabeledSwitch(label: &str, checked: bool, on_change: impl Fn(bool) + 'static) -> View {
    Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER)
        .gap(sp::SM))
    .child((
        Text(label).size(14.0).color(theme().on_surface_variant),
        Spacer(),
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
    Column(Modifier::new().align_items(AlignItems::STRETCH).gap(6.0)).child((
        Row(Modifier::new().align_items(AlignItems::CENTER)).child((
            Text(label)
                .size(14.0)
                .color(theme().on_surface_variant),
            Spacer(),
            Text(format!("{value:.2}"))
                .size(13.0)
                .color(theme().on_surface_variant),
        )),
        Slider(value, range, step, on_change, SliderConfig::default()),
    ))
}

/// Control + trailing label row.
pub fn Labeled(control: View, label: &str) -> View {
    Row(Modifier::new().align_items(AlignItems::CENTER).gap(10.0)).child((
        control,
        Text(label).size(14.0).color(theme().on_surface),
    ))
}

pub fn ShortcutHud(note: String, fired: bool) -> View {
    let th = theme();

    Box(Modifier::new()
        .absolute()
        .offset(None, None, Some(16.0), Some(16.0))
        .padding(10.0)
        .background(th.surface_container_high.with_alpha(232))
        .border(1.0, th.outline_variant, radius::LG)
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