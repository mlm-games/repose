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
                config.animation_spec,
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

    /// All registered `(offset, value)` anchors in order.
    pub fn anchors(&self) -> Vec<(f32, T)> {
        self.anchors.iter().cloned().collect()
    }

    /// Offset registered for an anchor value, if present.
    pub fn anchor_offset(&self, value: &T) -> Option<f32> {
        self.anchors.iter().find(|(_, v)| v == value).map(|(o, _)| *o)
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

pub struct DraggableConfig {
    pub orientation: Option<DragOrientation>,
    pub enabled: bool,
    pub reverse_direction: bool,
    pub start_drag_immediately: bool,
    pub slop_px: f32,
}

impl Default for DraggableConfig {
    fn default() -> Self {
        Self {
            orientation: None,
            enabled: true,
            reverse_direction: false,
            start_drag_immediately: false,
            slop_px: 18.0,
        }
    }
}

impl DragOrientation {
    pub fn horizontal() -> DraggableConfig {
        DraggableConfig {
            orientation: Some(DragOrientation::Horizontal),
            ..Default::default()
        }
    }

    pub fn vertical() -> DraggableConfig {
        DraggableConfig {
            orientation: Some(DragOrientation::Vertical),
            ..Default::default()
        }
    }
}

pub struct TransformableState {
    on_transform: Rc<dyn Fn(f32, crate::Vec2, f32)>,
    enabled: Cell<bool>,
    lock_rotation_on_zoom_pan: Cell<bool>,
    last_pinch: Cell<Option<Instant>>,
}

impl TransformableState {
    pub fn new(on_transform: impl Fn(f32, crate::Vec2, f32) + 'static) -> Self {
        Self {
            on_transform: Rc::new(on_transform),
            enabled: Cell::new(true),
            lock_rotation_on_zoom_pan: Cell::new(false),
            last_pinch: Cell::new(None),
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
    }

    pub fn set_lock_rotation_on_zoom_pan(&self, lock: bool) {
        self.lock_rotation_on_zoom_pan.set(lock);
    }

    pub fn handle_gesture(&self, gesture: &crate::shortcuts::Gesture) -> bool {
        if !self.enabled.get() {
            return false;
        }
        match gesture {
            crate::shortcuts::Gesture::PinchWithCenter { delta_scale, .. } => {
                self.last_pinch.set(Some(Instant::now()));
                (self.on_transform)(*delta_scale, crate::Vec2::ZERO, 0.0);
                request_frame();
                true
            }
            crate::shortcuts::Gesture::Rotate { delta_rotation, .. } => {
                if self.lock_rotation_on_zoom_pan.get()
                    && let Some(t) = self.last_pinch.get()
                    && t.elapsed().as_millis() < 120
                {
                    return true;
                }
                (self.on_transform)(1.0, crate::Vec2::ZERO, *delta_rotation);
                request_frame();
                true
            }
            crate::shortcuts::Gesture::Pan { delta, .. } => {
                (self.on_transform)(1.0, *delta, 0.0);
                request_frame();
                true
            }
            _ => false,
        }
    }
}

impl Clone for TransformableState {
    fn clone(&self) -> Self {
        Self {
            on_transform: self.on_transform.clone(),
            enabled: Cell::new(self.enabled.get()),
            lock_rotation_on_zoom_pan: Cell::new(self.lock_rotation_on_zoom_pan.get()),
            last_pinch: Cell::new(self.last_pinch.get()),
        }
    }
}

pub struct AnchoredDraggableState<T: Clone + PartialEq + 'static> {
    inner: SwipeableState<T>,
    config: SwipeableConfig,
    confirm_value_change: Rc<RefCell<Option<Rc<dyn Fn(T) -> bool>>>>,
    target: Rc<RefCell<Option<T>>>,
}

impl<T: Clone + PartialEq + 'static> AnchoredDraggableState<T> {
    pub fn new(
        anchors: Vec<(f32, T)>,
        config: SwipeableConfig,
        confirm_value_change: Option<Rc<dyn Fn(T) -> bool>>,
    ) -> Self {
        let target = anchors.first().map(|(_, v)| v.clone());
        Self {
            inner: SwipeableState::new(anchors, config.clone()),
            config,
            confirm_value_change: Rc::new(RefCell::new(confirm_value_change)),
            target: Rc::new(RefCell::new(target)),
        }
    }

    pub fn update_anchors(&mut self, anchors: Vec<(f32, T)>) {
        let current = self.inner.offset();
        self.inner = SwipeableState::new(anchors, self.config.clone());
        self.inner.snap_to(current);
    }

    pub fn anchors(&self) -> Vec<(f32, T)> {
        self.inner.anchors()
    }

    pub fn anchor_offset_for(&self, value: &T) -> Option<f32> {
        self.inner.anchor_offset(value)
    }



    pub fn offset(&self) -> f32 {
        self.inner.offset()
    }

    pub fn current_value(&self) -> T {
        self.inner.current_value()
    }

    pub fn target_value(&self) -> Option<T> {
        self.target.borrow().clone()
    }

    pub fn is_animating(&self) -> bool {
        self.inner.is_animating()
    }

    pub fn snap_to(&self, value: &T) {
        if self.confirm_value_change.borrow().as_ref().is_some_and(|f| !f(value.clone())) {
            return;
        }
        if let Some(offset) = self.inner.anchor_offset(value) {
            *self.target.borrow_mut() = Some(value.clone());
            self.inner.snap_to(offset);
        }
    }

    pub fn animate_to(&self, value: &T) {
        if self.confirm_value_change.borrow().as_ref().is_some_and(|f| !f(value.clone())) {
            return;
        }
        *self.target.borrow_mut() = Some(value.clone());
        self.inner.animate_to(value);
    }

    pub fn on_pointer_down(&self, axis_pos: f32) {
        self.inner.on_pointer_down(axis_pos);
    }

    pub fn on_pointer_move(&self, axis_pos: f32) {
        self.inner.on_pointer_move(axis_pos);
    }

    pub fn on_pointer_up(&self) {
        let candidate = self.inner.current_value();
        if self
            .confirm_value_change
            .borrow()
            .as_ref()
            .is_some_and(|f| !f(candidate.clone()))
        {
            let settled = self.target.borrow().clone();
            if let Some(settled) = settled {
                self.inner.animate_to(&settled);
            }
            return;
        }
        *self.target.borrow_mut() = Some(candidate);
        self.inner.on_pointer_up();
    }
}

