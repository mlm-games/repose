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
                if added_or_removed
                    && let Some(s) = &mut self.gesture_state {
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
        let was_empty = self.active_touches.is_empty();
        let _ = was_empty;
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
            if self.active_touches.len() >= 2 {
                self.update_gesture(Some(pos), true);
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
            let pointer_pos = self
                .primary_touch_id
                .and_then(|pid| self.active_touches.get(&pid).copied())
                .map(|(x, y)| Vec2 { x, y });
            self.update_gesture(pointer_pos, false);
            if let Some((raw_pan, raw_zoom, _raw_rot, center)) = self.multi_touch_delta() {
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
                    let zoom_motion = (self.accum_zoom - 1.0).abs() * centroid_size;
                    let pan_motion = (self.accum_pan.x * self.accum_pan.x
                        + self.accum_pan.y * self.accum_pan.y)
                        .sqrt();
                    if zoom_motion > touch_slop || pan_motion > touch_slop {
                        self.past_touch_slop = true;
                    }
                }
                if self.past_touch_slop {
                    pinch = Some((raw_zoom, center));
                    pan = Some(raw_pan);
                    self.touch_scrolled = true;
                    dirty = true;
                } else {
                    dirty = false;
                }
            }
            if self.primary_touch_id == Some(tid) {
                self.prev_touch_px = Some(pos_px);
            }
            return (dirty, pinch, pan);
        }

        if self.primary_touch_id != Some(tid) {
            return (dirty, None, None);
        }

        if let Some((pending_pos, pending_instant, _pending_tid)) = self.pending_primary {
            let dt = (web_time::Instant::now() - pending_instant).as_secs_f32();
            let dx = pos_px.0 - pending_pos.x;
            let dy = pos_px.1 - pending_pos.y;
            let dist = (dx * dx + dy * dy).sqrt();
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

    pub fn touch_ended(
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
        }

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
        swipe_right
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
        Some(MultiTouchDelta {
            zoom,
            translation,
            center: curr.avg_pos,
            num_touches: self.active_touches.len(),
        })
    }
}
