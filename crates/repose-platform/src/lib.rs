//! Platform runners
use crate::a11y::ReposeActionHandler;
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use accesskit_winit::Adapter;
use repose_core::locals::dp_to_px;
use repose_core::*;
use repose_ui::textfield::{
    self, TF_FONT_DP, TF_PADDING_X_DP, TextFieldState, caret_xy_for_byte, measure_text,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use web_time::Instant;

#[cfg(target_os = "android")]
pub mod android;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub mod a11y;
mod common;
#[cfg(not(target_arch = "wasm32"))]
mod common_android;
mod common_web;
pub mod render;

use common as rc;
#[cfg(not(target_arch = "wasm32"))]
use common_android as rc_android;
use common_web as rc_web;

pub use render::{ImageHandleGuard, RenderCommand, RenderContext};

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
use winit::window::Window;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static APP_WINDOW: OnceLock<Arc<Window>> = OnceLock::new();

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static CLOSE_TO_TRAY: AtomicBool = AtomicBool::new(false);

#[cfg(not(target_arch = "wasm32"))]
static EVENT_LOOP_PROXY: OnceLock<winit::event_loop::EventLoopProxy<()>> = OnceLock::new();

/// Optional callback invoked on every AboutToWait, regardless of redraw state.
/// Used for draining cross-thread commands (e.g. tray toggles) that must be
/// processed even when the window is hidden.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
static ABOUT_TO_WAIT_CALLBACK: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

static DEEPLINK_CB: Mutex<Option<Box<dyn Fn(Vec<u8>) + Send>>> = Mutex::new(None);
static PENDING_DEEPLINKS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Register a callback to receive deeplink payloads (raw bytes)
pub fn set_on_deeplink(callback: Box<dyn Fn(Vec<u8>) + Send>) {
    *DEEPLINK_CB.lock().unwrap() = Some(callback);
}

/// Push a deeplink payload from any thread (JNI callback, CLI watcher, etc).
pub fn push_deeplink(data: Vec<u8>) {
    PENDING_DEEPLINKS.lock().unwrap().push(data);
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(proxy) = EVENT_LOOP_PROXY.get() {
        let _ = proxy.send_event(());
    }
}

/// Drain queued deeplinks and dispatch them to the registered callback.
/// Called from each platform runner's `about_to_wait` handler.
pub(crate) fn process_deeplinks() {
    let mut queue = PENDING_DEEPLINKS.lock().unwrap();
    if queue.is_empty() {
        return;
    }
    let batch = std::mem::take(&mut *queue);
    drop(queue);

    if let Some(cb) = DEEPLINK_CB.lock().unwrap().as_ref() {
        for data in batch {
            cb(data);
        }
    }
}

/// Store the application window handle (called once during app setup).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_app_window(window: Arc<Window>) {
    let _ = APP_WINDOW.set(window);
}

/// Store the event loop proxy so tray commands / deeplinks can wake the event loop.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_event_loop_proxy(proxy: winit::event_loop::EventLoopProxy<()>) {
    let _ = EVENT_LOOP_PROXY.set(proxy);
}

/// Register a callback invoked on every AboutToWait (used for draining tray commands).
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_about_to_wait_callback(cb: Box<dyn Fn() + Send>) {
    *ABOUT_TO_WAIT_CALLBACK.lock().unwrap() = Some(cb);
}

/// Wake the winit event loop from another thread (e.g. tray's GTK thread, JNI callback).
#[cfg(not(target_arch = "wasm32"))]
pub fn wake_event_loop() {
    if let Some(proxy) = EVENT_LOOP_PROXY.get() {
        let _ = proxy.send_event(());
    }
}

/// Show the application window.
///
/// On Wayland, unminimizing might not be supported by the protocol?
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn show_app_window() {
    WINDOW_VISIBLE.store(true, Ordering::Relaxed);
    if let Some(w) = APP_WINDOW.get() {
        log::info!("show_app_window: calling set_visible(true)");
        w.set_visible(true);
        #[allow(deprecated)]
        w.focus_window();
    }
    repose_core::frame_clock::request_frame();
    wake_event_loop();
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn hide_app_window() {
    WINDOW_VISIBLE.store(false, Ordering::Relaxed);
    if let Some(w) = APP_WINDOW.get() {
        log::info!("hide_app_window: calling set_visible(false)");
        w.set_visible(false);
    }
    repose_core::frame_clock::request_frame();
    wake_event_loop();
}

/// Returns whether the application window is currently visible.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn window_is_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Relaxed)
}

/// The close button hides the window (via ``set_visible(false)``) instead of
/// closing. The tray "Quit" action still exits the process regardless.
#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn set_close_to_tray(enabled: bool) {
    CLOSE_TO_TRAY.store(enabled, Ordering::Relaxed);
}

/// Compose a single frame with density and text-scale applied, returning Frame.
pub fn compose_frame<F>(
    sched: &mut Scheduler,
    root_fn: &mut F,
    scale: f32,
    size_px_u32: (u32, u32),
    hover_id: Option<u64>,
    pressed_ids: &std::collections::HashSet<u64>,
    tf_states: &std::collections::HashMap<u64, Rc<RefCell<repose_ui::TextFieldState>>>,
    _focused: Option<u64>,
) -> Frame
where
    F: FnMut(&mut Scheduler) -> View,
{
    // Process any programmatic focus request from FocusRequester
    if let Some(requested_id) = repose_core::take_focus_request() {
        if requested_id == repose_core::runtime::CLEAR_FOCUS_MARKER {
            sched.focused = None;
        } else {
            sched.focused = Some(requested_id);
        }
    }

    set_density_default(Density { scale });

    // Use scheduler's focused state (which may have been updated by focus request)
    let current_focused = sched.focused;

    let frame = sched.repose(
        {
            let scale = scale;
            move |s: &mut Scheduler| with_density(Density { scale }, || (root_fn)(s))
        },
        {
            let hover_id = hover_id;
            let pressed_ids = pressed_ids.clone();
            move |view, _size| {
                let interactions = repose_ui::Interactions {
                    hover: hover_id,
                    pressed: pressed_ids.clone(),
                };

                with_density(Density { scale }, || {
                    repose_ui::layout_and_paint(
                        view,
                        size_px_u32,
                        tf_states,
                        &interactions,
                        current_focused,
                    )
                })
            }
        },
    );

    if let Some(fid) = sched.focused {
        if !frame.focus_chain.contains(&fid) {
            sched.focused = None;
        }
    }

    frame
}

