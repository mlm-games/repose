use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use repose_core::dnd;
use repose_core::input::{
    ImeEvent, Key, KeyEvent, KeyEventType, Modifiers, PointerButton, PointerEvent,
    PointerEventKind, PointerId, PointerKind,
};
use repose_core::locals::{Density, dp_to_px, set_density_default, with_density};
use repose_core::runtime::{Frame, Scheduler};
use repose_core::shortcuts::DragAction;
use repose_core::{
    CursorIcon, HitRegion, Interaction, RenderContext, Scene, Vec2, View, request_frame,
    take_focus_request,
};
use repose_ui::textfield::{
    TF_FONT_DP, TextFieldState, TextMeasureConfig, caret_xy_for_byte, measure_text,
};
use repose_ui::{Interactions, layout_and_paint};

/// Platform-directed side effects requested by the UI.
#[derive(Clone, Default)]
pub struct PlatformOutput {
    /// Cursor to display (None = default/system cursor).
    pub cursor: Option<CursorIcon>,
    /// Whether IME input is allowed for the currently focused widget.
    pub ime_allowed: bool,
    /// IME cursor area in logical (DPI-scaled) coordinates: (x, y, width, height).
    pub ime_cursor_area: Option<(f64, f64, f64, f64)>,
    /// Text to write to the clipboard (transient - set once per frame, cleared after read).
    pub clipboard_text: Option<String>,

    /// IME / soft-keyboard hints for the focused text field. The host should
    /// apply these to the OS keyboard and to `set_ime_purpose` / web attrs.
    pub ime_purpose: repose_core::ImePurposeHint,
    pub ime_auto_correct: bool,
    pub ime_capitalization: repose_core::KeyboardCapitalization,
    pub keyboard_type: repose_core::KeyboardType,

    /// Whether the app theme is dark, so the host can sync OS window chrome
    /// (titlebar, caption buttons). `None` = don't touch the OS chrome.
    pub window_theme_dark: Option<bool>,
}

/// Output of a single frame: the rendered scene plus metadata for the host.
pub struct FrameOutput {
    /// The scene graph for rendering.
    pub scene: Scene,
    /// Hit regions for pointer dispatch between frames.
    pub hit_regions: Vec<HitRegion>,
    /// Semantics nodes for a11y.
    pub semantics_nodes: Vec<repose_core::runtime::SemNode>,
    /// Focus chain for tab navigation.
    pub focus_chain: Vec<u64>,
    /// Platform-side effects (cursor, IME, clipboard).
    pub platform: PlatformOutput,
    /// Whether the UI wants pointer events (if false, host can pass events through).
    pub wants_pointer: bool,
    /// Whether the UI wants keyboard events (if false, host can pass events through).
    pub wants_keyboard: bool,
}

/// Result of a pointer-move event processed by the runtime.
pub struct PointerMoveResult {
    /// Updated cursor suggestion for the host.
    pub cursor: Option<CursorIcon>,
    /// The id of the element under the pointer, if any.
    pub hover_id: Option<u64>,
}

/// Result of a pointer-button event processed by the runtime.
pub struct PointerButtonResult {
    /// Id of the element that received focus (if any).
    pub focused: Option<u64>,
    /// Id of the captured element.
    pub capture_id: Option<u64>,
    /// Whether the event was consumed by the UI.
    pub consumed: bool,
    /// Whether an accessibility announcement was triggered.
    pub needs_a11y_announce: bool,
}

/// Embeddable Repose runtime.
///
/// Manages composition scheduling, input routing, text-field state, and
/// pointer/key dispatch.  The host owns the event loop and GPU device; this
/// is purely the UI logic layer.
pub struct ReposeRuntime {
    pub sched: Scheduler,
    pub scale: f32,

    // Input state
    pub modifiers: Modifiers,
    pub mouse_pos_px: (f32, f32),
    /// Whether the pointer is currently inside the window.
    pub pointer_inside: bool,
    pub hover_id: Option<u64>,
    /// Needed so `Leave` still fires
    /// even when the hovered hit region is removed from the tree between frames.
    /// Rebuilt on every `cache_frame`.
    hover_leave: HashMap<u64, (f32, f32, f32, f32, Rc<dyn Fn(PointerEvent)>)>,
    pub capture_id: Option<u64>,
    /// Hit path captured at pointer-down: every region under the pointer,
    /// ordered bottom-up (deepest child first, ancestors last).
    pub hit_path: Option<Vec<u64>>,
    /// Which scroll consumer currently owns the wheel gesture.
    pub scroll_capture_id: Option<u64>,
    last_scroll_at: Option<web_time::Instant>,
    pub pressed_ids: HashSet<u64>,
    pub ime_preedit: bool,
    pub key_pressed_active: Option<u64>,
    pub last_focus: Option<u64>,

    // Per-frame cache for hit testing
    pub frame_cache: Option<Frame>,

    // Platform output accumulator (cursor changes, etc.)
    cursor: Option<CursorIcon>,

    // Text field state
    pub textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
}

impl ReposeRuntime {
    pub fn new() -> Self {
        Self {
            sched: Scheduler::new(),
            scale: 1.0,
            modifiers: Modifiers::default(),
            mouse_pos_px: (0.0, 0.0),
            pointer_inside: false,
            hover_id: None,
            hover_leave: HashMap::new(),
            capture_id: None,
            hit_path: None,
            scroll_capture_id: None,
            last_scroll_at: None,
            pressed_ids: HashSet::new(),
            ime_preedit: false,
            key_pressed_active: None,
            last_focus: None,
            frame_cache: None,
            cursor: None,
            textfield_states: HashMap::new(),
        }
    }

    /// Set the logical viewport size (in device pixels).
    pub fn set_viewport(&mut self, width_px: u32, height_px: u32) {
        self.sched.size = (width_px, height_px);
    }

    /// Set viewport size and DPI scale factor.
    pub fn set_viewport_and_scale(&mut self, width_px: u32, height_px: u32, scale: f32) {
        self.scale = scale;
        self.sched.size = (width_px, height_px);
    }

    /// Advance animations. Call before `compose` each frame.
    pub fn tick_animations(&self) {
        repose_core::animation_driver::tick();
    }