impl<T: Clone + PartialEq + 'static> Clone for AnchoredDraggableState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            config: self.config.clone(),
            confirm_value_change: self.confirm_value_change.clone(),
            target: self.target.clone(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::HitRegion;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn test_transformable_forwards_deltas() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let seen_pan = seen.clone();
        let state = TransformableState::new(move |zoom, pan, rotation| {
            seen_pan.borrow_mut().push((zoom, pan.x, rotation));
        });
        assert!(state.handle_gesture(&crate::shortcuts::Gesture::PinchWithCenter {
            delta_scale: 2.0,
            center: crate::Vec2 { x: 1.0, y: 1.0 },
        }));
        assert!(state.handle_gesture(&crate::shortcuts::Gesture::Pan {
            delta: crate::Vec2 { x: 3.0, y: 4.0 },
            center: crate::Vec2 { x: 0.0, y: 0.0 },
        }));
        assert!(!state.handle_gesture(&crate::shortcuts::Gesture::SwipeLeft));
        assert_eq!(seen.borrow().len(), 2);
        assert_eq!(seen.borrow()[0].0, 2.0);
        assert_eq!(seen.borrow()[1].1, 3.0);
    }

    #[test]
    fn test_anchored_draggable_vetoes() {
        let state = AnchoredDraggableState::new(
            vec![(0.0, "a"), (100.0, "b")],
            SwipeableConfig::default(),
            Some(Rc::new(|v: &'static str| v != "b")),
        );
        state.animate_to(&"b");
        assert_eq!(state.target_value(), Some("a"));
        state.animate_to(&"a");
        assert_eq!(state.target_value(), Some("a"));
        assert_eq!(state.anchors().len(), 2);
        assert_eq!(state.anchor_offset_for(&"b"), Some(100.0));
    }

    #[test]
    fn test_route_action_to_point_walks_up() {
        let fired = Rc::new(RefCell::new(Vec::new()));
        let fired_child = fired.clone();
        let mut child = HitRegion::default();
        child.id = 2;
        child.parent = Some(1);
        child.rect = crate::Rect { x: 0.0, y: 0.0, w: 10.0, h: 10.0 };
        child.z_index = 2.0;
        child.on_action = Some(Rc::new(move |_| {
            fired_child.borrow_mut().push(2u64);
            false
        }));
        let mut parent = HitRegion::default();
        parent.id = 1;
        parent.rect = crate::Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        parent.z_index = 1.0;
        parent.on_action = Some(Rc::new(move |_| true));
        let hits = vec![parent, child];
        let action = crate::shortcuts::Action::Gesture(crate::shortcuts::Gesture::PinchWithCenter {
            delta_scale: 1.0,
            center: crate::Vec2 { x: 5.0, y: 5.0 },
        });
        assert!(crate::runtime::route_action_to_point(
            &hits,
            &action,
            crate::Vec2 { x: 5.0, y: 5.0 }
        ));
        assert_eq!(*fired.borrow(), vec![2u64]);
        assert!(!crate::runtime::route_action_to_point(
            &hits,
            &action,
            crate::Vec2 { x: 500.0, y: 500.0 }
        ));
    }
}
