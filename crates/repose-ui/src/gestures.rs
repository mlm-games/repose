use crate::input::*;
use std::time::{Duration, Instant};

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
        }
    }

    pub fn handle_pointer(&mut self, event: &PointerEvent) {
        match event.event {
            PointerEventKind::Down(_) => {
                self.press_start = Some((Instant::now(), event.position));
                self.drag_start = Some(event.position);

                // Check for double tap
                if let Some(last) = self.last_tap {
                    if last.elapsed() < Duration::from_millis(300) {
                        if let Some(cb) = &self.on_double_tap {
                            cb(event.position);
                        }
                        self.last_tap = None;
                        return;
                    }
                }
            }
            PointerEventKind::Up(_) => {
                if let Some((start_time, start_pos)) = self.press_start {
                    let elapsed = start_time.elapsed();
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
                            } else {
                                if dy > 0.0 {
                                    SwipeDirection::Down
                                } else {
                                    SwipeDirection::Up
                                }
                            };
                            cb(dir);
                        }
                    }
                }
                self.press_start = None;
                self.drag_start = None;
            }
            PointerEventKind::Move => {
                if let Some(start) = self.drag_start {
                    if let Some(cb) = &self.on_drag {
                        cb(DragEvent {
                            start,
                            current: event.position,
                            delta: Vec2 {
                                x: event.position.x - start.x,
                                y: event.position.y - start.y,
                            },
                            velocity: Vec2::default(), // Calculate from history
                        });
                    }
                }

                // Long press detection
                if let Some((start_time, pos)) = self.press_start {
                    if start_time.elapsed() > Duration::from_millis(500) {
                        if let Some(cb) = &self.on_long_press {
                            cb(pos);
                        }
                        self.press_start = None; // Fire once
                    }
                }
            }
            _ => {}
        }
    }
}