    /// Compose and layout a frame, returning the output for rendering.
    ///
    /// Call `tick_animations` before this and `cache_frame` after (once you
    /// have applied any host-specific overlays like the devtools inspector).
    pub fn compose<F>(&mut self, root_fn: &mut F, render_ctx: &RenderContext) -> Frame
    where
        F: FnMut(&mut Scheduler, &RenderContext) -> View,
    {
        let size = self.sched.size;
        let rc = render_ctx.clone();
        // Root-level panic guard: a stray panic during compose must not kill the
        // event loop / freeze the hosted demo.
        let mut compose_once = |this: &mut Self| {
            let mut inner = |s: &mut Scheduler| (root_fn)(s, &rc);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compose_frame_inner(
                    &mut this.sched,
                    &mut inner,
                    this.scale,
                    size,
                    this.hover_id,
                    &this.pressed_ids,
                    &this.textfield_states,
                )
            })) {
                Ok(frame) => frame,
                Err(_) => {
                    log::error!("compose panicked; presenting last good frame");
                    this.frame_cache.clone().unwrap_or_else(|| Frame {
                        scene: Default::default(),
                        hit_regions: Vec::new(),
                        semantics_nodes: Vec::new(),
                        focus_chain: Vec::new(),
                    })
                }
            }
        };

        let frame = compose_once(self);

        // Reconcile hover against the *new* hit list before presenting. If the
        // hover target changed, recompose once so paint uses the correct
        // Interactions.hover (eliminates 1-frame sticky/wrong hover).
        let hover_before = self.hover_id;
        self.reconcile_hover_from_mouse_pos(&frame);
        if self.hover_id != hover_before {
            // Refresh the retained leave map from the first frame so Leave on
            // further changes still works (cache_frame does this fully).
            self.hover_leave.clear();
            for h in &frame.hit_regions {
                if let Some(cb) = &h.on_pointer_leave {
                    self.hover_leave
                        .insert(h.id, (h.rect.x, h.rect.y, h.rect.w, h.rect.h, cb.clone()));
                }
            }
            // Hover should be stable: same geometry + same pointer. Do not loop.
            return compose_once(self);
        }
        frame
    }

    /// Compose a frame and return structured output for the host.
    pub fn frame(
        &mut self,
        mut root_fn: impl FnMut(&mut Scheduler, &RenderContext) -> View,
        render_ctx: &RenderContext,
    ) -> FrameOutput {
        let captured = Rc::new(RefCell::new(None::<String>));
        let hook = captured.clone();
        repose_core::clipboard::set_clipboard_observer(Box::new(move |text| {
            *hook.borrow_mut() = Some(text.to_string());
        }));

        let f = self.compose(&mut root_fn, render_ctx);

        repose_core::clipboard::clear_clipboard_observer();
        let clipboard_text = captured.borrow_mut().take();

        let wants_pointer = self.hover_id.is_some() || self.capture_id.is_some();
        let wants_keyboard = !self.textfield_states.is_empty() || self.ime_preedit;

        let ime_allowed = self.sched.focused.is_some_and(|fid| {
            f.semantics_nodes
                .iter()
                .any(|n| n.id == fid && n.role == repose_core::semantics::Role::TextField)
        });

        let focused_hit = self
            .sched
            .focused
            .and_then(|fid| f.hit_regions.iter().find(|h| h.id == fid));

        let ime_cursor_area = if ime_allowed {
            focused_hit.map(|hit| {
                let sf = self.scale as f64;
                (
                    hit.rect.x as f64 / sf,
                    hit.rect.y as f64 / sf,
                    hit.rect.w as f64 / sf,
                    hit.rect.h as f64 / sf,
                )
            })
        } else {
            None
        };

        let (ime_purpose, ime_auto_correct, ime_capitalization, keyboard_type) =
            match (ime_allowed, focused_hit) {
                (true, Some(hit)) => (
                    hit.keyboard_type.ime_purpose_hint(),
                    hit.auto_correct.unwrap_or(true),
                    hit.capitalization,
                    hit.keyboard_type,
                ),
                _ => (
                    repose_core::ImePurposeHint::Normal,
                    true,
                    repose_core::KeyboardCapitalization::Unspecified,
                    repose_core::KeyboardType::Unspecified,
                ),
            };

        let platform = PlatformOutput {
            cursor: self.take_cursor_suggestion(),
            ime_allowed,
            ime_cursor_area,
            clipboard_text,
            ime_purpose,
            ime_auto_correct,
            ime_capitalization,
            keyboard_type,
            window_theme_dark: Some(repose_core::locals::theme().is_dark()),
        };
        FrameOutput {
            scene: f.scene,
            hit_regions: f.hit_regions,
            semantics_nodes: f.semantics_nodes,
            focus_chain: f.focus_chain,
            platform,
            wants_pointer,
            wants_keyboard,
        }
    }

    /// Store the composed frame for event hit testing.
    pub fn cache_frame(&mut self, frame: Frame) {
        self.hover_leave.clear();
        for h in &frame.hit_regions {
            if let Some(cb) = &h.on_pointer_leave {
                self.hover_leave
                    .insert(h.id, (h.rect.x, h.rect.y, h.rect.w, h.rect.h, cb.clone()));
            }
        }
        self.frame_cache = Some(frame);
    }

    fn dispatch_pointer_to_path(&self, kind: PointerEventKind, pos: Vec2, path: &[u64]) {
        let Some(f) = &self.frame_cache else {
            return;
        };
        let base = PointerEvent::new(
            PointerId(0),
            PointerKind::Mouse,
            kind,
            pos,
            1.0,
            self.modifiers,
        );
        for &id in path {
            let Some(h) = f.hit_regions.iter().find(|h| h.id == id) else {
                continue;
            };
            let cb = match kind {
                PointerEventKind::Down(_) => &h.on_pointer_down,
                PointerEventKind::Up(_) => &h.on_pointer_up,
                PointerEventKind::Move => &h.on_pointer_move,
                PointerEventKind::Cancel => &h.on_pointer_cancel,
                PointerEventKind::Enter | PointerEventKind::Leave => continue,
            };
            let Some(cb) = cb else {
                continue;
            };
            let mut ev = base.clone();
            ev.origin = Vec2 {
                x: h.rect.x,
                y: h.rect.y,
            };
            ev.position = pos - ev.origin;
            cb(ev);
            if base.is_consumed() {
                break;
            }
        }
    }

    /// Process a pointer-move event. Returns cursor suggestion.
    pub fn handle_pointer_move(&mut self, pos: Vec2) -> PointerMoveResult {
        self.mouse_pos_px = (pos.x, pos.y);

        // DnD move
        if dnd::handle_drag_action(&DragAction::Move {
            position: pos,
            modifiers: self.modifiers,
        }) {
            request_frame();
            return PointerMoveResult {
                cursor: if dnd::is_dragging() {
                    Some(CursorIcon::Grabbing)
                } else {
                    self.cursor
                },
                hover_id: self.hover_id,
            };
        }

        let Some(f) = &self.frame_cache else {
            return PointerMoveResult {
                cursor: None,
                hover_id: None,
            };
        };

        // TextField/TextArea drag selection (if captured)
        if let Some(cid) = self.capture_id
            && is_textfield_in_frame(f, cid)
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
        {
            let key = tf_key_of(f, cid);
            if let Some(st_rc) = self.textfield_states.get(&key) {
                let mut st = st_rc.borrow_mut();
                let (ox, oy) = hit.tf_content_origin.unwrap_or((hit.rect.x, hit.rect.y));
                let content_x = (pos.x - ox + st.scroll_offset).max(0.0);
                let content_y = (pos.y - oy + st.scroll_offset_y).max(0.0);
                let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                let wrap_w = st.inner_width.max(1.0);
                let idx = if hit.tf_multiline {
                    index_for_xy_bytes_vt(&st, font_px, wrap_w, content_x, content_y)
                } else {
                    index_for_x_bytes_vt(&st, font_px, content_x)
                };
                st.drag_to(idx);
            }
        }

        // Determine topmost hit
        let top = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos));

        // Update cursor
        self.cursor = top.and_then(|h| h.cursor).or(Some(CursorIcon::Default));

        let new_hover = top.map(|h| h.id);

        // Enter / Leave
        if new_hover != self.hover_id {
            dispatch_hover_change(
                Some(f),
                &self.hover_leave,
                &mut self.hover_id,
                new_hover,
                pos,
                self.modifiers,
            );
            request_frame();
        }

        if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Move, pos, path);
        } else if let Some(h) = top
            && let Some(cb) = &h.on_pointer_move
        {
            let mut pe = PointerEvent::new(
                PointerId(0),
                PointerKind::Mouse,
                PointerEventKind::Move,
                pos,
                1.0,
                self.modifiers,
            );
            pe.origin = Vec2 {
                x: h.rect.x,
                y: h.rect.y,
            };
            pe.position = pe.position - pe.origin;
            cb(pe);
        }

        PointerMoveResult {
            cursor: self.cursor,
            hover_id: self.hover_id,
        }
    }

    /// Process a pointer button press. Returns focus/capture info.
    pub fn handle_pointer_press(
        &mut self,
        pos: Vec2,
        button: PointerButton,
    ) -> PointerButtonResult {
        self.mouse_pos_px = (pos.x, pos.y);

        let Some(f) = &self.frame_cache else {
            return PointerButtonResult {
                focused: None,
                capture_id: None,
                consumed: false,
                needs_a11y_announce: false,
            };
        };

        let mut result = PointerButtonResult {
            focused: None,
            capture_id: None,
            consumed: false,
            needs_a11y_announce: false,
        };

        if let Some(hit) = f.hit_regions.iter().rev().find(|h| h.rect.contains(pos)) {
            let path: Vec<u64> = f
                .hit_regions
                .iter()
                .rev()
                .filter(|h| h.rect.contains(pos))
                .map(|h| h.id)
                .collect();
            self.hit_path = Some(path.clone());

            // DnD press
            dnd::handle_drag_action(&DragAction::Press {
                position: pos,
                capture_id: hit.id,
                kind: PointerKind::Mouse,
                modifiers: self.modifiers,
            });

            // Capture
            self.capture_id = Some(hit.id);
            result.capture_id = Some(hit.id);
            result.consumed = true;

            // TextField caret placement
            if is_textfield_in_frame(f, hit.id) {
                let key = tf_key_of(f, hit.id);
                let st_rc = self
                    .textfield_states
                    .entry(key)
                    .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())));
                let mut st = st_rc.borrow_mut();
                let (ox, oy) = hit.tf_content_origin.unwrap_or((hit.rect.x, hit.rect.y));
                let content_x = (pos.x - ox + st.scroll_offset).max(0.0);
                let content_y = (pos.y - oy + st.scroll_offset_y).max(0.0);
                let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
                let wrap_w = st.inner_width.max(1.0);

                let idx = if hit.tf_multiline {
                    index_for_xy_bytes_vt(&st, font_px, wrap_w, content_x, content_y)
                } else {
                    index_for_x_bytes_vt(&st, font_px, content_x)
                };
                st.handle_pointer_down(idx, (pos.x, pos.y), self.modifiers.shift);
            }

            // Pressed visual
            self.pressed_ids.insert(hit.id);

            // Focus + IME
            if hit.focusable {
                self.sched.focused = Some(hit.id);
                result.focused = Some(hit.id);
                let key = tf_key_of(f, hit.id);
                self.textfield_states
                    .entry(key)
                    .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())));
            }

            self.dispatch_pointer_to_path(PointerEventKind::Down(button), pos, &path);

            request_frame();
        } else {
            // Click outside: drop focus
            self.hit_path = None;
            if self.ime_preedit {
                self.ime_preedit = false;
            }
            self.sched.focused = None;
            request_frame();
        }

        result
    }

    /// Process a pointer button release.
    pub fn handle_pointer_release(&mut self, pos: Vec2, button: PointerButton) {
        self.mouse_pos_px = (pos.x, pos.y);

        if dnd::handle_drag_action(&DragAction::Release {
            position: pos,
            modifiers: self.modifiers,
        }) {
            self.capture_id = None;
            self.hit_path = None;
            self.pressed_ids.clear();
            request_frame();
            return;
        }

        self.pressed_ids.clear();

        let Some(f) = &self.frame_cache else {
            self.capture_id = None;
            self.hit_path = None;
            return;
        };

        if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Up(button), pos, path);
        }

        // Click detection
        if let Some(cid) = self.capture_id
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
            && hit.rect.contains(pos)
            && let Some(cb) = &hit.on_click
        {
            cb();
        }

        // TextField drag end
        if let Some(cid) = self.capture_id
            && is_textfield_in_frame(f, cid)
        {
            let key = tf_key_of(f, cid);
            if let Some(state_rc) = self.textfield_states.get(&key) {
                state_rc.borrow_mut().end_drag();
            }
        }

        self.capture_id = None;
        self.hit_path = None;
        request_frame();
    }

    /// Cancel pointer state (focus lost, cursor left window, etc.).
    pub fn handle_pointer_cancel(&mut self) {
        dnd::handle_drag_action(&DragAction::Cancel);
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        dispatch_hover_change(
            self.frame_cache.as_ref(),
            &self.hover_leave,
            &mut self.hover_id,
            None,
            pos,
            self.modifiers,
        );
        if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Cancel, pos, path);
        }
        self.reset_pointer_state();
    }

    /// Clear hover state, emitting HoverLeave for the currently hovered region.
    pub fn clear_hover(&mut self) {
        if self.hover_id.is_none() {
            return;
        }
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        dispatch_hover_change(
            self.frame_cache.as_ref(),
            &self.hover_leave,
            &mut self.hover_id,
            None,
            pos,
            self.modifiers,
        );
    }

    /// Reconcile hover state when the composed frame changes.
    pub fn reconcile_hover_from_mouse_pos(&mut self, new_frame: &Frame) {
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };

        // If the previous hover target vanished from the new frame, deliver
        // Leave via the retained map (which survives tree removal), then clear.
        if let Some(prev_id) = self.hover_id
            && !new_frame.hit_regions.iter().any(|h| h.id == prev_id)
        {
            dispatch_hover_change(
                Some(new_frame),
                &self.hover_leave,
                &mut self.hover_id,
                None,
                pos,
                self.modifiers,
            );
        }

        if !self.pointer_inside {
            if self.hover_id.is_some() {
                dispatch_hover_change(
                    Some(new_frame),
                    &self.hover_leave,
                    &mut self.hover_id,
                    None,
                    pos,
                    self.modifiers,
                );
                request_frame();
            }
            return;
        }

        let new_hover = new_frame
            .hit_regions
            .iter()
            .rev()
            .find(|h| h.rect.contains(pos))
            .map(|h| h.id);

        self.cursor = if dnd::is_dragging() {
            Some(CursorIcon::Grabbing)
        } else {
            new_hover
                .and_then(|id| new_frame.hit_regions.iter().find(|h| h.id == id))
                .and_then(|h| h.cursor)
                .or(Some(CursorIcon::Default))
        };

        if new_hover == self.hover_id {
            return;
        }

        dispatch_hover_change(
            Some(new_frame),
            &self.hover_leave,
            &mut self.hover_id,
            new_hover,
            pos,
            self.modifiers,
        );
        request_frame();
    }

    fn reset_pointer_state(&mut self) {
        self.capture_id = None;
        self.hit_path = None;
        self.pressed_ids.clear();
    }

    /// Process a scroll event. Returns true if consumed.
    pub fn handle_scroll(&mut self, delta: Vec2) -> bool {
        let Some(f) = &self.frame_cache else {
            return false;
        };

        let now = web_time::Instant::now();
        if let Some(last) = self.last_scroll_at
            && now.duration_since(last).as_millis() > 250
        {
            self.scroll_capture_id = None;
        }
        self.last_scroll_at = Some(now);

        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        let (consumed, cap) = dispatch_scroll(f, pos, delta, self.scroll_capture_id);
        self.scroll_capture_id = cap;
        if consumed {
            request_frame();
        }
        consumed
    }

    /// Process a keyboard key event. Returns true if consumed.
    pub fn handle_key(&mut self, event: &KeyEvent) -> bool {
        // Owned clone so `dispatch_action` may take `&mut self` below
        // without conflicting with the long-lived frame borrow.
        let Some(frame) = self.frame_cache.clone() else {
            return false;
        };
        let f = &frame;

        // Escape / BrowserBack: cancel DnD first, then try focus key dispatch.
        // If nothing consumed it, do NOT consume: let the host handle back /
        // exit / window-chrome actions.
        if event.event_type == KeyEventType::Down && !event.is_repeat && event.key == Key::Escape {
            if dnd::handle_drag_action(&DragAction::Cancel) {
                request_frame();
                return true;
            }
            // Try dispatch through focus chain
            if self.dispatch_focus_key_event(f, event) {
                request_frame();
                return true;
            }
            return false;
        }

        // Dispatch through focus ancestor chain
        let consumed = self.dispatch_focus_key_event(f, event);
        if consumed {
            request_frame();
            return true;
        }

        // Action dispatch (shortcuts like Ctrl+C, Tab, etc.)
        if event.event_type == KeyEventType::Down
            && !event.is_repeat
            && let Some(action) = repose_core::shortcuts::resolve_action(
                repose_core::shortcuts::KeyChord::new(event.key.clone(), self.modifiers),
            )
        {
            // `dispatch_action` covers focus navigation internally.
            if self.dispatch_action(action.clone()) {
                return true;
            }
        }

        // Keyboard activation (Space/Enter on focused non-textfield)
        if let Some(fid) = self.sched.focused {
            let is_tf = f
                .semantics_nodes
                .iter()
                .any(|n| n.id == fid && n.role == repose_core::semantics::Role::TextField);
            if !is_tf {
                if event.event_type == KeyEventType::Down && !event.is_repeat {
                    if event.key == Key::Space || event.key == Key::Enter {
                        self.pressed_ids.insert(fid);
                        self.key_pressed_active = Some(fid);

                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                            if let Some(src) = &hit.interaction_source {
                                let local = Vec2 {
                                    x: hit.rect.w * 0.5,
                                    y: hit.rect.h * 0.5,
                                };
                                src.to_mutable().emit(Interaction::new_press(local));
                            }
                        }

                        request_frame();
                        return true;
                    }
                } else if event.event_type == KeyEventType::Up
                    && let Some(active_id) = self.key_pressed_active
                    && (event.key == Key::Space || event.key == Key::Enter)
                {
                    self.pressed_ids.remove(&active_id);
                    self.key_pressed_active = None;

                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == active_id) {
                        if let Some(src) = &hit.interaction_source {
                            let pid = src.collect_last_press_id().unwrap_or(0);
                            src.to_mutable().emit(Interaction::Release(pid));
                        }
                        if let Some(cb) = &hit.on_click {
                            cb();
                        }
                    }
                    request_frame();
                    return true;
                }
            }
        }

        // Enter submission for focused TextField
        if event.event_type == KeyEventType::Down
            && !event.is_repeat
            && event.key == Key::Enter
            && let Some(fid) = self.sched.focused
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
        {
            let is_multiline = hit.tf_multiline;
            let should_submit = if is_multiline {
                self.modifiers.ctrl || self.modifiers.meta
            } else {
                true
            };
            if should_submit {
                if let Some(on_submit) = &hit.on_text_submit {
                    let key = tf_key_of(f, fid);
                    if let Some(state_rc) = self.textfield_states.get(&key) {
                        let text = state_rc.borrow().text.clone();
                        on_submit(text);
                        request_frame();
                        return true;
                    }
                }
            } else {
                // Multiline plain Enter: insert newline
                let key = tf_key_of(f, fid);
                if let Some(state_rc) = self.textfield_states.get(&key) {
                    let mut st = state_rc.borrow_mut();
                    st.insert_text("\n");
                    let new_text = st.text.clone();
                    notify_text_change(f, fid, new_text);
                    tf_ensure_caret_visible(&mut st, hit.tf_multiline);
                    request_frame();
                    return true;
                }
            }
        }

        // TextField navigation / edit keys
        if event.event_type == KeyEventType::Down {
            if let Some(fid) = self.sched.focused {
                let key = tf_key_of(f, fid);
                if let Some(state_rc) = self.textfield_states.get(&key) {
                    let mut state = state_rc.borrow_mut();
                    match event.key {
                        Key::Backspace => {
                            state.delete_backward();
                            let new_text = state.text.clone();
                            notify_text_change(f, fid, new_text);
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::Delete => {
                            state.delete_forward();
                            let new_text = state.text.clone();
                            notify_text_change(f, fid, new_text);
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::ArrowLeft => {
                            state.move_cursor(-1, self.modifiers.shift);
                            state.preferred_x_px = None;
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::ArrowRight => {
                            state.move_cursor(1, self.modifiers.shift);
                            state.preferred_x_px = None;
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::ArrowUp => {
                            if is_multiline_id(f, fid)
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                let font_px = dp_to_px(TF_FONT_DP);
                                let cur = state.caret_index();
                                let (new_pos, px) = repose_ui::textfield::move_caret_vertical(
                                    &state.text,
                                    font_px,
                                    hit.rect.w,
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
                                let (cx, cy, _) = caret_xy_for_byte(
                                    &state.text,
                                    font_px,
                                    hit.rect.w,
                                    state.caret_index(),
                                );
                                let iw = state.inner_width;
                                let ih = state.inner_height;
                                state.ensure_caret_visible_xy(cx, cy, iw, ih, dp_to_px(2.0));
                                request_frame();
                                return true;
                            }
                        }
                        Key::ArrowDown => {
                            if is_multiline_id(f, fid)
                                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            {
                                let font_px = dp_to_px(TF_FONT_DP);
                                let cur = state.caret_index();
                                let (new_pos, px) = repose_ui::textfield::move_caret_vertical(
                                    &state.text,
                                    font_px,
                                    hit.rect.w,
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
                                let (cx, cy, _) = caret_xy_for_byte(
                                    &state.text,
                                    font_px,
                                    hit.rect.w,
                                    state.caret_index(),
                                );
                                let iw = state.inner_width;
                                let ih = state.inner_height;
                                state.ensure_caret_visible_xy(cx, cy, iw, ih, dp_to_px(2.0));
                                request_frame();
                                return true;
                            }
                        }
                        Key::Home => {
                            state.selection = 0..0;
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::End => {
                            let end = state.text.len();
                            state.selection = end..end;
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        _ => {}
                    }
                }
            }

            // Plain text input (non-IME)
            if !self.ime_preedit
                && !self.modifiers.ctrl
                && !self.modifiers.alt
                && !self.modifiers.meta
                && let Key::Character(c) = event.key
                && !c.is_control()
                && c != '\n'
                && c != '\r'
                && let Some(fid) = self.sched.focused
            {
                let key = tf_key_of(f, fid);
                if let Some(state_rc) = self.textfield_states.get(&key) {
                    let mut st = state_rc.borrow_mut();
                    let text = c.to_string();
                    st.insert_text(&text);
                    notify_text_change(f, fid, st.text.clone());
                    if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                        tf_ensure_caret_visible(&mut st, hit.tf_multiline);
                    }
                    request_frame();
                    return true;
                }
            }
        }

        // Key release: finish keyboard activation
        if event.event_type == KeyEventType::Up
            && let Some(active_id) = self.key_pressed_active
            && (event.key == Key::Space || event.key == Key::Enter)
        {
            self.pressed_ids.remove(&active_id);
            self.key_pressed_active = None;
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == active_id)
                && let Some(cb) = &hit.on_click
            {
                cb();
            }
            request_frame();
            return true;
        }

        false
    }

    /// Dispatch a key event through the focus ancestor chain.
    fn dispatch_focus_key_event(&self, f: &Frame, event: &KeyEvent) -> bool {
        let Some(focused) = self.sched.focused else {
            return false;
        };

        let hit_by_id: HashMap<u64, &HitRegion> = f.hit_regions.iter().map(|h| (h.id, h)).collect();
        let sem_parent_of: HashMap<u64, u64> = f
            .semantics_nodes
            .iter()
            .filter_map(|n| n.parent.map(|p| (n.id, p)))
            .collect();

        // Build ancestor chain
        let mut ancestors = Vec::new();
        let mut cur = focused;
        loop {
            ancestors.push(cur);
            if let Some(&p) = sem_parent_of.get(&cur) {
                cur = p;
            } else {
                break;
            }
        }

        // Top-down preview: root -> focused
        for &id in ancestors.iter().rev() {
            if let Some(hit) = hit_by_id.get(&id)
                && let Some(cb) = &hit.on_preview_key_event
                && cb(event.clone())
            {
                return true;
            }
        }

        // Bottom-up normal: focused -> root
        for &id in ancestors.iter() {
            if let Some(hit) = hit_by_id.get(&id)
                && let Some(cb) = &hit.on_key_event
                && cb(event.clone())
            {
                return true;
            }
        }

        false
    }

    /// Dispatch a shortcut action: widget handler first, then built-in
    /// textfield editing, then the global shortcut map, then focus navigation.
    /// Returns true if the action was consumed.
    pub fn dispatch_action(&mut self, action: repose_core::shortcuts::Action) -> bool {
        // 1) Widget-level handler
        if let Some(f) = &self.frame_cache
            && let Some(fid) = self.sched.focused
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
            && let Some(cb) = &hit.on_action
            && cb(action.clone())
        {
            request_frame();
            return true;
        }

        // 2) Built-in textfield editing (undo/redo/copy/cut/paste/select-all)
        if self.apply_text_editing_action(&action) {
            return true;
        }

        // 3) Global shortcut handler
        if repose_core::shortcuts::handle(action.clone()) {
            request_frame();
            return true;
        }

        // 4) Focus navigation (Tab / arrows)
        if let Some(f) = self.frame_cache.clone()
            && let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.sched, &f)
        {
            // End any in-flight keyboard press (e.g. Space held on the old focus).
            if let Some(active) = self.key_pressed_active.take() {
                self.pressed_ids.remove(&active);
            }
            // Lazy-init + reset the caret blink for the newly focused text field.
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == new_id)
                && let Some(key) = hit.tf_state_key
            {
                self.ensure_textfield_state(key).borrow_mut().reset_caret_blink();
            }
            request_frame();
            return true;
        }

        false
    }

    /// Apply built-in textfield editing actions (Undo/Redo/SelectAll/Copy/
    /// Cut/Paste) to the focused text field. Returns true if consumed.
    fn apply_text_editing_action(&mut self, action: &repose_core::shortcuts::Action) -> bool {
        use repose_core::shortcuts::Action;
        let Some(fid) = self.sched.focused else {
            return false;
        };
        let Some(f) = self.frame_cache.clone() else {
            return false;
        };
        if !is_textfield_in_frame(&f, fid) {
            return false;
        }
        let key = tf_key_of(&f, fid);
        let Some(state_rc) = self.textfield_states.get(&key).cloned() else {
            return false;
        };
        let multiline = is_multiline_id(&f, fid);

        match action {
            Action::Undo => {
                let mut st = state_rc.borrow_mut();
                if !st.can_undo() {
                    return false;
                }
                st.undo();
                notify_text_change(&f, fid, st.text.clone());
                tf_ensure_caret_visible(&mut st, multiline);
                request_frame();
                true
            }
            Action::Redo => {
                let mut st = state_rc.borrow_mut();
                if !st.can_redo() {
                    return false;
                }
                st.redo();
                notify_text_change(&f, fid, st.text.clone());
                tf_ensure_caret_visible(&mut st, multiline);
                request_frame();
                true
            }
            Action::SelectAll => {
                let mut st = state_rc.borrow_mut();
                let len = st.text.len();
                st.selection = 0..len;
                request_frame();
                true
            }
            Action::Copy => {
                let st = state_rc.borrow();
                let (a, b) = (
                    st.selection.start.min(st.selection.end),
                    st.selection.start.max(st.selection.end),
                );
                if a == b {
                    return false;
                }
                let slice = st.text.get(a..b).unwrap_or("").to_string();
                drop(st);
                if !slice.is_empty() {
                    repose_core::clipboard::copy_to_clipboard(&slice);
                }
                true
            }
            Action::Cut => {
                let mut st = state_rc.borrow_mut();
                let (a, b) = (
                    st.selection.start.min(st.selection.end),
                    st.selection.start.max(st.selection.end),
                );
                if a == b {
                    return false;
                }
                let slice = st.text.get(a..b).unwrap_or("").to_string();
                // Replacing the selection with "" deletes it.
                st.insert_text_atomic("");
                let new_text = st.text.clone();
                drop(st);
                if !slice.is_empty() {
                    repose_core::clipboard::copy_to_clipboard(&slice);
                }
                notify_text_change(&f, fid, new_text);
                if let Some(mut st) = self.textfield_states.get(&key).map(|s| s.borrow_mut()) {
                    tf_ensure_caret_visible(&mut st, multiline);
                }
                request_frame();
                true
            }
            Action::Paste => {
                if let Some(txt) = repose_core::clipboard::paste_text() {
                    self.paste_into_focused(&txt);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// Process an IME event.
    pub fn handle_ime(&mut self, event: &ImeEvent) {
        let Some(fid) = self.sched.focused else {
            return;
        };
        let Some(f) = &self.frame_cache else {
            return;
        };
        let key = tf_key_of(f, fid);
        let Some(state_rc) = self.textfield_states.get(&key) else {
            return;
        };

        let mut state = state_rc.borrow_mut();

        match event {
            ImeEvent::Start => {
                self.ime_preedit = false;
            }
            ImeEvent::Update { text, cursor } => {
                state.set_composition(text.clone(), *cursor);
                self.ime_preedit = !text.is_empty();
                repose_ui::textfield::ensure_caret_visible(&mut state, true);
                notify_text_change(f, fid, state.text.clone());
            }
            ImeEvent::Commit(text) => {
                state.commit_composition(text.clone());
                self.ime_preedit = false;
                repose_ui::textfield::ensure_caret_visible(&mut state, true);
                notify_text_change(f, fid, state.text.clone());
            }
            ImeEvent::Cancel => {
                self.ime_preedit = false;
                if state.composition.is_some() {
                    state.cancel_composition();
                    repose_ui::textfield::ensure_caret_visible(&mut state, true);
                    notify_text_change(f, fid, state.text.clone());
                }
            }
        }

        request_frame();
    }

    /// Handle focus lost (window unfocused, etc.).
    pub fn handle_focus_lost(&mut self) {
        dnd::handle_drag_action(&DragAction::Cancel);
        self.handle_pointer_cancel();
        self.ime_preedit = false;
    }

    /// Get or create a text field state by its key.
    pub fn ensure_textfield_state(&mut self, key: u64) -> Rc<RefCell<TextFieldState>> {
        self.textfield_states
            .entry(key)
            .or_insert_with(|| Rc::new(RefCell::new(TextFieldState::new())))
            .clone()
    }

    /// Look up the persistent state key for a visual hit-region id.
    pub fn tf_key_of(&self, visual_id: u64) -> u64 {
        self.frame_cache
            .as_ref()
            .map(|f| tf_key_of(f, visual_id))
            .unwrap_or(visual_id)
    }

    /// True if the given id belongs to a TextField.
    pub fn is_textfield(&self, id: u64) -> bool {
        self.frame_cache
            .as_ref()
            .map(|f| is_textfield_in_frame(f, id))
            .unwrap_or(false)
    }

    /// True if the given textfield id is multiline.
    pub fn is_multiline(&self, id: u64) -> bool {
        self.frame_cache
            .as_ref()
            .map(|f| is_multiline_id(f, id))
            .unwrap_or(false)
    }

    /// Keyboard hints of the currently focused text field, or defaults if none.
    /// Returns `(purpose, auto_correct, capitalization)` for the platform runner.
    pub fn focused_keyboard_hints(
        &self,
    ) -> (
        repose_core::ImePurposeHint,
        bool,
        repose_core::KeyboardCapitalization,
    ) {
        let defaults = || {
            (
                repose_core::ImePurposeHint::Normal,
                true,
                repose_core::KeyboardCapitalization::Unspecified,
            )
        };
        let Some(fid) = self.sched.focused else {
            return defaults();
        };
        let Some(f) = &self.frame_cache else {
            return defaults();
        };
        match f.hit_regions.iter().find(|h| h.id == fid) {
            Some(hit) => (
                hit.keyboard_type.ime_purpose_hint(),
                hit.auto_correct.unwrap_or(true),
                hit.capitalization,
            ),
            None => defaults(),
        }
    }

    /// Insert arbitrary text into the focused text field (composed keyboard
    /// text, clipboard paste, hardware-keyboard fallback, ...).
    /// Returns true if text was inserted.
    ///
    /// Control characters are filtered out; newlines are dropped for
    /// single-line text fields. Skips while an IME preedit is active so the
    /// composition isn't corrupted by duplicate host input.
    pub fn insert_text_into_focused(&mut self, text: &str) -> bool {
        if text.is_empty() || self.ime_preedit {
            return false;
        }
        let Some(fid) = self.sched.focused else {
            return false;
        };
        let Some(f) = self.frame_cache.clone() else {
            return false;
        };
        if !is_textfield_in_frame(&f, fid) {
            return false;
        }
        let key = tf_key_of(&f, fid);
        let Some(state_rc) = self.textfield_states.get(&key).cloned() else {
            return false;
        };
        let multiline = is_multiline_id(&f, fid);
        let filtered: String = text
            .chars()
            .filter(|c| !c.is_control() && *c != '\r' && (multiline || *c != '\n'))
            .collect();
        if filtered.is_empty() {
            return false;
        }
        {
            let mut st = state_rc.borrow_mut();
            st.insert_text(&filtered);
            let new_text = st.text.clone();
            notify_text_change(&f, fid, new_text);
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                tf_ensure_caret_visible(&mut st, hit.tf_multiline);
            }
        }
        request_frame();
        true
    }

    /// Insert text into a focused text field (used for paste). Uses an atomic
    /// (non-mergeable) edit so Ctrl+V doesn't merge with adjacent typing.
    pub fn paste_into_focused(&mut self, text: &str) {
        let Some(fid) = self.sched.focused else {
            return;
        };
        let Some(f) = &self.frame_cache.clone() else {
            return;
        };
        let key = tf_key_of(f, fid);
        if let Some(state_rc) = self.textfield_states.get(&key) {
            let mut st = state_rc.borrow_mut();
            st.insert_text_atomic(text);
            let new_text = st.text.clone();
            notify_text_change(f, fid, new_text);
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                tf_ensure_caret_visible(&mut st, hit.tf_multiline);
            }
        }
        request_frame();
    }

    /// Process a key event with an optional host-composed `text` payload
    /// (winit `key_event.text`, Android soft-keyboard text, ...).
    ///
    /// When the modifiers are free of Ctrl/Alt/Meta and the payload is
    /// printable, the text is inserted into the focused text field first;
    /// otherwise the event falls through to [`Self::handle_key`] (single
    /// characters, navigation, shortcuts, activation).
    pub fn handle_key_with_text(&mut self, event: &KeyEvent, composed_text: Option<&str>) -> bool {
        if event.event_type == KeyEventType::Down
            && !event.is_repeat
            && !self.ime_preedit
            && !self.modifiers.ctrl
            && !self.modifiers.alt
            && !self.modifiers.meta
            && let Some(text) = composed_text
            && !text.chars().all(|c| c.is_control())
            && self.insert_text_into_focused(text)
        {
            return true;
        }
        self.handle_key(event)
    }

    /// Process a scroll event at an explicit position, honoring a caller-owned
    /// scroll capture id (touch gestures initialize the capture themselves).
    /// Returns `(consumed, updated_capture_id)`.
    pub fn handle_scroll_at(
        &mut self,
        pos: Vec2,
        delta: Vec2,
        scroll_capture: Option<u64>,
    ) -> (bool, Option<u64>) {
        let Some(f) = &self.frame_cache else {
            return (false, scroll_capture);
        };
        let (consumed, cap) = dispatch_scroll(f, pos, delta, scroll_capture);
        if consumed {
            request_frame();
        }
        (consumed, cap)
    }

    /// Next caret blink edge (`Instant`) for the focused text field, if any.
    pub fn next_caret_blink_deadline(&self) -> Option<web_time::Instant> {
        let fid = self.sched.focused?;
        let frame = self.frame_cache.as_ref()?;
        let hit = frame.hit_regions.iter().find(|h| h.id == fid)?;
        let key = hit.tf_state_key?;
        self.textfield_states.get(&key)?.borrow().next_blink_deadline()
    }

    /// Tick host-facing overlays (snackbar timeouts) with the elapsed ms since
    /// the last presented frame. Call once per redraw.
    pub fn tick_overlays(&self, last_frame: web_time::Instant) {
        let now = web_time::Instant::now();
        let ms = now
            .saturating_duration_since(last_frame)
            .as_millis()
            .min(u32::MAX as u128) as u32;
        if ms > 0 {
            repose_ui::overlay::SnackbarController::tick_for_frame(ms);
        }
    }

    /// Get the cursor suggestion (set during pointer-move handling).
    pub fn cursor_suggestion(&self) -> Option<CursorIcon> {
        self.cursor
    }

    /// Take the cursor suggestion (clears it).
    pub fn take_cursor_suggestion(&mut self) -> Option<CursorIcon> {
        self.cursor.take()
    }
}

impl Default for ReposeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

/// Inner compose frame logic (no dependency on repose-platform).
pub fn compose_frame_inner<F>(
    sched: &mut Scheduler,
    root_fn: &mut F,
    scale: f32,
    size_px_u32: (u32, u32),
    hover_id: Option<u64>,
    pressed_ids: &HashSet<u64>,
    tf_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
) -> Frame
where
    F: FnMut(&mut Scheduler) -> View,
{
    if let Some(requested_id) = take_focus_request() {
        if requested_id == repose_core::runtime::CLEAR_FOCUS_MARKER {
            sched.focused = None;
        } else {
            sched.focused = Some(requested_id);
        }
    }

    set_density_default(Density { scale });

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
                let interactions = Interactions {
                    hover: hover_id,
                    pressed: pressed_ids.clone(),
                };
                with_density(Density { scale }, || {
                    layout_and_paint(view, size_px_u32, tf_states, &interactions, current_focused)
                })
            }
        },
    );

    if let Some(fid) = sched.focused
        && !frame.focus_chain.contains(&fid)
    {
        sched.focused = None;
    }

    frame
}

