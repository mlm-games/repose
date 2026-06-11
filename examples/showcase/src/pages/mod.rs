use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_ui::overlay::OverlayHandle;
use repose_ui::windowing::WindowManagerState;

use crate::app::Route;

pub mod adaptive;
pub mod animation;
pub mod canvas;
pub mod dnd;
pub mod docking;
pub mod errors;
pub mod grid;
pub mod home;
pub mod layout;
pub mod lists;
pub mod m3;
pub mod pager;
pub mod scroll;
pub mod scroll_features;
pub mod staggered_grid;
pub mod text;
pub mod widgets;
pub mod windows;

/// Everything a page might need from the app shell.
#[derive(Clone)]
pub struct PageCtx {
    pub overlay: OverlayHandle,
    pub global_windows: Rc<RefCell<WindowManagerState>>,
}

pub fn render(ctx: &PageCtx, route: Route) -> View {
    match route {
        Route::Home => home::screen(),
        Route::Layout => layout::screen(),
        Route::Widgets => widgets::screen(),
        Route::Text => text::screen(),
        Route::Scroll => scroll::screen(),
        Route::ScrollFeatures => scroll_features::screen(),
        Route::Canvas => canvas::screen(),
        Route::Animation => animation::screen(),
        Route::Lists => lists::screen(),
        Route::Grid => grid::screen(),
        Route::StaggeredGrid => staggered_grid::screen(),
        Route::Pager => pager::screen(),
        Route::Dnd => dnd::screen(),
        Route::Docking => docking::screen(),
        Route::Errors => errors::screen(),
        Route::Windows => windows::screen(ctx.global_windows.clone()),
        Route::M3 => m3::screen(ctx.overlay.clone()),
        Route::Adaptive => adaptive::screen(),
    }
}
