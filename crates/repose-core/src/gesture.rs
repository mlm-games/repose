use crate::Vec2;
use crate::animation::{AnimatedValue, AnimationSpec};
use crate::frame_clock::request_frame;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use web_time::Instant;

/// Axis for drag/swipe gestures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DragOrientation {
    Horizontal,
    Vertical,
}

/// Configuration for a [`SwipeableState`].
#[derive(Clone)]
pub struct SwipeableConfig {
    /// How the offset animates to the target anchor on release (default: spring_gentle).
    pub animation_spec: AnimationSpec,
    /// Fractional threshold between anchors for positional snapping (0.0–1.0).
    /// 0.3 means the swipe must cross 30% of the gap to snap to the next anchor.
    pub positional_threshold: f32,
    /// Velocity threshold in px/s. If the release velocity exceeds this,
    /// the offset snaps to the anchor in the direction of movement regardless
    /// of positional threshold. Set to `f32::MAX` to disable velocity-based snapping.
    pub velocity_threshold: f32,
    /// Maximum offset past the first/last anchor (resistance). 0 = no limit.
    pub max_overshoot: f32,
}

impl Default for SwipeableConfig {
    fn default() -> Self {
        Self {
            animation_spec: AnimationSpec::spring_gentle(),
            positional_threshold: 0.3,
            velocity_threshold: f32::MAX,
            max_overshoot: 0.0,
        }
    }
}

/// A generic swipeable state that tracks an animated offset and snaps to
/// the nearest anchor point on release.
///
/// # Type Parameters
///
/// * `T` - The anchor value type (e.g. `DismissValue`, `usize` for pager pages).
///
/// # Usage
///
/// ```ignore
/// use repose_core::*;
///
/// let state = SwipeableState::new(
///     vec![(0.0, "start"), (-200.0, "end")],
///     SwipeableConfig::default(),
/// );
///
/// // Wire pointer events using on_pointer_down/move/up modifier callbacks:
/// // state.on_pointer_down(e.position.x)
/// // state.on_pointer_move(e.position.x)
/// // state.on_pointer_up()
///
/// // Read offset each frame:
/// let offset = state.offset();
/// ```
pub struct SwipeableState<T: Clone + PartialEq + 'static> {
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    anchors: Rc<Vec<(f32, T)>>,
    config: Rc<SwipeableConfig>,
    drag_start: Rc<Cell<Option<f32>>>,
    drag_base: Rc<Cell<f32>>,
    last_move_time: Rc<Cell<Option<Instant>>>,
    last_move_pos: Rc<Cell<Option<f32>>>,
    release_velocity: Rc<Cell<f32>>,
}