/// Fire enter/leave callbacks when the hovered region changes, updating
/// `hover_id`.
fn dispatch_hover_change(
    frame: Option<&Frame>,
    leave_map: &HashMap<u64, (f32, f32, f32, f32, Rc<dyn Fn(PointerEvent)>)>,
    hover_id: &mut Option<u64>,
    new_hover: Option<u64>,
    pos: Vec2,
    modifiers: Modifiers,
) {
    if new_hover == *hover_id {
        return;
    }

    // --- Leave previous (ALWAYS if we still know how) ---
    if let Some(prev_id) = *hover_id {
        let leave_info = leave_map.get(&prev_id).cloned().or_else(|| {
            frame.and_then(|f| {
                f.hit_regions
                    .iter()
                    .find(|h| h.id == prev_id)
                    .and_then(|h| {
                        h.on_pointer_leave
                            .as_ref()
                            .map(|cb| (h.rect.x, h.rect.y, h.rect.w, h.rect.h, cb.clone()))
                    })
            })
        });
        if let Some((rx, ry, _rw, _rh, cb)) = leave_info {
            let mut pe = PointerEvent::new(
                PointerId(0),
                PointerKind::Mouse,
                PointerEventKind::Leave,
                pos,
                1.0,
                modifiers,
            );
            pe.origin = Vec2 { x: rx, y: ry };
            pe.position = pe.position - pe.origin;
            cb(pe);
        }
    }

    // --- Enter new ---
    if let Some(f) = frame
        && let Some(hid) = new_hover
        && let Some(h) = f.hit_regions.iter().find(|h| h.id == hid)
        && let Some(cb) = &h.on_pointer_enter
    {
        let mut pe = PointerEvent::new(
            PointerId(0),
            PointerKind::Mouse,
            PointerEventKind::Enter,
            pos,
            1.0,
            modifiers,
        );
        pe.origin = Vec2 {
            x: h.rect.x,
            y: h.rect.y,
        };
        pe.position = pe.position - pe.origin;
        cb(pe);
    }

    *hover_id = new_hover;
}

