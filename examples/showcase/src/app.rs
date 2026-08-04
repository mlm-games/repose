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
use repose_ui::{Column, ViewExt};
use serde::{Deserialize, Serialize};

use crate::pages::{self, PageCtx};
use crate::ui::{self, SettingsVm};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteGroup {
    Overview,
    Material,
    Foundation,
    Collections,
    Interaction,
    Platform,
}

impl RouteGroup {
    pub const ALL: [RouteGroup; 6] = [
        RouteGroup::Overview,
        RouteGroup::Material,
        RouteGroup::Foundation,
        RouteGroup::Collections,
        RouteGroup::Interaction,
        RouteGroup::Platform,
    ];

    pub fn title(self) -> &'static str {
        match self {
            RouteGroup::Overview => "Overview",
            RouteGroup::Material => "Material 3",
            RouteGroup::Foundation => "Foundation",
            RouteGroup::Collections => "Collections",
            RouteGroup::Interaction => "Interaction",
            RouteGroup::Platform => "Platform",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            RouteGroup::Overview => "Start here",
            RouteGroup::Material => "M3 controls",
            RouteGroup::Foundation => "Layout, text",
            RouteGroup::Collections => "Lazy lists",
            RouteGroup::Interaction => "Motion, input",
            RouteGroup::Platform => "Desktop surfaces",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Route {
    Home,
    Layout,
    Widgets,
    Text,
    Scroll,
    ScrollFeatures,
    Canvas,
    VectorMesh,
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
    /// Single source of truth for display order.
    pub const ALL: [Route; 19] = [
        Route::Home,
        Route::M3,
        Route::Widgets,
        Route::Adaptive,
        Route::Layout,
        Route::Text,
        Route::Canvas,
        Route::VectorMesh,
        Route::Lists,
        Route::Grid,
        Route::StaggeredGrid,
        Route::Pager,
        Route::Scroll,
        Route::ScrollFeatures,
        Route::Animation,
        Route::Dnd,
        Route::Docking,
        Route::Windows,
        Route::Errors,
    ];

    pub const FEATURED: [Route; 6] = [
        Route::M3,
        Route::Widgets,
        Route::Adaptive,
        Route::Animation,
        Route::Docking,
        Route::Windows,
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
            Route::VectorMesh => "Vector Mesh",
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

    pub fn description(self) -> &'static str {
        match self {
            Route::Home => "Overview.",
            Route::Layout => "Rows, columns, grids, shadows.",
            Route::Widgets => "Switches, sliders, chips, focus.",
            Route::Text => "Fields, spans, selection, symbols.",
            Route::Scroll => "Vertical and horizontal scroll.",
            Route::ScrollFeatures => "Overscroll, pull-to-refresh.",
            Route::Canvas => "Drawing primitives and scenes.",
            Route::VectorMesh => "Tessellated meshes, gradients, vector clips.",
            Route::Animation => "Springs, tweens, crossfades.",
            Route::Lists => "Lazy columns, carousel, swipe.",
            Route::Grid => "Virtualized vertical grids.",
            Route::StaggeredGrid => "Pinterest-style lazy grid.",
            Route::Pager => "Horizontal and vertical paging.",
            Route::Dnd => "Internal drag/drop and files.",
            Route::Docking => "Dockable tabs and split panels.",
            Route::Errors => "Error boundaries and recovery.",
            Route::Windows => "Floating windows and palettes.",
            Route::M3 => "Menus, sheets, rails, pickers.",
            Route::Adaptive => "Responsive list/detail panes.",
        }
    }

    pub fn group(self) -> RouteGroup {
        match self {
            Route::Home => RouteGroup::Overview,
            Route::M3 | Route::Widgets => RouteGroup::Material,
            Route::Adaptive | Route::Layout | Route::Text | Route::Canvas | Route::VectorMesh => {
                RouteGroup::Foundation
            }
            Route::Lists
            | Route::Grid
            | Route::StaggeredGrid
            | Route::Pager
            | Route::Scroll
            | Route::ScrollFeatures => RouteGroup::Collections,
            Route::Animation | Route::Dnd => RouteGroup::Interaction,
            Route::Docking | Route::Windows | Route::Errors => RouteGroup::Platform,
        }
    }

    pub fn badge(self) -> &'static str {
        match self {
            Route::Home => "R",
            Route::Layout => "Ly",
            Route::Widgets => "Ui",
            Route::Text => "Aa",
            Route::Scroll => "Sc",
            Route::ScrollFeatures => "Fx",
            Route::Canvas => "Cv",
            Route::VectorMesh => "Vm",
            Route::Animation => "Mo",
            Route::Lists => "Ls",
            Route::Grid => "Gr",
            Route::StaggeredGrid => "Sg",
            Route::Pager => "Pg",
            Route::Dnd => "Dn",
            Route::Docking => "Dk",
            Route::Errors => "Er",
            Route::Windows => "Wn",
            Route::M3 => "M3",
            Route::Adaptive => "Ad",
        }
    }

    /// Stable identity derived from the pages.
    pub fn id(self) -> u64 {
        self as u64 + 1
    }
}

fn light_theme() -> Theme {
    Theme::default().with_colors(ColorScheme::light())
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
                material3::SnackbarConfig::default(),
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
        nav: navigator.clone(),
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

    Column(Modifier::new().fill_max_size()).child((
        overlay_root,
        ui::ShortcutHud(shortcut_note.get(), shortcut_fired.get()),
    ))
}
