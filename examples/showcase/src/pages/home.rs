use repose_core::prelude::*;
use repose_navigation::Navigator;
use repose_ui::*;

use crate::app::Route;
use crate::ui::{Caption, Page, SectionWith, sp};

pub fn screen(nav: Navigator<Route>) -> View {
    Page(vec![
        hero(nav.clone()),
        SectionWith(
            "Featured demos",
            None,
            FlowRow(Modifier::new().fill_max_width().gap(sp::MD)).child(
                Route::FEATURED
                    .iter()
                    .copied()
                    .map(|route| feature_card(route, nav.clone()))
                    .collect::<Vec<_>>(),
            ),
        ),
    ])
}

fn hero(nav: Navigator<Route>) -> View {
    let th = theme();

    Box(Modifier::new()
        .fill_max_width()
        .background(th.primary_container)
        .border(1.0, th.outline_variant, 32.0)
        .clip_rounded(32.0)
        .padding(sp::XXL))
    .child(
        Column(Modifier::new().gap(sp::LG).align_items(AlignItems::CENTER)).child((
            Text("Repose UI Showcase")
                .size(42.0)
                .color(th.on_primary_container),
            Text("Adaptive M3 components across desktop, web, and Android.")
                .size(16.0)
                .color(th.on_primary_container.with_alpha(210)),
            Row(Modifier::new().fill_max_width().justify_content(JustifyContent::CENTER).gap(sp::MD)).child((
                cta_button("M3 Components", {
                    let nav = nav.clone();
                    move || nav.push(Route::M3)
                }),
                cta_button("Adaptive Layout", {
                    let nav = nav.clone();
                    move || nav.push(Route::Adaptive)
                }),
            )),
        )),
    )
}

fn cta_button(label: &'static str, on_click: impl Fn() + 'static) -> View {
    let th = theme();

    Box(Modifier::new()
        .padding(sp::MD)
        .background(th.surface)
        .border(1.0, th.outline_variant, 999.0)
        .clip_rounded(999.0)
        .clickable()
        .on_pointer_down(move |_| on_click()))
    .child(Text(label).size(14.0).color(th.primary))
}

fn feature_card(route: Route, nav: Navigator<Route>) -> View {
    let th = theme();

    Box(Modifier::new()
        .key(route.id())
        .width(280.0)
        .padding(sp::LG)
        .background(th.surface_container)
        .border(1.0, th.outline_variant, 24.0)
        .clip_rounded(24.0)
        .clickable()
        .on_pointer_down(move |_| nav.push(route)))
    .child(
        Column(Modifier::new().gap(sp::SM)).child((
            Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::MD)).child((
                badge(route),
                Column(Modifier::new().gap(1.0)).child((
                    Text(route.title()).size(17.0).color(th.on_surface),
                    Caption(route.description()),
                )),
            )),
            Text("Open →").size(13.0).color(th.primary),
        )),
    )
}

fn badge(route: Route) -> View {
    let th = theme();

    Box(Modifier::new()
        .size(44.0, 44.0)
        .background(th.primary)
        .clip_rounded(16.0)
        .align_items(AlignItems::CENTER)
        .justify_content(JustifyContent::CENTER))
    .child(Text(route.badge()).size(14.0).color(th.on_primary))
}
