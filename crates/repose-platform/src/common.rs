#[cfg(any(target_arch = "wasm32", target_os = "android"))]
use crate::*;

use std::collections::BTreeMap;

use repose_app::ReposeRuntime;
use repose_core::Vec2;
use repose_core::input::PointerButton;

#[derive(Clone, Copy, Debug)]
struct DynGestureState {
    avg_distance: f32,
    avg_pos: Vec2,
    heading: f32,
}

#[derive(Clone, Debug)]
struct GestureState {
    start_time: f64,
    start_pointer_pos: Vec2,
    pinch_type: PinchType,
    previous: Option<DynGestureState>,
    current: DynGestureState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinchType {
    Horizontal,
    Vertical,
    Proportional,
}

impl PinchType {
    fn classify(touches: &BTreeMap<u64, (f32, f32)>) -> Self {
        if touches.len() == 2 {
            let mut it = touches.values();
            let a = *it.next().unwrap();
            let b = *it.next().unwrap();
            let dx = (a.0 - b.0).abs();
            let dy = (a.1 - b.1).abs();
            if dx > 3.0 * dy {
                Self::Horizontal
            } else if dy > 3.0 * dx {
                Self::Vertical
            } else {
                Self::Proportional
            }
        } else {
            Self::Proportional
        }
    }
}

pub(crate) struct MultiTouchDelta {
    pub zoom: f32,
    pub translation: Vec2,
    pub center: Vec2,
    pub num_touches: usize,
}

pub(crate) struct TouchGestureState {
    active_touches: BTreeMap<u64, (f32, f32)>,
    primary_touch_id: Option<u64>,
    prev_touch_px: Option<(f32, f32)>,
    touch_start: Option<(web_time::Instant, (f32, f32))>,
    touch_scroll_accum_x_px: f32,
    touch_scroll_accum_y_px: f32,
    touch_scrolled: bool,
    scroll_capture_id: Option<u64>,
    gesture_state: Option<GestureState>,
    past_touch_slop: bool,
    accum_pan: Vec2,
    accum_zoom: f32,
    accum_rotation: f32,
    // single-finger pending (deferred press to allow 2nd finger to cancel)
    primary_press_dispatched: bool,
    pending_primary: Option<(Vec2, web_time::Instant, u64)>,
}

impl Default for TouchGestureState {
    fn default() -> Self {
        Self {
            active_touches: BTreeMap::new(),
            primary_touch_id: None,
            prev_touch_px: None,
            touch_start: None,
            touch_scroll_accum_x_px: 0.0,
            touch_scroll_accum_y_px: 0.0,
            touch_scrolled: false,
            scroll_capture_id: None,
            gesture_state: None,
            past_touch_slop: false,
            accum_pan: Vec2 { x: 0.0, y: 0.0 },
            accum_zoom: 1.0,
            accum_rotation: 0.0,
            primary_press_dispatched: false,
            pending_primary: None,
        }
    }
}

impl TouchGestureState {
    fn calc_dynamic_state(&self) -> Option<DynGestureState> {
        let n = self.active_touches.len();
        if n < 2 {
            return None;
        }
        let n_recip = 1.0 / n as f32;
        let mut avg_pos = Vec2 { x: 0.0, y: 0.0 };
        for (_, (x, y)) in &self.active_touches {
            avg_pos.x += *x;
            avg_pos.y += *y;
        }
        avg_pos.x *= n_recip;
        avg_pos.y *= n_recip;
        let mut avg_distance = 0.0;
        for (_, (x, y)) in &self.active_touches {
            let dx = avg_pos.x - *x;
            let dy = avg_pos.y - *y;
            avg_distance += (dx * dx + dy * dy).sqrt();
        }
        avg_distance *= n_recip;
        // heading for rotation (not used for pan/zoom-only, but kept for completeness)
        let first = self.active_touches.values().next().unwrap();
        let heading = (avg_pos.x - first.0).atan2(avg_pos.y - first.1);
        Some(DynGestureState {
            avg_distance: avg_distance.max(1.0),
            avg_pos,
            heading,
        })
    }

