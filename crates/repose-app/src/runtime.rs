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

fn ensure_tf_state(
    map: &mut HashMap<u64, Rc<RefCell<TextFieldState>>>,
    key: u64,
    seed: &str,
) -> Rc<RefCell<TextFieldState>> {
    map.entry(key)
        .or_insert_with(|| {
            Rc::new(RefCell::new(if seed.is_empty() {
                TextFieldState::new()
            } else {
                TextFieldState::with_text(seed.to_string())
            }))
        })
        .clone()
}

fn ensure_all_tf_states_from_frame(
    map: &mut HashMap<u64, Rc<RefCell<TextFieldState>>>,
    frame: &Frame,
) {
    for hit in &frame.hit_regions {
        if let Some(key) = hit.tf_state_key {
            let st = ensure_tf_state(map, key, hit.tf_value.as_str());
            // Only sync text; do not move caret.
            st.borrow_mut()
                .apply_controlled_value(hit.tf_value.as_str());
        }
    }
}

fn is_tf_hit(f: &Frame, id: u64) -> bool {
    f.hit_regions
        .iter()
        .any(|h| h.id == id && h.tf_state_key.is_some())
        || is_textfield_in_frame(f, id)
}

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

impl FrameOutput {
    /// Consume the frame into a `repose_core::Frame` for hit-testing/caching
    /// by the host. Drops the platform-output and pointer metadata.
    pub fn into_frame(self) -> Frame {
        Frame {
            scene: self.scene,
            hit_regions: self.hit_regions,
            semantics_nodes: self.semantics_nodes,
            focus_chain: self.focus_chain,
        }
    }
}

/// Result of a pointer-move event processed by the runtime.
pub struct PointerMoveResult {
    /// Updated cursor suggestion for the host.
    pub cursor: Option<CursorIcon>,
    /// The id of the element under the pointer, if any.
    pub hover_id: Option<u64>,
}

/// Result of a pointer-button event processed by the runtime.
#[derive(Debug)]
pub struct PointerButtonResult {
    /// Id of the element that received focus (if any).
    pub focused: Option<u64>,
    /// Id of the captured element.
    pub capture_id: Option<u64>,
    /// Whether the event was consumed by the UI.
    pub consumed: bool,
    /// Whether an accessibility announcement was triggered.
    pub needs_a11y_announce: bool,
    /// Set on release when a click fired, so hosts can announce activation
    /// without re-reading runtime state that has already been cleared.
    pub clicked_id: Option<u64>,
}

// ViewConfiguration defaults
const LONG_PRESS_MS: u128 = 500;
const DOUBLE_CLICK_MS: u128 = 300;
const DOUBLE_TAP_MIN_MS: u128 = 40;
const LONG_PRESS_SLOP_DP: f32 = 18.0;

/// Embeddable Repose runtime.
///
/// Manages composition scheduling, input routing, text-field state, and
/// pointer/key dispatch.  The host owns the event loop and GPU device. This
/// is purely the UI logic layer.
pub struct ReposeRuntime {
    pub sched: Scheduler,
    pub scale: f32,

    pub modifiers: Modifiers,
    pub mouse_pos_px: (f32, f32),
    /// Whether the pointer is currently inside the window.
    pub pointer_inside: bool,
    pub hover_id: Option<u64>,
    pub hover_ancestors: std::collections::HashSet<u64>,
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

    last_up: Option<(u64, web_time::Instant, f32, f32)>,
    /// Position/time of the most recent pointer-down, used to time the second
    /// tap of a double click (Compose: window + min time measured to the
    /// second DOWN, not its up).
    last_down: Option<(u64, web_time::Instant)>,
    /// Set when the second tap of a double-click qualifies (within
    /// [DOUBLE_TAP_MIN_MS, DOUBLE_CLICK_MS] of the first tap's up). Its up
    /// Confirms the double click. A canceled second tap falls back to the first tap's onClick.
    double_candidate: Option<u64>,
    long_press: Option<(u64, web_time::Instant, f32, f32)>,
    /// Keyboard long-press (Compose combinedClickable: holding Space/Enter
    /// past LONG_PRESS_MS fires on_long_click). `bool` = already fired.
    key_long_press: Option<(u64, web_time::Instant, bool)>,
    suppress_next_click: bool,
    pending_click: Option<(u64, web_time::Instant, Rc<dyn Fn()>)>,

