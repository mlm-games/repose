use std::cell::RefCell;
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
use repose_ui::{Stack, ViewExt};
use serde::{Deserialize, Serialize};

use crate::pages::{self, PageCtx};
use crate::ui::{self, SettingsVm};

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
    StaggeredGrid,
    Pager,
    Dnd,
    Docking,
    Errors,
    Windows,
    M3,
    Adaptive,
}

impl Route {
    /// Single source of truth for the nav rail (display order).
    pub const ALL: [Route; 18] = [
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
            Route::StaggeredGrid => "Staggered Grid",
            Route::Pager => "Pager",
            Route::Dnd => "Drag & Drop",
            Route::Docking => "Docking",
            Route::Errors => "Errors",
            Route::Windows => "Windows",
            Route::M3 => "M3 Components",
            Route::Adaptive => "Adaptive",
        }
    }

    /// Stable identity derived from the pages
    pub fn id(self) -> u64 {
        self as u64 + 1
    }
}

fn light_theme() -> Theme {
    let mut t = Theme::default().with_colors(ColorScheme::light());
    t.scrollbar_track = t.on_surface.with_alpha(24);
    t.scrollbar_thumb = t.on_surface.with_alpha(96);
    t
}

/// One place that builds the "saved" snackbar; the undo action is shared
/// between the request and the builder instead of being constructed 3 times.
fn save_snackbar_request(snackbar: &SnackbarController) -> SnackbarRequest {
    let undo: Rc<dyn Fn()> = Rc::new({
        let s = snackbar.clone();
        move || {
            log::info!("Snackbar undo");
            s.dismiss();
        }
    });
    SnackbarRequest {
        message: "Shortcut saved".into(),
        action: Some(SnackbarAction {
            label: "Undo".into(),
            on_click: undo.clone(),
        }),
        duration_ms: 2500,
        builder: Rc::new(move || {
            material3::Snackbar(
                "Shortcut saved",
                Some(SnackbarAction {
                    label: "Undo".into(),
                    on_click: undo.clone(),
                }),
                Modifier::new(),
            )
        }),
    }
}

fn install_save_shortcut(snackbar: SnackbarController, on_fire: Rc<dyn Fn()>) {
    let mut map = shortcuts::ShortcutMap::new();
    let mut mods = Modifiers {
        command: true,
        ..Modifiers::default()
    };
    if !cfg!(target_os = "macos") {
        mods.ctrl = true;
    }
    map.insert(
        Key::Character('s'),
        mods,
        shortcuts::Action::Custom("showcase.save".into()),
    );

    scoped_effect(move || {
        let map_scope = shortcuts::InstallShortcutMap(map.clone());
        let snackbar = snackbar.clone();
        let on_fire = on_fire.clone();
        let handler_scope = shortcuts::InstallShortcutHandler(Rc::new(move |action| {
            log::info!("Shortcut action: {:?}", action);
            if matches!(action, shortcuts::Action::Custom(key) if key.as_ref() == "showcase.save") {
                on_fire();
                snackbar.show(save_snackbar_request(&snackbar));
                true
            } else {
                false
            }
        }));
        Dispose::new(move || {
            map_scope.run();
            handler_scope.run();
        })
    });
}

pub fn app(_s: &mut Scheduler) -> View {
    // App state
    let dark = remember(|| signal(true));
    let rtl = remember(|| signal(false));
    let ui_scale = remember(|| signal(1.0f32));
    let text_scale = remember(|| signal(1.0f32));

    let overlay = remember(OverlayHandle::new);
    let snackbar = remember(|| SnackbarController::new((*overlay).clone()));

    let stack = remember_back_stack(Route::Home);
    let navigator = Navigator {
        stack: (*stack).clone(),
    };

    let global_windows = remember_with_key("showcase:global_windows", || {
        RefCell::new(WindowManagerState::new())
    });

    back::set(Some(Rc::new({
        let nav = navigator.clone();
        move || nav.pop()
    })));

    let current = stack.top().map(|(_, k, _, _)| k).unwrap_or(Route::Home);

    let ctx = PageCtx {
        overlay: (*overlay).clone(),
        global_windows: global_windows.clone(),
    };
    let render = renderer(move |scope| pages::render(&ctx, *scope.key()));

    // Environment defaults
    set_theme_default(if dark.get() {
        Theme::default()
    } else {
        light_theme()
    });
    set_text_direction_default(if rtl.get() {
        TextDirection::Rtl
    } else {
        TextDirection::Ltr
    });
    set_ui_scale_default(UiScale(ui_scale.get()));
    set_text_scale_default(TextScale(text_scale.get()));

    let settings = SettingsVm {
        dark: dark.get(),
        on_dark: Rc::new({
            let s = dark.clone();
            move |v| s.set(v)
        }),
        rtl: rtl.get(),
        on_rtl: Rc::new({
            let s = rtl.clone();
            move |v| s.set(v)
        }),
        density: ui_scale.get(),
        on_density: Rc::new({
            let s = ui_scale.clone();
            move |v| s.set(v.clamp(0.75, 2.0))
        }),
        text_scale: text_scale.get(),
        on_text_scale: Rc::new({
            let s = text_scale.clone();
            move |v| s.set(v.clamp(0.75, 2.0))
        }),
    };

    let content = ui::AppShell(
        current,
        navigator,
        (*overlay).clone(),
        settings,
        NavDisplay(stack.clone(), render, None, NavTransition::default()),
    );

    let content = WindowHost(
        "showcase_global_windows",
        Modifier::new().fill_max_size(),
        global_windows,
        content,
    );
    let overlay_root = (*overlay).host(Modifier::new().fill_max_size(), content);

    // Ctrl/Cmd+S demo
    let shortcut_note = remember(|| signal("Press Ctrl+S to trigger".to_string()));
    let shortcut_fired = remember(|| signal(false));
    install_save_shortcut(
        (*snackbar).clone(),
        Rc::new({
            let note = shortcut_note.clone();
            let fired = shortcut_fired.clone();
            move || {
                note.set("Shortcut override triggered".to_string());
                fired.set(true);
            }
        }),
    );

    Stack(Modifier::new().fill_max_size()).child((
        overlay_root,
        ui::ShortcutHud(shortcut_note.get(), shortcut_fired.get()),
    ))
}
