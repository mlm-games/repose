use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use repose_core::dnd;
use repose_core::input::{
    GamepadAxis, GamepadButton, GamepadEvent, ImeEvent, Key, KeyEvent, KeyEventType, Modifiers,
    PointerButton, PointerEvent, PointerEventKind, PointerId, PointerKind,
};
use repose_core::locals::{Density, set_density_default, with_density};
use repose_core::runtime::{Frame, Scheduler};
use repose_core::shortcuts::DragAction;
use repose_core::{
    CursorIcon, HitRegion, Interaction, Modifier, RenderContext, Scene, Vec2, View, request_frame,
};
use repose_ui::textfield::TextFieldState;
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

struct TouchPressState {
    capture_id: u64,
    pressed_ids: HashSet<u64>,
}

struct DndPointerCapture {
    touch: Option<u64>,
    source_id: u64,
}

/// Embeddable Repose runtime.
///
/// Manages composition scheduling, input routing, text-field state, and
/// pointer/key dispatch.  The host owns the event loop and GPU device. This
/// is purely the UI logic layer.
pub struct ReposeRuntime {
    pub sched: Scheduler,
    pub scale: f32,
    /// Ambient host layer for floating surfaces. Installed around
    /// composition each frame; entries render at the root.
    pub overlay: repose_ui::overlay::OverlayHandle,
    pub dnd_context: repose_core::dnd::DndContext,
    lifecycle_events: std::sync::Arc<crate::lifecycle::LifecycleDispatcher>,
    deeplink_events: std::sync::Arc<crate::lifecycle::DeeplinkDispatcher>,

    pub modifiers: Modifiers,
    pub mouse_pos_px: (f32, f32),
    /// Whether the pointer is currently inside the window.
    pub pointer_inside: bool,
    pub hover_id: Option<u64>,
    pub hover_ancestors: std::collections::HashSet<u64>,
    /// Needed so `Leave` still fires
    /// even when the hovered hit region is removed from the tree between frames.
    /// Rebuilt on every `cache_frame`.
    hover_leave: HashMap<u64, repose_ui::HitRegionSnapshot>,
    pub capture_id: Option<u64>,
    /// Hit path captured at pointer-down: every region under the pointer,
    /// ordered bottom-up (deepest child first, ancestors last).
    pub hit_path: Option<Vec<u64>>,
    /// Per-finger press table: every concurrent touch owns its own
    /// hit path (unlike the single mouse `hit_path`/`capture_id`,
    /// which is the single-primary tap emulation's). Game viewports
    /// stage per-finger contacts off the dispatched events, so the
    /// second finger's joystick/button press stages alongside the
    /// first instead of stealing its path. UI press arbitration
    /// (tap/click/scroll) stays single-primary in `touch_gesture`;
    /// this only routes the per-finger event stream.
    pub touch_paths: HashMap<u64, Vec<u64>>,
    mouse_targets: Option<Vec<repose_ui::HitRegionSnapshot>>,
    touch_targets: HashMap<u64, Vec<repose_ui::HitRegionSnapshot>>,
    touch_positions: HashMap<u64, Vec2>,
    touch_presses: HashMap<u64, TouchPressState>,
    dnd_capture: Option<DndPointerCapture>,
    touch_primary: Option<u64>,
    suppressed_touch_clicks: HashSet<u64>,
    /// Which scroll consumer currently owns the wheel gesture.
    pub scroll_capture_id: Option<u64>,
    last_scroll_at: Option<web_time::Instant>,
    pub pressed_ids: HashSet<u64>,
    pub ime_preedit: bool,
    pub key_pressed_active: Option<u64>,
    key_pressed_key: Option<Key>,
    pub last_focus: Option<u64>,
    /// Polled physical-key state, keyed by debug name (`KeyCode::KeyW`,
    /// `Digit1`, ...). Platform runners report every `KeyboardInput`
    /// press/release here so games can poll held keys GML-style
    /// (`keyboard_check`) instead of reconstructing them from
    /// focus-routed key events, which miss keys when focus moves or
    /// a key-up is swallowed (alt-tab, overlay, layout).
    /// Cleared on window focus loss.
    pub held_keys: HashSet<String>,
    /// Polled mouse-button state. `handle_pointer_press/release`
    /// maintain this for Primary/Secondary/Tertiary so games can read
    /// held buttons (`mouse_check_button`) without tracking
    /// press/release edges themselves.
    pub held_mouse: HashSet<PointerButton>,

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
    long_press_touch: Option<u64>,
    /// Keyboard long-press (Compose combinedClickable: holding Space/Enter
    /// past LONG_PRESS_MS fires on_long_click). `bool` = already fired.
    key_long_press: Option<(u64, web_time::Instant, bool)>,
    suppress_next_click: bool,
    pending_click: Option<(u64, web_time::Instant, Rc<dyn Fn()>)>,

    pub frame_cache: Option<Frame>,

    cursor: Option<CursorIcon>,

    pub shortcuts: repose_core::shortcuts::ShortcutState,

    pub textfield_states: HashMap<u64, Rc<RefCell<TextFieldState>>>,
    /// Connected gamepads by backend id: display name plus live button/axis
    /// state. Fed by [`ReposeRuntime::handle_gamepad`].
    pub gamepads: HashMap<u32, GamepadPad>,
    /// Queued dual-motor rumble requests.
    pub pending_rumble: Vec<(u32, f32, f32, u32)>,
}

/// Live state of one connected gamepad, mirrored from [`GamepadEvent`]s.
#[derive(Clone, Debug, Default)]
pub struct GamepadPad {
    pub name: String,
    pub pressed: HashSet<GamepadButton>,
    pub axes: HashMap<GamepadAxis, f32>,
}

impl GamepadPad {
    pub fn button(&self, button: GamepadButton) -> bool {
        self.pressed.contains(&button)
    }

    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }
}

impl ReposeRuntime {
    pub fn new() -> Self {
        Self::with_overlay(repose_ui::overlay::OverlayHandle::new())
    }

    /// Create a runtime sharing `overlay` as the ambient host layer.
    /// Entries posted through this handle render at the root; per-frame
    /// `show_guard` state can still target other handles as an escape hatch.
    pub fn with_overlay(overlay: repose_ui::overlay::OverlayHandle) -> Self {
        Self {
            overlay,
            dnd_context: repose_core::dnd::DndContext::default(),
            lifecycle_events: std::sync::Arc::new(crate::lifecycle::LifecycleDispatcher::default()),
            deeplink_events: std::sync::Arc::new(crate::lifecycle::DeeplinkDispatcher::default()),
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
            touch_paths: HashMap::new(),
            mouse_targets: None,
            touch_targets: HashMap::new(),
            touch_positions: HashMap::new(),
            touch_presses: HashMap::new(),
            dnd_capture: None,
            touch_primary: None,
            suppressed_touch_clicks: HashSet::new(),
            scroll_capture_id: None,
            last_scroll_at: None,
            pressed_ids: HashSet::new(),
            ime_preedit: false,
            key_pressed_active: None,
            key_pressed_key: None,
            last_focus: None,
            held_keys: HashSet::new(),
            held_mouse: HashSet::new(),
            last_up: None,
            last_down: None,
            double_candidate: None,
            long_press: None,
            long_press_touch: None,
            key_long_press: None,
            suppress_next_click: false,
            pending_click: None,
            frame_cache: None,
            cursor: None,
            shortcuts: repose_core::shortcuts::ShortcutState::new().without_global_fallback(),
            textfield_states: HashMap::new(),
            gamepads: HashMap::new(),
            pending_rumble: Vec::new(),
        }
    }

    pub fn event_dispatchers(&self) -> crate::lifecycle::RuntimeDispatchers {
        crate::lifecycle::RuntimeDispatchers {
            lifecycle: self.lifecycle_events.clone(),
            deeplink: self.deeplink_events.clone(),
        }
    }

    pub fn set_lifecycle_callback(
        &mut self,
        callback: Box<dyn Fn(crate::lifecycle::AppLifecycle) + Send>,
    ) {
        self.lifecycle_events.set_callback(callback);
    }

    pub fn add_lifecycle_listener(
        &mut self,
        callback: Box<dyn Fn(crate::lifecycle::AppLifecycle) + Send>,
    ) -> u64 {
        self.lifecycle_events.add_listener(callback)
    }

    pub fn remove_lifecycle_listener(&mut self, id: u64) -> bool {
        self.lifecycle_events.remove_listener(id)
    }

    pub fn current_lifecycle(&self) -> Option<crate::lifecycle::AppLifecycle> {
        self.lifecycle_events.current()
    }

    pub fn push_lifecycle(&mut self, state: crate::lifecycle::AppLifecycle) {
        self.lifecycle_events.push(state);
    }

    pub fn process_lifecycle(&mut self) {
        self.lifecycle_events.process();
    }

    pub fn set_deeplink_callback(&mut self, callback: Box<dyn Fn(Vec<u8>) + Send>) {
        self.deeplink_events.set_callback(callback);
    }

    pub fn add_deeplink_listener(&mut self, callback: Box<dyn Fn(Vec<u8>) + Send>) -> u64 {
        self.deeplink_events.add_listener(callback)
    }

    pub fn remove_deeplink_listener(&mut self, id: u64) -> bool {
        self.deeplink_events.remove_listener(id)
    }

    pub fn push_deeplink(&mut self, data: Vec<u8>) {
        self.deeplink_events.push(data);
    }

