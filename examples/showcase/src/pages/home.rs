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
        SectionWith("Platform status", None, platform_status()),
    ])
}

fn platform_status() -> View {
    use repose_platform::AppLifecycle;

    scoped_effect(|| {
        repose_platform::set_on_lifecycle(Box::new(|state| {
            log::info!("Lifecycle: {state:?}");
            repose_core::request_frame();
        }));
        Dispose::new(|| {})
    });

    let lifecycle = match repose_platform::current_lifecycle() {
        Some(AppLifecycle::Foreground) => "Foreground",
        Some(AppLifecycle::Background) => "Background",
        None => "Unknown",
    };

    let insets = window_insets();
    let th = theme();

    let uptime_s = remember(|| signal(0u64));
    let _uptime_tick = remember(|| {
        let uptime_s = uptime_s.clone();
        repose_core::timer::interval(web_time::Duration::from_secs(1), move || {
            uptime_s.update(|n| *n += 1)
        })
    });
    let greeted = remember(|| signal(false));
    let _greet_once = remember(|| {
        let greeted = greeted.clone();
        repose_core::timer::delay(web_time::Duration::from_millis(1500), move || {
            greeted.set(true)
        })
    });

    #[allow(unused_mut)]
    let mut rows: Vec<View> = vec![
        status_row("Lifecycle", lifecycle, th.primary),
        status_row("Uptime", &format!("{} s", uptime_s.get()), th.on_surface),
        status_row(
            "Timers",
            if greeted.get() {
                "delay(1.5s) fired"
            } else {
                "waiting for delay(1.5s)..."
            },
            th.on_surface,
        ),
        status_row(
            "Insets (px)",
            &format!(
                "top {}  bottom {}  left {}  right {}  ime {}",
                insets.top as i32,
                insets.bottom as i32,
                insets.left as i32,
                insets.right as i32,
                insets.ime_bottom as i32
            ),
            th.on_surface,
        ),
        status_row(
            "Redraw",
            if cfg!(target_os = "android") {
                "reactive (opt-in continuous)"
            } else {
                "reactive"
            },
            th.on_surface,
        ),
    ];

    // Android-only: runtime toggle for `set_continuous_redraw`.
    #[cfg(target_os = "android")]
    {
        use repose_material::material3::{Switch, SwitchConfig};

        let continuous = remember(|| signal(false));
        let is_on = continuous.get();
        rows.push(
            Row(Modifier::new()
                .fill_max_width()
                .align_items(AlignItems::CENTER)
                .gap(sp::SM))
            .child((
                Text("Continuous redraw")
                    .size(14.0)
                    .color(th.on_surface_variant),
                Spacer(),
                Switch(
                    is_on,
                    {
                        let continuous = continuous.clone();
                        move |on| {
                            continuous.set(on);
                            repose_platform::android::set_continuous_redraw(on);
                        }
                    },
                    SwitchConfig::default(),
                ),
            )),
        );
    }

    Column(Modifier::new().gap(sp::SM)).with_children(rows)
}

fn status_row(label: &str, value: &str, value_color: Color) -> View {
    Row(Modifier::new()
        .fill_max_width()
        .align_items(AlignItems::CENTER)
        .gap(sp::SM))
    .child((
        Caption(label),
        Spacer(),
        Text(value).size(13.0).color(value_color).single_line(),
    ))
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
            Row(Modifier::new()
                .fill_max_width()
                .justify_content(JustifyContent::CENTER)
                .gap(sp::MD))
            .child((
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
    .child(Column(Modifier::new().gap(sp::SM)).child((
        Row(Modifier::new().align_items(AlignItems::CENTER).gap(sp::MD)).child((
            badge(route),
            Column(Modifier::new().gap(1.0)).child((
                Text(route.title()).size(17.0).color(th.on_surface),
                Caption(route.description()),
            )),
        )),
        Text("Open ->").size(13.0).color(th.primary),
    )))
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