    fn update_gesture(&mut self, time: f64, pointer_pos: Option<Vec2>, added_or_removed: bool) {
        if let Some(dyn_state) = self.calc_dynamic_state() {
            if let Some(state) = &mut self.gesture_state {
                state.previous = Some(state.current);
                state.current = dyn_state;
                if added_or_removed {
                    state.previous = None;
                    // Reset accum so next delta is not a jump
                    self.past_touch_slop = false;
                    self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
                    self.accum_zoom = 1.0;
                    self.accum_rotation = 0.0;
                }
            } else if let Some(pointer_pos) = pointer_pos {
                self.gesture_state = Some(GestureState {
                    start_time: time,
                    start_pointer_pos: pointer_pos,
                    pinch_type: PinchType::classify(&self.active_touches),
                    previous: None,
                    current: dyn_state,
                });
                self.past_touch_slop = false;
                self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
                self.accum_zoom = 1.0;
                self.accum_rotation = 0.0;
                if added_or_removed {
                    if let Some(s) = &mut self.gesture_state {
                        s.previous = None;
                    }
                }
            }
        } else {
            self.gesture_state = None;
            self.past_touch_slop = false;
            self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
            self.accum_zoom = 1.0;
            self.accum_rotation = 0.0;
        }
    }

    fn multi_touch_delta(&self) -> Option<(Vec2, f32, f32, Vec2)> {
        let state = self.gesture_state.as_ref()?;
        let prev = state.previous.unwrap_or(state.current);
        let curr = state.current;
        let zoom = curr.avg_distance / prev.avg_distance;
        let pan = Vec2 {
            x: curr.avg_pos.x - prev.avg_pos.x,
            y: curr.avg_pos.y - prev.avg_pos.y,
        };
        let rotation = curr.heading - prev.heading;
        // Normalize rotation to [-π, π] like egui's normalized_angle
        let rotation = rotation.sin().atan2(rotation.cos());
        Some((pan, zoom, rotation, curr.avg_pos))
    }

    pub(crate) fn touch_started(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
    ) -> Option<u64> {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };
        let was_empty = self.active_touches.is_empty();
        self.active_touches.insert(tid, pos_px);

        let is_primary = self.primary_touch_id.is_none();
        if is_primary {
            self.primary_touch_id = Some(tid);
            self.touch_start = Some((web_time::Instant::now(), pos_px));
            self.touch_scrolled = false;
            self.scroll_capture_id = None;
            self.touch_scroll_accum_x_px = 0.0;
            self.touch_scroll_accum_y_px = 0.0;
            self.prev_touch_px = Some(pos_px);
            self.pending_primary = Some((pos, web_time::Instant::now(), tid));
            self.primary_press_dispatched = false;
            // If this is actually the 2nd finger making it multi-touch, gesture will be created on next move;
            // we don't dispatch press yet (deferred via pending).
            if self.active_touches.len() >= 2 {
                let time = web_time::Instant::now().elapsed().as_secs_f64();
                self.update_gesture(time, Some(pos), true);
            }
            return None;
        }
        if self.pending_primary.is_some() {
            self.pending_primary = None;
        }
        if self.primary_press_dispatched {
            rt.handle_pointer_cancel();
            self.primary_press_dispatched = false;
        }
        if self.active_touches.len() >= 2 {
            let time = web_time::Instant::now().elapsed().as_secs_f64();
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(time, pointer_pos, true);
        }
        None
    }

    pub(crate) fn touch_moved(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
        scale: f32,
    ) -> (bool, Option<(f32, Vec2)>, Option<Vec2>) {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };
        let mut dirty = false;
        let mut pinch: Option<(f32, Vec2)> = None;
        let mut pan: Option<Vec2> = None;
        self.active_touches.insert(tid, pos_px);

