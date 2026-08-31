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

    // Internal state
    last_tap: Option<Instant>,
    press_start: Option<(Instant, Vec2)>,
    drag_start: Option<Vec2>,
    last_position: Option<Vec2>,
    last_move_time: Option<Instant>,
}

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
        }
    }

    pub fn handle_pointer(&mut self, event: &PointerEvent) {
        match event.event {
            PointerEventKind::Down(_) => {
                self.press_start = Some((Instant::now(), event.position));
                self.drag_start = Some(event.position);
                self.last_position = Some(event.position);
                self.last_move_time = Some(Instant::now());

                if let Some(last) = self.last_tap
                    && (Instant::now() - last) < Duration::from_millis(300)
                {
                    if let Some(cb) = &self.on_double_tap {
                        cb(event.position);
                    }
                    self.last_tap = None;
                }
            }
            PointerEventKind::Up(_) => {
                if let Some((start_time, start_pos)) = self.press_start {
                    let elapsed = Instant::now() - start_time;
                    let distance = ((event.position.x - start_pos.x).powi(2)
                        + (event.position.y - start_pos.y).powi(2))
                    .sqrt();

                    if elapsed < Duration::from_millis(200) && distance < 10.0 {
                        // Tap
                        if let Some(cb) = &self.on_tap {
                            cb(event.position);
                        }
                        self.last_tap = Some(Instant::now());
                    } else if distance > 50.0 {
                        // Swipe detection
                        let dx = event.position.x - start_pos.x;
                        let dy = event.position.y - start_pos.y;

                        if let Some(cb) = &self.on_swipe {
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
                    }
                }
                self.press_start = None;
                self.drag_start = None;
                self.last_position = None;
                self.last_move_time = None;
            }
            PointerEventKind::Move => {
                if let Some(start) = self.drag_start
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

                    let velocity = if let (Some(prev_time), Some(now)) =
                        (self.last_move_time, Some(Instant::now()))
                    {
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
                self.last_move_time = Some(Instant::now());

                // Long press detection
                if let Some((start_time, pos)) = self.press_start
                    && (Instant::now() - start_time) > Duration::from_millis(500)
                {
                    if let Some(cb) = &self.on_long_press {
                        cb(pos);
                    }
                    self.press_start = None; // Fire once
                }
            }
            _ => {}
        }
    }
}