impl<T: Clone + PartialEq + 'static> SwipeableState<T> {
    /// Create a new swipeable state.
    ///
    /// `anchors` - pairs of `(offset_px, value)` sorted by offset. The first anchor
    /// is the initial position. At least one anchor is required.
    pub fn new(anchors: Vec<(f32, T)>, config: SwipeableConfig) -> Self {
        assert!(
            !anchors.is_empty(),
            "SwipeableState requires at least one anchor"
        );
        let initial_offset = anchors[0].0;
        Self {
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                initial_offset,
                config.animation_spec.clone(),
            ))),
            anchors: Rc::new(anchors),
            config: Rc::new(config),
            drag_start: Rc::new(Cell::new(None)),
            drag_base: Rc::new(Cell::new(initial_offset)),
            last_move_time: Rc::new(Cell::new(None)),
            last_move_pos: Rc::new(Cell::new(None)),
            release_velocity: Rc::new(Cell::new(0.0)),
        }
    }

    /// Current animated offset in pixels. Call every frame that needs the value.
    /// Requests a re-draw while the spring is still running.
    pub fn offset(&self) -> f32 {
        let mut anim = self.anim.borrow_mut();
        if anim.update() {
            request_frame();
        }
        *anim.get()
    }

    /// Resolve the nearest anchor value from the current offset.
    pub fn current_value(&self) -> T {
        let off = *self.anim.borrow().get();
        self.nearest_anchor(off).map(|(_, v)| v).unwrap_or_else(|| {
            self.anchors
                .first()
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("SwipeableState has no anchors"))
        })
    }

    /// Snap to an absolute offset instantly (used during active drag).
    pub fn snap_to(&self, off: f32) {
        let clamped = self.clamp_offset(off);
        self.anim.borrow_mut().snap_to(clamped);
        request_frame();
    }

    /// Animate to the offset corresponding to a target value.
    pub fn animate_to(&self, value: &T) {
        if let Some((offset, _)) = self.anchors.iter().find(|(_, v)| v == value) {
            self.anim.borrow_mut().set_target(*offset);
            request_frame();
        }
    }

    /// Returns true if the spring is still running.
    pub fn is_animating(&self) -> bool {
        self.anim.borrow().is_animating()
    }

    /// Call from `on_pointer_down` with the position along the drag axis.
    pub fn on_pointer_down(&self, axis_pos: f32) {
        self.drag_start.set(Some(axis_pos));
        self.drag_base.set(*self.anim.borrow().get());
        self.last_move_time.set(Some(Instant::now()));
        self.last_move_pos.set(Some(axis_pos));
        self.release_velocity.set(0.0);
    }

    /// Call from `on_pointer_move` with the position along the drag axis.
    pub fn on_pointer_move(&self, axis_pos: f32) {
        if let Some(start) = self.drag_start.get() {
            let now = Instant::now();
            let delta = axis_pos - start;
            let new_offset = self.drag_base.get() + delta;

            // Compute velocity from the last frame
            if let (Some(last_pos), Some(last_time)) =
                (self.last_move_pos.get(), self.last_move_time.get())
            {
                let dt = (now - last_time).as_secs_f32().max(1.0 / 240.0);
                let vel = (axis_pos - last_pos) / dt;
                self.release_velocity.set(vel);
            }

            self.snap_to(new_offset);
            self.last_move_pos.set(Some(axis_pos));
            self.last_move_time.set(Some(now));
        }
    }

    /// Call from `on_pointer_up`. Snaps to the appropriate anchor based on
    /// position and velocity.
    pub fn on_pointer_up(&self) {
        self.drag_start.set(None);
        let off = *self.anim.borrow().get();
        let vel = self.release_velocity.get();
        let config = &*self.config;

        // If velocity exceeds threshold, snap in the direction of movement.
        if vel.abs() > config.velocity_threshold {
            let target_off = if vel < 0.0 {
                self.next_anchor_down(off)
            } else {
                self.next_anchor_up(off)
            };
            self.anim.borrow_mut().set_target(target_off);
            request_frame();
            return;
        }

        // Otherwise, find the anchor pair surrounding the current offset and
        // check the fractional threshold.
        let target_off = self.snap_target(off, config.positional_threshold);
        self.anim.borrow_mut().set_target(target_off);
        request_frame();
    }

    /// Clamp offset to the anchor range (with optional overshoot).
    fn clamp_offset(&self, off: f32) -> f32 {
        let min = self.anchors.first().map(|(o, _)| *o).unwrap_or(0.0);
        let max = self.anchors.last().map(|(o, _)| *o).unwrap_or(0.0);
        let lo = if min <= max {
            min - self.config.max_overshoot
        } else {
            max - self.config.max_overshoot
        };
        let hi = if min <= max {
            max + self.config.max_overshoot
        } else {
            min + self.config.max_overshoot
        };
        off.clamp(lo.min(hi), lo.max(hi))
    }

    /// Find the anchor value nearest to a given offset.
    fn nearest_anchor(&self, off: f32) -> Option<(f32, T)> {
        self.anchors
            .iter()
            .min_by(|(a, _), (b, _)| (a - off).abs().partial_cmp(&(b - off).abs()).unwrap())
            .map(|(o, v)| (*o, v.clone()))
    }

    /// Next anchor to the left (more negative).
    fn next_anchor_down(&self, off: f32) -> f32 {
        self.anchors
            .iter()
            .rev()
            .find(|(a, _)| *a < off)
            .map(|(o, _)| *o)
            .unwrap_or_else(|| self.anchors.last().map(|(o, _)| *o).unwrap_or(off))
    }

    /// Next anchor to the right (more positive).
    fn next_anchor_up(&self, off: f32) -> f32 {
        self.anchors
            .iter()
            .find(|(a, _)| *a > off)
            .map(|(o, _)| *o)
            .unwrap_or_else(|| self.anchors.first().map(|(o, _)| *o).unwrap_or(off))
    }

    /// Determine the snap target based on positional fraction between anchors.
    fn snap_target(&self, off: f32, threshold: f32) -> f32 {
        // Find the surrounding anchor pair
        let upper = self.anchors.iter().find(|(a, _)| *a >= off);
        let lower = self.anchors.iter().rev().find(|(a, _)| *a <= off);

        match (lower, upper) {
            (Some((lo, _)), Some((hi, _))) if lo != hi => {
                // Fraction between lo and hi
                let range = hi - lo;
                if range == 0.0 {
                    *lo
                } else {
                    let fraction = (off - lo) / range;
                    if fraction >= threshold { *hi } else { *lo }
                }
            }
            (Some((lo, _)), _) => {
                // Past the minimum anchor - snap to it
                *lo
            }
            (_, Some((hi, _))) => {
                // Past the maximum anchor - snap to it
                *hi
            }
            _ => off,
        }
    }
}

impl<T: Clone + PartialEq + 'static> Clone for SwipeableState<T> {
    fn clone(&self) -> Self {
        Self {
            anim: self.anim.clone(),
            anchors: self.anchors.clone(),
            config: self.config.clone(),
            drag_start: self.drag_start.clone(),
            drag_base: self.drag_base.clone(),
            last_move_time: self.last_move_time.clone(),
            last_move_pos: self.last_move_pos.clone(),
            release_velocity: self.release_velocity.clone(),
        }
    }
}