        if self.active_touches.len() >= 2 {
            let time = web_time::Instant::now().elapsed().as_secs_f64();
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(time, pointer_pos, false);
            if let Some((raw_pan, raw_zoom, _raw_rot, center)) = self.multi_touch_delta() {
                // Compose-like
                let centroid_size = self
                    .gesture_state
                    .as_ref()
                    .map(|s| s.current.avg_distance)
                    .unwrap_or(1.0);
                let touch_slop = 18.0 * scale; // viewConfiguration.touchSlop for finger, 6 for mouse
                if !self.past_touch_slop {
                    self.accum_pan.x += raw_pan.x;
                    self.accum_pan.y += raw_pan.y;
                    self.accum_zoom *= raw_zoom;
                    // rotation not used (panZoomLock)
                    let zoom_motion = (self.accum_zoom - 1.0).abs() * centroid_size;
                    let pan_motion = (self.accum_pan.x * self.accum_pan.x
                        + self.accum_pan.y * self.accum_pan.y)
                        .sqrt();
                    if zoom_motion > touch_slop || pan_motion > touch_slop {
                        self.past_touch_slop = true;
                    }
                }
                if self.past_touch_slop {
                    // Exactly like Compose: after slop, dispatch raw per-frame deltas
                    // No per-frame clamp or deadzone — smoothness comes from centroid,
                    // previous=None on count change, and touchSlop gating. No compromises.
                    pinch = Some((raw_zoom, center));
                    pan = Some(raw_pan);
                    self.touch_scrolled = true;
                    dirty = true;
                } else {
                    dirty = false;
                }
            }
            // Keep single-finger prev in sync for when we go back to 1 finger
            if self.primary_touch_id == Some(tid) {
                self.prev_touch_px = Some(pos_px);
            }
            return (dirty, pinch, pan);
        }

        // Single-finger: deferred press + scroll
        if self.primary_touch_id != Some(tid) {
            return (dirty, None, None);
        }

        if let Some((pending_pos, pending_instant, _pending_tid)) = self.pending_primary {
            let dt = (web_time::Instant::now() - pending_instant).as_secs_f32();
            let dx = pos_px.0 - pending_pos.x;
            let dy = pos_px.1 - pending_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            // pastTouchSlop for single finger: 6*scale or 30ms
            if dt > 0.03 || dist > 6.0 * scale {
                let _focused = rt
                    .handle_pointer_press(pending_pos, PointerButton::Primary)
                    .focused;
                self.primary_press_dispatched = true;
                self.pending_primary = None;
            } else {
                self.prev_touch_px = Some(pos_px);
                return (dirty, None, None);
            }
        }

        if let Some(prev) = self.prev_touch_px {
            let dx_px = pos_px.0 - prev.0;
            let dy_px = pos_px.1 - prev.1;

            if dx_px.abs() > 0.0 || dy_px.abs() > 0.0 {
                self.touch_scroll_accum_x_px += dx_px;
                self.touch_scroll_accum_y_px += dy_px;

                let is_scroll = self.touch_scrolled
                    || self.touch_scroll_accum_x_px.abs() > 6.0 * scale
                    || self.touch_scroll_accum_y_px.abs() > 6.0 * scale;

                if is_scroll {
                    let (consumed, cap) = rt.handle_scroll_at(
                        pos,
                        Vec2 {
                            x: -dx_px,
                            y: -dy_px,
                        },
                        self.scroll_capture_id,
                    );
                    self.scroll_capture_id = cap;

                    if consumed {
                        self.touch_scrolled = true;
                    }
                }
            }

            if self.primary_press_dispatched {
                rt.handle_pointer_move(pos);
            }
            dirty = true;
        }

