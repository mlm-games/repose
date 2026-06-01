use std::rc::Rc;

use repose_core::{
    ColorScheme, TextDirection, prelude::*, set_text_direction_default, set_text_scale_default,
    set_theme_default, set_ui_scale_default, shortcuts, signal,
};
use repose_material::material3;
use repose_navigation::{
    NavDisplay, NavTransition, Navigator, back, remember_back_stack, renderer,
};
use repose_ui::overlay::{OverlayHandle, SnackbarAction, SnackbarController, SnackbarRequest};
use repose_ui::windowing::{WindowHost, WindowManagerState};
use repose_ui::{Box, Column, Stack, Text, TextStyle, ViewExt};
use serde::{Deserialize, Serialize};

use crate::{pages, ui};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Home,
    Layout,
    Widgets,
    Text,
    Scroll,
    ScrollFeatures,
    Canvas,
    Animation,
    Lists,
    Grid,
    Pager,
    Dnd,
    Docking,
    Errors,
    Windows,
    M3,
    Adaptive,
}

impl Route {
    pub fn title(self) -> &'static str {
        match self {
            Route::Home => "Home",
            Route::Layout => "Layout",
            Route::Widgets => "Widgets",
            Route::Text => "Text",
            Route::Scroll => "Scroll",
            Route::ScrollFeatures => "Scroll Features",
            Route::Canvas => "Canvas",
            Route::Animation => "Animation",
            Route::Lists => "Lists",
            Route::Grid => "Grid",
            Route::Pager => "Pager",
            Route::Dnd => "Drag & Drop",
            Route::Docking => "Docking",
            Route::Errors => "Errors",
            Route::Windows => "Windows",
            Route::M3 => "M3 Components",
            Route::Adaptive => "Adaptive",
        }
    }
    pub fn id(self) -> u64 {
        match self {
            Route::Home => 1,
            Route::Layout => 2,
            Route::Widgets => 3,
            Route::Text => 4,
            Route::Scroll => 5,
            Route::ScrollFeatures => 6,
            Route::Canvas => 7,
            Route::Animation => 8,
            Route::Lists => 9,
            Route::Grid => 10,
            Route::Pager => 11,
            Route::Dnd => 12,
            Route::Docking => 13,
            Route::Errors => 14,
            Route::Windows => 15,
            Route::M3 => 16,
            Route::Adaptive => 17,
        }
    }
}

