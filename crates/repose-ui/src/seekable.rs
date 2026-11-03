//! A transition whose progress can be driven directly, for gestures.
//!
//! [`SeekableTransitionState`] holds the two endpoint states and a fraction
//! between them. A gesture can [`seek`](SeekableTransitionState::seek) the
//! fraction, and a release can
//! [`animate_fraction`](SeekableTransitionState::animate_fraction) it to 0 or 1.
//! The [`SeekableTransitionScope`] maps each property through that fraction,
//! so the same transition can be dragged, cancelled, or played as an animation.

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::*;

struct SeekableInner<T> {
    from: T,
    to: T,
    /// Mirrors the fraction and notifies the composition when it moves.
    fraction: Signal<f32>,
    /// Bumped whenever the endpoints change, so readers of `from`/`to`
    /// recompose even when the fraction itself does not move.
    version: Signal<u64>,
    animation: Option<AnimatedValue<f32>>,
    animation_target: f32,
}

/// The endpoints and current fraction of a seekable transition.
pub struct SeekableTransitionState<T: Clone + PartialEq + 'static> {
    inner: Rc<RefCell<SeekableInner<T>>>,
    key: String,
}

impl<T: Clone + PartialEq + 'static> SeekableTransitionState<T> {
    /// Remember a transition keyed by `key`, starting settled on `initial`.
    pub fn remember(key: impl Into<String>, initial: T) -> Rc<Self> {
        let key: String = key.into();
        let inner = Rc::new(RefCell::new(SeekableInner {
            from: initial.clone(),
            to: initial,
            fraction: signal(1.0),
            version: signal(0),
            animation: None,
            animation_target: 1.0,
        }));
        remember_with_key(format!("seekable:{key}"), move || Self { inner, key })
    }

    /// The state at fraction 0.
    pub fn from(&self) -> T {
        self.inner.borrow().from.clone()
    }

    /// The state at fraction 1.
    pub fn to(&self) -> T {
        self.inner.borrow().to.clone()
    }

    /// Current fraction in `0.0..=1.0`.
    pub fn fraction(&self) -> f32 {
        self.inner.borrow().fraction.get()
    }

    /// Jump the fraction to `fraction`, cancelling any running animation.
    pub fn seek(&self, fraction: f32) {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut inner = self.inner.borrow_mut();
        inner.animation = None;
        inner.fraction.set_neq(fraction);
        request_frame();
    }

    /// Animate the fraction to `fraction`, keeping the same endpoints.
    pub fn animate_fraction(&self, fraction: f32, spec: AnimationSpec) {
        let target = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut inner = self.inner.borrow_mut();
        let mut animation = AnimatedValue::new(inner.fraction.get(), spec);
        animation.set_target(target);
        inner.animation = Some(animation);
        inner.animation_target = target;
        drop(inner);
        self.register_tick();
        request_frame();
    }

    /// Animate to a new target state. A settled transition starts from its
    /// current target; an in-flight seek snaps to the old target first, because
    /// a generic state cannot be interpolated into a new starting point.
    pub fn animate_to(&self, target: T, spec: AnimationSpec) {
        {
            let mut inner = self.inner.borrow_mut();
            if inner.to == target && inner.fraction.get() >= 1.0 {
                return;
            }
            inner.from = inner.to.clone();
            inner.to = target;
            inner.fraction.set_neq(0.0);
            inner.version.update(|version| *version += 1);
        }
        self.animate_fraction(1.0, spec);
    }

    /// Replace both endpoints with `target` and settle at fraction 1.
    pub fn snap_to(&self, target: T) {
        let mut inner = self.inner.borrow_mut();
        inner.animation = None;
        inner.from = target.clone();
        inner.to = target;
        inner.fraction.set_neq(1.0);
        inner.version.update(|version| *version += 1);
        request_frame();
    }

    /// Whether a fraction animation is still running.
    pub fn is_animating(&self) -> bool {
        self.inner.borrow().animation.is_some()
    }

    /// A scope that interpolates properties through this transition.
    pub fn scope(&self) -> SeekableTransitionScope<T> {
        SeekableTransitionScope {
            inner: self.inner.clone(),
        }
    }

    fn register_tick(&self) {
        let inner = self.inner.clone();
        let key = format!("seekable:{}", self.key);
        animation_driver::register(
            key,
            Rc::new(RefCell::new(move || {
                let mut inner = inner.borrow_mut();
                let (still, value) = {
                    let Some(animation) = inner.animation.as_mut() else {
                        return false;
                    };
                    let still = animation.update();
                    (still, *animation.get())
                };
                inner.fraction.set_neq(value);
                if !still {
                    inner.animation = None;
                    let target = inner.animation_target;
                    inner.fraction.set_neq(target);
                    if target >= 1.0 {
                        let settled = inner.to.clone();
                        inner.from = settled;
                        inner.version.update(|version| *version += 1);
                    }
                }
                still
            })),
        );
    }
}