/// Helper: ensure caret visibility for a TextFieldState inside a given rect (px).
pub fn tf_ensure_visible_in_rect(state: &mut repose_ui::TextFieldState, inner_rect: Rect) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let m = measure_text(&state.text, font_px, None, 400, 0);
    let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
    state.ensure_caret_visible(
        caret_x_px,
        inner_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
        dp_to_px(2.0),
    );
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
fn map_cursor(c: repose_core::CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
    match c {
        repose_core::CursorIcon::Default => W::Default,
        repose_core::CursorIcon::Pointer => W::Pointer,
        repose_core::CursorIcon::Text => W::Text,
        repose_core::CursorIcon::EwResize => W::EwResize,
        repose_core::CursorIcon::NsResize => W::NsResize,
        repose_core::CursorIcon::Grab => W::Grab,
        repose_core::CursorIcon::Grabbing => W::Grabbing,
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn run_desktop_app(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
) -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{Window, WindowAttributes};

    use crate::a11y::A11yTree;

    struct ReposeActivationHandler {
        initial_tree: Option<accesskit::TreeUpdate>,
    }

    impl accesskit::ActivationHandler for ReposeActivationHandler {
        fn request_initial_tree(&mut self) -> Option<accesskit::TreeUpdate> {
            self.initial_tree.take()
        }
    }

    struct ReposeDeactivationHandler;

    impl accesskit::DeactivationHandler for ReposeDeactivationHandler {
        fn deactivate_accessibility(&mut self) {
            // Nothing to clean up for now
        }
    }

    struct App {
        root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>,
        render: RenderContext,
        window: Option<Arc<Window>>,
        backend: Option<repose_render_wgpu::WgpuBackend>,
        sched: Scheduler,
        inspector: repose_devtools::Inspector,
        frame_cache: Option<Frame>,
        mouse_pos_px: (f32, f32),
        modifiers: Modifiers,
        textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
        ime_preedit: bool,
        hover_id: Option<u64>,
        capture_id: Option<u64>,
        pressed_ids: HashSet<u64>,

        // Files
        pending_dropped_files: Vec<std::path::PathBuf>,
        pending_drop_pos_px: Option<(f32, f32)>,

        // External file drag hover (HoveredFile / Cancelled)
        external_file_drag: bool,
        hovered_files: Vec<std::path::PathBuf>,

        key_pressed_active: Option<u64>,
        clipboard: Option<clipawl::Clipboard>,
        a11y: Box<dyn A11yBridge>,
        last_focus: Option<u64>,

        accesskit_adapter: Option<Adapter>,
        a11y_actions: Arc<Mutex<Vec<accesskit::ActionRequest>>>,
        a11y_tree: A11yTree,

        last_redraw: Instant,
        pending_redraw: bool,

        // Tracks whether a redraw was requested by app code
        redraw_requested: Cell<bool>,
    }

    impl App {
        fn process_a11y_actions(&mut self) {
            let mut actions = self.a11y_actions.lock().unwrap();
            if actions.is_empty() {
                return;
            }
            let pending = actions.drain(..).collect::<Vec<_>>();
            drop(actions);

            let Some(f) = &self.frame_cache else {
                return;
            };

            for req in pending {
                let target_id = req.target_node.0;
                match req.action {
                    accesskit::Action::Click => {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id)
                            && let Some(cb) = &hit.on_click
                        {
                            cb();
                            self.request_redraw();
                        }
                    }
                    accesskit::Action::Focus => {
                        self.sched.focused = Some(target_id);
                        self.request_redraw();
                    }
                    _ => {}
                }
            }
        }

        fn new(root: Box<dyn FnMut(&mut Scheduler, &RenderContext) -> View>) -> Self {
            Self {
                root,
                render: RenderContext::new(),
                window: None,
                backend: None,
                sched: Scheduler::new(),
                inspector: repose_devtools::Inspector::new(),
                frame_cache: None,
                mouse_pos_px: (0.0, 0.0),
                modifiers: Modifiers::default(),
                textfield_states: HashMap::new(),
                ime_preedit: false,
                hover_id: None,
                capture_id: None,
                pressed_ids: HashSet::new(),
                pending_dropped_files: Vec::new(),
                pending_drop_pos_px: None,

                external_file_drag: false,
                hovered_files: Vec::new(),

                key_pressed_active: None,
                clipboard: None,
                a11y: {
                    #[cfg(target_os = "linux")]
                    {
                        Box::new(LinuxAtspiStub) as Box<dyn A11yBridge>
                    }
                    #[cfg(not(target_os = "linux"))]
                    {
                        Box::new(NoopA11y) as Box<dyn A11yBridge>
                    }
                },
                last_focus: None,

                accesskit_adapter: None,
                a11y_actions: Arc::new(Mutex::new(Vec::new())),
                a11y_tree: A11yTree::default(),

                last_redraw: Instant::now(),
                pending_redraw: false,
                redraw_requested: Cell::new(false),
            }
        }

        fn request_redraw(&self) {
            self.redraw_requested.set(true);
            repose_core::request_frame();
            rc::request_redraw(&self.window);
        }

        // Ensure caret is visible after edits/moves (all units in px)
        fn tf_ensure_caret_visible(st: &mut TextFieldState, is_multiline: bool) {
            rc::tf_ensure_caret_visible(st, is_multiline);
        }

        fn paste_from_primary(&self) -> Option<String> {
            let opts = clipawl::ClipboardOptions {
                linux: clipawl::LinuxOptions {
                    selection: clipawl::LinuxSelection::Primary,
                    ..Default::default()
                },
            };
            if let Ok(cb) = clipawl::Clipboard::new_with_options(opts) {
                match pollster::block_on(cb.read()) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        eprintln!("Primary paste error: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = self.backend.as_mut() else {
                return;
            };
            rc::process_render_commands(backend, self.render.drain());
        }

        fn reset_pointer_state(&mut self) {
            self.capture_id = None;
            self.pressed_ids.clear();
            self.hover_id = None;
        }

        fn is_textfield(&self, id: u64) -> bool {
            rc::is_textfield_in_frame(&self.frame_cache, id)
        }

        fn is_multiline_id(&self, id: u64) -> bool {
            if let Some(f) = &self.frame_cache {
                f.hit_regions
                    .iter()
                    .find(|h| h.id == id)
                    .map(|h| h.tf_multiline)
                    .unwrap_or(false)
            } else {
                false
            }
        }

        fn hit_by_id(f: &Frame, id: u64) -> Option<&HitRegion> {
            f.hit_regions.iter().find(|h| h.id == id)
        }

        fn padding_px(&self) -> f32 {
            dp_to_px(TF_PADDING_X_DP)
        }

        fn dp_px(&self, dp: f32) -> f32 {
            dp_to_px(dp)
        }
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            self.clipboard = clipawl::Clipboard::new()
                .map_err(|e| {
                    eprintln!("clipawl clipboard init failed: {e}");
                    e
                })
                .ok();
            repose_core::clipboard::set_clipboard_read_fn(Box::new(|| {
                clipawl::blocking::read().ok()
            }));
            // Register for SelectableText (Ctrl+C) - use blocking API directly
            repose_core::clipboard::set_clipboard_fn(Box::new(move |text| {
                if let Err(e) = clipawl::blocking::write(text) {
                    eprintln!("clipboard write error: {e}");
                }
            }));

            repose_core::clipboard::set_primary_fn(Box::new(|text| {
                let opts = clipawl::ClipboardOptions {
                    linux: clipawl::LinuxOptions {
                        selection: clipawl::LinuxSelection::Primary,
                        ..Default::default()
                    },
                };
                match clipawl::Clipboard::new_with_options(opts) {
                    Ok(cb) => {
                        if let Err(e) = pollster::block_on(cb.write(text)) {
                            eprintln!("primary selection write error: {e}");
                        }
                    }
                    Err(e) => eprintln!("primary clipboard init error: {e}"),
                }
            }));

            if self.window.is_none() {
                match el.create_window(
                    WindowAttributes::default()
                        .with_title("Repose")
                        .with_inner_size(PhysicalSize::new(1280, 800))
                        .with_visible(false),
                ) {
                    Ok(win) => {
                        let w = Arc::new(win);

                        let activation_handler = ReposeActivationHandler {
                            initial_tree: Some(A11yTree::initial_tree()),
                        };

                        let action_handler = ReposeActionHandler {
                            pending_actions: self.a11y_actions.clone(),
                        };

                        let deactivation_handler = ReposeDeactivationHandler;

                        let adapter = Adapter::with_direct_handlers(
                            el,
                            &w,
                            activation_handler,
                            action_handler,
                            deactivation_handler,
                        );

                        self.accesskit_adapter = Some(adapter);

                        w.set_visible(true);

                        let size = w.inner_size();
                        self.sched.size = (size.width, size.height);

                        match repose_render_wgpu::WgpuBackend::new(w.clone()) {
                            Ok(b) => {
                                self.backend = Some(b);
                                set_app_window(w.clone());
                                self.window = Some(w);
                                self.request_redraw();
                            }
                            Err(e) => {
                                log::error!("Failed to create WGPU backend: {e:?}");
                                el.exit();
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to create window: {e:?}");
                        el.exit();
                    }
                }
            }
        }

        fn window_event(
            &mut self,
            el: &winit::event_loop::ActiveEventLoop,
            _id: winit::window::WindowId,
            event: WindowEvent,
        ) {
            // Process AccessKit events first!
            if let Some(adapter) = &mut self.accesskit_adapter {
                adapter.process_event(self.window.as_ref().unwrap(), &event);
            }

            match event {
                WindowEvent::CloseRequested => {
                    if CLOSE_TO_TRAY.load(Ordering::Relaxed) {
                        // Drop GPU backend before null-buffer unmap.
                        self.backend = None;
                        if let Some(w) = &self.window {
                            w.set_visible(false);
                        }
                        WINDOW_VISIBLE.store(false, Ordering::Relaxed);
                    } else {
                        el.exit();
                    }
                }

                WindowEvent::Focused(false) => {
                    // Cancel any active drag operation
                    repose_core::dnd::handle_drag_action(
                        &repose_core::shortcuts::DragAction::Cancel,
                    );

                    // Emit interaction Cancel for the captured hit region
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                            if let Some(cb) = &hit.on_pointer_cancel {
                                let pos = repose_core::Vec2 {
                                    x: self.mouse_pos_px.0,
                                    y: self.mouse_pos_px.1,
                                };
                                let pe = repose_core::input::PointerEvent {
                                    id: repose_core::input::PointerId(0),
                                    kind: repose_core::input::PointerKind::Mouse,
                                    event: repose_core::input::PointerEventKind::Cancel,
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                };
                                cb(pe);
                            }
                        }
                    }

                    // Defensive reset: Wayland/KDE can "eat" releases during DnD.
                    self.external_file_drag = false;
                    self.hovered_files.clear();
                    self.reset_pointer_state();

                    if let Some(w) = &self.window {
                        rc_web::set_ime_for_textfield(w, false);
                    }
                    self.ime_preedit = false;

                    self.request_redraw();
                }

                WindowEvent::HoveredFile(path) => {
                    // Mark external drag active and keep a small bounded list
                    self.external_file_drag = true;
                    if self.hovered_files.len() < 32 {
                        self.hovered_files.push(path);
                    }
                    // Update drop position (best effort)
                    if self.pending_drop_pos_px.is_none() {
                        self.pending_drop_pos_px = Some(self.mouse_pos_px);
                    }
                    self.request_redraw();
                }

                WindowEvent::HoveredFileCancelled => {
                    self.external_file_drag = false;
                    self.hovered_files.clear();

                    // Defensive: cancel any internal capture/drag that might be left stuck
                    self.reset_pointer_state();

                    self.request_redraw();
                }

                WindowEvent::DroppedFile(path) => {
                    // DroppedFile is emitted once per file. Batch them.
                    self.pending_dropped_files.push(path);
                    if self.pending_drop_pos_px.is_none() {
                        self.pending_drop_pos_px = Some(self.mouse_pos_px);
                    }

                    // Drop ends the external file drag session.
                    self.external_file_drag = false;
                    self.hovered_files.clear();

                    self.request_redraw();
                }

                WindowEvent::Resized(size) => {
                    self.sched.size = (size.width, size.height);
                    if let Some(b) = self.backend.as_mut() {
                        b.configure_surface(size.width, size.height);
                    }
                    if let Some(w) = &self.window {
                        let sf = w.scale_factor() as f32;
                        let dp_w = size.width as f32 / sf;
                        let dp_h = size.height as f32 / sf;
                        log::info!(
                            "Resized: fb={}x{} px, scale_factor={}, ~{}x{} dp",
                            size.width,
                            size.height,
                            sf,
                            dp_w as i32,
                            dp_h as i32
                        );
                    }
                    self.request_redraw();
                }

                WindowEvent::CursorMoved { position, .. } => {
                    self.mouse_pos_px = (position.x as f32, position.y as f32);

                    if self.external_file_drag {
                        self.pending_drop_pos_px = Some(self.mouse_pos_px);
                    }

                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };

                    if repose_core::dnd::handle_drag_action(
                        &repose_core::shortcuts::DragAction::Move {
                            position: pos,
                            modifiers: self.modifiers,
                        },
                    ) {
                        self.request_redraw();
                        return;
                    }

                    // Inspector hover
                    if self.inspector.hud.inspector_enabled
                        && let Some(f) = &self.frame_cache
                    {
                        let hit = f.hit_regions.iter().find(|h| {
                            h.rect.contains(Vec2 {
                                x: self.mouse_pos_px.0,
                                y: self.mouse_pos_px.1,
                            })
                        });
                        let hover_rect = hit.map(|h| h.rect);
                        let hover_info = hit.and_then(|h| {
                            f.semantics_nodes.iter().find(|s| s.id == h.id).map(|s| {
                                repose_devtools::HoveredInfo {
                                    id: s.id,
                                    role: format!("{:?}", s.role),
                                    label: s.label.clone(),
                                }
                            })
                        });
                        self.inspector.hud.set_hovered(hover_rect, hover_info);
                        self.request_redraw();
                    }

                    // TextField/TextArea drag selection (if captured)
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                        && self.is_textfield(cid)
                        && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                    {
                        let key = self.tf_key_of(cid);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut st = state_rc.borrow_mut();

                            let pad_x = dp_to_px(TF_PADDING_X_DP);
                            let inner_x = hit.rect.x + pad_x;
                            let inner_y = hit.rect.y + dp_to_px(8.0);
                            let inner_w = (hit.rect.w - 2.0 * pad_x).max(1.0);
                            let inner_h = (hit.rect.h - dp_to_px(16.0)).max(1.0);

                            st.set_inner_width(inner_w);
                            st.set_inner_height(inner_h);

                            let content_x =
                                (self.mouse_pos_px.0 - inner_x + st.scroll_offset).max(0.0);
                            let content_y =
                                (self.mouse_pos_px.1 - inner_y + st.scroll_offset_y).max(0.0);

                            let font_px =
                                dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;

                            let idx = if hit.tf_multiline {
                                rc::index_for_xy_bytes_vt(
                                    &st, font_px, inner_w, content_x, content_y,
                                )
                            } else {
                                rc::index_for_x_bytes_vt(&st, font_px, content_x)
                            };

                            st.drag_to(idx);

                            // Ensure caret visible
                            if hit.tf_multiline {
                                let (cx, cy, _) =
                                    caret_xy_for_byte(&st.text, font_px, inner_w, st.caret_index());
                                st.ensure_caret_visible_xy(cx, cy, inner_w, inner_h, dp_to_px(2.0));
                            } else {
                                let m = measure_text(&st.text, font_px, None, 400, 0);
                                let cx = m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
                                st.ensure_caret_visible(cx, inner_w, dp_to_px(2.0));
                            }

                            self.request_redraw();
                        }
                    }

                    // Pointer routing: hover + move/capture
                    if let Some(f) = &self.frame_cache {
                        // Determine topmost hit
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };
                        let top = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos));

                        // Update cursor icon based on hit
                        if let Some(win) = &self.window {
                            let c = top
                                .and_then(|h| h.cursor)
                                .unwrap_or(repose_core::CursorIcon::Default);
                            win.set_cursor(winit::window::Cursor::Icon(map_cursor(c)));
                        }

                        let new_hover = top.map(|h| h.id);

                        // Enter/Leave
                        if new_hover != self.hover_id {
                            if let Some(prev_id) = self.hover_id
                                && let Some(prev) = f.hit_regions.iter().find(|h| h.id == prev_id)
                                && let Some(cb) = &prev.on_pointer_leave
                            {
                                let pe = repose_core::input::PointerEvent {
                                    id: repose_core::input::PointerId(0),
                                    kind: repose_core::input::PointerKind::Mouse,
                                    event: repose_core::input::PointerEventKind::Leave,
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                };
                                cb(pe);
                            }
                            if let Some(h) = top
                                && let Some(cb) = &h.on_pointer_enter
                            {
                                let pe = repose_core::input::PointerEvent {
                                    id: repose_core::input::PointerId(0),
                                    kind: repose_core::input::PointerKind::Mouse,
                                    event: repose_core::input::PointerEventKind::Enter,
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                };
                                cb(pe);
                            }
                            self.hover_id = new_hover;
                            self.request_redraw();
                        }

                        // Build PointerEvent
                        let pe = repose_core::input::PointerEvent {
                            id: repose_core::input::PointerId(0),
                            kind: repose_core::input::PointerKind::Mouse,
                            event: repose_core::input::PointerEventKind::Move,
                            position: pos,
                            pressure: 1.0,
                            modifiers: self.modifiers,
                        };

                        // Move delivery (captured first)
                        if let Some(cid) = self.capture_id {
                            if let Some(h) = f.hit_regions.iter().find(|h| h.id == cid)
                                && let Some(cb) = &h.on_pointer_move
                            {
                                cb(pe.clone());
                            }
                        } else if let Some(h) = &top
                            && let Some(cb) = &h.on_pointer_move
                        {
                            cb(pe);
                        }
                    }
                }

                WindowEvent::MouseWheel { delta, .. } => {
                    // Convert line deltas (logical) to px; pixel delta is already px
                    let (dx_px, dy_px) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            let unit_px = dp_to_px(60.0);
                            (-(x * unit_px), -(y * unit_px))
                        }
                        MouseScrollDelta::PixelDelta(lp) => (-(lp.x as f32), -(lp.y as f32)),
                    };
                    log::debug!("MouseWheel: dx={}, dy={}", dx_px, dy_px);

                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };

                        if rc::dispatch_scroll(f, pos, Vec2 { x: dx_px, y: dy_px }, None).0 {
                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } => {
                    let mut need_announce = false;
                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };
                        if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                        {
                            repose_core::dnd::handle_drag_action(
                                &repose_core::shortcuts::DragAction::Press {
                                    position: Vec2 {
                                        x: self.mouse_pos_px.0,
                                        y: self.mouse_pos_px.1,
                                    },
                                    capture_id: hit.id,
                                    kind: repose_core::input::PointerKind::Mouse,
                                    modifiers: self.modifiers,
                                },
                            );

                            // Capture starts on press
                            self.capture_id = Some(hit.id);

                            // Text input caret placement + begin drag selection
                            if self.is_textfield(hit.id) {
                                let key = self.tf_key_of(hit.id);
                                self.textfield_states.entry(key).or_insert_with(|| {
                                    Rc::new(RefCell::new(TextFieldState::new()))
                                });
                                if let Some(st_rc) = self.textfield_states.get(&key) {
                                    let mut st = st_rc.borrow_mut();
                                    let pad = self.padding_px();
                                    let inner_x = hit.rect.x + pad;
                                    let inner_y = hit.rect.y + self.dp_px(8.0);
                                    let content_x =
                                        (self.mouse_pos_px.0 - inner_x + st.scroll_offset).max(0.0);
                                    let content_y = (self.mouse_pos_px.1 - inner_y
                                        + st.scroll_offset_y)
                                        .max(0.0);
                                    let font_px = self.dp_px(TF_FONT_DP)
                                        * repose_core::locals::text_scale().0;

                                    let idx = if hit.tf_multiline {
                                        rc::index_for_xy_bytes_vt(
                                            &st,
                                            font_px,
                                            hit.rect.w - 2.0 * pad,
                                            content_x,
                                            content_y,
                                        )
                                    } else {
                                        rc::index_for_x_bytes_vt(&st, font_px, content_x)
                                    };

                                    st.begin_drag(idx, self.modifiers.shift);

                                    // Ensure caret visible
                                    let caret_idx = st.caret_index();
                                    let iw = st.inner_width;
                                    let ih = st.inner_height;
                                    let wrap_w = hit.rect.w - 2.0 * pad;
                                    if hit.tf_multiline {
                                        let (cx, cy, _) = textfield::caret_xy_for_byte(
                                            &st.text, font_px, wrap_w, caret_idx,
                                        );
                                        st.ensure_caret_visible_xy(cx, cy, iw, ih, self.dp_px(2.0));
                                    } else {
                                        let m = measure_text(&st.text, font_px, None, 400, 0);
                                        let cx = m.positions.get(caret_idx).copied().unwrap_or(0.0);
                                        st.ensure_caret_visible(cx, iw, self.dp_px(2.0));
                                    }
                                }
                            }
                            // Pressed visual for mouse
                            self.pressed_ids.insert(hit.id);
                            // Repaint for pressed state
                            self.request_redraw();

                            // Focus & IME first for focusables (so state exists)
                            if hit.focusable {
                                self.sched.focused = Some(hit.id);
                                need_announce = true;
                                let key = self.tf_key_of(hit.id);
                                self.textfield_states.entry(key).or_insert_with(|| {
                                    Rc::new(RefCell::new(TextFieldState::new()))
                                });
                                if let Some(win) = &self.window {
                                    let sf = win.scale_factor();
                                    rc_web::set_ime_for_textfield(win, true);
                                    win.set_ime_cursor_area(
                                        LogicalPosition::new(
                                            hit.rect.x as f64 / sf,
                                            hit.rect.y as f64 / sf,
                                        ),
                                        LogicalSize::new(
                                            hit.rect.w as f64 / sf,
                                            hit.rect.h as f64 / sf,
                                        ),
                                    );
                                }
                            }

                            // PointerDown callback (legacy)
                            if let Some(cb) = &hit.on_pointer_down {
                                let pe = repose_core::input::PointerEvent {
                                    id: repose_core::input::PointerId(0),
                                    kind: repose_core::input::PointerKind::Mouse,
                                    event: repose_core::input::PointerEventKind::Down(
                                        repose_core::input::PointerButton::Primary,
                                    ),
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                };
                                cb(pe);
                            }

                            if need_announce {
                                self.announce_focus_change();
                            }

                            self.request_redraw();
                        } else {
                            // Click outside: drop focus/IME
                            if self.ime_preedit {
                                if let Some(win) = &self.window {
                                    rc_web::set_ime_for_textfield(win, false);
                                }
                                self.ime_preedit = false;
                            }
                            self.sched.focused = None;
                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Middle,
                    ..
                } => {
                    let Some(f) = &self.frame_cache else {
                        return;
                    };
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };
                    if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos)) {
                        // Dispatch Tertiary pointer event
                        if let Some(cb) = &hit.on_pointer_down {
                            cb(repose_core::input::PointerEvent {
                                id: repose_core::input::PointerId(0),
                                kind: repose_core::input::PointerKind::Mouse,
                                event: repose_core::input::PointerEventKind::Down(
                                    repose_core::input::PointerButton::Tertiary,
                                ),
                                position: pos,
                                pressure: 1.0,
                                modifiers: self.modifiers,
                            });
                        }
                        // Paste primary selection into textfield
                        if self.is_textfield(hit.id) {
                            let key = self.tf_key_of(hit.id);
                            if let Some(state_rc) = self.textfield_states.get(&key) {
                                if let Some(txt) = self.paste_from_primary() {
                                    let mut st = state_rc.borrow_mut();
                                    st.insert_text_atomic(&txt);
                                    self.notify_text_change(hit.id, st.text.clone());
                                    if let Some(f) = &self.frame_cache
                                        && let Some(h) =
                                            f.hit_regions.iter().find(|h| h.id == hit.id)
                                    {
                                        App::tf_ensure_caret_visible(&mut st, h.tf_multiline);
                                    }
                                }
                            }
                        }
                    }
                    self.request_redraw();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } => {
                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };

                    if repose_core::dnd::handle_drag_action(
                        &repose_core::shortcuts::DragAction::Release {
                            position: pos,
                            modifiers: self.modifiers,
                        },
                    ) {
                        self.capture_id = None;
                        self.pressed_ids.clear();
                        repose_core::request_frame();
                        return;
                    }

                    if let Some(cid) = self.capture_id {
                        self.pressed_ids.remove(&cid);
                        self.request_redraw();
                    }

                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                        && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                        && let Some(cb) = &hit.on_pointer_up
                    {
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };
                        let pe = repose_core::input::PointerEvent {
                            id: repose_core::input::PointerId(0),
                            kind: repose_core::input::PointerKind::Mouse,
                            event: repose_core::input::PointerEventKind::Up(
                                repose_core::input::PointerButton::Primary,
                            ),
                            position: pos,
                            pressure: 1.0,
                            modifiers: self.modifiers,
                        };
                        cb(pe);
                    }

                    // Click on release if pointer is still over the captured hit region
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
                            && hit.rect.contains(pos)
                            && let Some(cb) = &hit.on_click
                        {
                            cb();
                            // A11y: announce activation (mouse)
                            if let Some(node) = f.semantics_nodes.iter().find(|n| n.id == cid) {
                                let label = node.label.as_deref().unwrap_or("");
                                self.a11y.announce(&format!("Activated {}", label));
                            }
                        }
                    }
                    // TextField drag end
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                        && let Some(_sem) = f
                            .semantics_nodes
                            .iter()
                            .find(|n| n.id == cid && n.role == Role::TextField)
                    {
                        let key = self.tf_key_of(cid);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            state_rc.borrow_mut().end_drag();
                        }
                    }

                    self.capture_id = None;

                    repose_core::request_frame();
                }

                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Middle,
                    ..
                } => {
                    if let Some(f) = &self.frame_cache {
                        let pos = Vec2 {
                            x: self.mouse_pos_px.0,
                            y: self.mouse_pos_px.1,
                        };
                        if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos))
                        {
                            if let Some(cb) = &hit.on_pointer_up {
                                cb(repose_core::input::PointerEvent {
                                    id: repose_core::input::PointerId(0),
                                    kind: repose_core::input::PointerKind::Mouse,
                                    event: repose_core::input::PointerEventKind::Up(
                                        repose_core::input::PointerButton::Tertiary,
                                    ),
                                    position: pos,
                                    pressure: 1.0,
                                    modifiers: self.modifiers,
                                });
                            }
                        }
                    }
                }

                WindowEvent::ModifiersChanged(new_mods) => {
                    rc::update_modifiers(&mut self.modifiers, &new_mods.state());
                }

                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::BrowserBack)
                            | PhysicalKey::Code(KeyCode::Escape) => {
                                use repose_navigation::back;

                                if repose_core::dnd::handle_drag_action(
                                    &repose_core::shortcuts::DragAction::Cancel,
                                ) {
                                    return;
                                }

                                if !back::handle() {
                                    // el.exit();
                                }
                                return;
                            }
                            _ => {}
                        }
                    }

                    // Dispatch key event through focus ancestor chain (Compose-compatible)
                    let mapped_key = rc::map_key(key_event.physical_key);
                    let utf16 = match mapped_key { repose_core::input::Key::Character(c) => c as u16, _ => 0 };
                    let mods = self.modifiers;
                    let repeat = key_event.repeat;
                    let ev_type = if key_event.state == ElementState::Pressed { repose_core::input::KeyEventType::Down } else { repose_core::input::KeyEventType::Up };
                    let consumed = self.frame_cache.as_ref().and_then(|f| {
                        let focused = self.sched.focused.or_else(|| {
                            f.semantics_nodes.iter().find(|n| n.parent.is_none()).map(|n| n.id)
                        })?;
                        let sem_parent_of: std::collections::HashMap<u64, u64> = f.semantics_nodes.iter().filter_map(|n| n.parent.map(|p| (n.id, p))).collect();
                        let hit_by_id: std::collections::HashMap<u64, &HitRegion> = f.hit_regions.iter().map(|h| (h.id, h)).collect();
                        let mut ancestors = Vec::new();
                        let mut cur = focused;
                        loop {
                            ancestors.push(cur);
                            if let Some(&p) = sem_parent_of.get(&cur) { cur = p; } else { break; }
                        }
                        let make_ke = || repose_core::input::KeyEvent {
                            key: mapped_key.clone(),
                            modifiers: mods,
                            is_repeat: repeat,
                            event_type: ev_type,
                            utf16_code_point: utf16,
                        };
                        // Top-down preview: root → focused
                        for &id in ancestors.iter().rev() {
                            if let Some(hit) = hit_by_id.get(&id) {
                                if let Some(cb) = &hit.on_preview_key_event {
                                    if cb(make_ke()) { return Some(true); }
                                }
                            }
                        }
                        // Bottom-up normal: focused → root
                        for &id in ancestors.iter() {
                            if let Some(hit) = hit_by_id.get(&id) {
                                if let Some(cb) = &hit.on_key_event {
                                    if cb(make_ke()) { return Some(true); }
                                }
                            }
                        }
                        None
                    }).unwrap_or(false);
                    if consumed {
                        self.request_redraw();
                        return;
                    }

                    if key_event.state == ElementState::Pressed
                        && let Some(action) = repose_core::shortcuts::resolve_action(
                            repose_core::shortcuts::KeyChord::new(
                                mapped_key,
                                self.modifiers,
                            ),
                        )
                        && self.dispatch_action(action)
                    {
                        self.request_redraw();
                        return;
                    }

                    if let Some(fid) = self.sched.focused {
                        // If focused is NOT a TextField, allow Space/Enter activation
                        let is_textfield = if let Some(f) = &self.frame_cache {
                            f.semantics_nodes
                                .iter()
                                .any(|n| n.id == fid && n.role == Role::TextField)
                        } else {
                            false
                        };

                        if !is_textfield {
                            match key_event.physical_key {
                                PhysicalKey::Code(KeyCode::Space)
                                | PhysicalKey::Code(KeyCode::Enter) => {
                                    if key_event.state == ElementState::Pressed && !key_event.repeat
                                    {
                                        self.pressed_ids.insert(fid);
                                        self.key_pressed_active = Some(fid);
                                        self.request_redraw();
                                        return;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }

                    // Keyboard activation for focused TextField submit on Enter
                    // For multiline: Ctrl+Enter or Cmd+Enter submits, plain Enter inserts newline
                    // For single-line: Enter submits
                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key
                        && let Some(focused_id) = self.sched.focused
                        && let Some(f) = &self.frame_cache
                        && let Some(hit) = f.hit_regions.iter().find(|h| h.id == focused_id)
                    {
                        let is_multiline = hit.tf_multiline;
                        let should_submit = if is_multiline {
                            // Multiline: Ctrl+Enter or Cmd+Enter submits
                            self.modifiers.ctrl || self.modifiers.meta
                        } else {
                            // Single-line: Enter always submits
                            true
                        };

                        if should_submit {
                            if let Some(on_submit) = &hit.on_text_submit {
                                let key = self.tf_key_of(focused_id);
                                if let Some(state) = self.textfield_states.get(&key) {
                                    let text = state.borrow().text.clone();
                                    on_submit(text);
                                    self.request_redraw();
                                    return;
                                }
                            }
                        } else {
                            // Multiline with plain Enter: insert newline
                            let key = self.tf_key_of(focused_id);
                            if let Some(state_rc) = self.textfield_states.get(&key) {
                                let mut st = state_rc.borrow_mut();
                                st.insert_text("\n");
                                let new_text = st.text.clone();
                                self.notify_text_change(focused_id, new_text);
                                App::tf_ensure_caret_visible(&mut st, hit.tf_multiline);
                                self.request_redraw();
                                return;
                            }
                        }
                    }

                    if key_event.state == ElementState::Pressed {
                        // Inspector hotkey: Ctrl+Shift+I
                        if self.modifiers.ctrl
                            && self.modifiers.shift
                            && let PhysicalKey::Code(KeyCode::KeyI) = key_event.physical_key
                        {
                            self.inspector.hud.toggle_inspector();
                            self.request_redraw();
                            return;
                        }

                        // TextField navigation/edit
                        if let Some(focused_id) = self.sched.focused {
                            let key = self.tf_key_of(focused_id);
                            if let Some(state_rc) = self.textfield_states.get(&key) {
                                let mut state = state_rc.borrow_mut();
                                match key_event.physical_key {
                                    PhysicalKey::Code(KeyCode::Backspace) => {
                                        state.delete_backward();
                                        let new_text = state.text.clone();
                                        self.notify_text_change(focused_id, new_text);
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Delete) => {
                                        state.delete_forward();
                                        let new_text = state.text.clone();
                                        self.notify_text_change(focused_id, new_text);
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                        state.move_cursor(-1, self.modifiers.shift);
                                        state.preferred_x_px = None; // Reset preferred x on horizontal movement
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                                        state.move_cursor(1, self.modifiers.shift);
                                        state.preferred_x_px = None; // Reset preferred x on horizontal movement
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowUp) => {
                                        if self.is_multiline_id(focused_id)
                                            && let Some(f) = &self.frame_cache
                                            && let Some(hit) =
                                                f.hit_regions.iter().find(|h| h.id == focused_id)
                                        {
                                            let font_px = dp_to_px(TF_FONT_DP);
                                            let pad = self.padding_px();
                                            let wrap_w = hit.rect.w - 2.0 * pad;
                                            let cur = state.caret_index();
                                            let (new_pos, px) =
                                                repose_ui::textfield::move_caret_vertical(
                                                    &state.text,
                                                    font_px,
                                                    wrap_w,
                                                    cur,
                                                    -1,
                                                    state.preferred_x_px,
                                                );
                                            if self.modifiers.shift {
                                                state.selection.end = new_pos;
                                            } else {
                                                state.selection = new_pos..new_pos;
                                            }
                                            state.preferred_x_px = Some(px);
                                            // Use multiline-aware caret visibility
                                            let (cx, cy, _) = caret_xy_for_byte(
                                                &state.text,
                                                font_px,
                                                wrap_w,
                                                state.caret_index(),
                                            );
                                            let iw = state.inner_width;
                                            let ih = state.inner_height;
                                            state.ensure_caret_visible_xy(
                                                cx,
                                                cy,
                                                iw,
                                                ih,
                                                self.dp_px(2.0),
                                            );
                                            self.request_redraw();
                                        }
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowDown) => {
                                        if self.is_multiline_id(focused_id)
                                            && let Some(f) = &self.frame_cache
                                            && let Some(hit) =
                                                f.hit_regions.iter().find(|h| h.id == focused_id)
                                        {
                                            let font_px = dp_to_px(TF_FONT_DP);
                                            let pad = self.padding_px();
                                            let wrap_w = hit.rect.w - 2.0 * pad;
                                            let cur = state.caret_index();
                                            let (new_pos, px) =
                                                repose_ui::textfield::move_caret_vertical(
                                                    &state.text,
                                                    font_px,
                                                    wrap_w,
                                                    cur,
                                                    1,
                                                    state.preferred_x_px,
                                                );
                                            if self.modifiers.shift {
                                                state.selection.end = new_pos;
                                            } else {
                                                state.selection = new_pos..new_pos;
                                            }
                                            state.preferred_x_px = Some(px);
                                            // Use multiline-aware caret visibility
                                            let (cx, cy, _) = caret_xy_for_byte(
                                                &state.text,
                                                font_px,
                                                wrap_w,
                                                state.caret_index(),
                                            );
                                            let iw = state.inner_width;
                                            let ih = state.inner_height;
                                            state.ensure_caret_visible_xy(
                                                cx,
                                                cy,
                                                iw,
                                                ih,
                                                self.dp_px(2.0),
                                            );
                                            self.request_redraw();
                                        }
                                    }
                                    PhysicalKey::Code(KeyCode::Home) => {
                                        state.selection = 0..0;
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::End) => {
                                        {
                                            let end = state.text.len();
                                            state.selection = end..end;
                                        }
                                        App::tf_ensure_caret_visible(
                                            &mut state,
                                            self.is_multiline_id(focused_id),
                                        );
                                        self.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                        }

                        // Plain text input when IME is not active
                        if !self.ime_preedit
                            && !self.modifiers.ctrl
                            && !self.modifiers.alt
                            && !self.modifiers.meta
                            && let Some(raw) = key_event.text.as_deref()
                        {
                            let text: String = raw
                                .chars()
                                .filter(|c| !c.is_control() && *c != '\n' && *c != '\r')
                                .collect();
                            if !text.is_empty()
                                && let Some(fid) = self.sched.focused
                            {
                                let key = self.tf_key_of(fid);
                                if let Some(state_rc) = self.textfield_states.get(&key) {
                                    let mut st = state_rc.borrow_mut();
                                    st.insert_text(&text);
                                    self.notify_text_change(fid, st.text.clone());
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == fid)
                                    {
                                        App::tf_ensure_caret_visible(&mut st, hit.tf_multiline);
                                    }
                                    self.request_redraw();
                                }
                            }
                        }
                    } else if key_event.state == ElementState::Released {
                        // Finish keyboard activation on release (Space/Enter)
                        if let Some(active_id) = self.key_pressed_active {
                            match key_event.physical_key {
                                PhysicalKey::Code(KeyCode::Space)
                                | PhysicalKey::Code(KeyCode::Enter) => {
                                    self.pressed_ids.remove(&active_id);
                                    self.key_pressed_active = None;

                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == active_id)
                                    {
                                        if let Some(cb) = &hit.on_click {
                                            cb();
                                        } else if let Some(cb) = &hit.on_pointer_down {
                                            let pe = repose_core::input::PointerEvent {
                                                id: repose_core::input::PointerId(0),
                                                kind: repose_core::input::PointerKind::Mouse,
                                                event: repose_core::input::PointerEventKind::Down(
                                                    repose_core::input::PointerButton::Primary,
                                                ),
                                                position: repose_core::Vec2 { x: 0.0, y: 0.0 },
                                                pressure: 1.0,
                                                modifiers: self.modifiers,
                                            };
                                            cb(pe);
                                        }
                                        if let Some(node) =
                                            f.semantics_nodes.iter().find(|n| n.id == active_id)
                                        {
                                            let label = node.label.as_deref().unwrap_or("");
                                            self.a11y.announce(&format!("Activated {}", label));
                                        }
                                    }
                                    self.request_redraw();
                                }
                                _ => {}
                            }
                        }
                    }
                }

                WindowEvent::Ime(ime) => {
                    if let Some(focused_id) = self.sched.focused {
                        let key = self.tf_key_of(focused_id);
                        if let Some(state_rc) = self.textfield_states.get(&key)
                            && let Some(f) = &self.frame_cache
                            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == focused_id)
                        {
                            let mut state = state_rc.borrow_mut();
                            let hit_rect = hit.rect;
                            let on_text_change = hit.on_text_change.clone();
                            let mut notify = |text: String| {
                                if let Some(cb) = &on_text_change {
                                    cb(text);
                                }
                            };
                            rc_android::handle_ime_event(
                                ime,
                                &mut state,
                                hit_rect,
                                &mut notify,
                                &mut self.ime_preedit,
                            );
                            self.request_redraw();
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    // 1. Check our redraw flag before processing a11y.
                    if !self.redraw_requested.replace(false) {
                        self.process_a11y_actions();
                        self.process_render_commands();
                        log::trace!("RedrawRequested: no frame request, skipping compose");
                        return;
                    }
                    log::trace!("RedrawRequested: frame request pending, composing");

                    // 2. Process a11y actions and render commands before compose.
                    self.process_a11y_actions();
                    self.process_render_commands();

                    let Some(win) = self.window.as_ref() else {
                        return;
                    };
                    if self.backend.is_none() {
                        return;
                    }

                    // Advance animations before composition (Compose pattern).
                    // Mirrors broadcastFrameClock.sendFrame() before performRecompose().
                    repose_core::animation_driver::tick();

                    let t0 = Instant::now();
                    let scale = win.scale_factor() as f32;
                    let size_px_u32 = self.sched.size;
                    let focused = self.sched.focused;

                    let rc = self.render.clone();
                    let root_fn = &mut self.root;
                    let mut composed_root = |s: &mut Scheduler| (root_fn)(s, &rc);

                    let frame = compose_frame(
                        &mut self.sched,
                        &mut composed_root,
                        scale,
                        size_px_u32,
                        self.hover_id,
                        &self.pressed_ids,
                        &self.textfield_states,
                        focused,
                    );

                    if focused.is_some() && self.sched.focused.is_none() && self.ime_preedit {
                        rc_web::set_ime_for_textfield(win, false);
                        self.ime_preedit = false;
                    }

                    let build_layout_ms = (Instant::now() - t0).as_secs_f32() * 1000.0;

                    // UPDATE ACCESSIBILITY TREE
                    if let Some(adapter) = &mut self.accesskit_adapter {
                        let win = self.window.as_ref().unwrap();
                        let scale = win.scale_factor();
                        if let Some(update) =
                            self.a11y_tree
                                .update(&frame.semantics_nodes, scale, self.sched.focused)
                        {
                            adapter.update_if_active(|| update);
                        }
                    }

                    // Render
                    let mut scene = frame.scene.clone();
                    // Update HUD metrics before overlay draws
                    let widget_count = frame.semantics_nodes.len() + frame.hit_regions.len();
                    let signal_count = self.sched.id_count() as usize;
                    self.inspector.hud.metrics = Some(repose_devtools::Metrics {
                        build_ms: build_layout_ms,
                        layout_ms: build_layout_ms * 0.5,
                        scene_nodes: scene.nodes.len(),
                        widget_count,
                        signal_count,
                    });
                    self.inspector.frame(&mut scene);

                    // Drag indicator overlay (internal + file drop)
                    repose_core::dnd::overlay_drag_indicator(
                        &mut scene,
                        self.mouse_pos_px,
                        self.external_file_drag,
                    );

                    // Now borrow backend mutably only for the frame() call
                    let win = self.window.as_ref().unwrap();
                    let scale = win.scale_factor() as f32;
                    if let Some(backend) = self.backend.as_mut() {
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                    }

                    // Initialize TextFieldState for any focused TextField that
                    // doesn't have one yet (e.g. after FocusRequester::request_focus)
                    if let Some(fid) = self.sched.focused {
                        if let Some(hit) = frame.hit_regions.iter().find(|h| h.id == fid)
                            && let Some(key) = hit.tf_state_key
                            && !self.textfield_states.contains_key(&key)
                        {
                            self.textfield_states
                                .entry(key)
                                .or_insert_with(|| {
                                    Rc::new(RefCell::new(repose_ui::TextFieldState::new()))
                                })
                                .borrow_mut()
                                .reset_caret_blink();
                        }
                    }

                    repose_core::dnd::set_dnd_frame(Some(frame.clone()));
                    self.frame_cache = Some(frame);
                    repose_core::dnd::set_dnd_scale(scale);

                    self.dispatch_file_drop_now();

                    rc::tick_snackbar(self.last_redraw);
                    self.last_redraw = Instant::now();
                }

                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            // Process cross-thread commands (e.g. tray toggles, deeplinks) before any
            // redraw check, so hide/show commands work even when hidden
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if let Some(cb) = ABOUT_TO_WAIT_CALLBACK.lock().unwrap().as_ref() {
                cb();
            }
            process_deeplinks();

            // On Wayland, wgpu creates an xdg_surface from the winit window and it shouldn't be recreated with a new id?
            // It doesn't take a lot of resources anyway, so let the backend be present.
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            if WINDOW_VISIBLE.load(Ordering::Relaxed) && self.backend.is_none() {
                if let Some(w) = &self.window {
                    log::info!("about_to_wait: recreating GPU backend");
                    match repose_render_wgpu::WgpuBackend::new(w.clone()) {
                        Ok(b) => self.backend = Some(b),
                        Err(e) => log::error!("about_to_wait: failed to recreate backend: {e:?}"),
                    }
                }
            }

            if take_frame_request() {
                self.pending_redraw = true;
            }
            if !self.pending_redraw {
                let now = Instant::now();
                let idle_interval = web_time::Duration::from_millis(1000);
                if now.saturating_duration_since(self.last_redraw) >= idle_interval {
                    self.redraw_requested.set(true);
                    request_frame();
                    rc::request_redraw(&self.window);
                    self.last_redraw = now;
                }
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    self.last_redraw + idle_interval,
                ));
                return;
            }

            let now = Instant::now();
            let interval = web_time::Duration::from_millis(16);

            if now.saturating_duration_since(self.last_redraw) >= interval {
                self.pending_redraw = false;
                self.redraw_requested.set(true);
                rc::request_redraw(&self.window);
                self.last_redraw = now;
            } else {
                el.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
                    self.last_redraw + interval,
                ));
            }
        }

        fn new_events(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::event::StartCause,
        ) {
        }
        fn user_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: ()) {
            self.pending_redraw = true;
        }
        fn device_event(
            &mut self,
            _: &winit::event_loop::ActiveEventLoop,
            _: winit::event::DeviceId,
            _: winit::event::DeviceEvent,
        ) {
        }
        fn suspended(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
        fn exiting(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
        fn memory_warning(&mut self, _: &winit::event_loop::ActiveEventLoop) {}
    }

    impl App {
        fn announce_focus_change(&mut self) {
            if let Some(f) = &self.frame_cache {
                let focused_node = self
                    .sched
                    .focused
                    .and_then(|id| f.semantics_nodes.iter().find(|n| n.id == id));
                self.a11y.focus_changed(focused_node);
            }
        }

        fn notify_text_change(&self, id: u64, text: String) {
            if let Some(f) = &self.frame_cache
                && let Some(h) = f.hit_regions.iter().find(|h| h.id == id)
                && let Some(cb) = &h.on_text_change
            {
                cb(text);
            }
        }

        fn tf_key_of(&self, visual_id: u64) -> u64 {
            rc::tf_key_of_in_frame(&self.frame_cache, visual_id)
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            use repose_core::shortcuts;

            if let (Some(f), Some(fid)) = (&self.frame_cache, self.sched.focused)
                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                && let Some(cb) = &hit.on_action
                && cb(action.clone())
            {
                return true;
            }

            if shortcuts::handle(action.clone()) {
                return true;
            }

            // Focus navigation (Tab/arrows)
            if let Some(f) = &self.frame_cache {
                if let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.sched, f)
                {
                    if let Some(active) = self.key_pressed_active.take() {
                        self.pressed_ids.remove(&active);
                    }
                    let tf_state_key = f
                        .hit_regions
                        .iter()
                        .find(|h| h.id == new_id)
                        .and_then(|h| h.tf_state_key);
                    if let Some(key) = tf_state_key {
                        self.textfield_states.entry(key).or_insert_with(|| {
                            Rc::new(RefCell::new(repose_ui::TextFieldState::new()))
                        });
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            state_rc.borrow_mut().reset_caret_blink();
                        }
                    }
                    if let Some(win) = &self.window {
                        let is_textfield = f.semantics_nodes.iter().any(|n| {
                            n.id == new_id && n.role == repose_core::semantics::Role::TextField
                        });
                        rc_web::set_ime_for_textfield(win, is_textfield);
                    }
                    self.announce_focus_change();
                    return true;
                }
            }

            false
        }

        fn dispatch_file_drop_now(&mut self) {
            let Some(f) = &self.frame_cache else {
                self.pending_dropped_files.clear();
                self.pending_drop_pos_px = None;
                return;
            };

            if self.pending_dropped_files.is_empty() {
                return;
            }

            let pos_px = self.pending_drop_pos_px.unwrap_or(self.mouse_pos_px);
            let pos = Vec2 {
                x: pos_px.0,
                y: pos_px.1,
            };

            let mut files = Vec::new();
            for p in self.pending_dropped_files.drain(..) {
                let name = p
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file")
                    .to_string();
                files.push(repose_core::dnd::DroppedFile {
                    name,
                    path: Some(p),
                });
            }

            let payload: repose_core::dnd::DragPayload =
                std::rc::Rc::new(repose_core::dnd::DroppedFiles { files });

            let Some(target_id) = repose_core::dnd::dnd_target_id_at(f, pos) else {
                self.pending_drop_pos_px = None;
                return;
            };

            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id)
                && let Some(cb) = &hit.on_drop
            {
                let accepted = cb(repose_core::dnd::DropEvent {
                    source_id: 0, // external source (OS)
                    target_id,
                    position: pos,
                    modifiers: self.modifiers,
                    payload: payload.clone(),
                });

                if accepted && let Some(node) = f.semantics_nodes.iter().find(|n| n.id == target_id)
                {
                    let label = node.label.as_deref().unwrap_or("");
                    self.a11y.announce(&format!("Dropped files on {}", label));
                }
            }

            self.pending_drop_pos_px = None;
            self.request_redraw();
        }
    }

    let event_loop = EventLoop::new()?;
    set_event_loop_proxy(event_loop.create_proxy());
    let mut app = App::new(Box::new(root));
    // Install system clock once
    repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));
    event_loop.run_app(&mut app)?;
    Ok(())
}