fn is_textfield_in_frame(f: &Frame, id: u64) -> bool {
    f.semantics_nodes
        .iter()
        .any(|n| n.id == id && n.role == repose_core::semantics::Role::TextField)
}

fn is_multiline_id(f: &Frame, id: u64) -> bool {
    f.hit_regions
        .iter()
        .find(|h| h.id == id)
        .map(|h| h.tf_multiline)
        .unwrap_or(false)
}

fn tf_key_of(frame: &Frame, visual_id: u64) -> u64 {
    if let Some(i) = frame.hit_regions.iter().position(|h| h.id == visual_id) {
        let hr = &frame.hit_regions[i];
        return hr.tf_state_key.unwrap_or(hr.id);
    }
    visual_id
}

fn notify_text_change(f: &Frame, id: u64, text: String) {
    if let Some(h) = f.hit_regions.iter().find(|h| h.id == id)
        && let Some(cb) = &h.on_text_change
    {
        cb(text);
    }
}

fn tf_ensure_caret_visible(state: &mut TextFieldState, is_multiline: bool) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let wrap_width = state.inner_width;

    if is_multiline {
        let (cx, cy, _) = caret_xy_for_byte(&state.text, font_px, wrap_width, state.caret_index());
        state.ensure_caret_visible_xy(cx, cy, state.inner_width, state.inner_height, dp_to_px(2.0));
    } else {
        let caret_idx = state.caret_index();
        let (display, caret_display_off) = if let Some(vt) = &state.visual_transformation {
            let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
            let tfmd = vt.filter(&annotated);
            let off =
                repose_core::original_offset_to_display(&state.text, tfmd.text.as_str(), caret_idx);
            (tfmd.text.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = measure_text(&display, font_px, TextMeasureConfig::default());
        let caret_x_px = m.positions.get(caret_display_off).copied().unwrap_or(0.0);
        state.ensure_caret_visible(caret_x_px, wrap_width, dp_to_px(2.0));
    }
}

fn index_for_x_bytes_vt(state: &TextFieldState, font_px: f32, x_px: f32) -> usize {
    if let Some(vt) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let tfmd = vt.filter(&annotated);
        let display_idx =
            repose_ui::textfield::index_for_x_bytes(tfmd.text.as_str(), font_px, x_px, 400, 0);
        tfmd.offset_mapping.transformed_to_original(display_idx)
    } else {
        repose_ui::textfield::index_for_x_bytes(&state.text, font_px, x_px, 400, 0)
    }
}