        self.prev_touch_px = Some(pos_px);
        (dirty, None, None)
    }

    pub(crate) fn touch_ended(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
        cancelled: bool,
    ) -> Option<bool> {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };

        let is_primary = self.primary_touch_id == Some(tid);
        let was_multi = self.active_touches.len() >= 2;

        if is_primary {
            if let Some((pending_pos, _, _)) = self.pending_primary.take() {
                if !cancelled && !was_multi && self.active_touches.len() < 2 {
                    let _ = rt.handle_pointer_press(pending_pos, PointerButton::Primary);
                    rt.handle_pointer_release(pos, PointerButton::Primary);
                } else {
                    // Pending was for a tap that became multi-touch, so no press/click
                }
                self.primary_press_dispatched = false;
            } else if self.primary_press_dispatched {
                if cancelled || was_multi {
                    rt.handle_pointer_cancel();
                } else {
                    rt.handle_pointer_release(pos, PointerButton::Primary);
                }
                self.primary_press_dispatched = false;
            } else if cancelled || was_multi {
                rt.handle_pointer_cancel();
            }
        } else {
            if self.pending_primary.is_some() && self.active_touches.len() < 2 {}
        }

        self.active_touches.remove(&tid);
        if self.active_touches.len() >= 2 {
            let time = web_time::Instant::now().elapsed().as_secs_f64();
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(time, pointer_pos, true);
        } else {
            self.gesture_state = None;
            self.past_touch_slop = false;
            self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
            self.accum_zoom = 1.0;
            self.accum_rotation = 0.0;
        }

        let mut swipe_right = None;
        if is_primary {
            self.primary_touch_id = None;
            if let Some((t0, p0)) = self.touch_start.take() {
                let dt = (web_time::Instant::now() - t0).as_secs_f32();
                let dx = pos_px.0 - p0.0;
                let dy = pos_px.1 - p0.1;

                if dt < 0.35 && dy.abs() < 40.0 && dx.abs() > 80.0 && !self.touch_scrolled {
                    swipe_right = Some(dx > 0.0);
                }
            }
            self.scroll_capture_id = None;
            self.prev_touch_px = None;
        }
        swipe_right
    }

    pub(crate) fn multi_touch_info(&self) -> Option<MultiTouchDelta> {
        let state = self.gesture_state.as_ref()?;
        let prev = state.previous.unwrap_or(state.current);
        let curr = state.current;
        let zoom = curr.avg_distance / prev.avg_distance;
        let translation = Vec2 {
            x: curr.avg_pos.x - prev.avg_pos.x,
            y: curr.avg_pos.y - prev.avg_pos.y,
        };
        Some(MultiTouchDelta {
            zoom,
            translation,
            center: curr.avg_pos,
            num_touches: self.active_touches.len(),
        })
    }
}