// Accessibility bridge stub (Noop by default; logs on Linux for now)
/// Bridge from Repose's semantics tree to platform accessibility APIs.
///
/// Implementations are responsible for:
/// - Exposing nodes to the OS (AT‑SPI, Android accessibility, etc.).
/// - Updating focus when `focus_changed` is called.
/// - Announcing transient messages (e.g. button activation) via screen readers.
pub trait A11yBridge: Send {
    /// Publish (or update) the full semantics tree for the current frame.
    fn publish_tree(&mut self, nodes: &[repose_core::runtime::SemNode]);

    /// Notify that the focused node has changed. `None` means focus cleared.
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>);

    /// Announce a one‑off message via the platform's accessibility channel.
    fn announce(&mut self, msg: &str);
}

struct NoopA11y;
impl A11yBridge for NoopA11y {
    fn publish_tree(&mut self, _nodes: &[repose_core::runtime::SemNode]) {
        // no-op
    }
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>) {
        if let Some(n) = node {
            log::info!("A11y focus: {:?} {:?}", n.role, n.label);
        } else {
            log::info!("A11y focus: None");
        }
    }
    fn announce(&mut self, msg: &str) {
        log::info!("A11y announce: {msg}");
    }
}

#[cfg(target_os = "linux")]
struct LinuxAtspiStub;
#[cfg(target_os = "linux")]
impl A11yBridge for LinuxAtspiStub {
    fn publish_tree(&mut self, nodes: &[repose_core::runtime::SemNode]) {
        log::debug!("AT-SPI stub: publish {} nodes", nodes.len());
    }
    fn focus_changed(&mut self, node: Option<&repose_core::runtime::SemNode>) {
        if let Some(n) = node {
            log::info!("AT-SPI stub focus: {:?} {:?}", n.role, n.label);
        } else {
            log::info!("AT-SPI stub focus: None");
        }
    }
    fn announce(&mut self, msg: &str) {
        log::info!("AT-SPI stub announce: {msg}");
    }
}