pub fn app(_s: &mut Scheduler) -> View {
    // App state
    let dark = remember(|| signal(true));
    let rtl = remember(|| signal(false));

    let ui_scale = remember(|| signal(1.0f32)); // extra scale multiplier
    let text_scale = remember(|| signal(1.0f32)); // font multiplier

    let overlay = remember(|| OverlayHandle::new());
    let snackbar = remember(|| SnackbarController::new((*overlay).clone()));

    // Theme presets
    let theme_light = {
        let mut t = Theme::default().with_colors(ColorScheme::light());
        t.scrollbar_track = t.on_surface.with_alpha(24);
        t.scrollbar_thumb = t.on_surface.with_alpha(96);
        t
    };
    let theme_dark = Theme::default();

    let stack = remember_back_stack(Route::Home);
    let navigator = Navigator {
        stack: (*stack).clone(),
    };

    let global_windows = remember_with_key("showcase:global_windows", || {
        std::cell::RefCell::new(WindowManagerState::new())
    });

    // Back handler: set each frame (simple + robust).
    back::set(Some(Rc::new({
        let nav = navigator.clone();
        move || nav.pop()
    })));

    let current = stack
        .top()
        .map(|(_, k, _saved, _scope)| k)
        .unwrap_or(Route::Home);

    // Typed route -> page renderer
    let overlay_clone = overlay.clone();
    let render = renderer({
        let global_windows = global_windows.clone();
        let overlay = overlay_clone;
        move |scope| match *scope.key() {
            Route::Home => pages::home::screen(),
            Route::Layout => pages::layout::screen(),
            Route::Widgets => pages::widgets::screen(),
            Route::Text => pages::text::screen(),
            Route::Scroll => pages::scroll::screen(),
            Route::ScrollFeatures => pages::scroll_features::screen(),
            Route::Canvas => pages::canvas::screen(),
            Route::Animation => pages::animation::screen(),
            Route::Lists => pages::lists::screen(),
            Route::Grid => pages::grid::screen(),
            Route::Pager => pages::pager::screen(),
            Route::Dnd => pages::dnd::screen(),
            Route::Docking => pages::docking::screen(),
            Route::Errors => pages::errors::screen(),
            Route::Windows => pages::windows::screen(global_windows.clone()),
            Route::M3 => pages::m3::screen((*overlay).clone()),
            Route::Adaptive => pages::adaptive::screen(),
        }
    });

    let dir = if rtl.get() {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    };

    let chosen_theme = if dark.get() { theme_dark } else { theme_light };

    set_theme_default(chosen_theme);
    set_text_direction_default(dir);
    set_ui_scale_default(UiScale(ui_scale.get()));
    set_text_scale_default(TextScale(text_scale.get()));

    let content = ui::AppShell(
        current,
        navigator.clone(),
        (*overlay).clone(),
        dark.get(),
        {
            let dark = dark.clone();
            move |v| dark.set(v)
        },
        rtl.get(),
        {
            let rtl = rtl.clone();
            move |v| rtl.set(v)
        },
        ui_scale.get(),
        {
            let ui_scale = ui_scale.clone();
            move |v| ui_scale.set(v.clamp(0.75, 2.0))
        },
        text_scale.get(),
        {
            let text_scale = text_scale.clone();
            move |v| text_scale.set(v.clamp(0.75, 2.0))
        },
        NavDisplay(
            stack.clone(),
            render.clone(),
            None,
            NavTransition::default(),
        ),
    );

    let content = WindowHost(
        "showcase_global_windows",
        Modifier::new().fill_max_size(),
        global_windows,
        content,
    );

    let overlay_root = (*overlay).host(Modifier::new().fill_max_size(), content);

    let snackbar = snackbar.clone();
    let shortcut_note = remember(|| signal("Press Ctrl+S to trigger".to_string()));
    let shortcut_demo = remember(|| signal(false));

    let mut map = shortcuts::ShortcutMap::new();
    let mut save_mods = Modifiers {
        command: true,
        ..Modifiers::default()
    };
    if !cfg!(target_os = "macos") {
        save_mods.ctrl = true;
    }
    map.insert(
        Key::Character('s'),
        save_mods,
        shortcuts::Action::Custom("showcase.save".into()),
    );
    let snackbar = snackbar.clone();
    let shortcut_note = shortcut_note.clone();
    let shortcut_demo = shortcut_demo.clone();
    let shortcut_note_clone = shortcut_note.clone();
    let shortcut_demo_clone = shortcut_demo.clone();
    scoped_effect(move || {
        let _map_scope = shortcuts::InstallShortcutMap(map.clone());
        let snackbar_for_handler = snackbar.clone();
        let _handler_scope = shortcuts::InstallShortcutHandler(Rc::new(move |action| {
            log::info!("Shortcut action: {:?}", action);
            if matches!(action, shortcuts::Action::Custom(key) if key.as_ref() == "showcase.save") {
                shortcut_note_clone.set("Shortcut override triggered".to_string());
                shortcut_demo_clone.set(true);
                let snackbar_for_action = snackbar_for_handler.clone();
                let snackbar_for_builder = snackbar_for_handler.clone();
                snackbar_for_handler.show(SnackbarRequest {
                    message: "Shortcut saved".to_string(),
                    action: Some(SnackbarAction {
                        label: "Undo".to_string(),
                        on_click: Rc::new(move || {
                            log::info!("Snackbar undo");
                            snackbar_for_action.dismiss();
                        }),
                    }),
                    duration_ms: 2500,
                    builder: Rc::new(move || {
                        let snackbar_dismiss = snackbar_for_builder.clone();
                        material3::Snackbar(
                            "Shortcut saved",
                            Some(SnackbarAction {
                                label: "Undo".to_string(),
                                on_click: Rc::new(move || {
                                    log::info!("Snackbar undo");
                                    snackbar_dismiss.dismiss();
                                }),
                            }),
                            Modifier::new(),
                        )
                    }),
                });
                true
            } else {
                false
            }
        }));
        Dispose::new(move || {
            _map_scope.run();
            _handler_scope.run();
        })
    });

    let overlay_root = Stack(Modifier::new().fill_max_size()).child((
        overlay_root,
        {
            let th = theme();
            Box(Modifier::new()
                .absolute()
                .offset(None, None, Some(16.0), Some(16.0))
                .padding(10.0)
                .background(th.surface.with_alpha(204))
                .clip_rounded(12.0)
                .hit_passthrough()
                .render_z_index(1000.0)) // Render on top of scroll content
        }
        .child(
            Column(Modifier::new()).child(
                std::iter::once(
                    Text("Shortcut Overrides")
                        .size(12.0)
                        .color(theme().on_surface.with_alpha(204)),
                )
                .chain(std::iter::once(Box(Modifier::new().size(1.0, 6.0))))
                .chain(std::iter::once(
                    Text(shortcut_note.get())
                        .size(12.0)
                        .color(theme().on_surface.with_alpha(153)),
                ))
                .chain(std::iter::once(Box(Modifier::new().size(1.0, 4.0))))
                .chain(std::iter::once(if shortcut_demo.get() {
                    Text("Snackbar triggered").size(12.0).color(theme().primary)
                } else {
                    Text("Snackbar idle")
                        .size(12.0)
                        .color(theme().on_surface.with_alpha(102))
                }))
                .collect::<Vec<_>>(),
            ),
        ),
    ));

    overlay_root
}
