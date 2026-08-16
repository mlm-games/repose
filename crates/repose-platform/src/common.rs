#[cfg(any(target_arch = "wasm32", target_os = "android"))]
use crate::*;

use std::collections::HashMap;

use repose_app::ReposeRuntime;
use repose_core::input::PointerButton;
use repose_core::Vec2;

/// Shared multi-touch gesture state used by the desktop, Android, and web
/// runners. Recognizes touch-scroll (with a scroll capture id), pinch zoom,
/// and horizontal swipe-back gestures, dispatching them through a host hook.
///
/// Winit delivers `WindowEvent::Touch` uniformly, so the same state machine
/// powers touchscreens on every platform.
pub(crate) struct TouchGestureState {
    active_touches: HashMap<u64, (f32, f32)>,
    primary_touch_id: Option<u64>,
    prev_touch_px: Option<(f32, f32)>,
    touch_start: Option<(web_time::Instant, (f32, f32))>,
    touch_scroll_accum_x_px: f32,
    touch_scroll_accum_y_px: f32,
    touch_scrolled: bool,
    scroll_capture_id: Option<u64>,
    pinch_last_dist: Option<f32>,
}

impl Default for TouchGestureState {
    fn default() -> Self {
        Self {
            active_touches: HashMap::new(),
            primary_touch_id: None,
            prev_touch_px: None,
            touch_start: None,
            touch_scroll_accum_x_px: 0.0,
            touch_scroll_accum_y_px: 0.0,
            touch_scrolled: false,
            scroll_capture_id: None,
            pinch_last_dist: None,
        }
    }
}

impl TouchGestureState {
    /// Handle a touch down. Returns the id that was focused by the press (if
    /// any) so hosts can set up IME for a focused text field.
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
        self.active_touches.insert(tid, pos_px);

        self.touch_scrolled = false;
        self.scroll_capture_id = None;
        self.touch_scroll_accum_x_px = 0.0;
        self.touch_scroll_accum_y_px = 0.0;

        if self.primary_touch_id.is_none() {
            self.primary_touch_id = Some(tid);
            self.touch_start = Some((web_time::Instant::now(), pos_px));
        }

        self.prev_touch_px = Some(pos_px);
        rt.handle_pointer_press(pos, PointerButton::Primary)
            .focused
    }

    /// Handle a touch move. Returns `(dirty, pinch_delta_scale)`; the host
    /// dispatches any pinch gesture through its own action handler.
    pub(crate) fn touch_moved(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
        scale: f32,
    ) -> (bool, Option<f32>) {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };
        let mut dirty = false;
        let mut pinch_delta = None;
        self.active_touches.insert(tid, pos_px);

        // Pinch gesture (two active touches)
        if self.active_touches.len() == 2 {
            let mut it = self.active_touches.values();
            let a = it.next().copied().unwrap();
            let b = it.next().copied().unwrap();
            let dx = a.0 - b.0;
            let dy = a.1 - b.1;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

            if let Some(prev) = self.pinch_last_dist.replace(dist) {
                pinch_delta = Some((dist / prev).clamp(0.5, 2.0));
            }
        }

        // Only the primary touch drives scroll / pointer-move.
        if self.primary_touch_id != Some(tid) {
            self.prev_touch_px = Some(pos_px);
            return (dirty, pinch_delta);
        }

        if let Some(prev) = self.prev_touch_px {
            let dx_px = pos_px.0 - prev.0;
            let dy_px = pos_px.1 - prev.1;

            if dx_px.abs() > 0.0 || dy_px.abs() > 0.0 {
                self.touch_scroll_accum_x_px += dx_px;
                self.touch_scroll_accum_y_px += dy_px;

                let (consumed, cap) = rt.handle_scroll_at(
                    pos,
                    Vec2 {
                        x: -dx_px,
                        y: -dy_px,
                    },
                    self.scroll_capture_id,
                );
                self.scroll_capture_id = cap;

                if consumed
                    && (self.touch_scroll_accum_x_px.abs() > 6.0 * scale
                        || self.touch_scroll_accum_y_px.abs() > 6.0 * scale)
                {
                    self.touch_scrolled = true;
                }
            }

            // Enter/leave/move dispatch
            rt.handle_pointer_move(pos);
            dirty = true;
        }

        self.prev_touch_px = Some(pos_px);
        (dirty, pinch_delta)
    }

    /// Handle a touch up / cancel. Returns `Some(right)` when a horizontal
    /// swipe gesture fired (`right == dx > 0`), which the host dispatches.
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

        if cancelled {
            rt.handle_pointer_cancel();
        } else {
            rt.handle_pointer_release(pos, PointerButton::Primary);
        }

        self.active_touches.remove(&tid);
        if self.active_touches.len() < 2 {
            self.pinch_last_dist = None;
        }

        // Swipe gesture for the primary touch
        let mut swipe_right = None;
        if self.primary_touch_id == Some(tid) {
            self.primary_touch_id = None;
            if let Some((t0, p0)) = self.touch_start.take() {
                let dt = (web_time::Instant::now() - t0).as_secs_f32();
                let dx = pos_px.0 - p0.0;
                let dy = pos_px.1 - p0.1;

                if dt < 0.35 && dy.abs() < 40.0 && dx.abs() > 80.0 && !self.touch_scrolled {
                    swipe_right = Some(dx > 0.0);
                }
            }
        }

        self.scroll_capture_id = None;
        self.prev_touch_px = None;
        swipe_right
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

pub(crate) fn map_key(key: winit::keyboard::PhysicalKey) -> repose_core::input::Key {
    use repose_core::input::Key;
    use winit::keyboard::{KeyCode, PhysicalKey};

    match key {
        PhysicalKey::Code(KeyCode::Enter) => Key::Enter,
        PhysicalKey::Code(KeyCode::Tab) => Key::Tab,
        PhysicalKey::Code(KeyCode::Backspace) => Key::Backspace,
        PhysicalKey::Code(KeyCode::Delete) => Key::Delete,
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
        PhysicalKey::Code(KeyCode::Digit0) => Key::Character('0'),
        PhysicalKey::Code(KeyCode::Digit1) => Key::Character('1'),
        PhysicalKey::Code(KeyCode::Digit2) => Key::Character('2'),
        PhysicalKey::Code(KeyCode::Digit3) => Key::Character('3'),
        PhysicalKey::Code(KeyCode::Digit4) => Key::Character('4'),
        PhysicalKey::Code(KeyCode::Digit5) => Key::Character('5'),
        PhysicalKey::Code(KeyCode::Digit6) => Key::Character('6'),
        PhysicalKey::Code(KeyCode::Digit7) => Key::Character('7'),
        PhysicalKey::Code(KeyCode::Digit8) => Key::Character('8'),
        PhysicalKey::Code(KeyCode::Digit9) => Key::Character('9'),
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