fn index_for_xy_bytes_vt(
    state: &TextFieldState,
    font_px: f32,
    wrap_w: f32,
    x_px: f32,
    y_px: f32,
) -> usize {
    if let Some(vt) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let tfmd = vt.filter(&annotated);
        let display_idx = repose_ui::textfield::index_for_xy_bytes(
            tfmd.text.as_str(),
            font_px,
            wrap_w,
            x_px,
            y_px,
        );
        tfmd.offset_mapping.transformed_to_original(display_idx)
    } else {
        repose_ui::textfield::index_for_xy_bytes(&state.text, font_px, wrap_w, x_px, y_px)
    }
}

/// Dispatch scroll to scroll consumers. Returns (consumed, optional capture id).
fn dispatch_scroll(
    frame: &Frame,
    pos: Vec2,
    delta: Vec2,
    scroll_capture: Option<u64>,
) -> (bool, Option<u64>) {
    if let Some(cid) = scroll_capture
        && let Some(cb) = frame
            .hit_regions
            .iter()
            .find(|h| h.id == cid)
            .and_then(|h| h.on_scroll.as_ref())
    {
        cb(delta);
        return (true, Some(cid));
    }
    // Captured region vanished from the tree -> fall through and re-pick.

    let mut remaining = delta;
    for hit in frame
        .hit_regions
        .iter()
        .rev()
        .filter(|h| h.rect.contains(pos))
    {
        if let Some(cb) = &hit.on_scroll {
            let before = remaining;
            let leftover = cb(before);
            let consumed =
                (before.x - leftover.x).abs() > 0.001 || (before.y - leftover.y).abs() > 0.001;
            if consumed {
                return (true, Some(hit.id));
            }
            remaining = leftover;
            if remaining.x.abs() <= 0.001 && remaining.y.abs() <= 0.001 {
                break;
            }
        }
    }
    (false, scroll_capture)
}
