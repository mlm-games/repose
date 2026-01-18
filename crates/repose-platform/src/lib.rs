//! Platform runners
use crate::a11y::ReposeActionHandler;
use accesskit_winit::Adapter;
use repose_core::locals::dp_to_px;
use repose_core::*;
use repose_ui::textfield::{
    self, TF_FONT_DP, TF_PADDING_X_DP, TextFieldState, caret_xy_for_byte, index_for_x_bytes,
    index_for_xy_bytes, measure_text,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use web_time::Instant;

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;

#[cfg(all(target_arch = "wasm32"))]
pub mod web;

pub mod a11y;
mod common;
pub mod render;

pub use render::{ImageHandleGuard, RenderCommand, RenderContext};

#[derive(Clone)]
struct DragSession {
    source_id: u64,
    payload: repose_core::dnd::DragPayload,
    start_px: (f32, f32),
    over_id: Option<u64>,
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
    focused: Option<u64>,
) -> Frame
where
    F: FnMut(&mut Scheduler) -> View,
{
    set_density_default(Density { scale });

    sched.repose(
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
                        focused,
                    )
                })
            }
        },
    )
}

/// Helper: ensure caret visibility for a TextFieldState inside a given rect (px).
pub fn tf_ensure_visible_in_rect(state: &mut repose_ui::TextFieldState, inner_rect: Rect) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let m = measure_text(&state.text, font_px);
    let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
    state.ensure_caret_visible(
        caret_x_px,
        inner_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
        dp_to_px(2.0),
    );
}