    pub frame_cache: Option<Frame>,

    cursor: Option<CursorIcon>,

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
            hover_ancestors: std::collections::HashSet::new(),
            hover_leave: HashMap::new(),
            capture_id: None,
            hit_path: None,
            scroll_capture_id: None,
            last_scroll_at: None,
            pressed_ids: HashSet::new(),
            ime_preedit: false,
            key_pressed_active: None,
            last_focus: None,
            last_up: None,
            last_down: None,
            double_candidate: None,
            long_press: None,
            key_long_press: None,
            suppress_next_click: false,
            pending_click: None,
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

    pub fn poll_gesture_timers(&mut self) {
        self.poll_long_press();
        self.flush_pending_click();
        self.poll_key_long_press();
    }

    /// Compose and layout a frame, returning the output for rendering.
    ///
    /// Call `tick_animations` before this and `cache_frame` after (once you
    /// have applied any host-specific overlays like the devtools inspector).
    pub fn compose<F>(&mut self, root_fn: &mut F, render_ctx: &RenderContext) -> Frame
    where
        F: FnMut(&mut Scheduler, &RenderContext) -> View,
    {
        self.poll_long_press();
        self.flush_pending_click();
        self.poll_key_long_press();

        let size = self.sched.size;
        let rc = render_ctx.clone();
        let mut compose_once = |this: &mut Self| {
            let mut inner = |s: &mut Scheduler| (root_fn)(s, &rc);
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                compose_frame_inner_with_ancestors(
                    &mut this.sched,
                    &mut inner,
                    this.scale,
                    size,
                    this.hover_id,
                    &this.hover_ancestors,
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

        let focused_is_textfield = ime_allowed
            || self.sched.focused.is_some_and(|fid| {
                f.hit_regions
                    .iter()
                    .any(|h| h.id == fid && h.tf_state_key.is_some())
            });
        let wants_keyboard = focused_is_textfield || self.ime_preedit;

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

    /// Post-compose host-agnostic bookkeeping.
    /// this lazy-initializes text-field state for focus-requester paths, reconciles
    /// hover against the new hit list, and publishes the frame to the DnD
    /// registry. Replaces platform-local copies of this logic.
    pub fn after_compose(&mut self, frame: &Frame, scale: f32) {
        ensure_all_tf_states_from_frame(&mut self.textfield_states, frame);
        self.ensure_focused_state_in_frame(frame);
        self.reconcile_hover_from_mouse_pos(frame);
        repose_core::dnd::set_dnd_frame(Some(frame.clone()));
        repose_core::dnd::set_dnd_scale(scale);
    }

    /// Lazy-init the focused textfield's persistent state (FocusRequester
    /// paths don't create it until first click). Resets the caret blink.
    pub fn ensure_focused_textfield_state(&mut self) {
        let Some(f) = self.frame_cache.clone() else {
            return;
        };
        self.ensure_focused_state_in_frame(&f);
    }

    /// Shared helper: create a persistent `TextFieldState` for the focused
    /// widget (if it is a textfield with a state key) and reset its caret
    /// blink. No-op when the focused widget already has state.
    fn ensure_focused_state_in_frame(&mut self, frame: &Frame) {
        let Some(fid) = self.sched.focused else {
            return;
        };
        if let Some(hit) = frame.hit_regions.iter().find(|h| h.id == fid)
            && let Some(key) = hit.tf_state_key
        {
            let st = ensure_tf_state(&mut self.textfield_states, key, hit.tf_value.as_str());
            st.borrow_mut().apply_controlled_value(&hit.tf_value);
        }
    }

    /// Cache a composed [`FrameOutput`] for hit-testing: rebuilds the retained
    /// hover-leave map, reconciles hover, lazy-initializes focused textfield
    /// state, and publishes the DnD frame/scale to the input registry.
    pub fn cache_from_output(&mut self, out: &FrameOutput) {
        let frame = Frame {
            scene: out.scene.clone(),
            hit_regions: out.hit_regions.clone(),
            semantics_nodes: out.semantics_nodes.clone(),
            focus_chain: out.focus_chain.clone(),
        };
        self.after_compose(&frame, self.scale);
        self.cache_frame(frame);
    }

    /// One-shot host tick: advance animations, compose a frame, and publish
    /// the result (hover reconciliation, focused textfield lazy-init, DnD
    /// frame/scale) in a single call.
    pub fn compose_frame_output<F>(
        &mut self,
        root: &mut F,
        render_ctx: &RenderContext,
    ) -> FrameOutput
    where
        F: FnMut(&mut Scheduler, &RenderContext) -> View,
    {
        self.tick_animations();
        let out = self.frame(root, render_ctx);
        self.cache_from_output(&out);
        out
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

        if let Some((_, _, x0, y0)) = self.long_press {
            let slop = LONG_PRESS_SLOP_DP * self.scale;
            let dx = pos.x - x0;
            let dy = pos.y - y0;
            if dx * dx + dy * dy > slop * slop {
                self.long_press = None;
            }
        }

        // Cancel the long press once the pointer leaves the element's bounds
        if self.long_press.is_some()
            && let Some(lid) = self.long_press.map(|(id, _, _, _)| id)
            && f.hit_regions
                .iter()
                .find(|h| h.id == lid)
                .map_or(true, |h| !h.rect.contains(pos))
        {
            self.long_press = None;
        }

        // TextField/TextArea drag selection (if captured)
        if let Some(cid) = self.capture_id
            && is_tf_hit(f, cid)
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

        let top = f
            .hit_regions
            .iter()
            .rev()
            .find(|h| !h.disabled && h.rect.contains(pos));

        self.cursor = top.and_then(|h| h.cursor).or(Some(CursorIcon::Default));

        let new_hover = top.map(|h| h.id);

        let old_chain = hover_chain_for(Some(f), self.hover_id);
        let new_chain = hover_chain_for(Some(f), new_hover);
        if new_chain != old_chain {
            dispatch_hover_change_bubbled(
                Some(f),
                &self.hover_leave,
                &mut self.hover_id,
                &mut self.hover_ancestors,
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
        let _ = repose_core::request_input_mode(repose_core::InputMode::Touch);

        let Some(f) = &self.frame_cache else {
            return PointerButtonResult {
                focused: None,
                capture_id: None,
                consumed: false,
                needs_a11y_announce: false,
                clicked_id: None,
            };
        };

        let mut result = PointerButtonResult {
            focused: None,
            capture_id: None,
            consumed: false,
            needs_a11y_announce: false,
            clicked_id: None,
        };

        if let Some(hit) = f
            .hit_regions
            .iter()
            .rev()
            .find(|h| !h.disabled && h.rect.contains(pos))
        {
            let mut path: Vec<u64> = vec![hit.id];
            let mut cur = hit.parent;
            while let Some(pid) = cur {
                path.push(pid);
                cur = f
                    .hit_regions
                    .iter()
                    .find(|h| h.id == pid)
                    .and_then(|h| h.parent);
            }
            self.hit_path = Some(path.clone());

            dnd::handle_drag_action(&DragAction::Press {
                position: pos,
                capture_id: hit.id,
                kind: PointerKind::Mouse,
                modifiers: self.modifiers,
            });

            self.capture_id = Some(hit.id);
            result.capture_id = Some(hit.id);
            result.consumed = true;

            // A new press cancels a still-pending delayed single click only
            // when it qualifies as the second tap of a double click on the
            // same element (Compose detectTapGestures).
            self.last_down = Some((hit.id, web_time::Instant::now()));
            // The second DOWN must land within
            // [doubleTapMinTimeMillis, doubleTapTimeoutMillis] after the first
            // tap's UP. No distance/slop requirement between the taps.
            self.double_candidate = if hit.on_double_click.is_some()
                && self.last_up.is_some_and(|(pid, t0, _, _)| {
                    pid == hit.id
                        && self.last_down.is_some_and(|(did, dt)| {
                            did == hit.id
                                && dt.duration_since(t0).as_millis() >= DOUBLE_TAP_MIN_MS
                                && dt.duration_since(t0).as_millis() <= DOUBLE_CLICK_MS
                        })
                }) {
                Some(hit.id)
            } else {
                None
            };
            if self.double_candidate.is_some() {
                self.pending_click = None;
            }

            match button {
                PointerButton::Primary => {
                    self.long_press = if hit.on_long_click.is_some() {
                        Some((hit.id, web_time::Instant::now(), pos.x, pos.y))
                    } else {
                        None
                    };
                    self.suppress_next_click = false;
                }
                _ => {}
            }

            if hit.tf_state_key.is_some() || is_textfield_in_frame(f, hit.id) {
                let key = tf_key_of(f, hit.id);
                let seed = hit.tf_value.as_str();
                let st_rc = ensure_tf_state(&mut self.textfield_states, key, seed);
                {
                    let mut st = st_rc.borrow_mut();
                    // Sync text only; never place-at-end here.
                    st.apply_controlled_value(seed);

                    if st.inner_width <= 0.0 {
                        let w = hit
                            .tf_content_origin
                            .map(|_| hit.rect.w)
                            .unwrap_or(hit.rect.w)
                            .max(1.0);
                        st.set_inner_width(w);
                        st.set_inner_height(hit.rect.h.max(1.0));
                    }

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
                    // caret was placed by pointer this gesture
                }
            }

            self.pressed_ids.insert(hit.id);

            if hit.focusable {
                self.sched.focused = Some(hit.id);
                result.focused = Some(hit.id);
                if hit.tf_state_key.is_some() {
                    let key = tf_key_of(f, hit.id);
                    let st =
                        ensure_tf_state(&mut self.textfield_states, key, hit.tf_value.as_str());
                    let mut s = st.borrow_mut();
                    s.apply_controlled_value(&hit.tf_value);
                    s.reset_caret_blink();
                }
            }

            self.dispatch_pointer_to_path(PointerEventKind::Down(button), pos, &path);

            request_frame();
        } else {
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
    pub fn handle_pointer_release(
        &mut self,
        pos: Vec2,
        button: PointerButton,
    ) -> PointerButtonResult {
        self.mouse_pos_px = (pos.x, pos.y);
        let mut result = PointerButtonResult {
            focused: self.sched.focused,
            capture_id: self.capture_id,
            consumed: false,
            needs_a11y_announce: false,
            clicked_id: None,
        };

        if dnd::handle_drag_action(&DragAction::Release {
            position: pos,
            modifiers: self.modifiers,
        }) {
            self.capture_id = None;
            self.hit_path = None;
            self.pressed_ids.clear();
            request_frame();
            result.consumed = true;
            return result;
        }

        self.pressed_ids.clear();

        let Some(f) = &self.frame_cache else {
            self.capture_id = None;
            self.hit_path = None;
            return result;
        };

        if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Up(button), pos, path);
            result.consumed = true;
        }

        // Long-press resolution: `poll_long_press` normally fires on timeout when held.
        if let Some((lid, t0, _, _)) = self.long_press.take() {
            if Some(lid) == self.capture_id
                && t0.elapsed().as_millis() >= LONG_PRESS_MS
                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == lid && !h.disabled)
                && let Some(cb) = &hit.on_long_click
            {
                cb();
                self.suppress_next_click = true;
                self.pending_click = None;
                result.clicked_id = Some(lid);
                result.needs_a11y_announce = true;
                result.consumed = true;
            }
        }

        if self.double_candidate.is_none()
            && !self.suppress_next_click
            && let Some(cid) = self.capture_id
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid && !h.disabled)
            && hit.rect.contains(pos)
        {
            let now = web_time::Instant::now();
            // With onDoubleTap present, single
            // clicks are delayed until the double-tap window elapses.
            if hit.on_double_click.is_some() {
                if let Some(cb) = hit.on_click.clone() {
                    self.pending_click = Some((cid, now, cb));
                }
                self.last_up = Some((cid, now, pos.x, pos.y));
                result.consumed = true;
                request_frame(); // need another frame to flush pending
            } else {
                if let Some(cb) = &hit.on_click {
                    cb();
                }
                self.last_up = Some((cid, now, pos.x, pos.y));
                result.clicked_id = Some(cid);
                result.needs_a11y_announce = true;
                result.consumed = true;
            }
        }
        self.suppress_next_click = false;

        // Double-click resolution. The second DOWN (handle_pointer_down)
        // qualifies the pair; the second UP confirms it. A canceled second tap
        // (moved out of bounds) falls back to the first tap's onClick.
        if let Some(dc) = self.double_candidate.take() {
            self.pending_click = None;
            self.last_up = None;
            self.last_down = None;
            if self.capture_id == Some(dc)
                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == dc && !h.disabled)
                && hit.rect.contains(pos)
            {
                if let Some(cb) = &hit.on_double_click {
                    cb();
                }
                result.clicked_id = Some(dc);
                result.needs_a11y_announce = true;
                result.consumed = true;
            } else if self.capture_id == Some(dc)
                && let Some(hit) = f.hit_regions.iter().find(|h| h.id == dc && !h.disabled)
            {
                // Second tap canceled -> the first tap counts as a click.
                if let Some(cb) = &hit.on_click {
                    cb();
                }
                result.clicked_id = Some(dc);
                result.needs_a11y_announce = true;
                result.consumed = true;
            }
        }

        // TextField drag end
        if let Some(cid) = self.capture_id
            && is_tf_hit(f, cid)
        {
            let key = tf_key_of(f, cid);
            if let Some(state_rc) = self.textfield_states.get(&key) {
                state_rc.borrow_mut().end_drag();
            }
        }

        self.capture_id = None;
        self.hit_path = None;
        request_frame();
        result
    }

    /// Cancel pointer state (focus lost, cursor left window, etc.).
    pub fn handle_pointer_cancel(&mut self) {
        self.long_press = None;
        self.last_up = None;
        dnd::handle_drag_action(&DragAction::Cancel);
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        dispatch_hover_change_bubbled(
            self.frame_cache.as_ref(),
            &self.hover_leave,
            &mut self.hover_id,
            &mut self.hover_ancestors,
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
        if self.hover_id.is_none() && self.hover_ancestors.is_empty() {
            return;
        }
        let pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        dispatch_hover_change_bubbled(
            self.frame_cache.as_ref(),
            &self.hover_leave,
            &mut self.hover_id,
            &mut self.hover_ancestors,
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
            dispatch_hover_change_bubbled(
                Some(new_frame),
                &self.hover_leave,
                &mut self.hover_id,
                &mut self.hover_ancestors,
                None,
                pos,
                self.modifiers,
            );
        }

        if !self.pointer_inside {
            if self.hover_id.is_some() || !self.hover_ancestors.is_empty() {
                dispatch_hover_change_bubbled(
                    Some(new_frame),
                    &self.hover_leave,
                    &mut self.hover_id,
                    &mut self.hover_ancestors,
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
            .find(|h| !h.disabled && h.rect.contains(pos))
            .map(|h| h.id);

        self.cursor = if dnd::is_dragging() {
            Some(CursorIcon::Grabbing)
        } else {
            new_hover
                .and_then(|id| new_frame.hit_regions.iter().find(|h| h.id == id))
                .and_then(|h| h.cursor)
                .or(Some(CursorIcon::Default))
        };

        let new_chain = hover_chain_for(Some(new_frame), new_hover);
        let old_chain = hover_chain_for(Some(new_frame), self.hover_id);
        if new_chain == old_chain {
            return;
        }

        dispatch_hover_change_bubbled(
            Some(new_frame),
            &self.hover_leave,
            &mut self.hover_id,
            &mut self.hover_ancestors,
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
        self.pending_click = None;
        self.last_down = None;
        self.double_candidate = None;
        self.key_long_press = None;
        self.suppress_next_click = false;
    }

    fn flush_pending_click(&mut self) {
        let Some((id, t0, cb)) = self.pending_click.take() else {
            return;
        };
        if t0.elapsed().as_millis() >= DOUBLE_CLICK_MS {
            cb();
            request_frame();
        } else {
            self.pending_click = Some((id, t0, cb));
            request_frame();
        }
    }

    fn poll_long_press(&mut self) {
        let Some(f) = self.frame_cache.clone() else {
            return;
        };
        let Some((lid, t0, _, _)) = self.long_press else {
            return;
        };
        if t0.elapsed().as_millis() < LONG_PRESS_MS {
            request_frame();
            return;
        }
        // Still captured and within the element bounds? (Compose cancels the
        // long press when the pointer leaves the element.)
        if self.capture_id != Some(lid) {
            self.long_press = None;
            return;
        }
        let (mx, my) = self.mouse_pos_px;
        let in_bounds = f
            .hit_regions
            .iter()
            .find(|h| h.id == lid)
            .map_or(false, |h| h.rect.contains(Vec2 { x: mx, y: my }));
        if !in_bounds {
            self.long_press = None;
            return;
        }
        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == lid && !h.disabled)
            && let Some(cb) = &hit.on_long_click
        {
            self.long_press = None;
            self.suppress_next_click = true;
            self.pending_click = None;
            self.last_up = None;
            cb();
            request_frame();
        } else {
            self.long_press = None;
        }
    }

    /// Holding Space/Enter past LONG_PRESS_MS fires long-click. The following KeyUp must not fire onClick.
    fn poll_key_long_press(&mut self) {
        let Some(f) = self.frame_cache.clone() else {
            return;
        };
        let Some((kid, t0, fired)) = self.key_long_press else {
            return;
        };
        if t0.elapsed().as_millis() < LONG_PRESS_MS {
            request_frame();
            return;
        }
        if !fired {
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == kid && !h.disabled)
                && let Some(cb) = &hit.on_long_click
            {
                cb();
            }
            self.key_long_press = Some((kid, t0, true));
            request_frame();
        }
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
        if event.event_type == KeyEventType::Down {
            let _ = repose_core::request_input_mode(repose_core::InputMode::Keyboard);
        }

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
                        let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) else {
                            return false;
                        };
                        if hit.on_click.is_none()
                            && hit.on_long_click.is_none()
                            && hit.on_double_click.is_none()
                        {
                            return false; // don't steal keys from non-clickable focusables
                        }
                        self.pressed_ids.insert(fid);
                        self.key_pressed_active = Some(fid);
                        self.key_long_press = if hit.on_long_click.is_some() {
                            Some((fid, web_time::Instant::now(), false))
                        } else {
                            None
                        };

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

                    let long_fired = self
                        .key_long_press
                        .take()
                        .map(|(_, _, fired)| fired)
                        .unwrap_or(false);

                    if let Some(hit) = f
                        .hit_regions
                        .iter()
                        .find(|h| h.id == active_id && !h.disabled)
                    {
                        if let Some(src) = &hit.interaction_source {
                            let pid = src.collect_last_press_id().unwrap_or(0);
                            src.to_mutable().emit(Interaction::Release(pid));
                        }
                        if !long_fired {
                            if let Some(cb) = &hit.on_click {
                                cb();
                            }
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
                if !is_tf_editable(f, fid) {
                    return true;
                }
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
                            if !is_tf_editable(f, fid) {
                                return true;
                            }
                            state.delete_backward();
                            let new_text = state.text.clone();
                            notify_text_change(f, fid, new_text);
                            tf_ensure_caret_visible(&mut state, is_multiline_id(f, fid));
                            request_frame();
                            return true;
                        }
                        Key::Delete => {
                            if !is_tf_editable(f, fid) {
                                return true;
                            }
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
                if !is_tf_editable(f, fid) {
                    return true;
                }
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

        for &id in ancestors.iter().rev() {
            if let Some(hit) = hit_by_id.get(&id)
                && let Some(cb) = &hit.on_preview_key_event
                && cb(event.clone())
            {
                return true;
            }
        }

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
            self.key_long_press = None;
            // Lazy-init + reset the caret blink for the newly focused text field.
            if let Some(hit) = f.hit_regions.iter().find(|h| h.id == new_id)
                && let Some(key) = hit.tf_state_key
            {
                let st = ensure_tf_state(&mut self.textfield_states, key, hit.tf_value.as_str());
                {
                    let mut s = st.borrow_mut();
                    s.apply_controlled_value(&hit.tf_value);
                    s.reset_caret_blink();
                }
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
        if !is_tf_hit(&f, fid) {
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
                if !is_tf_editable(&f, fid) {
                    return false;
                }
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
        if !is_tf_editable(f, fid) {
            return;
        }
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

    pub fn ensure_textfield_state_seeded(
        &mut self,
        key: u64,
        seed: &str,
    ) -> Rc<RefCell<TextFieldState>> {
        ensure_tf_state(&mut self.textfield_states, key, seed)
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
    /// Control chars are filtered. Newlines are dropped for single-line fields. Skipped during IME preedit.
    pub fn insert_text_into_focused(&mut self, text: &str) -> bool {
        if text.is_empty()
            || self.ime_preedit
            || self.modifiers.ctrl
            || self.modifiers.alt
            || self.modifiers.meta
        {
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
        if !is_tf_editable(&f, fid) {
            return false;
        }
        let key = tf_key_of(&f, fid);
        let Some(state_rc) = self.textfield_states.get(&key).cloned() else {
            return false;
        };
        let multiline = is_multiline_id(&f, fid);
        let filtered: String = text
            .chars()
            .filter(|c| {
                // Keep newlines for multiline fields; otherwise drop control
                // chars and CR (\n is a control char, so it needs an explicit
                // exception or it never survives for multiline fields).
                (*c == '\n' && multiline) || (!c.is_control() && *c != '\r')
            })
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

    /// Insert plain text into the focused textfield (winit `key_event.text`,
    /// Android soft-keyboard text, web paste). Alias for
    /// [`Self::insert_text_into_focused`].
    pub fn insert_text(&mut self, text: &str) -> bool {
        self.insert_text_into_focused(text)
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
        if !is_tf_editable(f, fid) {
            return;
        }
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
    /// Printable text goes to the focused field first. Otherwise falls through to handle_key.
    pub fn handle_key_with_text(&mut self, event: &KeyEvent, composed_text: Option<&str>) -> bool {
        if event.event_type == KeyEventType::Down {
            let _ = repose_core::request_input_mode(repose_core::InputMode::Keyboard);
        }
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
        self.textfield_states
            .get(&key)?
            .borrow()
            .next_blink_deadline()
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
    compose_frame_inner_with_ancestors(
        sched,
        root_fn,
        scale,
        size_px_u32,
        hover_id,
        &std::collections::HashSet::new(),
        pressed_ids,
        tf_states,
    )
}

pub fn compose_frame_inner_with_ancestors<F>(
    sched: &mut Scheduler,
    root_fn: &mut F,
    scale: f32,
    size_px_u32: (u32, u32),
    hover_id: Option<u64>,
    hover_ancestors: &std::collections::HashSet<u64>,
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
            let hover_ancestors = hover_ancestors.clone();
            let pressed_ids = pressed_ids.clone();
            move |view, _size| {
                let interactions = Interactions {
                    hover: hover_id,
                    hover_ancestors: hover_ancestors.clone(),
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

fn hover_chain_for(frame: Option<&Frame>, hover: Option<u64>) -> std::collections::HashSet<u64> {
    let Some(f) = frame else {
        return std::collections::HashSet::new();
    };
    let Some(mut cur) = hover else {
        return std::collections::HashSet::new();
    };
    let map: std::collections::HashMap<u64, Option<u64>> =
        f.hit_regions.iter().map(|h| (h.id, h.parent)).collect();
    let mut set = std::collections::HashSet::new();
    loop {
        set.insert(cur);
        if let Some(Some(parent)) = map.get(&cur).copied() {
            cur = parent;
        } else {
            break;
        }
    }
    set
}

fn dispatch_hover_change_bubbled(
    frame: Option<&Frame>,
    leave_map: &HashMap<u64, (f32, f32, f32, f32, Rc<dyn Fn(PointerEvent)>)>,
    hover_id: &mut Option<u64>,
    hover_ancestors: &mut std::collections::HashSet<u64>,
    new_hover: Option<u64>,
    pos: Vec2,
    modifiers: Modifiers,
) {
    let old_hover = *hover_id;
    let old_chain = hover_chain_for(frame, old_hover);
    let new_chain = hover_chain_for(frame, new_hover);
    if old_chain == new_chain {
        return;
    }
    for leave_id in old_chain.difference(&new_chain) {
        let leave_info = leave_map.get(leave_id).cloned().or_else(|| {
            frame.and_then(|f| {
                f.hit_regions
                    .iter()
                    .find(|h| h.id == *leave_id)
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
    for enter_id in new_chain.difference(&old_chain) {
        if let Some(f) = frame
            && let Some(h) = f.hit_regions.iter().find(|h| h.id == *enter_id)
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
    }
    *hover_id = new_hover;
    hover_ancestors.clear();
    for id in &new_chain {
        if Some(*id) != new_hover {
            hover_ancestors.insert(*id);
        }
    }
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

/// `enabled=false` rejects edits. `readOnly` also rejects
/// edits but keeps selection/focus/copy working.
fn tf_can_edit(hit: &HitRegion) -> bool {
    hit.tf_enabled && !hit.tf_read_only
}

fn is_tf_editable(f: &Frame, id: u64) -> bool {
    f.hit_regions
        .iter()
        .find(|h| h.id == id)
        .is_some_and(tf_can_edit)
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