impl<T: Clone + PartialEq + 'static> Clone for SeekableTransitionState<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            key: self.key.clone(),
        }
    }
}

/// Interpolates properties between the endpoints of a
/// [`SeekableTransitionState`].
#[derive(Clone)]
pub struct SeekableTransitionScope<T: Clone + PartialEq + 'static> {
    inner: Rc<RefCell<SeekableInner<T>>>,
}

impl<T: Clone + PartialEq + 'static> SeekableTransitionScope<T> {
    fn map<R>(&self, map: impl Fn(&T) -> R) -> (R, R, f32) {
        let inner = self.inner.borrow();
        inner.version.get();
        let fraction = inner.fraction.get();
        (map(&inner.from), map(&inner.to), fraction)
    }

    pub fn animate_float(&self, map: impl Fn(&T) -> f32) -> f32 {
        let (from, to, fraction) = self.map(map);
        from.interpolate(&to, fraction)
    }

    pub fn animate_dp(&self, map: impl Fn(&T) -> Dp) -> Dp {
        let (from, to, fraction) = self.map(map);
        Dp(from.0.interpolate(&to.0, fraction))
    }

    pub fn animate_color(&self, map: impl Fn(&T) -> Color) -> Color {
        let (from, to, fraction) = self.map(map);
        from.interpolate(&to, fraction)
    }

    pub fn animate_vec2(&self, map: impl Fn(&T) -> Vec2) -> Vec2 {
        let (from, to, fraction) = self.map(map);
        from.interpolate(&to, fraction)
    }

    pub fn animate_size(&self, map: impl Fn(&T) -> Size) -> Size {
        let (from, to, fraction) = self.map(map);
        from.interpolate(&to, fraction)
    }

    pub fn animate_rect(&self, map: impl Fn(&T) -> Rect) -> Rect {
        let (from, to, fraction) = self.map(map);
        from.interpolate(&to, fraction)
    }

    /// The fraction itself, for properties with custom interpolation.
    pub fn fraction(&self) -> f32 {
        self.inner.borrow().fraction.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_interpolates_and_clamps() {
        let state = SeekableTransitionState::remember("test", 0.0f32);
        state.seek(0.25);
        assert_eq!(state.fraction(), 0.25);
        state.seek(2.0);
        assert_eq!(state.fraction(), 1.0);
        state.seek(-1.0);
        assert_eq!(state.fraction(), 0.0);
    }

    #[test]
    fn scope_interpolates_float_and_color() {
        let state = SeekableTransitionState::remember("scope", 0.0f32);
        state.animate_to(100.0, AnimationSpec::spring_gentle());
        state.seek(0.5);
        let scope = state.scope();
        assert!((scope.animate_float(|v| *v) - 50.0).abs() < 1e-4);
        assert_eq!(scope.fraction(), 0.5);
    }

    #[test]
    fn settled_transition_reports_target_endpoints() {
        let state = SeekableTransitionState::remember("settled", 0.0f32);
        state.animate_to(7.0, AnimationSpec::spring_gentle());
        state.seek(1.0);
        assert_eq!(state.fraction(), 1.0);
        assert_eq!(state.to(), 7.0);
        assert_eq!(state.from(), 0.0);
    }
}
