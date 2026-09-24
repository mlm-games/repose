use std::collections::BTreeMap;

use repose_core::Vec2;
use repose_core::input::PointerButton;

use crate::runtime::ReposeRuntime;

#[derive(Clone, Copy, Debug)]
struct DynGestureState {
    avg_distance: f32,
    avg_pos: Vec2,
    heading: f32,
}

#[derive(Clone, Debug)]
struct GestureState {
    previous: Option<DynGestureState>,
    current: DynGestureState,
}

pub struct MultiTouchDelta {
    pub zoom: f32,
    pub translation: Vec2,
    pub rotation: f32,
    pub center: Vec2,
    pub num_touches: usize,
}

pub struct TouchGestureState {
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
    primary_press_focus: Option<Option<u64>>,
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
            primary_press_focus: None,
            pending_primary: None,
        }
    }
}

pub struct TouchEnded {
    pub swipe_right: Option<bool>,
    /// Focus result of a press dispatched on release. `None` when no press
    /// ran (scroll/pinch release, cancel); `Some` carries the press's
    /// focused id (`None` inside means the tap explicitly defocused).
    pub press: Option<Option<u64>>,
}

impl TouchGestureState {
    fn calc_dynamic_state(&self) -> Option<DynGestureState> {
        let n = self.active_touches.len();
        if n < 2 {
            return None;
        }
        let n_recip = 1.0 / n as f32;
        let mut avg_pos = Vec2 { x: 0.0, y: 0.0 };
        for (x, y) in self.active_touches.values() {
            avg_pos.x += *x;
            avg_pos.y += *y;
        }
        avg_pos.x *= n_recip;
        avg_pos.y *= n_recip;
        let mut avg_distance = 0.0;
        for (x, y) in self.active_touches.values() {
            let dx = avg_pos.x - *x;
            let dy = avg_pos.y - *y;
            avg_distance += (dx * dx + dy * dy).sqrt();
        }
        avg_distance *= n_recip;
        let first = self.active_touches.values().next().copied()?;
        let heading = (avg_pos.x - first.0).atan2(avg_pos.y - first.1);
        Some(DynGestureState {
            avg_distance: avg_distance.max(1.0),
            avg_pos,
            heading,
        })
    }