pub(crate) fn request_redraw(window: &Option<std::sync::Arc<winit::window::Window>>) {
    if let Some(w) = window {
        w.request_redraw();
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn is_textfield_in_frame(frame_cache: &Option<Frame>, id: u64) -> bool {
    if let Some(f) = frame_cache {
        f.semantics_nodes
            .iter()
            .any(|n| n.id == id && n.role == Role::TextField)
    } else {
        false
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn update_modifiers(modifiers: &mut Modifiers, state: &winit::keyboard::ModifiersState) {
    modifiers.shift = state.shift_key();
    modifiers.ctrl = state.control_key();
    modifiers.alt = state.alt_key();
    modifiers.meta = state.super_key();
    modifiers.command = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
}

#[cfg(any(target_arch = "wasm32", target_os = "android"))]
pub(crate) fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
}

/// Map a physical key to a Repose key. Letters are always lowercase (chord
/// matching keys off the modifier flags); digits and punctuation produce the
/// US-layout (most used) character the key would type, honoring `mods.shift`, so Shift+8
/// arrives as '*' and Shift+Equal as '+'.
pub(crate) fn map_key(
    key: winit::keyboard::PhysicalKey,
    mods: &repose_core::input::Modifiers,
) -> repose_core::input::Key {
    use repose_core::input::Key;
    use winit::keyboard::{KeyCode, PhysicalKey};

    // (unshifted, shifted) character pair for a US-layout key.
    let shifted = |a: char, b: char| Key::Character(if mods.shift { b } else { a });

    match key {
        PhysicalKey::Code(KeyCode::Enter) => Key::Enter,
        PhysicalKey::Code(KeyCode::Tab) => Key::Tab,
        PhysicalKey::Code(KeyCode::Backspace) => Key::Backspace,
        PhysicalKey::Code(KeyCode::Delete) => Key::Delete,
        PhysicalKey::Code(KeyCode::Insert) => Key::Insert,
        PhysicalKey::Code(KeyCode::Escape) => Key::Escape,
        PhysicalKey::Code(KeyCode::ArrowLeft) => Key::ArrowLeft,
        PhysicalKey::Code(KeyCode::ArrowRight) => Key::ArrowRight,
        PhysicalKey::Code(KeyCode::ArrowUp) => Key::ArrowUp,
        PhysicalKey::Code(KeyCode::ArrowDown) => Key::ArrowDown,
        PhysicalKey::Code(KeyCode::Home) => Key::Home,
        PhysicalKey::Code(KeyCode::End) => Key::End,
        PhysicalKey::Code(KeyCode::PageUp) => Key::PageUp,
        PhysicalKey::Code(KeyCode::PageDown) => Key::PageDown,
        PhysicalKey::Code(KeyCode::Space) => Key::Space,
        PhysicalKey::Code(KeyCode::KeyA) => Key::Character('a'),
        PhysicalKey::Code(KeyCode::KeyB) => Key::Character('b'),
        PhysicalKey::Code(KeyCode::KeyC) => Key::Character('c'),
        PhysicalKey::Code(KeyCode::KeyD) => Key::Character('d'),
        PhysicalKey::Code(KeyCode::KeyE) => Key::Character('e'),
        PhysicalKey::Code(KeyCode::KeyF) => Key::Character('f'),
        PhysicalKey::Code(KeyCode::KeyG) => Key::Character('g'),
        PhysicalKey::Code(KeyCode::KeyH) => Key::Character('h'),
        PhysicalKey::Code(KeyCode::KeyI) => Key::Character('i'),
        PhysicalKey::Code(KeyCode::KeyJ) => Key::Character('j'),
        PhysicalKey::Code(KeyCode::KeyK) => Key::Character('k'),
        PhysicalKey::Code(KeyCode::KeyL) => Key::Character('l'),
        PhysicalKey::Code(KeyCode::KeyM) => Key::Character('m'),
        PhysicalKey::Code(KeyCode::KeyN) => Key::Character('n'),
        PhysicalKey::Code(KeyCode::KeyO) => Key::Character('o'),
        PhysicalKey::Code(KeyCode::KeyP) => Key::Character('p'),
        PhysicalKey::Code(KeyCode::KeyQ) => Key::Character('q'),
        PhysicalKey::Code(KeyCode::KeyR) => Key::Character('r'),
        PhysicalKey::Code(KeyCode::KeyS) => Key::Character('s'),
        PhysicalKey::Code(KeyCode::KeyT) => Key::Character('t'),
        PhysicalKey::Code(KeyCode::KeyU) => Key::Character('u'),
        PhysicalKey::Code(KeyCode::KeyV) => Key::Character('v'),
        PhysicalKey::Code(KeyCode::KeyW) => Key::Character('w'),
        PhysicalKey::Code(KeyCode::KeyX) => Key::Character('x'),
        PhysicalKey::Code(KeyCode::KeyY) => Key::Character('y'),
        PhysicalKey::Code(KeyCode::KeyZ) => Key::Character('z'),
        PhysicalKey::Code(KeyCode::Digit0) => shifted('0', ')'),
        PhysicalKey::Code(KeyCode::Digit1) => shifted('1', '!'),
        PhysicalKey::Code(KeyCode::Digit2) => shifted('2', '@'),
        PhysicalKey::Code(KeyCode::Digit3) => shifted('3', '#'),
        PhysicalKey::Code(KeyCode::Digit4) => shifted('4', '$'),
        PhysicalKey::Code(KeyCode::Digit5) => shifted('5', '%'),
        PhysicalKey::Code(KeyCode::Digit6) => shifted('6', '^'),
        PhysicalKey::Code(KeyCode::Digit7) => shifted('7', '&'),
        PhysicalKey::Code(KeyCode::Digit8) => shifted('8', '*'),
        PhysicalKey::Code(KeyCode::Digit9) => shifted('9', '('),
        PhysicalKey::Code(KeyCode::Minus) => shifted('-', '_'),
        PhysicalKey::Code(KeyCode::Equal) => shifted('=', '+'),
        PhysicalKey::Code(KeyCode::BracketLeft) => shifted('[', '{'),
        PhysicalKey::Code(KeyCode::BracketRight) => shifted(']', '}'),
        PhysicalKey::Code(KeyCode::Backslash) => shifted('\\', '|'),
        PhysicalKey::Code(KeyCode::Semicolon) => shifted(';', ':'),
        PhysicalKey::Code(KeyCode::Quote) => shifted('\'', '"'),
        PhysicalKey::Code(KeyCode::Comma) => shifted(',', '<'),
        PhysicalKey::Code(KeyCode::Period) => shifted('.', '>'),
        PhysicalKey::Code(KeyCode::Slash) => shifted('/', '?'),
        PhysicalKey::Code(KeyCode::Backquote) => shifted('`', '~'),
        PhysicalKey::Code(KeyCode::NumpadAdd) => Key::Character('+'),
        PhysicalKey::Code(KeyCode::NumpadSubtract) => Key::Character('-'),
        PhysicalKey::Code(KeyCode::NumpadMultiply) => Key::Character('*'),
        PhysicalKey::Code(KeyCode::NumpadDivide) => Key::Character('/'),
        PhysicalKey::Code(KeyCode::NumpadEnter) => Key::Enter,
        PhysicalKey::Code(KeyCode::NumpadDecimal) => Key::Character('.'),
        PhysicalKey::Code(KeyCode::Numpad0) => Key::Character('0'),
        PhysicalKey::Code(KeyCode::Numpad1) => Key::Character('1'),
        PhysicalKey::Code(KeyCode::Numpad2) => Key::Character('2'),
        PhysicalKey::Code(KeyCode::Numpad3) => Key::Character('3'),
        PhysicalKey::Code(KeyCode::Numpad4) => Key::Character('4'),
        PhysicalKey::Code(KeyCode::Numpad5) => Key::Character('5'),
        PhysicalKey::Code(KeyCode::Numpad6) => Key::Character('6'),
        PhysicalKey::Code(KeyCode::Numpad7) => Key::Character('7'),
        PhysicalKey::Code(KeyCode::Numpad8) => Key::Character('8'),
        PhysicalKey::Code(KeyCode::Numpad9) => Key::Character('9'),
        PhysicalKey::Code(KeyCode::F1) => Key::F(1),
        PhysicalKey::Code(KeyCode::F2) => Key::F(2),
        PhysicalKey::Code(KeyCode::F3) => Key::F(3),
        PhysicalKey::Code(KeyCode::F4) => Key::F(4),
        PhysicalKey::Code(KeyCode::F5) => Key::F(5),
        PhysicalKey::Code(KeyCode::F6) => Key::F(6),
        PhysicalKey::Code(KeyCode::F7) => Key::F(7),
        PhysicalKey::Code(KeyCode::F8) => Key::F(8),
        PhysicalKey::Code(KeyCode::F9) => Key::F(9),
        PhysicalKey::Code(KeyCode::F10) => Key::F(10),
        PhysicalKey::Code(KeyCode::F11) => Key::F(11),
        PhysicalKey::Code(KeyCode::F12) => Key::F(12),
        _ => Key::Unknown,
    }
}