#[cfg(feature = "desktop")]
pub fn run_desktop_app(
    root: impl FnMut(&mut Scheduler, &RenderContext) -> View + 'static,
) -> anyhow::Result<()> {
    use std::collections::{HashMap, HashSet};
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{ImePurpose, Window, WindowAttributes};

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

        // Drag & Drop (internal)
        mouse_down_pos_px: Option<(f32, f32)>,
        drag: Option<DragSession>,

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
                let target_id = req.target.0;
                match req.action {
                    accesskit::Action::Click => {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id) {
                            if let Some(cb) = &hit.on_click {
                                cb();
                                self.request_redraw();
                            }
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
                mouse_down_pos_px: None,
                drag: None,
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
            }
        }

        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // Ensure caret is visible after edits/moves (all units in px)
        fn tf_ensure_caret_visible(st: &mut TextFieldState) {
            let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
            let m = measure_text(&st.text, font_px);
            let caret_x_px = m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
            let iw = st.inner_width;
            st.ensure_caret_visible(caret_x_px, iw, dp_to_px(2.0));
        }

        fn copy_to_clipboard(&mut self, text: String) {
            if let Some(cb) = &mut self.clipboard {
                let _ = pollster::block_on(cb.set_text(&text));
            }
        }

        fn paste_from_clipboard(&mut self) -> Option<String> {
            if let Some(cb) = &mut self.clipboard {
                match pollster::block_on(cb.get_text()) {
                    Ok(t) => Some(t),
                    Err(e) => {
                        eprintln!("Paste error: {}", e);
                        None
                    }
                }
            } else {
                None
            }
        }

        fn process_render_commands(&mut self) {
            let Some(backend) = &mut self.backend else {
                return;
            };

            for cmd in self.render.drain() {
                match cmd {
                    RenderCommand::SetImageEncoded {
                        handle,
                        bytes,
                        srgb,
                    } => {
                        let _ = backend.set_image_from_bytes(handle, &bytes, srgb);
                    }
                    RenderCommand::SetImageRgba8 {
                        handle,
                        w,
                        h,
                        rgba,
                        srgb,
                    } => {
                        let _ = backend.set_image_rgba8(handle, w, h, &rgba, srgb);
                    }
                    RenderCommand::SetImageNv12 {
                        handle,
                        w,
                        h,
                        y,
                        uv,
                        full_range,
                    } => {
                        let _ = backend.set_image_nv12(handle, w, h, &y, &uv, full_range);
                    }
                    RenderCommand::RemoveImage { handle } => {
                        backend.remove_image(handle);
                    }
                }
            }
        }

        fn reset_pointer_state(&mut self) {
            self.capture_id = None;
            self.pressed_ids.clear();
            self.mouse_down_pos_px = None;
            self.drag = None;
            self.hover_id = None;
        }

        fn overlay_drag_indicator(&self, scene: &mut Scene) {
            let dragging_internal = self.drag.is_some();
            let dragging_files = self.external_file_drag;

            if !(dragging_internal || dragging_files) {
                return;
            }

            let pos = Vec2 {
                x: self.mouse_pos_px.0,
                y: self.mouse_pos_px.1,
            };

            // Highlight best drop target under cursor (if we have a frame)
            if let Some(f) = &self.frame_cache {
                if let Some(tid) = Self::dnd_target_id_at(f, pos) {
                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == tid) {
                        let color = if dragging_files {
                            Color::from_hex("#FFAA00")
                        } else {
                            Color::from_hex("#44AAFF")
                        };
                        scene.nodes.push(SceneNode::Border {
                            rect: hit.rect,
                            color,
                            width: dp_to_px(2.0),
                            radius: dp_to_px(8.0),
                        });
                    }
                }
            }

            // Cursor badge
            let label = if dragging_files {
                "FILE DROP"
            } else {
                "DRAGGING"
            };
            let bg = if dragging_files {
                Color::from_hex("#FFAA0077")
            } else {
                Color::from_hex("#44AAFF77")
            };

            let badge = Rect {
                x: pos.x + dp_to_px(12.0),
                y: pos.y + dp_to_px(12.0),
                w: dp_to_px(110.0),
                h: dp_to_px(24.0),
            };

            scene.nodes.push(SceneNode::Rect {
                rect: badge,
                brush: Brush::Solid(bg),
                radius: dp_to_px(8.0),
            });
            scene.nodes.push(SceneNode::Text {
                rect: Rect {
                    x: badge.x + dp_to_px(8.0),
                    y: badge.y + dp_to_px(6.0),
                    w: 0.0,
                    h: dp_to_px(14.0),
                },
                text: Arc::<str>::from(label),
                color: Color::WHITE,
                size: dp_to_px(12.0),
            });
        }

        fn is_textfield(&self, id: u64) -> bool {
            if let Some(f) = &self.frame_cache {
                f.semantics_nodes
                    .iter()
                    .any(|n| n.id == id && n.role == Role::TextField)
            } else {
                false
            }
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
            self.clipboard = clipawl::Clipboard::new().ok();

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
                    el.exit();
                }

                WindowEvent::Focused(false) => {
                    // Defensive reset: Wayland/KDE can "eat" releases during DnD.
                    self.external_file_drag = false;
                    self.hovered_files.clear();
                    self.reset_pointer_state();

                    if let Some(w) = &self.window {
                        w.set_ime_allowed(false);
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
                    if let Some(b) = &mut self.backend {
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

                    let pos = Vec2 {
                        x: self.mouse_pos_px.0,
                        y: self.mouse_pos_px.1,
                    };

                    if self.drag.is_some() {
                        self.dnd_update_over(pos);
                        self.request_redraw();
                        return;
                    }

                    if self.dnd_try_begin(pos) {
                        self.dnd_update_over(pos);
                        return;
                    }

                    // Inspector hover
                    if self.inspector.hud.inspector_enabled
                        && let Some(f) = &self.frame_cache
                    {
                        let hover_rect = f
                            .hit_regions
                            .iter()
                            .find(|h| {
                                h.rect.contains(Vec2 {
                                    x: self.mouse_pos_px.0,
                                    y: self.mouse_pos_px.1,
                                })
                            })
                            .map(|h| h.rect);
                        self.inspector.hud.set_hovered(hover_rect);
                        self.request_redraw();
                    }

                    // TextField/TextArea drag selection (if captured)
                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                        && self.is_textfield(cid)
                    {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
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
                                    index_for_xy_bytes(
                                        &st.text, font_px, inner_w, content_x, content_y,
                                    )
                                } else {
                                    index_for_x_bytes(&st.text, font_px, content_x)
                                };

                                st.drag_to(idx);

                                // Ensure caret visible
                                if hit.tf_multiline {
                                    let (cx, cy, _) = caret_xy_for_byte(
                                        &st.text,
                                        font_px,
                                        inner_w,
                                        st.caret_index(),
                                    );
                                    st.ensure_caret_visible_xy(
                                        cx,
                                        cy,
                                        inner_w,
                                        inner_h,
                                        dp_to_px(2.0),
                                    );
                                } else {
                                    let m = measure_text(&st.text, font_px);
                                    let cx =
                                        m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
                                    st.ensure_caret_visible(cx, inner_w, dp_to_px(2.0));
                                }

                                self.request_redraw();
                            }
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

                        for hit in f.hit_regions.iter().rev().filter(|h| h.rect.contains(pos)) {
                            if let Some(cb) = &hit.on_scroll {
                                log::debug!("Calling on_scroll for hit region id={}", hit.id);
                                let before = Vec2 { x: dx_px, y: dy_px };
                                let leftover = cb(before);
                                let consumed_x = (before.x - leftover.x).abs() > 0.001;
                                let consumed_y = (before.y - leftover.y).abs() > 0.001;
                                if consumed_x || consumed_y {
                                    self.request_redraw();
                                    break; // stop after first consumer
                                }
                            }
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
                            self.mouse_down_pos_px = Some(self.mouse_pos_px);
                            self.drag = None;

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
                                        textfield::index_for_xy_bytes(
                                            &st.text,
                                            font_px,
                                            hit.rect.w - 2.0 * pad,
                                            content_x,
                                            content_y,
                                        )
                                    } else {
                                        textfield::index_for_x_bytes(&st.text, font_px, content_x)
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
                                        let m = measure_text(&st.text, font_px);
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
                                    win.set_ime_allowed(true);
                                    win.set_ime_purpose(ImePurpose::Normal);
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
                                    win.set_ime_allowed(false);
                                }
                                self.ime_preedit = false;
                            }
                            self.sched.focused = None;
                            self.request_redraw();
                        }
                    }
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

                    if self.drag.is_some() {
                        self.dnd_finish(pos, true);
                        self.capture_id = None;
                        self.pressed_ids.clear();
                        repose_core::request_frame();
                        return;
                    }

                    if let Some(cid) = self.capture_id {
                        self.pressed_ids.remove(&cid);
                        self.request_redraw();
                    }

                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id) {
                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                            if let Some(cb) = &hit.on_pointer_up {
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
                        }
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

                WindowEvent::ModifiersChanged(new_mods) => {
                    self.modifiers.shift = new_mods.state().shift_key();
                    self.modifiers.ctrl = new_mods.state().control_key();
                    self.modifiers.alt = new_mods.state().alt_key();
                    self.modifiers.meta = new_mods.state().super_key();
                    self.modifiers.command = if cfg!(target_os = "macos") {
                        self.modifiers.meta
                    } else {
                        self.modifiers.ctrl
                    };
                }

                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::BrowserBack)
                            | PhysicalKey::Code(KeyCode::Escape) => {
                                use repose_navigation::back;

                                if self.drag.is_some() {
                                    self.dnd_cancel();
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
                    // Focus traversal: Tab / Shift+Tab
                    if matches!(key_event.physical_key, PhysicalKey::Code(KeyCode::Tab)) {
                        // Only act on initial press, ignore repeats
                        if key_event.state == ElementState::Pressed
                            && !key_event.repeat
                            && let Some(f) = &self.frame_cache
                        {
                            let chain = &f.focus_chain;
                            if !chain.is_empty() {
                                // If a button was "pressed" via keyboard, clear it when we move focus
                                if let Some(active) = self.key_pressed_active.take() {
                                    self.pressed_ids.remove(&active);
                                }

                                let shift = self.modifiers.shift;
                                let current = self.sched.focused;
                                let next = if let Some(cur) = current {
                                    if let Some(idx) = chain.iter().position(|&id| id == cur) {
                                        if shift {
                                            if idx == 0 {
                                                chain[chain.len() - 1]
                                            } else {
                                                chain[idx - 1]
                                            }
                                        } else {
                                            chain[(idx + 1) % chain.len()]
                                        }
                                    } else {
                                        chain[0]
                                    }
                                } else {
                                    chain[0]
                                };
                                self.sched.focused = Some(next);

                                // IME only for TextField
                                if let Some(win) = &self.window {
                                    if f.semantics_nodes
                                        .iter()
                                        .any(|n| n.id == next && n.role == Role::TextField)
                                    {
                                        win.set_ime_allowed(true);
                                        win.set_ime_purpose(ImePurpose::Normal);
                                    } else {
                                        win.set_ime_allowed(false);
                                    }
                                }
                                self.announce_focus_change();
                                self.request_redraw();
                            }
                        }
                        return; // swallow Tab
                    }

                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && self.modifiers.command
                    {
                        use repose_core::shortcuts::Action;

                        let handled = match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::KeyC) => self.dispatch_action(Action::Copy),
                            PhysicalKey::Code(KeyCode::KeyX) => self.dispatch_action(Action::Cut),
                            PhysicalKey::Code(KeyCode::KeyV) => self.dispatch_action(Action::Paste),
                            PhysicalKey::Code(KeyCode::KeyA) => {
                                self.dispatch_action(Action::SelectAll)
                            }
                            PhysicalKey::Code(KeyCode::KeyZ) => {
                                self.dispatch_action(if self.modifiers.shift {
                                    Action::Redo
                                } else {
                                    Action::Undo
                                })
                            }
                            PhysicalKey::Code(KeyCode::KeyF) => self.dispatch_action(Action::Find),
                            PhysicalKey::Code(KeyCode::KeyS) => self.dispatch_action(Action::Save),
                            _ => false,
                        };

                        if handled {
                            self.request_redraw();
                            return;
                        }
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
                    if key_event.state == ElementState::Pressed
                        && !key_event.repeat
                        && let PhysicalKey::Code(KeyCode::Enter) = key_event.physical_key
                        && let Some(focused_id) = self.sched.focused
                        && let Some(f) = &self.frame_cache
                        && let Some(hit) = f.hit_regions.iter().find(|h| h.id == focused_id)
                        && let Some(on_submit) = &hit.on_text_submit
                    {
                        let key = self.tf_key_of(focused_id);

                        if let Some(state) = self.textfield_states.get(&key) {
                            let text = state.borrow().text.clone();
                            on_submit(text);
                            self.request_redraw();
                            return; // don't continue as button activation
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
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Delete) => {
                                        state.delete_forward();
                                        let new_text = state.text.clone();
                                        self.notify_text_change(focused_id, new_text);
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowLeft) => {
                                        state.move_cursor(-1, self.modifiers.shift);
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::ArrowRight) => {
                                        state.move_cursor(1, self.modifiers.shift);
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::Home) => {
                                        state.selection = 0..0;
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::End) => {
                                        {
                                            let end = state.text.len();
                                            state.selection = end..end;
                                        }
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    PhysicalKey::Code(KeyCode::KeyA) if self.modifiers.ctrl => {
                                        state.selection = 0..state.text.len();
                                        App::tf_ensure_caret_visible(&mut state);
                                        self.request_redraw();
                                    }
                                    _ => {}
                                }
                            }
                            if self.modifiers.ctrl {
                                match key_event.physical_key {
                                    PhysicalKey::Code(KeyCode::KeyC) => {
                                        if let Some(fid) = self.sched.focused {
                                            let key = self.tf_key_of(fid);
                                            if let Some(state) = self.textfield_states.get(&key) {
                                                let txt = state.borrow().selected_text();
                                                if !txt.is_empty() {
                                                    let _ = self.copy_to_clipboard(txt);
                                                }
                                            }
                                        }
                                        return;
                                    }
                                    PhysicalKey::Code(KeyCode::KeyX) => {
                                        if let Some(fid) = self.sched.focused {
                                            let key = self.tf_key_of(fid);
                                            if let Some(state_rc) =
                                                self.textfield_states.get(&key).cloned()
                                            {
                                                // Copy
                                                let txt = state_rc.borrow().selected_text();
                                                if !txt.is_empty() {
                                                    {
                                                        let _ = self.copy_to_clipboard(txt.clone());
                                                    }
                                                    // Cut (delete selection)
                                                    {
                                                        let mut st = state_rc.borrow_mut();
                                                        st.insert_text(""); // replace selection with empty
                                                        let new_text = st.text.clone();
                                                        self.notify_text_change(
                                                            focused_id, new_text,
                                                        );
                                                        App::tf_ensure_caret_visible(&mut st);
                                                    }
                                                    self.request_redraw();
                                                }
                                            }
                                        }
                                        return;
                                    }
                                    PhysicalKey::Code(KeyCode::KeyV) => {
                                        if let Some(fid) = self.sched.focused {
                                            let key = self.tf_key_of(fid);
                                            if let Some(state_rc) =
                                                self.textfield_states.get(&key).cloned()
                                                && let Some(mut txt) = self.paste_from_clipboard()
                                            {
                                                // Single-line TextField: strip control/newlines
                                                txt.retain(|c| {
                                                    !c.is_control() && c != '\n' && c != '\r'
                                                });
                                                if !txt.is_empty() {
                                                    let mut st = state_rc.borrow_mut();
                                                    st.insert_text(&txt);
                                                    let new_text = st.text.clone();
                                                    self.notify_text_change(focused_id, new_text);
                                                    App::tf_ensure_caret_visible(&mut st);
                                                    self.request_redraw();
                                                }
                                            }
                                        }
                                        return;
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
                                    App::tf_ensure_caret_visible(&mut st);
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
                                        && let Some(cb) = &hit.on_click
                                    {
                                        cb();
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
                    use winit::event::Ime;
                    if let Some(focused_id) = self.sched.focused {
                        let key = self.tf_key_of(focused_id);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut state = state_rc.borrow_mut();
                            match ime {
                                Ime::Enabled => {
                                    // IME allowed, but not necessarily composing
                                    self.ime_preedit = false;
                                }
                                Ime::Preedit(text, cursor) => {
                                    let cursor_usize = cursor.map(|(a, b)| (a, b));
                                    state.set_composition(text.clone(), cursor_usize);
                                    self.ime_preedit = !text.is_empty();
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == focused_id)
                                    {
                                        let inner = Rect {
                                            x: hit.rect.x + dp_to_px(TF_PADDING_X_DP),
                                            y: hit.rect.y,
                                            w: hit.rect.w,
                                            h: hit.rect.h,
                                        };
                                        tf_ensure_visible_in_rect(&mut state, inner);
                                    }
                                    // notify on-change if you wired it:
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.request_redraw();
                                }
                                Ime::Commit(text) => {
                                    state.commit_composition(text);
                                    self.ime_preedit = false;
                                    if let Some(f) = &self.frame_cache
                                        && let Some(hit) =
                                            f.hit_regions.iter().find(|h| h.id == focused_id)
                                    {
                                        let inner = Rect {
                                            x: hit.rect.x + dp_to_px(TF_PADDING_X_DP),
                                            y: hit.rect.y,
                                            w: hit.rect.w,
                                            h: hit.rect.h,
                                        };
                                        tf_ensure_visible_in_rect(&mut state, inner);
                                    }
                                    self.notify_text_change(focused_id, state.text.clone());
                                    self.request_redraw();
                                }
                                Ime::Disabled => {
                                    self.ime_preedit = false;
                                    if state.composition.is_some() {
                                        state.cancel_composition();
                                        if let Some(f) = &self.frame_cache
                                            && let Some(hit) =
                                                f.hit_regions.iter().find(|h| h.id == focused_id)
                                        {
                                            let inner = Rect {
                                                x: hit.rect.x + dp_to_px(TF_PADDING_X_DP),
                                                y: hit.rect.y,
                                                w: hit.rect.w,
                                                h: hit.rect.h,
                                            };
                                            tf_ensure_visible_in_rect(&mut state, inner);
                                        }
                                        self.notify_text_change(focused_id, state.text.clone());
                                    }
                                    self.request_redraw();
                                }
                            }
                        }
                    }
                }

                WindowEvent::RedrawRequested => {
                    // 1. Process any pending A11y actions (clicks from screen reader)
                    self.process_a11y_actions();
                    self.dispatch_file_drop_now();
                    self.process_render_commands();

                    let Some(win) = self.window.as_ref() else {
                        return;
                    };
                    if self.backend.is_none() {
                        return;
                    }

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
                    self.inspector.hud.metrics = Some(repose_devtools::Metrics {
                        build_layout_ms,
                        scene_nodes: scene.nodes.len(),
                    });
                    self.inspector.frame(&mut scene);

                    // Drag indicator overlay (internal + file drop)
                    self.overlay_drag_indicator(&mut scene);

                    // Now borrow backend mutably only for the frame() call
                    let win = self.window.as_ref().unwrap();
                    let scale = win.scale_factor() as f32;
                    if let Some(backend) = self.backend.as_mut() {
                        backend.frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                    }

                    self.frame_cache = Some(frame);
                    self.last_redraw = Instant::now();
                }

                _ => {}
            }
        }

        fn about_to_wait(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            if take_frame_request() {
                self.pending_redraw = true;
            }
            if !self.pending_redraw {
                return;
            }

            let now = Instant::now();
            let interval = web_time::Duration::from_millis(16);

            if now.saturating_duration_since(self.last_redraw) >= interval {
                self.pending_redraw = false;
                self.request_redraw();
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
        fn user_event(&mut self, _: &winit::event_loop::ActiveEventLoop, _: ()) {}
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
            if let Some(f) = &self.frame_cache
                && let Some(hr) = f.hit_regions.iter().find(|h| h.id == visual_id)
            {
                return hr.tf_state_key.unwrap_or(hr.id);
            }
            visual_id
        }

        fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            use repose_core::shortcuts;

            if let (Some(f), Some(fid)) = (&self.frame_cache, self.sched.focused) {
                if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                    if let Some(cb) = &hit.on_action {
                        if cb(action.clone()) {
                            return true;
                        }
                    }
                }
            }

            if shortcuts::handle(action.clone()) {
                return true;
            }

            self.dispatch_default_action(action)
        }

        fn dispatch_default_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
            use repose_core::shortcuts::Action;

            let Some(fid) = self.sched.focused else {
                return false;
            };
            let key = self.tf_key_of(fid);
            let Some(state_rc) = self.textfield_states.get(&key).cloned() else {
                return false;
            };

            match action {
                Action::Copy => {
                    let txt = state_rc.borrow().selected_text();
                    if txt.is_empty() {
                        return false;
                    }
                    self.copy_to_clipboard(txt);
                    true
                }
                Action::Cut => {
                    let txt = state_rc.borrow().selected_text();
                    if txt.is_empty() {
                        return false;
                    }
                    self.copy_to_clipboard(txt);
                    {
                        let mut st = state_rc.borrow_mut();
                        st.insert_text("");
                        self.notify_text_change(fid, st.text.clone());
                        App::tf_ensure_caret_visible(&mut st);
                    }
                    true
                }
                Action::Paste => {
                    let Some(mut txt) = self.paste_from_clipboard() else {
                        return false;
                    };
                    txt.retain(|c| !c.is_control() && c != '\n' && c != '\r');
                    if txt.is_empty() {
                        return false;
                    }
                    {
                        let mut st = state_rc.borrow_mut();
                        st.insert_text(&txt);
                        self.notify_text_change(fid, st.text.clone());
                        App::tf_ensure_caret_visible(&mut st);
                    }
                    true
                }
                Action::SelectAll => {
                    {
                        let mut st = state_rc.borrow_mut();
                        st.selection = 0..st.text.len();
                        App::tf_ensure_caret_visible(&mut st);
                    }
                    true
                }
                _ => false,
            }
        }

        fn is_dnd_target(hit: &HitRegion) -> bool {
            hit.on_drop.is_some()
                || hit.on_drag_enter.is_some()
                || hit.on_drag_over.is_some()
                || hit.on_drag_leave.is_some()
        }

        fn dnd_slop_px(&self) -> f32 {
            dp_to_px(6.0)
        }

        fn dnd_target_id_at(f: &Frame, pos: Vec2) -> Option<u64> {
            f.hit_regions
                .iter()
                .rev()
                .filter(|h| h.rect.contains(pos))
                .find(|h| Self::is_dnd_target(h))
                .map(|h| h.id)
        }

        fn dnd_update_over(&mut self, pos: Vec2) {
            let Some(f) = &self.frame_cache else {
                return;
            };
            let Some(session) = self.drag.as_mut() else {
                return;
            };

            let new_over = Self::dnd_target_id_at(f, pos);

            if new_over != session.over_id {
                if let Some(prev) = session.over_id {
                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == prev) {
                        if let Some(cb) = &hit.on_drag_leave {
                            cb(repose_core::dnd::DragOver {
                                source_id: session.source_id,
                                target_id: prev,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }

                if let Some(now) = new_over {
                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == now) {
                        if let Some(cb) = &hit.on_drag_enter {
                            cb(repose_core::dnd::DragOver {
                                source_id: session.source_id,
                                target_id: now,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }

                session.over_id = new_over;
            }

            if let Some(over) = session.over_id {
                if let Some(hit) = f.hit_regions.iter().find(|h| h.id == over) {
                    if let Some(cb) = &hit.on_drag_over {
                        cb(repose_core::dnd::DragOver {
                            source_id: session.source_id,
                            target_id: over,
                            position: pos,
                            modifiers: self.modifiers,
                            payload: session.payload.clone(),
                        });
                    }
                }
            }
        }

        fn dnd_try_begin(&mut self, pos: Vec2) -> bool {
            if self.drag.is_some() {
                return true;
            }

            let Some((sx, sy)) = self.mouse_down_pos_px else {
                return false;
            };
            let Some(cid) = self.capture_id else {
                return false;
            };
            if !self.pressed_ids.contains(&cid) {
                return false;
            }

            let dx = pos.x - sx;
            let dy = pos.y - sy;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < self.dnd_slop_px() {
                return false;
            }

            let Some(f) = &self.frame_cache else {
                return false;
            };
            let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) else {
                return false;
            };

            let Some(cb) = &hit.on_drag_start else {
                return false;
            };

            let payload = cb(repose_core::dnd::DragStart {
                source_id: cid,
                position: pos,
                modifiers: self.modifiers,
            });
            let Some(payload) = payload else {
                return false;
            };

            self.drag = Some(DragSession {
                source_id: cid,
                payload,
                start_px: (sx, sy),
                over_id: None,
            });

            // Don't keep "pressed" visuals once dragging
            self.pressed_ids.remove(&cid);
            self.request_redraw();
            true
        }

        fn dnd_finish(&mut self, pos: Vec2, accept_if_possible: bool) {
            let Some(f) = &self.frame_cache else {
                self.drag = None;
                self.capture_id = None;
                self.mouse_down_pos_px = None;
                self.request_redraw();
                return;
            };

            let Some(session) = self.drag.take() else {
                return;
            };

            let mut accepted = false;

            if accept_if_possible {
                let drop_target = Self::dnd_target_id_at(f, pos);
                if let Some(tid) = drop_target {
                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == tid) {
                        if let Some(cb) = &hit.on_drop {
                            accepted = cb(repose_core::dnd::DropEvent {
                                source_id: session.source_id,
                                target_id: tid,
                                position: pos,
                                modifiers: self.modifiers,
                                payload: session.payload.clone(),
                            });
                        }
                    }
                }
            }

            // Notify source end
            if let Some(source_hit) = f.hit_regions.iter().find(|h| h.id == session.source_id) {
                if let Some(cb) = &source_hit.on_drag_end {
                    cb(repose_core::dnd::DragEnd { accepted });
                }
            }

            self.capture_id = None;
            self.mouse_down_pos_px = None;
            self.request_redraw();
        }

        fn dnd_cancel(&mut self) {
            let pos = Vec2 {
                x: self.mouse_pos_px.0,
                y: self.mouse_pos_px.1,
            };
            self.dnd_finish(pos, false);
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

            let Some(target_id) = Self::dnd_target_id_at(f, pos) else {
                self.pending_drop_pos_px = None;
                return;
            };

            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == target_id) {
                if let Some(cb) = &hit.on_drop {
                    let accepted = cb(repose_core::dnd::DropEvent {
                        source_id: 0, // external source (OS)
                        target_id,
                        position: pos,
                        modifiers: self.modifiers,
                        payload: payload.clone(),
                    });

                    if accepted {
                        if let Some(node) = f.semantics_nodes.iter().find(|n| n.id == target_id) {
                            let label = node.label.as_deref().unwrap_or("");
                            self.a11y.announce(&format!("Dropped files on {}", label));
                        }
                    }
                }
            }

            self.pending_drop_pos_px = None;
            self.request_redraw();
        }
    }

    let event_loop = EventLoop::new()?;
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