    fn update_gesture(&mut self, pointer_pos: Option<Vec2>, added_or_removed: bool) {
        if let Some(dyn_state) = self.calc_dynamic_state() {
            if let Some(state) = &mut self.gesture_state {
                state.previous = Some(state.current);
                state.current = dyn_state;
                if added_or_removed {
                    state.previous = None;
                    self.past_touch_slop = false;
                    self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
                    self.accum_zoom = 1.0;
                    self.accum_rotation = 0.0;
                }
            } else if pointer_pos.is_some() {
                self.gesture_state = Some(GestureState {
                    previous: None,
                    current: dyn_state,
                });
                self.past_touch_slop = false;
                self.accum_pan = Vec2 { x: 0.0, y: 0.0 };
                self.accum_zoom = 1.0;
                self.accum_rotation = 0.0;
                if added_or_removed && let Some(s) = &mut self.gesture_state {
                    s.previous = None;
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
        let rotation = rotation.sin().atan2(rotation.cos());
        Some((pan, zoom, rotation, curr.avg_pos))
    }

    pub fn touch_started(
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
        // Every finger dispatches its own press immediately: games stage
        // per-finger contacts off these events (GML `device_mouse_*`
        // parity).
        let press = rt.handle_touch_press(Self::touch_finger(tid), pos, PointerButton::Primary);

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
            self.primary_press_focus = Some(press.focused);
            if self.active_touches.len() >= 2 {
                self.update_gesture(Some(pos), true);
            }
            return press.focused;
        }
        if let Some(primary_tid) = self.primary_touch_id {
            rt.suppress_touch_click(primary_tid);
        }
        self.pending_primary = None;
        if self.active_touches.len() >= 2 {
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(pointer_pos, true);
        }
        None
    }

    pub fn touch_moved(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
        scale: f32,
    ) -> (
        bool,
        Option<(f32, Vec2)>,
        Option<(Vec2, Vec2)>,
        Option<(f32, Vec2)>,
    ) {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };
        let mut dirty;
        let mut pinch: Option<(f32, Vec2)> = None;
        let mut pan: Option<(Vec2, Vec2)> = None;
        let mut rotation: Option<(f32, Vec2)> = None;
        self.active_touches.insert(tid, pos_px);
        rt.handle_touch_move(Self::touch_finger(tid), pos);
        dirty = true;

        if self.active_touches.len() >= 2 {
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(pointer_pos, false);
            if let Some((raw_pan, raw_zoom, raw_rot, center)) = self.multi_touch_delta() {
                let centroid_size = self
                    .gesture_state
                    .as_ref()
                    .map(|s| s.current.avg_distance)
                    .unwrap_or(1.0);
                let touch_slop = 18.0 * scale;
                if !self.past_touch_slop {
                    self.accum_pan.x += raw_pan.x;
                    self.accum_pan.y += raw_pan.y;
                    self.accum_zoom *= raw_zoom;
                    self.accum_rotation += raw_rot;
                    let zoom_motion = (self.accum_zoom - 1.0).abs() * centroid_size;
                    let rotation_motion = self.accum_rotation.abs() * centroid_size;
                    let pan_motion = (self.accum_pan.x * self.accum_pan.x
                        + self.accum_pan.y * self.accum_pan.y)
                        .sqrt();
                    if zoom_motion > touch_slop
                        || rotation_motion > touch_slop
                        || pan_motion > touch_slop
                    {
                        self.past_touch_slop = true;
                    }
                }
                if self.past_touch_slop {
                    pinch = Some((raw_zoom, center));
                    pan = Some((raw_pan, center));
                    if raw_rot != 0.0 {
                        rotation = Some((raw_rot, center));
                    }
                    self.touch_scrolled = true;
                    dirty = true;
                } else {
                    dirty = false;
                }
            }
            if self.primary_touch_id == Some(tid) {
                self.prev_touch_px = Some(pos_px);
            }
            return (dirty, pinch, pan, rotation);
        }

        if self.primary_touch_id != Some(tid) {
            return (dirty, None, None, None);
        }

        if let Some((pending_pos, pending_instant, pending_tid)) = self.pending_primary {
            let dt = (web_time::Instant::now() - pending_instant).as_secs_f32();
            let dx = pos_px.0 - pending_pos.x;
            let dy = pos_px.1 - pending_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dt > 0.03 || dist > 6.0 * scale {
                let _ = pending_pos;
                rt.suppress_touch_click(pending_tid);
                self.pending_primary = None;
            } else {
                self.prev_touch_px = Some(pos_px);
                return (dirty, None, None, None);
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
            dirty = true;
        }

        self.prev_touch_px = Some(pos_px);
        (dirty, None, None, None)
    }

    pub fn touch_ended(
        &mut self,
        rt: &mut ReposeRuntime,
        tid: u64,
        pos_px: (f32, f32),
        cancelled: bool,
    ) -> TouchEnded {
        rt.mouse_pos_px = pos_px;
        let pos = Vec2 {
            x: pos_px.0,
            y: pos_px.1,
        };

        let is_primary = self.primary_touch_id == Some(tid);
        if cancelled {
            rt.handle_touch_cancel_at(tid, pos);
        } else {
            rt.handle_touch_release(Self::touch_finger(tid), pos, PointerButton::Primary);
        }
        let press = if is_primary && !cancelled {
            self.pending_primary.take();
            self.primary_press_focus.take()
        } else {
            if is_primary && cancelled {
                self.pending_primary = None;
                self.primary_press_focus = None;
            }
            None
        };

        self.active_touches.remove(&tid);
        if self.active_touches.len() >= 2 {
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(pointer_pos, true);
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
        TouchEnded { swipe_right, press }
    }

    /// Live touch contacts in physical px, keyed by winit touch id.
    /// Single source for game touch zones (unlike the press
    /// edge, which fires once and is gone).
    /// Finger id carried on gesture-dispatched pointer events, so
    /// game viewports can attribute each touch to its finger.
    fn touch_finger(tid: u64) -> Option<u64> {
        Some(tid)
    }

    pub fn active_touches(&self) -> &BTreeMap<u64, (f32, f32)> {
        &self.active_touches
    }

    /// Record a live contact (Started/Moved). Called from
    /// `handle_touch_raw` before the gesture layer runs.
    pub fn contact_down(&mut self, tid: u64, pos_px: (f32, f32)) {
        self.active_touches.insert(tid, pos_px);
    }

    /// Drop a live contact (Ended/Cancelled). Called from
    /// `handle_touch_raw` before the gesture layer runs.
    pub fn contact_up(&mut self, tid: u64) {
        self.active_touches.remove(&tid);
    }

    pub fn multi_touch_info(&self) -> Option<MultiTouchDelta> {
        let state = self.gesture_state.as_ref()?;
        let prev = state.previous.unwrap_or(state.current);
        let curr = state.current;
        let zoom = curr.avg_distance / prev.avg_distance;
        let translation = Vec2 {
            x: curr.avg_pos.x - prev.avg_pos.x,
            y: curr.avg_pos.y - prev.avg_pos.y,
        };
        let rotation = curr.heading - prev.heading;
        let rotation = rotation.sin().atan2(rotation.cos());
        Some(MultiTouchDelta {
            zoom,
            translation,
            rotation,
            center: curr.avg_pos,
            num_touches: self.active_touches.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn twist_reports_rotation_once_past_slop() {
        let mut rt = ReposeRuntime::new();
        let mut g = TouchGestureState::default();
        g.touch_started(&mut rt, 0, (0.0, 0.0));
        g.touch_started(&mut rt, 1, (100.0, 0.0));
        let (_, _, _, rotation) = g.touch_moved(&mut rt, 1, (100.0, 20.0), 1.0);
        let _ = rotation;
        let (_, _, _, _) = g.touch_moved(&mut rt, 1, (50.0, 80.0), 1.0);
        let (dirty, pinch, pan, rotation) = g.touch_moved(&mut rt, 1, (0.0, 100.0), 1.0);
        assert!(dirty, "twist past slop must mark dirty");
        assert!(pinch.is_some(), "zoom channel still reported");
        assert!(pan.is_some(), "pan channel still reported");
        let (delta_rot, center) = rotation.expect("rotation must be propagated, not dropped");
        assert!(
            delta_rot.abs() > 0.1,
            "expected non-trivial twist, got {delta_rot}"
        );
        assert!(center.x.is_finite() && center.y.is_finite());
        let info = g.multi_touch_info().expect("info mirrors deltas");
        assert!(info.rotation.abs() > 0.0);
    }

    #[test]
    fn pure_pinch_reports_no_rotation_spam() {
        let mut rt = ReposeRuntime::new();
        let mut g = TouchGestureState::default();
        g.touch_started(&mut rt, 0, (0.0, 0.0));
        g.touch_started(&mut rt, 1, (100.0, 0.0));
        let (_, _, _, _) = g.touch_moved(&mut rt, 1, (140.0, 0.0), 1.0);
        let (dirty, pinch, _, rotation) = g.touch_moved(&mut rt, 1, (180.0, 0.0), 1.0);
        assert!(dirty);
        assert!(pinch.is_some());
        assert!(
            rotation.is_none(),
            "exact-zero rotation must not spam Rotate gestures"
        );
    }

    #[test]
    fn second_finger_down_registers_before_any_move() {
        let mut rt = ReposeRuntime::new();
        let mut g = TouchGestureState::default();
        g.contact_down(1, (10.0, 10.0));
        g.touch_started(&mut rt, 1, (10.0, 10.0));
        g.contact_down(2, (200.0, 200.0));
        g.touch_started(&mut rt, 2, (200.0, 200.0));
        assert_eq!(
            g.active_touches().len(),
            2,
            "both fingers visible while held still"
        );
        assert!(
            g.active_touches().contains_key(&1) && g.active_touches().contains_key(&2),
            "no finger dropped before its first move"
        );
    }

    #[test]
    fn every_finger_dispatches_its_own_press_move_release() {
        use repose_core::input::{PointerEvent, PointerEventKind, PointerKind};
        use std::collections::HashMap;
        let mut rt = ReposeRuntime::new();
        let seen: Rc<RefCell<HashMap<u64, Vec<PointerEventKind>>>> =
            Rc::new(RefCell::new(HashMap::new()));
        let seen_down = seen.clone();
        let mk = move |_kind: PointerKind| {
            let seen_down = seen_down.clone();
            Rc::new(move |ev: PointerEvent| {
                seen_down
                    .borrow_mut()
                    .entry(ev.id.0)
                    .or_default()
                    .push(ev.event);
            })
        };
        rt.cache_frame(repose_core::runtime::Frame {
            scene: Default::default(),
            hit_regions: vec![repose_core::HitRegion {
                id: 1,
                rect: repose_core::Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 400.0,
                    h: 400.0,
                },
                on_pointer_down: Some(mk(PointerKind::Touch)),
                on_pointer_move: Some(mk(PointerKind::Touch)),
                on_pointer_up: Some(mk(PointerKind::Touch)),
                ..Default::default()
            }],
            semantics_nodes: Vec::new(),
            focus_chain: Vec::new(),
        });
        let mut g = TouchGestureState::default();
        g.touch_started(&mut rt, 1, (10.0, 10.0));
        g.touch_started(&mut rt, 2, (200.0, 200.0));
        g.touch_moved(&mut rt, 1, (12.0, 10.0), 1.0);
        g.touch_moved(&mut rt, 2, (200.0, 202.0), 1.0);
        g.touch_ended(&mut rt, 1, (12.0, 10.0), false);
        g.touch_ended(&mut rt, 2, (200.0, 202.0), false);
        let seen = seen.borrow();
        for fid in [1u64, 2u64] {
            let kinds: Vec<_> = seen.get(&fid).cloned().unwrap_or_default();
            assert!(
                kinds.iter().any(|k| matches!(k, PointerEventKind::Down(_))),
                "finger {fid} must dispatch its own press, got {kinds:?}"
            );
            assert!(
                kinds.iter().any(|k| matches!(k, PointerEventKind::Move)),
                "finger {fid} must dispatch its own move, got {kinds:?}"
            );
            assert!(
                kinds.iter().any(|k| matches!(k, PointerEventKind::Up(_))),
                "finger {fid} must dispatch its own release, got {kinds:?}"
            );
        }
        assert!(
            rt.touch_paths.is_empty(),
            "both finger paths must drop on release"
        );
    }
}
