//! Platform runners
use repose_core::locals::dp_to_px;
use repose_core::*;
use repose_ui::textfield::{TF_FONT_DP, TF_PADDING_X_DP, index_for_x_bytes, measure_text};
use std::cell::RefCell;
use std::rc::Rc;
use web_time::Instant;

#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;

#[cfg(all(target_arch = "wasm32"))]
pub mod web;

mod common;

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
    let font_dp = TF_FONT_DP as u32;
    let m = measure_text(&state.text, font_dp);
    let caret_x_px = m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
    state.ensure_caret_visible(caret_x_px, inner_rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP));
}

#[cfg(feature = "desktop")]
pub fn run_desktop_app(root: impl FnMut(&mut Scheduler) -> View + 'static) -> anyhow::Result<()> {
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;
    use std::sync::Arc;

    use repose_ui::TextFieldState;
    use winit::application::ApplicationHandler;
    use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
    use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
    use winit::event_loop::EventLoop;
    use winit::keyboard::{KeyCode, PhysicalKey};
    use winit::window::{ImePurpose, Window, WindowAttributes};

    struct App {
        // App state
        root: Box<dyn FnMut(&mut Scheduler) -> View>,
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
        key_pressed_active: Option<u64>, // for Space/Enter press/release activation
        clipboard: Option<clipawl::Clipboard>,
        a11y: Box<dyn A11yBridge>,
        last_focus: Option<u64>,
    }

    impl App {
        fn new(root: Box<dyn FnMut(&mut Scheduler) -> View>) -> Self {
            Self {
                root,
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
            }
        }

        fn request_redraw(&self) {
            if let Some(w) = &self.window {
                w.request_redraw();
            }
        }

        // Ensure caret is visible after edits/moves (all units in px)
        fn tf_ensure_caret_visible(st: &mut TextFieldState) {
            let font_dp = TF_FONT_DP as u32;
            let m = measure_text(&st.text, font_dp);
            let caret_x_px = m.positions.get(st.caret_index()).copied().unwrap_or(0.0);
            st.ensure_caret_visible(caret_x_px, st.inner_width);
        }

        fn copy_to_clipboard(&mut self, text: String) {
            if let Some(cb) = &mut self.clipboard {
                // pollster::block_on executes synchronously (since CAwl is async)
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
    }

    impl ApplicationHandler<()> for App {
        fn resumed(&mut self, el: &winit::event_loop::ActiveEventLoop) {
            self.clipboard = clipawl::Clipboard::new().ok();
            // Create the window once when app resumes.
            if self.window.is_none() {
                match el.create_window(
                    WindowAttributes::default()
                        .with_title("Repose")
                        .with_inner_size(PhysicalSize::new(1280, 800)),
                ) {
                    Ok(win) => {
                        let w = Arc::new(win);
                        let size = w.inner_size();
                        self.sched.size = (size.width, size.height);
                        // Create WGPU backend
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
            match event {
                WindowEvent::CloseRequested => {
                    el.exit();
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

                    if let (Some(f), Some(cid)) = (&self.frame_cache, self.capture_id)
                        && let Some(_sem) = f
                            .semantics_nodes
                            .iter()
                            .find(|n| n.id == cid && n.role == Role::TextField)
                    {
                        let key = self.tf_key_of(cid);
                        if let Some(state_rc) = self.textfield_states.get(&key) {
                            let mut state = state_rc.borrow_mut();
                            // inner content left edge in px
                            let inner_x_px = f
                                .hit_regions
                                .iter()
                                .find(|h| h.id == cid)
                                .map(|h| h.rect.x + dp_to_px(TF_PADDING_X_DP))
                                .unwrap_or(0.0);
                            let content_x_px =
                                self.mouse_pos_px.0 - inner_x_px + state.scroll_offset;
                            let font_dp = TF_FONT_DP as u32;
                            let idx =
                                index_for_x_bytes(&state.text, font_dp, content_x_px.max(0.0));
                            state.drag_to(idx);

                            // Scroll caret into view
                            let m = measure_text(&state.text, font_dp);
                            let caret_x_px =
                                m.positions.get(state.caret_index()).copied().unwrap_or(0.0);
                            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid) {
                                state.ensure_caret_visible(
                                    caret_x_px,
                                    hit.rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                );
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
                            // Capture starts on press
                            self.capture_id = Some(hit.id);
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
                                    Rc::new(RefCell::new(
                                        repose_ui::textfield::TextFieldState::new(),
                                    ))
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

                            // TextField: place caret and start drag selection
                            if let Some(_sem) = f
                                .semantics_nodes
                                .iter()
                                .find(|n| n.id == hit.id && n.role == Role::TextField)
                            {
                                let key = self.tf_key_of(hit.id);
                                if let Some(state_rc) = self.textfield_states.get(&key) {
                                    let mut state = state_rc.borrow_mut();
                                    let inner_x_px = hit.rect.x + dp_to_px(TF_PADDING_X_DP);
                                    let content_x_px =
                                        self.mouse_pos_px.0 - inner_x_px + state.scroll_offset;
                                    let font_dp = TF_FONT_DP as u32;
                                    let idx = index_for_x_bytes(
                                        &state.text,
                                        font_dp,
                                        content_x_px.max(0.0),
                                    );
                                    state.begin_drag(idx, self.modifiers.shift);
                                    let m = measure_text(&state.text, font_dp);
                                    let caret_x_px = m
                                        .positions
                                        .get(state.caret_index())
                                        .copied()
                                        .unwrap_or(0.0);
                                    state.ensure_caret_visible(
                                        caret_x_px,
                                        hit.rect.w - 2.0 * dp_to_px(TF_PADDING_X_DP),
                                    );
                                }
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
                    if let Some(cid) = self.capture_id {
                        self.pressed_ids.remove(&cid);
                        self.request_redraw();
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
                }
                WindowEvent::ModifiersChanged(new_mods) => {
                    self.modifiers.shift = new_mods.state().shift_key();
                    self.modifiers.ctrl = new_mods.state().control_key();
                    self.modifiers.alt = new_mods.state().alt_key();
                    self.modifiers.meta = new_mods.state().super_key();
                }
                WindowEvent::KeyboardInput {
                    event: key_event, ..
                } => {
                    if key_event.state == ElementState::Pressed && !key_event.repeat {
                        match key_event.physical_key {
                            PhysicalKey::Code(KeyCode::BrowserBack)
                            | PhysicalKey::Code(KeyCode::Escape) => {
                                use repose_navigation::back;

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
                                // If a button was “pressed” via keyboard, clear it when we move focus
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
                            return; // don’t continue as button activation
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
                                    self.notify_text_change(fid, text.clone());
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
                    if let (Some(backend), Some(win)) =
                        (self.backend.as_mut(), self.window.as_ref())
                    {
                        let t0 = Instant::now();
                        let scale = win.scale_factor() as f32;
                        let size_px_u32 = self.sched.size;
                        let focused = self.sched.focused;

                        let frame = compose_frame(
                            &mut self.sched,
                            &mut self.root,
                            scale,
                            size_px_u32,
                            self.hover_id,
                            &self.pressed_ids,
                            &self.textfield_states,
                            focused,
                        );

                        let build_layout_ms = (Instant::now() - t0).as_secs_f32() * 1000.0;

                        // A11y: publish semantics tree each frame (cheap for now)
                        self.a11y.publish_tree(&frame.semantics_nodes);
                        // If focus id changed since last publish, send focused node
                        if self.last_focus != self.sched.focused {
                            let focused_node = self
                                .sched
                                .focused
                                .and_then(|id| frame.semantics_nodes.iter().find(|n| n.id == id));
                            self.a11y.focus_changed(focused_node);
                            self.last_focus = self.sched.focused;
                        }

                        // Render
                        let mut scene = frame.scene.clone();
                        // Update HUD metrics before overlay draws
                        self.inspector.hud.metrics = Some(repose_devtools::Metrics {
                            build_layout_ms,
                            scene_nodes: scene.nodes.len(),
                        });
                        self.inspector.frame(&mut scene);
                        backend
                            // .lock()
                            .frame(&scene, GlyphRasterConfig { px: 18.0 * scale });
                        self.frame_cache = Some(frame);
                    }
                }
                _ => {}
            }
        }

        fn about_to_wait(&mut self, _el: &winit::event_loop::ActiveEventLoop) {
            self.request_redraw();
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
