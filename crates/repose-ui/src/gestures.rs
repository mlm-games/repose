use repose_core::Vec2;

use crate::input::*;
use std::rc::Rc;
use web_time::{Duration, Instant};

pub struct GestureDetector {
    on_tap: Option<Rc<dyn Fn(Vec2)>>,
    on_double_tap: Option<Rc<dyn Fn(Vec2)>>,
    on_long_press: Option<Rc<dyn Fn(Vec2)>>,
    on_drag: Option<Rc<dyn Fn(DragEvent)>>,
    on_swipe: Option<Rc<dyn Fn(SwipeDirection)>>,

    last_tap: Option<(Instant, Vec2)>,
    press_start: Option<(Instant, Vec2)>,
    drag_start: Option<Vec2>,
    last_position: Option<Vec2>,
    last_move_time: Option<Instant>,
    drag_past_slop: bool,
    long_press_fired: bool,
}

const DOUBLE_TAP_MS: u64 = 300;
const DOUBLE_TAP_SLOP_PX: f32 = 24.0;
const TAP_MAX_MS: u64 = 200;
const TAP_SLOP_PX: f32 = 10.0;
const DRAG_SLOP_PX: f32 = 8.0;
const LONG_PRESS_MS: u64 = 500;
const LONG_PRESS_SLOP_PX: f32 = 12.0;
const SWIPE_MIN_PX: f32 = 50.0;

pub struct DragEvent {
    pub start: Vec2,
    pub current: Vec2,
    pub delta: Vec2,
    pub velocity: Vec2,
}

pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

impl Default for GestureDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GestureDetector {
    pub fn new() -> Self {
        Self {
            on_tap: None,
            on_double_tap: None,
            on_long_press: None,
            on_drag: None,
            on_swipe: None,
            last_tap: None,
            press_start: None,
            drag_start: None,
            last_position: None,
            last_move_time: None,
            drag_past_slop: false,
            long_press_fired: false,
        }
    }

    pub fn handle_pointer(&mut self, event: &PointerEvent) {
        let now = Instant::now();
        self.poll_long_press_at(now);
        match event.event {
            PointerEventKind::Down(_) => {
                self.press_start = Some((now, event.position));
                self.drag_start = Some(event.position);
                self.last_position = Some(event.position);
                self.last_move_time = Some(now);
                self.drag_past_slop = false;
                self.long_press_fired = false;

                if let Some((last, last_pos)) = self.last_tap {
                    let dt = now - last;
                    let dist = ((event.position.x - last_pos.x).powi(2)
                        + (event.position.y - last_pos.y).powi(2))
                    .sqrt();
                    if dt < Duration::from_millis(DOUBLE_TAP_MS) && dist < DOUBLE_TAP_SLOP_PX {
                        if let Some(cb) = &self.on_double_tap {
                            cb(event.position);
                        }
                        self.last_tap = None;
                    }
                }
            }
            PointerEventKind::Up(_) => {
                if let Some((start_time, start_pos)) = self.press_start {
                    let elapsed = now - start_time;
                    let distance = ((event.position.x - start_pos.x).powi(2)
                        + (event.position.y - start_pos.y).powi(2))
                    .sqrt();

                    if !self.drag_past_slop
                        && !self.long_press_fired
                        && elapsed < Duration::from_millis(TAP_MAX_MS)
                        && distance < TAP_SLOP_PX
                    {
                        if let Some(cb) = &self.on_tap {
                            cb(event.position);
                        }
                        self.last_tap = Some((now, event.position));
                    } else if !self.long_press_fired && distance > SWIPE_MIN_PX {
                        let dx = event.position.x - start_pos.x;
                        let dy = event.position.y - start_pos.y;
                        let dominant = dx.abs() > dy.abs() * 1.5 || dy.abs() > dx.abs() * 1.5;
                        if dominant && let Some(cb) = &self.on_swipe {
                            let dir = if dx.abs() > dy.abs() {
                                if dx > 0.0 {
                                    SwipeDirection::Right
                                } else {
                                    SwipeDirection::Left
                                }
                            } else if dy > 0.0 {
                                SwipeDirection::Down
                            } else {
                                SwipeDirection::Up
                            };
                            cb(dir);
                        }
                        self.last_tap = None;
                    } else {
                        self.last_tap = None;
                    }
                }
                self.press_start = None;
                self.drag_start = None;
                self.last_position = None;
                self.last_move_time = None;
                self.drag_past_slop = false;
                self.long_press_fired = false;
            }
            PointerEventKind::Move => {
                if let Some(start) = self.drag_start {
                    let total = ((event.position.x - start.x).powi(2)
                        + (event.position.y - start.y).powi(2))
                    .sqrt();
                    if total >= DRAG_SLOP_PX {
                        self.drag_past_slop = true;
                    }
                }
                if self.drag_past_slop
                    && let Some(start) = self.drag_start
                    && let Some(cb) = &self.on_drag
                {
                    let delta = if let Some(prev) = self.last_position {
                        Vec2 {
                            x: event.position.x - prev.x,
                            y: event.position.y - prev.y,
                        }
                    } else {
                        Vec2::default()
                    };

                    let velocity = if let Some(prev_time) = self.last_move_time {
                        let dt = (now - prev_time).as_secs_f32().max(1.0 / 240.0);
                        Vec2 {
                            x: delta.x / dt,
                            y: delta.y / dt,
                        }
                    } else {
                        Vec2::default()
                    };

                    cb(DragEvent {
                        start,
                        current: event.position,
                        delta,
                        velocity,
                    });
                }

                self.last_position = Some(event.position);
                self.last_move_time = Some(now);

                self.poll_long_press_at(now);
            }
            PointerEventKind::Cancel => {
                self.last_tap = None;
                self.press_start = None;
                self.drag_start = None;
                self.last_position = None;
                self.last_move_time = None;
                self.drag_past_slop = false;
                self.long_press_fired = false;
            }
            _ => {}
        }
    }

    /// Fire long-press when the press outlasts the timeout without moving
    /// past slop. Called on every pointer event and available for per-frame
    /// polling so a stationary hold (no Moves) still fires.
    pub fn poll_long_press(&mut self) {
        self.poll_long_press_at(Instant::now());
    }

    fn poll_long_press_at(&mut self, now: Instant) {
        if self.long_press_fired {
            return;
        }
        if let Some((start_time, pos)) = self.press_start
            && (now - start_time) > Duration::from_millis(LONG_PRESS_MS)
            && !self.drag_past_slop
        {
            let cur = self.last_position.unwrap_or(pos);
            let dist = ((cur.x - pos.x).powi(2) + (cur.y - pos.y).powi(2)).sqrt();
            if dist <= LONG_PRESS_SLOP_PX {
                if let Some(cb) = &self.on_long_press {
                    cb(pos);
                }
                self.long_press_fired = true;
                self.last_tap = None;
            }
        }
    }
}
