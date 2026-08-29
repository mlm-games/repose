//! Android/Web: `collect_screens` returns primary; `apply` is no-op like compose.
use repose_core::{Rect, Size};
use repose_ui::window_v2::{
    Screen, ScreenInsets, WindowBoundsProvider, WindowGeometryProviderScope, WindowMetrics,
    WindowPlacement, WindowScreenProviderScope, WindowState,
};

pub use repose_ui::window_v2::{
    DialogState, WindowConstraints, WindowPositionProvider, WindowScreenProvider,
    WindowSizeProvider,
};

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn collect_screens(
    event_loop: &winit::event_loop::ActiveEventLoop,
    fallback_bounds: Rect,
) -> Vec<Screen> {
    let mut screens = Vec::new();
    for monitor in event_loop.available_monitors() {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor() as f32;
        let bounds = Rect {
            x: pos.x as f32 / scale,
            y: pos.y as f32 / scale,
            w: size.width as f32 / scale,
            h: size.height as f32 / scale,
        };
        let id = monitor
            .name()
            .unwrap_or_else(|| format!("monitor-{}", screens.len()));
        screens.push(Screen::new(id, bounds, ScreenInsets::default()));
    }
    if screens.is_empty() {
        screens.push(Screen::primary(fallback_bounds));
    }
    screens
}
#[cfg(any(target_os = "android", target_arch = "wasm32"))]
pub fn collect_screens(_: (), fallback_bounds: Rect) -> Vec<Screen> {
    vec![Screen::primary(fallback_bounds)]
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn apply_window_state_to_winit(
    state: &mut WindowState,
    window: &winit::window::Window,
    event_loop: Option<&winit::event_loop::ActiveEventLoop>,
) {
    let scale = window.scale_factor() as f32;
    let inner = window.inner_size();
    let outer_pos = window
        .outer_position()
        .map(|p| {
            winit::dpi::LogicalPosition::new(p.x as f64 / scale as f64, p.y as f64 / scale as f64)
        })
        .unwrap_or(winit::dpi::LogicalPosition::new(0.0, 0.0));
    let current_bounds = Rect {
        x: outer_pos.x as f32,
        y: outer_pos.y as f32,
        w: inner.width as f32 / scale,
        h: inner.height as f32 / scale,
    };
    let screens = if let Some(el) = event_loop {
        collect_screens(el, current_bounds)
    } else {
        vec![Screen::primary(current_bounds)]
    };
    let default = screens
        .first()
        .cloned()
        .unwrap_or_else(|| Screen::primary(current_bounds));
    let screen_scope = WindowScreenProviderScope::new(screens.clone(), default.clone());
    let screen_for_metrics = screens
        .iter()
        .find(|s| state.try_screen_id().map(|id| id == s.id).unwrap_or(false))
        .cloned()
        .unwrap_or(default);
    let metrics = WindowMetrics::new(screen_for_metrics, current_bounds, ScreenInsets::default());
    let geometry_scope = WindowGeometryProviderScope::new(None, metrics, |_c| Size {
        width: current_bounds.w,
        height: current_bounds.h,
    });
    if let Some(p) = state.take_pending_screen() {
        let target = p.get_screen(&screen_scope);
        let avail = target.available_bounds();
        let x = avail.x + (avail.w - current_bounds.w) / 2.0;
        let y = avail.y + (avail.h - current_bounds.h) / 2.0;
        window.set_outer_position(winit::dpi::LogicalPosition::new(x as f64, y as f64));
    }
    if let Some(p) = state.take_pending_placement() {
        apply_placement_to_winit(window, p);
        state.on_host_placement_changed(p);
    }
    if let Some(m) = state.take_pending_minimized() {
        window.set_minimized(m);
        state.on_host_minimized_changed(m);
    }
    let pending = state.drain_pending_bounds();
    for provider in pending {
        let rect = provider.get_bounds(&geometry_scope);
        let phys_w = (rect.w * scale).round() as u32;
        let phys_h = (rect.h * scale).round() as u32;
        let phys_x = (rect.x * scale).round() as i32;
        let phys_y = (rect.y * scale).round() as i32;
        let _ =
            window.request_inner_size(winit::dpi::PhysicalSize::new(phys_w.max(1), phys_h.max(1)));
        window.set_outer_position(winit::dpi::PhysicalPosition::new(phys_x, phys_y));
        state.on_host_bounds_changed(rect, geometry_scope.window_metrics.screen.id.clone());
        if state.try_placement() != Some(WindowPlacement::Floating) {
            state.on_host_placement_changed(WindowPlacement::Floating);
        }
    }
    if !state.is_initialized {
        state.is_initialized = true;
    }
}
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn apply_placement_to_winit(window: &winit::window::Window, placement: WindowPlacement) {
    match placement {
        WindowPlacement::Floating => {
            window.set_maximized(false);
            window.set_fullscreen(None);
        }
        WindowPlacement::Maximized => {
            window.set_fullscreen(None);
            window.set_maximized(true);
        }
        WindowPlacement::Fullscreen => {
            window.set_maximized(false);
            if let Some(m) = window
                .current_monitor()
                .or_else(|| window.available_monitors().next())
            {
                window.set_fullscreen(Some(winit::window::Fullscreen::Borderless(Some(m))));
            }
        }
    }
}
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn sync_window_state_from_winit(
    state: &mut WindowState,
    window: &winit::window::Window,
    new_inner_size: Option<winit::dpi::PhysicalSize<u32>>,
    new_outer_position: Option<winit::dpi::PhysicalPosition<i32>>,
) {
    let scale = window.scale_factor() as f32;
    let size = new_inner_size.unwrap_or_else(|| window.inner_size());
    let pos = new_outer_position
        .or_else(|| window.outer_position().ok())
        .map(|p| {
            winit::dpi::LogicalPosition::new(p.x as f64 / scale as f64, p.y as f64 / scale as f64)
        })
        .unwrap_or(winit::dpi::LogicalPosition::new(0.0, 0.0));
    let bounds = Rect {
        x: pos.x as f32,
        y: pos.y as f32,
        w: size.width as f32 / scale,
        h: size.height as f32 / scale,
    };
    let screen_id = window
        .current_monitor()
        .and_then(|m| m.name())
        .unwrap_or_else(|| "primary".into());
    state.on_host_bounds_changed(bounds, screen_id);
}
pub fn resolve_initial_bounds(
    provider: &WindowBoundsProvider,
    screen: Screen,
    current_bounds: Rect,
    measure_content: impl Fn(WindowConstraints) -> Size,
) -> Rect {
    let metrics = WindowMetrics::new(screen, current_bounds, ScreenInsets::default());
    let scope = WindowGeometryProviderScope::new(None, metrics, measure_content);
    provider.get_bounds(&scope)
}