    pub fn process_deeplinks(&mut self) {
        self.deeplink_events.process();
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
        let _dnd_guard = self.dnd_context.enter();
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        self.poll_long_press();
        self.flush_pending_click();
        self.poll_key_long_press();

        let size = self.sched.size;
        let rc = render_ctx.clone();
        let overlay = self.overlay.clone();
        let mut compose_once = |this: &mut Self| {
            let overlay = overlay.clone();
            let ambient = this.overlay.clone();
            let mut inner = |s: &mut Scheduler| {
                repose_ui::overlay::with_ambient_overlay(ambient.clone(), || {
                    let content = (root_fn)(s, &rc);
                    overlay.host(Modifier::new().fill_max_size(), content)
                })
            };
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

        let shortcut_state = self.shortcuts.clone();
        let frame =
            repose_core::shortcuts::with_runtime_state(&shortcut_state, || compose_once(self));

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
                if h.on_pointer_leave.is_some() {
                    self.hover_leave
                        .insert(h.id, repose_ui::HitRegionSnapshot::new(h));
                }
            }
            // Hover should be stable: same geometry + same pointer. Do not loop.
            return repose_core::shortcuts::with_runtime_state(&shortcut_state, || {
                compose_once(self)
            });
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

        let focused_hit = self
            .sched
            .focused
            .and_then(|fid| f.hit_regions.iter().find(|h| h.id == fid));
        let ime_allowed = focused_hit.is_some_and(|hit| {
            hit.tf_enabled
                && !hit.tf_read_only
                && f.semantics_nodes.iter().any(|node| {
                    node.id == hit.id && node.role == repose_core::semantics::Role::TextField
                })
        });

        let ime_cursor_area = if ime_allowed {
            focused_hit.map(|hit| {
                let sf = self.scale as f64;
                let origin = repose_ui::hit_region_transformed_origin(hit);
                let local = repose_ui::hit_region_local_rect(hit);
                (
                    origin.x as f64 / sf,
                    origin.y as f64 / sf,
                    local.w as f64 / sf,
                    local.h as f64 / sf,
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
            cursor: self
                .sched
                .cursor_override
                .take()
                .or_else(|| self.take_cursor_suggestion()),
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
        let _dnd_guard = self.dnd_context.enter();
        if self.key_pressed_active.is_some_and(|id| {
            self.sched.focused != Some(id)
                || !frame
                    .hit_regions
                    .iter()
                    .any(|hit| hit.id == id && !hit.disabled)
        }) {
            self.cancel_keyboard_press();
        }
        self.hover_leave.clear();
        for hit in &frame.hit_regions {
            if hit.on_pointer_leave.is_some() {
                self.hover_leave
                    .insert(hit.id, repose_ui::HitRegionSnapshot::new(hit));
            }
        }
        self.frame_cache = Some(frame);
        self.reconcile_cached_targets();
    }

    /// Post-compose host-agnostic bookkeeping.
    /// this lazy-initializes text-field state for focus-requester paths, reconciles
    /// hover against the new hit list, and publishes the frame to the DnD
    /// registry. Replaces platform-local copies of this logic.
    pub fn after_compose(&mut self, frame: &Frame, scale: f32) {
        let _dnd_guard = self.dnd_context.enter();
        ensure_all_tf_states_from_frame(&mut self.textfield_states, frame);
        self.prune_textfield_states(frame);
        self.ensure_focused_state_in_frame(frame);
        self.reconcile_hover_from_mouse_pos(frame);
        repose_core::dnd::set_dnd_frame(Some(frame.clone()));
        repose_core::dnd::set_dnd_scale(scale);
    }

    fn prune_textfield_states(&mut self, frame: &Frame) {
        if self.textfield_states.is_empty() {
            return;
        }
        let live: std::collections::HashSet<u64> = frame
            .hit_regions
            .iter()
            .filter_map(|h| h.tf_state_key)
            .collect();
        self.textfield_states.retain(|k, _| live.contains(k));
    }

    fn end_textfield_drag(&mut self, id: u64) {
        let key = self
            .frame_cache
            .as_ref()
            .and_then(|frame| is_tf_hit(frame, id).then(|| tf_key_of(frame, id)));
        if let Some(key) = key {
            self.end_textfield_drag_key(key);
        }
    }

    fn end_textfield_drag_key(&mut self, key: u64) {
        if let Some(state) = self.textfield_states.get(&key) {
            state.borrow_mut().end_drag();
        }
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
        self.cache_frame(frame.clone());
        self.after_compose(&frame, self.scale);
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

    fn dispatch_pointer_to_targets(
        &self,
        kind: PointerEventKind,
        pos: Vec2,
        targets: &[repose_ui::HitRegionSnapshot],
        touch: Option<u64>,
    ) {
        let (id, pkind) = match touch {
            Some(tid) => (PointerId(tid), PointerKind::Touch),
            None => (PointerId(0), PointerKind::Mouse),
        };
        let base = PointerEvent::new(id, pkind, kind, pos, 1.0, self.modifiers);
        for target in targets {
            let hit = target.hit();
            let callback = match kind {
                PointerEventKind::Down(_) => &hit.on_pointer_down,
                PointerEventKind::Up(_) => &hit.on_pointer_up,
                PointerEventKind::Move => &hit.on_pointer_move,
                PointerEventKind::Cancel => &hit.on_pointer_cancel,
                PointerEventKind::Enter | PointerEventKind::Leave => continue,
            };
            let Some(callback) = callback else {
                continue;
            };
            let mut event = base.clone();
            let (origin, local) = target.pointer_coordinates(pos);
            event.origin = origin;
            event.position = local;
            callback(event);
            if !matches!(kind, PointerEventKind::Cancel) && base.is_consumed() {
                break;
            }
        }
    }

    fn dispatch_pointer_to_path(
        &self,
        kind: PointerEventKind,
        pos: Vec2,
        path: &[u64],
        touch: Option<u64>,
    ) {
        let Some(frame) = &self.frame_cache else {
            return;
        };
        let targets: Vec<_> = path
            .iter()
            .filter_map(|id| frame.hit_regions.iter().find(|hit| hit.id == *id))
            .map(repose_ui::HitRegionSnapshot::new)
            .collect();
        self.dispatch_pointer_to_targets(kind, pos, &targets, touch);
    }

    fn refreshed_capture_targets(
        &self,
        targets: &[repose_ui::HitRegionSnapshot],
    ) -> (
        Vec<repose_ui::HitRegionSnapshot>,
        Vec<repose_ui::HitRegionSnapshot>,
    ) {
        let Some(frame) = &self.frame_cache else {
            return (targets.to_vec(), Vec::new());
        };
        let mut live = Vec::with_capacity(targets.len());
        let mut stale = Vec::new();
        for target in targets {
            if let Some(hit) = frame
                .hit_regions
                .iter()
                .find(|hit| hit.id == target.id() && !hit.disabled)
            {
                live.push(repose_ui::HitRegionSnapshot::new(hit));
            } else {
                stale.push(target.clone());
            }
        }
        (live, stale)
    }

    fn cancel_dnd_capture(&mut self, touch: Option<u64>, source_id: u64) -> bool {
        if !self
            .dnd_capture
            .as_ref()
            .is_some_and(|capture| capture.touch == touch && capture.source_id == source_id)
        {
            return false;
        }
        self.dnd_capture = None;
        dnd::handle_drag_action(&DragAction::Cancel)
    }

    fn cancel_active_dnd(&mut self) -> bool {
        if self.dnd_capture.take().is_none() {
            return false;
        }
        dnd::handle_drag_action(&DragAction::Cancel)
    }

    fn reconcile_cached_targets(&mut self) {
        let mouse_pos = Vec2 {
            x: self.mouse_pos_px.0,
            y: self.mouse_pos_px.1,
        };
        if let Some(targets) = self.mouse_targets.take() {
            let capture_id = targets.first().map(|target| target.id());
            let capture_key = targets.first().and_then(|target| target.hit().tf_state_key);
            let (live, stale) = self.refreshed_capture_targets(&targets);
            if !stale.is_empty() {
                self.dispatch_pointer_to_targets(PointerEventKind::Cancel, mouse_pos, &stale, None);
            }
            let capture_stale =
                capture_id.is_some_and(|id| stale.iter().any(|target| target.id() == id));
            if capture_stale {
                self.pressed_ids.remove(&capture_id.unwrap_or_default());
                self.capture_id = None;
                self.hit_path = None;
                if let Some(key) = capture_key {
                    self.end_textfield_drag_key(key);
                }
                if self
                    .long_press
                    .is_some_and(|(id, _, _, _)| Some(id) == capture_id)
                {
                    self.long_press = None;
                    self.long_press_touch = None;
                }
                self.cancel_dnd_capture(None, capture_id.unwrap_or_default());
            } else {
                self.mouse_targets = (!live.is_empty()).then_some(live);
                self.hit_path = self
                    .mouse_targets
                    .as_ref()
                    .map(|targets| targets.iter().map(|target| target.id()).collect());
            }
            if !stale.is_empty() {
                request_frame();
            }
        }

        let mut touch_ids: Vec<u64> = self.touch_targets.keys().copied().collect();
        touch_ids.sort_unstable();
        let mut changed = false;
        for tid in touch_ids {
            let Some(targets) = self.touch_targets.remove(&tid) else {
                continue;
            };
            let pos = self.touch_positions.get(&tid).copied().unwrap_or(mouse_pos);
            let old_capture = self.touch_presses.get(&tid).map(|state| state.capture_id);
            let old_capture_key = targets.first().and_then(|target| target.hit().tf_state_key);
            let (live, stale) = self.refreshed_capture_targets(&targets);
            if !stale.is_empty() {
                self.dispatch_pointer_to_targets(PointerEventKind::Cancel, pos, &stale, Some(tid));
                changed = true;
            }
            let capture_stale =
                old_capture.is_some_and(|id| stale.iter().any(|target| target.id() == id));
            if capture_stale {
                self.touch_targets.remove(&tid);
                self.touch_paths.remove(&tid);
                self.touch_presses.remove(&tid);
                self.suppressed_touch_clicks.remove(&tid);
                if let Some(key) = old_capture_key {
                    self.end_textfield_drag_key(key);
                }
                if self.long_press_touch == Some(tid)
                    && old_capture.is_some_and(|id| {
                        self.long_press
                            .is_some_and(|(long_id, _, _, _)| long_id == id)
                    })
                {
                    self.long_press = None;
                    self.long_press_touch = None;
                }
                if self.touch_primary == Some(tid) {
                    self.touch_primary = None;
                    self.double_candidate = None;
                    self.pending_click = None;
                    self.last_up = None;
                    self.last_down = None;
                    self.suppress_next_click = false;
                    self.scroll_capture_id = None;
                    if let Some(source_id) = old_capture {
                        self.cancel_dnd_capture(Some(tid), source_id);
                    }
                }
            } else if live.is_empty() {
                self.touch_paths.remove(&tid);
                self.touch_presses.remove(&tid);
                self.suppressed_touch_clicks.remove(&tid);
            } else {
                self.touch_targets.insert(tid, live.clone());
                self.touch_paths
                    .insert(tid, live.iter().map(|target| target.id()).collect());
            }
        }
        if changed {
            self.rebuild_pressed_ids();
            request_frame();
        }
    }

    /// Process a pointer-move event. Returns cursor suggestion.
    pub fn handle_pointer_move(&mut self, pos: Vec2) -> PointerMoveResult {
        self.handle_touch_move(None, pos)
    }

    /// Touch move with a stable finger id (same pairing as
    /// [`Self::handle_touch_press`]; hover fallback stays mouse).
    pub fn handle_touch_move(&mut self, touch: Option<u64>, pos: Vec2) -> PointerMoveResult {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let _dnd_guard = self.dnd_context.enter();
        self.mouse_pos_px = (pos.x, pos.y);
        if let Some(tid) = touch {
            self.touch_positions.insert(tid, pos);
        }
        self.pointer_inside = true;
        if touch.is_none() {
            self.sched.pointer_pos_px = Some((pos.x, pos.y));
        }

        let owns_dnd_move = self
            .dnd_capture
            .as_ref()
            .is_some_and(|capture| capture.touch == touch);
        let dnd_move_consumed = owns_dnd_move
            && dnd::handle_drag_action(&DragAction::Move {
                position: pos,
                modifiers: self.modifiers,
            });
        if dnd_move_consumed {
            self.cancel_keyboard_press();
            request_frame();
        }

        let Some(f) = &self.frame_cache else {
            return PointerMoveResult {
                cursor: None,
                hover_id: None,
            };
        };

        let active_long_press_matches = match (touch, self.long_press_touch) {
            (Some(tid), Some(active)) => tid == active,
            (None, None) => true,
            _ => false,
        };
        if active_long_press_matches && let Some((_, _, x0, y0)) = self.long_press {
            let slop = LONG_PRESS_SLOP_DP * self.scale;
            let dx = pos.x - x0;
            let dy = pos.y - y0;
            if dx * dx + dy * dy > slop * slop {
                self.long_press = None;
                self.long_press_touch = None;
            }
        }

        // Cancel the long press once the pointer leaves the element's bounds
        if active_long_press_matches
            && self.long_press.is_some()
            && let Some(lid) = self.long_press.map(|(id, _, _, _)| id)
            && f.hit_regions
                .iter()
                .find(|h| h.id == lid)
                .is_none_or(|h| !repose_ui::hit_region_contains(h, pos))
        {
            self.long_press = None;
            self.long_press_touch = None;
        }

        let text_capture = match touch {
            Some(tid) => self.touch_presses.get(&tid).map(|state| state.capture_id),
            None => self.capture_id,
        };
        let owns_text_drag = touch.is_none() || self.touch_primary == touch;
        if owns_text_drag
            && let Some(cid) = text_capture
            && is_tf_hit(f, cid)
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid)
        {
            let key = tf_key_of(f, cid);
            if let Some(st_rc) = self.textfield_states.get(&key) {
                let mut st = st_rc.borrow_mut();
                let local = repose_ui::hit_region_to_local(hit, pos);
                let local_rect = repose_ui::hit_region_local_rect(hit);
                let content_origin = hit
                    .tf_content_origin
                    .map(|(x, y)| repose_ui::hit_region_to_local(hit, Vec2 { x, y }))
                    .unwrap_or(Vec2 {
                        x: local_rect.x,
                        y: local_rect.y,
                    });
                let content_x = (local.x - content_origin.x + st.scroll_offset).max(0.0);
                let content_y = (local.y - content_origin.y + st.scroll_offset_y).max(0.0);
                let metrics = repose_ui::textfield::textfield_metrics(hit);
                let wrap_w = st.inner_width.max(1.0);
                let idx = if hit.tf_multiline {
                    index_for_xy_bytes_vt(&st, &metrics, wrap_w, content_x, content_y)
                } else {
                    index_for_x_bytes_vt(&st, &metrics, content_x)
                };
                st.drag_to(idx);
            }
        }

        let top = repose_ui::hit_test_enabled_frame(f, pos);

        self.cursor = top
            .and_then(|h| h.cursor.clone())
            .or(Some(CursorIcon::Default));
        if dnd_move_consumed && dnd::is_dragging() {
            self.cursor = Some(CursorIcon::Grabbing);
        }

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

        if let Some(tid) = touch {
            if let Some(targets) = self.touch_targets.get(&tid) {
                self.dispatch_pointer_to_targets(PointerEventKind::Move, pos, targets, Some(tid));
            } else if let Some(path) = self.touch_paths.get(&tid).cloned() {
                self.dispatch_pointer_to_path(PointerEventKind::Move, pos, &path, Some(tid));
            }
            return PointerMoveResult {
                cursor: self.cursor.clone(),
                hover_id: self.hover_id,
            };
        }

        if let Some(targets) = &self.mouse_targets {
            self.dispatch_pointer_to_targets(PointerEventKind::Move, pos, targets, None);
        } else if let Some(path) = self.hit_path.clone() {
            self.dispatch_pointer_to_path(PointerEventKind::Move, pos, &path, None);
        }
        if self.hit_path.is_none()
            && let Some(h) = top
            && let Some(cb) = &h.on_pointer_move
        {
            let (id, kind) = match touch {
                Some(tid) => (PointerId(tid), PointerKind::Touch),
                None => (PointerId(0), PointerKind::Mouse),
            };
            let mut pe =
                PointerEvent::new(id, kind, PointerEventKind::Move, pos, 1.0, self.modifiers);
            let (origin, local) = repose_ui::hit_region_pointer_coordinates(h, pos);
            pe.origin = origin;
            pe.position = local;
            cb(pe);
        }

        PointerMoveResult {
            cursor: self.cursor.clone(),
            hover_id: self.hover_id,
        }
    }

    /// Process a pointer button press. Returns focus/capture info.
    pub fn handle_pointer_press(
        &mut self,
        pos: Vec2,
        button: PointerButton,
    ) -> PointerButtonResult {
        self.handle_touch_press(None, pos, button)
    }

    /// Touch press with a stable finger id: same hit-path dispatch as
    /// the mouse press, but the event carries `PointerKind::Touch` +
    /// the finger id, so game viewports stage per-finger contacts
    /// instead of one shared mouse point (GML `device_mouse_*` parity).
    pub fn handle_touch_press(
        &mut self,
        touch: Option<u64>,
        pos: Vec2,
        button: PointerButton,
    ) -> PointerButtonResult {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let _dnd_guard = self.dnd_context.enter();
        self.mouse_pos_px = (pos.x, pos.y);
        if let Some(tid) = touch {
            self.touch_positions.insert(tid, pos);
        }
        if touch.is_none() {
            self.sched.pointer_pos_px = Some((pos.x, pos.y));
        }
        if touch.is_none() {
            self.held_mouse.insert(button);
        }
        let _ = repose_core::request_input_mode(repose_core::InputMode::Touch);

        if touch.is_none() && (self.capture_id.is_some() || self.hit_path.is_some()) {
            self.handle_pointer_cancel();
        }
        if let Some(tid) = touch
            && self.touch_presses.contains_key(&tid)
        {
            self.handle_touch_cancel(Some(tid));
        }
        if let Some(tid) = touch {
            self.touch_positions.insert(tid, pos);
        }

        let Some(frame) = self.frame_cache.clone() else {
            return PointerButtonResult {
                focused: None,
                capture_id: None,
                consumed: false,
                needs_a11y_announce: false,
                clicked_id: None,
            };
        };
        let f = &frame;

        let mut result = PointerButtonResult {
            focused: None,
            capture_id: None,
            consumed: false,
            needs_a11y_announce: false,
            clicked_id: None,
        };
        let primary_pointer = match touch {
            Some(tid) => {
                if self.touch_primary.is_none() {
                    self.touch_primary = Some(tid);
                }
                self.touch_primary == Some(tid)
            }
            None => true,
        };

        if primary_pointer {
            self.cancel_keyboard_press();
        }

        let path = repose_ui::hit_test_frame_path(f, pos);
        if let Some(&capture_id) = path.first()
            && let Some(hit) = f.hit_regions.iter().find(|hit| hit.id == capture_id)
        {
            if touch.is_none() {
                self.hit_path = Some(path.clone());
            }

            if touch.is_none() || primary_pointer {
                dnd::handle_drag_action(&DragAction::Press {
                    position: pos,
                    capture_id: hit.id,
                    kind: match touch {
                        Some(_) => PointerKind::Touch,
                        None => PointerKind::Mouse,
                    },
                    modifiers: self.modifiers,
                });
                self.dnd_capture = Some(DndPointerCapture {
                    touch,
                    source_id: hit.id,
                });
            }

            if touch.is_none() {
                self.capture_id = Some(hit.id);
            }
            result.capture_id = Some(hit.id);
            result.consumed = true;
            let targets: Vec<_> = path
                .iter()
                .filter_map(|id| f.hit_regions.iter().find(|hit| hit.id == *id))
                .map(repose_ui::HitRegionSnapshot::new)
                .collect();
            if let Some(tid) = touch {
                self.touch_paths.insert(tid, path.clone());
                self.touch_targets.insert(tid, targets);
                self.touch_presses.insert(
                    tid,
                    TouchPressState {
                        capture_id: hit.id,
                        pressed_ids: HashSet::from([hit.id]),
                    },
                );
            } else {
                self.mouse_targets = Some(targets);
            }

            // A new press cancels a still-pending delayed single click only
            // when it qualifies as the second tap of a double click on the
            // same element (Compose detectTapGestures).
            if primary_pointer {
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

                if let PointerButton::Primary = button {
                    self.long_press = if hit.on_long_click.is_some() {
                        Some((hit.id, web_time::Instant::now(), pos.x, pos.y))
                    } else {
                        None
                    };
                    self.long_press_touch = touch;
                    self.suppress_next_click = false;
                }
            }

            if primary_pointer
                && hit.tf_enabled
                && (hit.tf_state_key.is_some() || is_textfield_in_frame(f, hit.id))
            {
                let key = tf_key_of(f, hit.id);
                let seed = hit.tf_value.as_str();
                let st_rc = ensure_tf_state(&mut self.textfield_states, key, seed);
                {
                    let mut st = st_rc.borrow_mut();
                    // Sync text only; never place-at-end here.
                    st.apply_controlled_value(seed);

                    if st.inner_width <= 0.0 {
                        let local_rect = repose_ui::hit_region_local_rect(hit);
                        st.set_inner_width(local_rect.w.max(1.0));
                        st.set_inner_height(local_rect.h.max(1.0));
                    }

                    let local = repose_ui::hit_region_to_local(hit, pos);
                    let local_rect = repose_ui::hit_region_local_rect(hit);
                    let content_origin = hit
                        .tf_content_origin
                        .map(|(x, y)| repose_ui::hit_region_to_local(hit, Vec2 { x, y }))
                        .unwrap_or(Vec2 {
                            x: local_rect.x,
                            y: local_rect.y,
                        });
                    let content_x = (local.x - content_origin.x + st.scroll_offset).max(0.0);
                    let content_y = (local.y - content_origin.y + st.scroll_offset_y).max(0.0);
                    let metrics = repose_ui::textfield::textfield_metrics(hit);
                    let wrap_w = st.inner_width.max(1.0);

                    let idx = if hit.tf_multiline {
                        index_for_xy_bytes_vt(&st, &metrics, wrap_w, content_x, content_y)
                    } else {
                        index_for_x_bytes_vt(&st, &metrics, content_x)
                    };
                    st.handle_pointer_down(idx, (pos.x, pos.y), self.modifiers.shift);
                    // caret was placed by pointer this gesture
                }
            }

            self.pressed_ids.insert(hit.id);

            if primary_pointer && hit.focusable {
                if self.sched.focused != Some(hit.id) {
                    self.finish_compositions();
                }
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

            let targets = if touch.is_some() {
                self.touch_targets.get(&touch.unwrap()).cloned()
            } else {
                self.mouse_targets.clone()
            };
            if let Some(targets) = targets {
                self.dispatch_pointer_to_targets(
                    PointerEventKind::Down(button),
                    pos,
                    &targets,
                    touch,
                );
            }

            request_frame();
        } else if primary_pointer {
            if touch.is_none() {
                self.hit_path = None;
                self.mouse_targets = None;
                self.capture_id = None;
                self.pressed_ids.clear();
            } else if let Some(tid) = touch {
                self.touch_paths.remove(&tid);
                self.touch_targets.remove(&tid);
                self.touch_positions.remove(&tid);
            }
            self.finish_compositions();
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
        self.handle_touch_release(None, pos, button)
    }

    /// Touch release with a stable finger id (same pairing as
    /// [`Self::handle_touch_press`]).
    pub fn handle_touch_release(
        &mut self,
        touch: Option<u64>,
        pos: Vec2,
        button: PointerButton,
    ) -> PointerButtonResult {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let _dnd_guard = self.dnd_context.enter();
        self.mouse_pos_px = (pos.x, pos.y);
        if let Some(tid) = touch {
            self.touch_positions.insert(tid, pos);
        } else {
            self.sched.pointer_pos_px = Some((pos.x, pos.y));
            self.held_mouse.remove(&button);
        }
        let mut result = PointerButtonResult {
            focused: self.sched.focused,
            capture_id: self.capture_id,
            consumed: false,
            needs_a11y_announce: false,
            clicked_id: None,
        };
        if let Some(tid) = touch {
            return self.finish_touch_release(tid, pos, button, result);
        }

        let owns_dnd_release = self.dnd_capture.as_ref().is_some_and(|capture| {
            capture.touch.is_none() && Some(capture.source_id) == self.capture_id
        });
        let drag_consumed = if owns_dnd_release {
            self.dnd_capture = None;
            dnd::handle_drag_action(&DragAction::Release {
                position: pos,
                modifiers: self.modifiers,
            })
        } else {
            false
        };
        if let Some(targets) = &self.mouse_targets {
            self.dispatch_pointer_to_targets(PointerEventKind::Up(button), pos, targets, None);
            result.consumed = true;
        } else if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Up(button), pos, path, None);
            result.consumed = true;
        }
        if let Some(cid) = self.capture_id {
            self.pressed_ids.remove(&cid);
            self.end_textfield_drag(cid);
        }
        if drag_consumed {
            self.capture_id = None;
            self.hit_path = None;
            self.mouse_targets = None;
            request_frame();
            result.consumed = true;
            return result;
        }

        let f = match &self.frame_cache {
            Some(f) => f.clone(),
            None => {
                self.capture_id = None;
                self.hit_path = None;
                self.mouse_targets = None;
                self.pressed_ids.clear();
                return result;
            }
        };

        // Long-press resolution: `poll_long_press` normally fires on timeout when held.
        if self.long_press_touch.is_none()
            && let Some((lid, t0, _, _)) = self.long_press.take()
            && Some(lid) == self.capture_id
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

        if self.double_candidate.is_none()
            && !self.suppress_next_click
            && let Some(cid) = self.capture_id
            && let Some(hit) = f.hit_regions.iter().find(|h| h.id == cid && !h.disabled)
            && repose_ui::hit_region_contains(hit, pos)
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
                && repose_ui::hit_region_contains(hit, pos)
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

        self.capture_id = None;
        self.hit_path = None;
        self.mouse_targets = None;
        request_frame();
        result
    }

    fn finish_touch_release(
        &mut self,
        tid: u64,
        pos: Vec2,
        button: PointerButton,
        mut result: PointerButtonResult,
    ) -> PointerButtonResult {
        let capture_id = self
            .touch_presses
            .remove(&tid)
            .map(|state| state.capture_id);
        result.capture_id = capture_id;
        let targets = self.touch_targets.remove(&tid);
        if let Some(targets) = &targets {
            self.dispatch_pointer_to_targets(PointerEventKind::Up(button), pos, targets, Some(tid));
            result.consumed = true;
        } else if let Some(path) = self.touch_paths.remove(&tid) {
            self.dispatch_pointer_to_path(PointerEventKind::Up(button), pos, &path, Some(tid));
            result.consumed = true;
        }
        self.touch_paths.remove(&tid);
        self.touch_positions.remove(&tid);
        let primary = self.touch_primary == Some(tid);
        if let Some(cid) = capture_id {
            self.end_textfield_drag(cid);
        }
        let owns_dnd_release = self.dnd_capture.as_ref().is_some_and(|capture| {
            capture.touch == Some(tid) && Some(capture.source_id) == capture_id
        });
        let drag_consumed = if owns_dnd_release {
            self.dnd_capture = None;
            dnd::handle_drag_action(&DragAction::Release {
                position: pos,
                modifiers: self.modifiers,
            })
        } else {
            false
        };
        let suppressed = self.suppressed_touch_clicks.remove(&tid);
        if primary
            && !drag_consumed
            && !suppressed
            && let Some(frame) = self.frame_cache.clone()
            && let Some(cid) = capture_id
        {
            if self.long_press_touch == Some(tid)
                && let Some((lid, t0, _, _)) = self.long_press.take()
                && lid == cid
                && t0.elapsed().as_millis() >= LONG_PRESS_MS
                && let Some(hit) = frame
                    .hit_regions
                    .iter()
                    .find(|hit| hit.id == lid && !hit.disabled)
                && let Some(callback) = &hit.on_long_click
            {
                callback();
                self.suppress_next_click = true;
                self.pending_click = None;
                result.clicked_id = Some(lid);
                result.needs_a11y_announce = true;
                result.consumed = true;
            }

            if self.double_candidate.is_none()
                && !self.suppress_next_click
                && let Some(hit) = frame
                    .hit_regions
                    .iter()
                    .find(|hit| hit.id == cid && !hit.disabled)
                && repose_ui::hit_region_contains(hit, pos)
            {
                let now = web_time::Instant::now();
                if hit.on_double_click.is_some() {
                    if let Some(callback) = hit.on_click.clone() {
                        self.pending_click = Some((cid, now, callback));
                    }
                    self.last_up = Some((cid, now, pos.x, pos.y));
                    result.consumed = true;
                    request_frame();
                } else {
                    if let Some(callback) = &hit.on_click {
                        callback();
                    }
                    self.last_up = Some((cid, now, pos.x, pos.y));
                    result.clicked_id = Some(cid);
                    result.needs_a11y_announce = true;
                    result.consumed = true;
                }
            }
            self.suppress_next_click = false;

            if let Some(double_id) = self.double_candidate.take() {
                self.pending_click = None;
                self.last_up = None;
                self.last_down = None;
                if let Some(hit) = frame
                    .hit_regions
                    .iter()
                    .find(|hit| hit.id == double_id && !hit.disabled)
                {
                    if repose_ui::hit_region_contains(hit, pos) {
                        if let Some(callback) = &hit.on_double_click {
                            callback();
                        }
                    } else if let Some(callback) = &hit.on_click {
                        callback();
                    }
                    result.clicked_id = Some(double_id);
                    result.needs_a11y_announce = true;
                    result.consumed = true;
                }
            }
        }
        if self.long_press_touch == Some(tid) {
            self.long_press = None;
            self.long_press_touch = None;
        }
        self.rebuild_pressed_ids();
        if primary {
            self.touch_primary = None;
        }
        request_frame();
        result
    }

    fn cancel_keyboard_press(&mut self) {
        let active = self.key_pressed_active.take();
        self.key_pressed_key = None;
        self.key_long_press = None;
        let Some(active) = active else {
            return;
        };
        self.pressed_ids.remove(&active);
        let hit = self
            .frame_cache
            .as_ref()
            .and_then(|frame| frame.hit_regions.iter().find(|hit| hit.id == active))
            .cloned();
        if let Some(hit) = hit
            && let Some(source) = &hit.interaction_source
        {
            let press_id = source.collect_last_press_id().unwrap_or(0);
            source.to_mutable().emit(Interaction::Cancel(press_id));
        }
    }

    fn rebuild_pressed_ids(&mut self) {
        self.pressed_ids = self
            .touch_presses
            .values()
            .flat_map(|state| state.pressed_ids.iter().copied())
            .collect();
        if let Some(active) = self.key_pressed_active {
            self.pressed_ids.insert(active);
        }
    }

    pub fn suppress_touch_click(&mut self, tid: u64) {
        self.suppressed_touch_clicks.insert(tid);
        if self.touch_primary == Some(tid) {
            self.suppress_next_click = true;
            self.long_press = None;
            self.pending_click = None;
            self.double_candidate = None;
        }
    }

    /// Cancel mouse pointer state (focus lost). Touch fingers never feed
    /// the polled mouse position, so only a mouse cancel clears it.
    /// A touch cancel drops just that finger's path (the viewport
    /// must drop the contact); a mouse cancel (`None`) keeps the old
    /// full-reset behavior for the single mouse path.
    pub fn handle_touch_cancel(&mut self, touch: Option<u64>) {
        if let Some(tid) = touch {
            let pos = self.touch_positions.get(&tid).copied().unwrap_or(Vec2 {
                x: self.mouse_pos_px.0,
                y: self.mouse_pos_px.1,
            });
            self.handle_touch_cancel_at(tid, pos);
        } else {
            self.handle_pointer_cancel();
        }
    }

    pub fn handle_touch_cancel_at(&mut self, tid: u64, pos: Vec2) {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let _dnd_guard = self.dnd_context.enter();
        let was_primary = self.touch_primary == Some(tid);
        let capture_id = self
            .touch_presses
            .remove(&tid)
            .map(|state| state.capture_id);
        let targets = self.touch_targets.remove(&tid);
        if let Some(targets) = &targets {
            self.dispatch_pointer_to_targets(PointerEventKind::Cancel, pos, targets, Some(tid));
        } else if let Some(path) = self.touch_paths.remove(&tid) {
            self.dispatch_pointer_to_path(PointerEventKind::Cancel, pos, &path, Some(tid));
        }
        self.touch_paths.remove(&tid);
        self.touch_positions.remove(&tid);
        if let Some(cid) = capture_id {
            self.end_textfield_drag(cid);
        }
        self.suppressed_touch_clicks.remove(&tid);
        if was_primary {
            self.touch_primary = None;
            self.double_candidate = None;
            self.pending_click = None;
            self.last_up = None;
            self.last_down = None;
            self.suppress_next_click = false;
            self.scroll_capture_id = None;
        }
        if self.long_press_touch == Some(tid) {
            self.long_press = None;
            self.long_press_touch = None;
        }
        self.rebuild_pressed_ids();
        if let Some(source_id) = capture_id {
            self.cancel_dnd_capture(Some(tid), source_id);
        }
        request_frame();
    }

    pub fn handle_pointer_cancel(&mut self) {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let _dnd_guard = self.dnd_context.enter();
        self.sched.pointer_pos_px = None;
        self.held_mouse.clear();
        self.sched.mouse_primary = false;
        self.sched.mouse_secondary = false;
        self.sched.mouse_middle = false;
        self.scroll_capture_id = None;
        self.last_scroll_at = None;
        if let Some(cid) = self.capture_id {
            self.end_textfield_drag(cid);
        }
        let touch_targets = std::mem::take(&mut self.touch_targets);
        let touch_paths = std::mem::take(&mut self.touch_paths);
        let mut touch_ids: Vec<u64> = touch_targets
            .keys()
            .chain(touch_paths.keys())
            .copied()
            .collect();
        touch_ids.sort_unstable();
        touch_ids.dedup();
        for tid in touch_ids {
            let pos = self.touch_positions.remove(&tid).unwrap_or(Vec2 {
                x: self.mouse_pos_px.0,
                y: self.mouse_pos_px.1,
            });
            if let Some(targets) = touch_targets.get(&tid) {
                self.dispatch_pointer_to_targets(PointerEventKind::Cancel, pos, targets, Some(tid));
            } else if let Some(path) = touch_paths.get(&tid) {
                self.dispatch_pointer_to_path(PointerEventKind::Cancel, pos, path, Some(tid));
            }
        }
        let touch_presses = std::mem::take(&mut self.touch_presses);
        for (tid, state) in &touch_presses {
            let source_id = state.capture_id;
            self.cancel_dnd_capture(Some(*tid), source_id);
            self.end_textfield_drag(source_id);
        }
        self.touch_primary = None;
        self.suppressed_touch_clicks.clear();
        self.cancel_keyboard_press();
        self.long_press = None;
        self.long_press_touch = None;
        self.last_up = None;
        self.last_down = None;
        self.double_candidate = None;
        self.suppress_next_click = false;
        if self.dnd_capture.is_some() {
            self.cancel_active_dnd();
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
        if let Some(targets) = &self.mouse_targets {
            self.dispatch_pointer_to_targets(PointerEventKind::Cancel, pos, targets, None);
        } else if let Some(path) = &self.hit_path {
            self.dispatch_pointer_to_path(PointerEventKind::Cancel, pos, path, None);
        }
        self.reset_pointer_state();
        request_frame();
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

        let new_hover = repose_ui::hit_test_enabled_frame(new_frame, pos).map(|hit| hit.id);

        self.cursor = if dnd::is_dragging() {
            Some(CursorIcon::Grabbing)
        } else {
            new_hover
                .and_then(|id| new_frame.hit_regions.iter().find(|h| h.id == id))
                .and_then(|h| h.cursor.clone())
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
        self.mouse_targets = None;
        self.touch_paths.clear();
        self.touch_targets.clear();
        self.touch_positions.clear();
        self.touch_presses.clear();
        self.dnd_capture = None;
        self.touch_primary = None;
        self.suppressed_touch_clicks.clear();
        self.pressed_ids.clear();
        self.pending_click = None;
        self.last_down = None;
        self.double_candidate = None;
        self.key_pressed_active = None;
        self.key_pressed_key = None;
        self.key_long_press = None;
        self.last_up = None;
        self.last_down = None;
        self.double_candidate = None;
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
        let captured = self
            .long_press_touch
            .and_then(|tid| self.touch_presses.get(&tid).map(|state| state.capture_id))
            .or(self.capture_id);
        if captured != Some(lid) {
            self.long_press = None;
            self.long_press_touch = None;
            return;
        }
        let (mx, my) = self.mouse_pos_px;
        let in_bounds = f
            .hit_regions
            .iter()
            .find(|h| h.id == lid)
            .is_some_and(|h| repose_ui::hit_region_contains(h, Vec2 { x: mx, y: my }));
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

    fn resolve_shortcut_action(
        &self,
        chord: &repose_core::shortcuts::KeyChord,
    ) -> Option<repose_core::shortcuts::Action> {
        self.shortcuts.resolve_action(chord)
    }

    fn handle_shortcut_action(&self, action: repose_core::shortcuts::Action) -> bool {
        self.shortcuts.handle(action)
    }

    /// Process a keyboard key event. Returns true if consumed.
    pub fn handle_key(&mut self, event: &KeyEvent) -> bool {
        let _event_scope = crate::lifecycle::enter_dispatchers(self.event_dispatchers());
        let shortcut_state = self.shortcuts.clone();
        repose_core::shortcuts::with_runtime_state(&shortcut_state, || self.handle_key_inner(event))
    }

    fn handle_key_inner(&mut self, event: &KeyEvent) -> bool {
        let _dnd_guard = self.dnd_context.enter();
        if event.event_type == KeyEventType::Down {
            let _ = repose_core::request_input_mode(repose_core::InputMode::Keyboard);
        }

        let Some(frame) = self.frame_cache.clone() else {
            return false;
        };
        let f = &frame;

        if event.event_type == KeyEventType::Down && !event.is_repeat && event.key == Key::Escape {
            self.cancel_keyboard_press();
            if self.cancel_active_dnd() {
                request_frame();
                return true;
            }
        }

        if event.event_type == KeyEventType::Down && !event.is_repeat {
            let is_back = event.key == Key::Escape
                || self
                    .resolve_shortcut_action(&repose_core::shortcuts::KeyChord::new(
                        event.key.clone(),
                        self.modifiers,
                    ))
                    .is_some_and(|action| matches!(action, repose_core::shortcuts::Action::Back));
            if is_back && self.overlay.handle_back() {
                request_frame();
                return true;
            }
        }

        if self.dispatch_key_event(f, event) {
            request_frame();
            return true;
        }

        // Action dispatch (shortcuts like Ctrl+C, Tab, etc.)
        if event.event_type == KeyEventType::Down
            && !event.is_repeat
            && let Some(action) = self.resolve_shortcut_action(
                &repose_core::shortcuts::KeyChord::new(event.key.clone(), self.modifiers),
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
                        if hit.disabled {
                            return false;
                        }
                        if hit.on_click.is_none()
                            && hit.on_long_click.is_none()
                            && hit.on_double_click.is_none()
                        {
                            return false; // don't steal keys from non-clickable focusables
                        }
                        self.cancel_keyboard_press();
                        self.pressed_ids.insert(fid);
                        self.key_pressed_active = Some(fid);
                        self.key_pressed_key = Some(event.key.clone());
                        self.key_long_press = if hit.on_long_click.is_some() {
                            Some((fid, web_time::Instant::now(), false))
                        } else {
                            None
                        };

                        if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid)
                            && let Some(src) = &hit.interaction_source
                        {
                            let local = Vec2 {
                                x: hit.rect.w * 0.5,
                                y: hit.rect.h * 0.5,
                            };
                            src.to_mutable().emit(Interaction::new_press(local));
                        }

                        request_frame();
                        return true;
                    }
                } else if event.event_type == KeyEventType::Up
                    && let Some(active_id) = self.key_pressed_active
                    && self
                        .key_pressed_key
                        .as_ref()
                        .is_some_and(|key| key == &event.key)
                {
                    self.pressed_ids.remove(&active_id);
                    self.key_pressed_active = None;
                    self.key_pressed_key = None;

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
                        if !long_fired && let Some(cb) = &hit.on_click {
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
                let key = tf_key_of(f, fid);
                if let Some(state_rc) = self.textfield_states.get(&key) {
                    let text = state_rc.borrow().text.clone();
                    let action = match hit.ime_action {
                        repose_core::text::ImeAction::Unspecified => {
                            repose_core::text::ImeAction::Done
                        }
                        action => action,
                    };
                    repose_ui::textfield::dispatch_textfield_keyboard_action(hit, action, &|| {
                        if let Some(on_submit) = &hit.on_text_submit {
                            on_submit(text.clone());
                        }
                    });
                    request_frame();
                    return true;
                }
            } else {
                // Multiline plain Enter: insert newline
                let key = tf_key_of(f, fid);
                if !is_tf_editable(f, fid) {
                    return true;
                }
                if let Some(state_rc) = self.textfield_states.get(&key) {
                    let mut edited = state_rc.borrow().clone();
                    repose_ui::textfield::insert_text_with_input_transformation(
                        hit,
                        &mut edited,
                        "\n",
                        false,
                    );
                    tf_ensure_caret_visible_for_hit(hit, &mut edited);
                    let new_text = edited.text.clone();
                    *state_rc.borrow_mut() = edited;
                    notify_text_change(f, fid, new_text);
                    request_frame();
                    return true;
                }
            }
        }

        // TextField navigation / edit keys
        if event.event_type == KeyEventType::Down
            && let Some(fid) = self.sched.focused
            && self.handle_text_navigation_key(f, fid, &event.key)
        {
            return true;
        }

        if event.event_type == KeyEventType::Down {
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
                    let new_text = if let Some(hit) = f.hit_regions.iter().find(|h| h.id == fid) {
                        let mut edited = state_rc.borrow().clone();
                        repose_ui::textfield::insert_text_with_input_transformation(
                            hit,
                            &mut edited,
                            &c.to_string(),
                            false,
                        );
                        tf_ensure_caret_visible_for_hit(hit, &mut edited);
                        let text = edited.text.clone();
                        *state_rc.borrow_mut() = edited;
                        text
                    } else {
                        return false;
                    };
                    notify_text_change(f, fid, new_text);
                    request_frame();
                    return true;
                }
            }
        }

        false
    }

    fn display_text_for_state(
        state: &TextFieldState,
    ) -> (String, usize, Option<Box<dyn repose_core::OffsetMapping>>) {
        let caret = state.caret_index();
        let Some(transformation) = state.visual_transformation.clone() else {
            return (state.text.clone(), caret, None);
        };
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let transformed = transformation.filter(&annotated);
        let display_caret = transformed.offset_mapping.original_to_transformed(caret);
        (
            transformed.text.text,
            display_caret,
            Some(transformed.offset_mapping),
        )
    }

    fn handle_text_navigation_key(&mut self, frame: &Frame, id: u64, key: &Key) -> bool {
        let Some(hit) = frame.hit_regions.iter().find(|hit| hit.id == id) else {
            return false;
        };
        let state_key = tf_key_of(frame, id);
        let Some(state) = self.textfield_states.get(&state_key) else {
            return false;
        };
        let editable = is_tf_editable(frame, id);
        let metrics = repose_ui::textfield::textfield_metrics(hit);
        let shift = self.modifiers.shift;
        let mut edited = state.borrow().clone();
        let (display, display_caret, mapping) = Self::display_text_for_state(&edited);
        let mut changed = false;
        let consumed = match key {
            Key::Backspace => {
                changed = editable
                    && repose_ui::textfield::delete_backward_with_input_transformation(
                        hit,
                        &mut edited,
                    );
                true
            }
            Key::Delete => {
                changed = editable
                    && repose_ui::textfield::delete_forward_with_input_transformation(
                        hit,
                        &mut edited,
                    );
                true
            }
            Key::ArrowLeft => {
                edited.move_cursor(-1, shift);
                edited.preferred_x_px = None;
                true
            }
            Key::ArrowRight => {
                edited.move_cursor(1, shift);
                edited.preferred_x_px = None;
                true
            }
            Key::ArrowUp if hit.tf_multiline => {
                let (next_display, preferred) =
                    repose_ui::textfield::move_caret_vertical_with_metrics(
                        &display,
                        edited.inner_width.max(1.0),
                        display_caret,
                        -1,
                        edited.preferred_x_px,
                        &metrics,
                    );
                let next = mapping.as_ref().map_or(next_display, |mapping| {
                    mapping.transformed_to_original(next_display)
                });
                set_caret(&mut edited, next, shift);
                edited.preferred_x_px = Some(preferred);
                true
            }
            Key::ArrowDown if hit.tf_multiline => {
                let (next_display, preferred) =
                    repose_ui::textfield::move_caret_vertical_with_metrics(
                        &display,
                        edited.inner_width.max(1.0),
                        display_caret,
                        1,
                        edited.preferred_x_px,
                        &metrics,
                    );
                let next = mapping.as_ref().map_or(next_display, |mapping| {
                    mapping.transformed_to_original(next_display)
                });
                set_caret(&mut edited, next, shift);
                edited.preferred_x_px = Some(preferred);
                true
            }
            Key::Home => {
                let next_display = repose_ui::textfield::line_home_end_with_metrics(
                    &display,
                    edited.inner_width.max(1.0),
                    display_caret,
                    false,
                    &metrics,
                );
                let next = mapping.as_ref().map_or(next_display, |mapping| {
                    mapping.transformed_to_original(next_display)
                });
                set_caret(&mut edited, next, shift);
                edited.preferred_x_px = None;
                true
            }
            Key::End => {
                let next_display = repose_ui::textfield::line_home_end_with_metrics(
                    &display,
                    edited.inner_width.max(1.0),
                    display_caret,
                    true,
                    &metrics,
                );
                let next = mapping.as_ref().map_or(next_display, |mapping| {
                    mapping.transformed_to_original(next_display)
                });
                set_caret(&mut edited, next, shift);
                edited.preferred_x_px = None;
                true
            }
            _ => false,
        };
        if !consumed {
            return false;
        }
        tf_ensure_caret_visible_for_hit(hit, &mut edited);
        let text = changed.then(|| edited.text.clone());
        *state.borrow_mut() = edited;
        if let Some(text) = text {
            notify_text_change(frame, id, text);
        }
        request_frame();
        true
    }

    /// Dispatch a key event through the compose hierarchy, mirroring
    /// Compose `FocusOwner.dispatchKeyEvent`: resolve the focused key-input
    /// node (falling back to the root when unfocused), then run the preview
    /// tunnel root -> focused and the bubble focused -> root.
    fn dispatch_key_event(&self, f: &Frame, event: &KeyEvent) -> bool {
        let chain = key_ancestor_chain(f, self.sched.focused);
        let hit_by_id: HashMap<u64, &HitRegion> = f.hit_regions.iter().map(|h| (h.id, h)).collect();

        for &id in &chain {
            let Some(hit) = hit_by_id.get(&id) else {
                continue;
            };
            if hit.disabled {
                continue;
            }
            if let Some(cb) = &hit.on_preview_key_event
                && cb(event.clone())
            {
                return true;
            }
        }

        for &id in chain.iter().rev() {
            let Some(hit) = hit_by_id.get(&id) else {
                continue;
            };
            if hit.disabled {
                continue;
            }
            if let Some(cb) = &hit.on_key_event
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
        if self.handle_shortcut_action(action.clone()) {
            request_frame();
            return true;
        }

        // 4) Focus navigation (Tab / arrows)
        if let Some(f) = self.frame_cache.clone()
            && let Some(new_id) = repose_core::focus::handle_action(&action, &mut self.sched, &f)
        {
            // End any in-flight keyboard press (e.g. Space held on the old focus).
            self.cancel_keyboard_press();
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
        let hit = f.hit_regions.iter().find(|hit| hit.id == fid);

        match action {
            Action::Undo => {
                if !is_tf_editable(&f, fid) {
                    return true;
                }
                let Some(hit) = hit else {
                    return false;
                };
                let mut edited = state_rc.borrow().clone();
                if !repose_ui::textfield::undo_with_input_transformation(hit, &mut edited) {
                    return false;
                }
                tf_ensure_caret_visible_for_hit(hit, &mut edited);
                let new_text = edited.text.clone();
                *state_rc.borrow_mut() = edited;
                notify_text_change(&f, fid, new_text);
                request_frame();
                true
            }
            Action::Redo => {
                if !is_tf_editable(&f, fid) {
                    return true;
                }
                let Some(hit) = hit else {
                    return false;
                };
                let mut edited = state_rc.borrow().clone();
                if !repose_ui::textfield::redo_with_input_transformation(hit, &mut edited) {
                    return false;
                }
                tf_ensure_caret_visible_for_hit(hit, &mut edited);
                let new_text = edited.text.clone();
                *state_rc.borrow_mut() = edited;
                notify_text_change(&f, fid, new_text);
                request_frame();
                true
            }
            Action::SelectAll => {
                let mut st = state_rc.borrow_mut();
                let len = st.text.len();
                st.selection = 0..len;
                if let Some(hit) = hit {
                    tf_ensure_caret_visible_for_hit(hit, &mut st);
                }
                request_frame();
                true
            }
            Action::Copy => {
                if hit.is_some_and(tf_is_sensitive) {
                    return true;
                }
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
                if hit.is_some_and(tf_is_sensitive) {
                    return true;
                }
                if !is_tf_editable(&f, fid) {
                    return true;
                }
                let Some(hit) = hit else {
                    return false;
                };
                let mut edited = state_rc.borrow().clone();
                let Some(slice) =
                    repose_ui::textfield::cut_with_input_transformation(hit, &mut edited)
                else {
                    return false;
                };
                tf_ensure_caret_visible_for_hit(hit, &mut edited);
                let new_text = edited.text.clone();
                *state_rc.borrow_mut() = edited;
                repose_core::clipboard::copy_to_clipboard(&slice);
                notify_text_change(&f, fid, new_text);
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

    fn cancel_ime_compositions(&mut self) {
        let mut changed = None;
        for (key, state) in &self.textfield_states {
            let mut edited = state.borrow().clone();
            if edited.composition.is_none() {
                continue;
            }
            edited.cancel_composition();
            *state.borrow_mut() = edited;
            changed = Some((*key, self.textfield_states[key].borrow().text.clone()));
        }
        self.ime_preedit = false;
        if let (Some((key, text)), Some(frame)) = (changed, &self.frame_cache)
            && let Some(id) = frame
                .hit_regions
                .iter()
                .find(|hit| hit.tf_state_key.unwrap_or(hit.id) == key)
                .map(|hit| hit.id)
        {
            notify_text_change(frame, id, text);
        }
        request_frame();
    }

    /// Process an IME event.
    pub fn handle_ime(&mut self, event: &ImeEvent) {
        if matches!(event, ImeEvent::Cancel) {
            self.cancel_ime_compositions();
            return;
        }
        let Some(fid) = self.sched.focused else {
            return;
        };
        let Some(frame) = self.frame_cache.clone() else {
            return;
        };
        if !matches!(event, ImeEvent::Cancel) && !is_tf_editable(&frame, fid) {
            return;
        }
        let key = tf_key_of(&frame, fid);
        let Some(state) = self.textfield_states.get(&key).cloned() else {
            return;
        };
        let hit = frame.hit_regions.iter().find(|hit| hit.id == fid);
        let mut edited = state.borrow().clone();
        let changed = match event {
            ImeEvent::Start => {
                self.ime_preedit = false;
                None
            }
            ImeEvent::Update { text, cursor } => {
                edited.set_composition(text.clone(), *cursor);
                if let Some(hit) = hit {
                    repose_ui::textfield::apply_textfield_input_transformation(hit, &mut edited);
                    tf_ensure_caret_visible_for_hit(hit, &mut edited);
                }
                self.ime_preedit = !text.is_empty();
                Some(edited.text.clone())
            }
            ImeEvent::Commit(text) => {
                if let Some(hit) = hit {
                    repose_ui::textfield::commit_composition_with_input_transformation(
                        hit,
                        &mut edited,
                        text.clone(),
                    );
                    tf_ensure_caret_visible_for_hit(hit, &mut edited);
                } else {
                    edited.commit_composition(text.clone());
                }
                self.ime_preedit = false;
                Some(edited.text.clone())
            }
            ImeEvent::Cancel => unreachable!(),
        };
        *state.borrow_mut() = edited;
        if let Some(text) = changed {
            notify_text_change(&frame, fid, text);
        }
        request_frame();
    }

    /// Finish the active composition in every field without deleting text.
    /// Used for focus changes and IME disconnects where the confirmed text
    /// must be kept; distinct from cancelling, which discards the preedit.
    pub fn finish_compositions(&mut self) {
        for state_rc in self.textfield_states.values() {
            let mut st = state_rc.borrow_mut();
            if st.composition.is_some() {
                st.finish_composition();
            }
        }
        self.ime_preedit = false;
    }

    /// Handle focus lost (window unfocused, etc.). Physical-key and
    /// mouse-button levels clear too: winit does not deliver key-ups
    /// for keys still down across an alt-tab, so without this the
    /// polled `held_keys` set sticks until the next press of that key.
    pub fn handle_focus_lost(&mut self) {
        self.pointer_inside = false;
        self.sched.window_focused = false;
        self.handle_pointer_cancel();
        self.held_keys.clear();
        self.held_mouse.clear();
        self.sched.held_keys.clear();
        self.sched.mouse_primary = false;
        self.sched.mouse_secondary = false;
        self.sched.mouse_middle = false;
        self.sched.touch_points.clear();
        self.key_pressed_active = None;
        self.key_pressed_key = None;
        self.key_long_press = None;
        self.pressed_ids.clear();
        self.pending_click = None;
        self.last_down = None;
        self.last_up = None;
        self.double_candidate = None;
        self.scroll_capture_id = None;
        self.last_scroll_at = None;
        self.finish_compositions();
    }

    /// Report one physical key transition (`true` = down). Platform
    /// runners call this from the raw `KeyboardInput` event alongside
    /// the focus-routed `handle_key_with_text`, so `held` reflects
    /// hardware even when no widget has focus.
    pub fn set_physical_key(&mut self, name: &str, down: bool) {
        if down {
            self.held_keys.insert(name.to_string());
        } else {
            self.held_keys.remove(name);
        }
    }

    /// Snapshot of currently held physical keys (debug names).
    pub fn held_physical_keys(&self) -> Vec<String> {
        let mut out: Vec<String> = self.held_keys.iter().cloned().collect();
        out.sort();
        out
    }

    /// True while the named physical key is down (`KeyCode::KeyW`, ...).
    pub fn physical_key_held(&self, name: &str) -> bool {
        self.held_keys.contains(name)
    }

    /// True while the mouse button is down.
    pub fn mouse_button_held(&self, button: PointerButton) -> bool {
        self.held_mouse.contains(&button)
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

    /// True if the given id is an enabled, editable text field.
    pub fn is_editable_textfield(&self, id: u64) -> bool {
        self.frame_cache
            .as_ref()
            .map(|f| is_tf_editable(f, id))
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

    pub fn focused_ime_action(&self) -> repose_core::text::ImeAction {
        self.frame_cache
            .as_ref()
            .and_then(|frame| {
                self.sched
                    .focused
                    .and_then(|id| frame.hit_regions.iter().find(|hit| hit.id == id))
            })
            .map(|hit| hit.ime_action)
            .unwrap_or_default()
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
        let new_text = if let Some(hit) = f.hit_regions.iter().find(|hit| hit.id == fid) {
            let mut edited = state_rc.borrow().clone();
            repose_ui::textfield::insert_text_with_input_transformation(
                hit,
                &mut edited,
                &filtered,
                false,
            );
            tf_ensure_caret_visible_for_hit(hit, &mut edited);
            let text = edited.text.clone();
            *state_rc.borrow_mut() = edited;
            text
        } else {
            return false;
        };
        notify_text_change(&f, fid, new_text);
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
        let Some(state_rc) = self.textfield_states.get(&key) else {
            return;
        };
        let Some(hit) = f.hit_regions.iter().find(|hit| hit.id == fid) else {
            return;
        };
        let filtered: String = text
            .chars()
            .filter(|c| (*c == '\n' && hit.tf_multiline) || (!c.is_control() && *c != '\r'))
            .collect();
        if filtered.is_empty() {
            return;
        }
        let new_text = {
            let mut edited = state_rc.borrow().clone();
            repose_ui::textfield::insert_text_with_input_transformation(
                hit,
                &mut edited,
                &filtered,
                true,
            );
            tf_ensure_caret_visible_for_hit(hit, &mut edited);
            let text = edited.text.clone();
            *state_rc.borrow_mut() = edited;
            text
        };
        notify_text_change(f, fid, new_text);
        request_frame();
    }

    /// Process a gamepad event: mirror connection/button/axis state into
    /// [`ReposeRuntime::gamepads`]. Sticks, shoulders and face extras stay raw
    /// gameplay input for the game to read from [`ReposeRuntime::gamepads`].
    ///
    /// Returns `true` when the event drove UI navigation.
    pub fn handle_gamepad(&mut self, event: &GamepadEvent) -> bool {
        match event {
            GamepadEvent::Connected { id, name } => {
                self.gamepads.insert(
                    id.0,
                    GamepadPad {
                        name: name.clone(),
                        ..GamepadPad::default()
                    },
                );
                request_frame();
                return false;
            }
            GamepadEvent::Disconnected { id } => {
                self.gamepads.remove(&id.0);
                request_frame();
                return false;
            }
            GamepadEvent::Button {
                id,
                button,
                pressed,
            } => {
                if let Some(pad) = self.gamepads.get_mut(&id.0) {
                    if *pressed {
                        pad.pressed.insert(*button);
                    } else {
                        pad.pressed.remove(button);
                    }
                }
                if *pressed {
                    let _ = repose_core::request_input_mode(repose_core::InputMode::Keyboard);
                }
                let key = match button {
                    GamepadButton::South => Key::Space,
                    GamepadButton::East => Key::Escape,
                    GamepadButton::Start => Key::Enter,
                    GamepadButton::DPadUp => Key::ArrowUp,
                    GamepadButton::DPadDown => Key::ArrowDown,
                    GamepadButton::DPadLeft => Key::ArrowLeft,
                    GamepadButton::DPadRight => Key::ArrowRight,
                    _ => return false,
                };
                let synthetic = KeyEvent {
                    key,
                    modifiers: self.modifiers,
                    is_repeat: false,
                    event_type: if *pressed {
                        KeyEventType::Down
                    } else {
                        KeyEventType::Up
                    },
                    utf16_code_point: 0,
                    physical: None,
                };
                return self.handle_key(&synthetic);
            }
            GamepadEvent::Axis { id, axis, value } => {
                if let Some(pad) = self.gamepads.get_mut(&id.0) {
                    pad.axes.insert(*axis, *value);
                }
                return false;
            }
        }
    }

    /// Queue a dual-motor rumble for `id` (SDL-style: low = strong motor,
    /// high = weak motor, 0.0..=1.0, `duration_ms`). The platform runner
    /// drains the queue each frame into `GamepadBackend::set_rumble`.
    /// A `duration_ms` of 0 stops. No-op when the pad is unknown.
    pub fn request_rumble(
        &mut self,
        id: repose_core::input::GamepadId,
        low_freq: f32,
        high_freq: f32,
        duration_ms: u32,
    ) {
        self.pending_rumble.push((
            id.0,
            low_freq.clamp(0.0, 1.0),
            high_freq.clamp(0.0, 1.0),
            duration_ms,
        ));
    }

    /// Queue a rumble stop for `id`.
    pub fn stop_rumble(&mut self, id: repose_core::input::GamepadId) {
        self.pending_rumble.push((id.0, 0.0, 0.0, 0));
    }

    /// Drain queued rumble requests (platform runners call this after `poll`).
    pub fn take_rumble_requests(&mut self) -> Vec<(u32, f32, f32, u32)> {
        std::mem::take(&mut self.pending_rumble)
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
    /// Internal - platform should use `next_wakeup_deadline()` instead.
    fn next_caret_blink_deadline(&self) -> Option<web_time::Instant> {
        let fid = self.sched.focused?;
        let frame = self.frame_cache.as_ref()?;
        let hit = frame.hit_regions.iter().find(|h| h.id == fid)?;
        let key = hit.tf_state_key?;
        self.textfield_states
            .get(&key)?
            .borrow()
            .next_blink_deadline()
    }

    /// Centralized wakeup helper for platform runners (caret, snackbar, timers, etc.).
    /// Debounced entries live on the shared timer queue, so `timer` covers them.
    pub fn next_wakeup_deadline(&self) -> Option<web_time::Instant> {
        [
            self.next_caret_blink_deadline(),
            repose_ui::overlay::SnackbarController::next_deadline(),
            repose_core::timer::next_deadline(),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Whether a scheduled wakeup is due at `now` (deadline <= now).
    pub fn is_wakeup_due(&self, now: web_time::Instant) -> bool {
        self.next_wakeup_deadline().is_some_and(|d| d <= now)
    }

    /// Whether a caret blink edge is due at `now` (deadline <= now).
    /// Deprecated: use `is_wakeup_due`.
    pub fn is_caret_blink_due(&self, now: web_time::Instant) -> bool {
        self.is_wakeup_due(now)
    }

    /// Desktop-style deadline with idle keep-alive fallback.
    /// `idle_cap` is the maximum time the host should sleep without a
    /// scheduled wakeup (e.g. 1s on desktop to handle tray Deeplinks).
    pub fn next_frame_deadline(
        &self,
        now: web_time::Instant,
        idle_cap: web_time::Duration,
    ) -> web_time::Instant {
        self.next_wakeup_deadline().unwrap_or(now + idle_cap)
    }

    /// Tick host-facing overlays (snackbar timeouts) and timers (including
    /// debounced entries, which live on the shared timer queue).
    /// Call once per redraw.
    pub fn tick_overlays(&self) {
        repose_ui::overlay::SnackbarController::tick_all();
        repose_core::timer::poll();
    }

    /// Get the cursor suggestion (set during pointer-move handling).
    pub fn cursor_suggestion(&self) -> Option<CursorIcon> {
        self.cursor.clone()
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
    for requested_id in repose_core::runtime::drain_focus_requests() {
        if requested_id == repose_core::runtime::CLEAR_FOCUS_MARKER {
            sched.focused = None;
            continue;
        }
        sched.focused = Some(requested_id);
    }

    set_density_default(Density { scale });

    let current_focused = sched.focused;

    let frame = sched.repose(
        move |s: &mut Scheduler| with_density(Density { scale }, || (root_fn)(s)),
        {
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

    for requested_id in repose_core::runtime::drain_focus_requests() {
        if requested_id == repose_core::runtime::CLEAR_FOCUS_MARKER {
            sched.focused = None;
        } else if frame.focus_chain.contains(&requested_id) {
            sched.focused = Some(requested_id);
        }
    }

    if let Some(fid) = sched.focused
        && !frame.focus_chain.contains(&fid)
    {
        sched.focused = None;
    }

    frame
}

/// Compose `FocusOwner.dispatchKeyEvent` chain: focused node up through its
/// hit-region ancestors, root first.
/// Falls back to the root alone when unfocused, unknown, or orphaned.
fn key_ancestor_chain(f: &Frame, focused: Option<u64>) -> Vec<u64> {
    let parent_of: HashMap<u64, Option<u64>> =
        f.hit_regions.iter().map(|h| (h.id, h.parent)).collect();
    let mut leaf = focused.filter(|id| parent_of.contains_key(id));
    if leaf.is_none() {
        let ids: HashSet<u64> = parent_of.keys().copied().collect();
        leaf = f
            .hit_regions
            .iter()
            .filter(|h| !h.disabled)
            .find(|h| h.parent.is_none_or(|p| !ids.contains(&p)))
            .map(|h| h.id);
    }
    let Some(mut cur) = leaf else {
        return Vec::new();
    };
    let mut chain = vec![cur];
    while let Some(parent) = parent_of.get(&cur).copied().flatten() {
        if chain.contains(&parent) {
            break;
        }
        chain.push(parent);
        cur = parent;
    }
    chain.reverse();
    chain
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
    leave_map: &HashMap<u64, repose_ui::HitRegionSnapshot>,
    hover_id: &mut Option<u64>,
    hover_ancestors: &mut std::collections::HashSet<u64>,
    new_hover: Option<u64>,
    pos: Vec2,
    modifiers: Modifiers,
) {
    let old_hover = *hover_id;
    let mut old_chain = hover_ancestors.clone();
    if let Some(id) = old_hover {
        old_chain.insert(id);
    }
    let new_chain = hover_chain_for(frame, new_hover);
    if old_chain == new_chain {
        return;
    }
    for leave_id in old_chain.difference(&new_chain) {
        let current = frame.and_then(|f| f.hit_regions.iter().find(|h| h.id == *leave_id));
        if let Some(hit) = current
            && let Some(cb) = &hit.on_pointer_leave
        {
            let mut pe = PointerEvent::new(
                PointerId(0),
                PointerKind::Mouse,
                PointerEventKind::Leave,
                pos,
                1.0,
                modifiers,
            );
            let (origin, local) = repose_ui::hit_region_pointer_coordinates(hit, pos);
            pe.origin = origin;
            pe.position = local;
            cb(pe);
            continue;
        }
        if let Some(snapshot) = leave_map.get(leave_id)
            && let Some(callback) = &snapshot.hit().on_pointer_leave
        {
            let mut pe = PointerEvent::new(
                PointerId(0),
                PointerKind::Mouse,
                PointerEventKind::Leave,
                pos,
                1.0,
                modifiers,
            );
            let (origin, local) = snapshot.pointer_coordinates(pos);
            pe.origin = origin;
            pe.position = local;
            callback(pe);
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
            let (origin, local) = repose_ui::hit_region_pointer_coordinates(h, pos);
            pe.origin = origin;
            pe.position = local;
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

pub fn is_textfield_in_frame(f: &Frame, id: u64) -> bool {
    f.semantics_nodes
        .iter()
        .any(|n| n.id == id && n.role == repose_core::semantics::Role::TextField)
}

pub fn is_textfield_in_frame_cache(frame_cache: &Option<Frame>, id: u64) -> bool {
    if let Some(f) = frame_cache {
        is_textfield_in_frame(f, id)
    } else {
        false
    }
}

pub fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
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

fn tf_is_sensitive(hit: &HitRegion) -> bool {
    hit.tf_sensitive
        || matches!(
            hit.keyboard_type,
            repose_core::KeyboardType::Password
                | repose_core::KeyboardType::NumberPassword
                | repose_core::KeyboardType::DecimalPassword
                | repose_core::KeyboardType::NumberPasswordSigned
                | repose_core::KeyboardType::DecimalPasswordSigned
        )
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

fn set_caret(state: &mut TextFieldState, position: usize, extend: bool) {
    if extend {
        state.selection.end = position;
    } else {
        state.selection = position..position;
    }
}

fn notify_text_change(f: &Frame, id: u64, text: String) {
    if let Some(h) = f.hit_regions.iter().find(|h| h.id == id)
        && let Some(cb) = &h.on_text_change
    {
        cb(text);
    }
}

fn tf_ensure_caret_visible_for_hit(hit: &HitRegion, state: &mut TextFieldState) {
    let metrics = repose_ui::textfield::textfield_metrics(hit);
    tf_ensure_caret_visible_with_metrics(state, hit.tf_multiline, &metrics);
}

fn tf_ensure_caret_visible_with_metrics(
    state: &mut TextFieldState,
    is_multiline: bool,
    metrics: &repose_ui::textfield::TextFieldMetrics,
) {
    repose_ui::textfield::ensure_caret_visible_with_metrics(state, is_multiline, metrics);
}

fn index_for_x_bytes_vt(
    state: &TextFieldState,
    metrics: &repose_ui::textfield::TextFieldMetrics,
    x_px: f32,
) -> usize {
    if let Some(transformation) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let transformed = transformation.filter(&annotated);
        let display = repose_ui::textfield::index_for_x_bytes_with_config(
            transformed.text.as_str(),
            metrics.font_px,
            x_px,
            metrics.measure_config(),
        );
        transformed.offset_mapping.transformed_to_original(display)
    } else {
        repose_ui::textfield::index_for_x_bytes_with_config(
            &state.text,
            metrics.font_px,
            x_px,
            metrics.measure_config(),
        )
    }
}

fn index_for_xy_bytes_vt(
    state: &TextFieldState,
    metrics: &repose_ui::textfield::TextFieldMetrics,
    wrap_width: f32,
    x_px: f32,
    y_px: f32,
) -> usize {
    if let Some(transformation) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let transformed = transformation.filter(&annotated);
        let display = repose_ui::textfield::index_for_xy_bytes_with_metrics(
            transformed.text.as_str(),
            wrap_width,
            x_px,
            y_px,
            metrics,
        );
        transformed.offset_mapping.transformed_to_original(display)
    } else {
        repose_ui::textfield::index_for_xy_bytes_with_metrics(
            &state.text,
            wrap_width,
            x_px,
            y_px,
            metrics,
        )
    }
}

/// Dispatch scroll to scroll consumers. Returns (consumed, optional capture id).
fn dispatch_scroll(
    frame: &Frame,
    pos: Vec2,
    delta: Vec2,
    scroll_capture: Option<u64>,
) -> (bool, Option<u64>) {
    let mut remaining = delta;
    let mut first_consumer: Option<u64> = None;
    if let Some(cid) = scroll_capture
        && let Some(cb) = frame
            .hit_regions
            .iter()
            .find(|h| h.id == cid)
            .and_then(|h| h.on_scroll.as_ref())
    {
        let leftover = cb(delta);
        if (delta.x - leftover.x).abs() > 0.001 || (delta.y - leftover.y).abs() > 0.001 {
            first_consumer = Some(cid);
        }
        remaining = leftover;
        if remaining.x.abs() <= 0.001 && remaining.y.abs() <= 0.001 {
            return (true, Some(cid));
        }
    }

    let mut consumed_any = first_consumer.is_some();
    for id in repose_ui::hit_test_frame_regions(frame, pos) {
        if Some(id) == scroll_capture {
            continue;
        }
        let Some(hit) = frame.hit_regions.iter().find(|hit| hit.id == id) else {
            continue;
        };
        if remaining.x.abs() <= 0.001 && remaining.y.abs() <= 0.001 {
            break;
        }
        if let Some(cb) = &hit.on_scroll {
            let before = remaining;
            let leftover = cb(before);
            if (before.x - leftover.x).abs() > 0.001 || (before.y - leftover.y).abs() > 0.001 {
                consumed_any = true;
                if first_consumer.is_none() {
                    first_consumer = Some(hit.id);
                }
            }
            remaining = leftover;
        }
    }
    if consumed_any {
        (true, first_consumer)
    } else {
        (false, None)
    }
}

#[cfg(test)]
mod ime_tests {
    use super::*;
    use repose_core::input::ImeEvent;

    const TF_ID: u64 = 100;

    fn editable_frame(id: u64) -> Frame {
        let rect = repose_core::Rect {
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 30.0,
        };
        Frame {
            scene: Default::default(),
            hit_regions: vec![HitRegion {
                id,
                rect,
                focusable: true,
                tf_state_key: Some(id),
                tf_multiline: false,
                tf_enabled: true,
                tf_read_only: false,
                ..Default::default()
            }],
            semantics_nodes: vec![repose_core::runtime::SemNode {
                id,
                role: repose_core::semantics::Role::TextField,
                label: Some("Field".into()),
                rect,
                focused: true,
                ..Default::default()
            }],
            focus_chain: vec![id],
        }
    }

    fn focused_rt() -> ReposeRuntime {
        let mut rt = ReposeRuntime::new();
        rt.sched.focused = Some(TF_ID);
        rt.cache_frame(editable_frame(TF_ID));
        let _ = rt.ensure_textfield_state(TF_ID);
        rt
    }

    #[test]
    fn preedit_cursor_uses_byte_offsets_for_non_ascii() {
        let mut rt = focused_rt();
        rt.handle_ime(&ImeEvent::Update {
            text: "あb".to_string(),
            cursor: Some((3, 3)),
        });
        let st = rt.textfield_states[&TF_ID].borrow();
        assert_eq!(st.text, "あb");
        assert_eq!(st.selection, 3..3);
    }

    #[test]
    fn cancel_discards_preedit() {
        let mut rt = focused_rt();
        rt.handle_ime(&ImeEvent::Update {
            text: "あ".to_string(),
            cursor: None,
        });
        assert!(rt.ime_preedit);
        rt.handle_ime(&ImeEvent::Cancel);
        let st = rt.textfield_states[&TF_ID].borrow();
        assert_eq!(st.text, "");
        assert!(st.composition.is_none());
        assert!(!rt.ime_preedit);
    }

    #[test]
    fn defocus_finish_keeps_text() {
        let mut rt = focused_rt();
        rt.handle_ime(&ImeEvent::Update {
            text: "x".to_string(),
            cursor: None,
        });
        rt.sched.focused = None;
        rt.handle_focus_lost();
        assert_eq!(rt.textfield_states[&TF_ID].borrow().text, "x");
        assert!(!rt.ime_preedit);
    }
}

#[cfg(test)]
mod gamepad_tests {
    use super::*;
    use repose_core::input::{GamepadAxis, GamepadButton, GamepadEvent, GamepadId};

    #[test]
    fn gamepad_state_mirrors_connection_and_inputs() {
        let mut rt = ReposeRuntime::new();
        rt.handle_gamepad(&GamepadEvent::Connected {
            id: GamepadId(0),
            name: "Pad".to_string(),
        });
        assert_eq!(rt.gamepads.len(), 1);

        rt.handle_gamepad(&GamepadEvent::Button {
            id: GamepadId(0),
            button: GamepadButton::West,
            pressed: true,
        });
        assert!(rt.gamepads[&0].button(GamepadButton::West));

        rt.handle_gamepad(&GamepadEvent::Axis {
            id: GamepadId(0),
            axis: GamepadAxis::LeftStickX,
            value: 0.5,
        });
        assert_eq!(rt.gamepads[&0].axis(GamepadAxis::LeftStickX), 0.5);

        rt.handle_gamepad(&GamepadEvent::Button {
            id: GamepadId(0),
            button: GamepadButton::West,
            pressed: false,
        });
        assert!(!rt.gamepads[&0].button(GamepadButton::West));

        rt.handle_gamepad(&GamepadEvent::Disconnected { id: GamepadId(0) });
        assert!(rt.gamepads.is_empty());
    }

    #[test]
    fn rumble_requests_queue_and_drain() {
        let mut rt = ReposeRuntime::new();
        assert!(rt.take_rumble_requests().is_empty());
        rt.request_rumble(GamepadId(1), 2.0, -1.0, 150);
        rt.stop_rumble(GamepadId(1));
        let reqs = rt.take_rumble_requests();
        assert_eq!(reqs.len(), 2);
        assert_eq!(reqs[0], (1, 1.0, 0.0, 150));
        assert_eq!(reqs[1], (1, 0.0, 0.0, 0));
        assert!(rt.take_rumble_requests().is_empty());
    }
}
