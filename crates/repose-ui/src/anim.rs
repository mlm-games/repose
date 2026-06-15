use std::cell::RefCell;
use std::rc::Rc;

use repose_core::Color;
use repose_core::{
    Rect, Size, Vec2,
    animation::{AnimatedValue, AnimationSpec, KeyframesSpec, RepeatableSpec},
    remember_state_with_key,
};

/// Animate f32 from an explicit initial value to a target.
/// - On first creation for this key, starts at `initial` then animates toward `target`.
/// - On later calls, animates from the current value to the new target.
pub fn animate_f32_from(
    key: impl Into<String>,
    initial: f32,
    target: f32,
    spec: AnimationSpec,
) -> f32 {
    let key = key.into();
    let anim = remember_state_with_key(format!("anim:f32:{key}"), || {
        AnimatedValue::new(initial, spec)
    });
    let last = remember_state_with_key(format!("anim:f32_last:{key}"), || f32::NAN);

    let mut a = anim.borrow_mut();
    let mut lt = last.borrow_mut();
    let should_set_target = lt.is_nan() || (*lt - target).abs() > 1e-6;
    if should_set_target {
        a.set_spec(spec);
        a.set_target(target);
        *lt = target;
    }
    drop(lt);

    a.update();

    *a.get()
}

/// Animate f32 to the given target; starts at the target on first mount (legacy behavior).
pub fn animate_f32(key: impl Into<String>, target: f32, spec: AnimationSpec) -> f32 {
    animate_f32_from(key, target, target, spec)
}

macro_rules! animate_from_impl {
    ($name:ident, $type:ty, $prefix:expr) => {
        pub fn $name(
            key: impl Into<String>,
            initial: $type,
            target: $type,
            spec: AnimationSpec,
        ) -> $type {
            let key = key.into();
            let anim = remember_state_with_key(format!("anim:{}:{}", $prefix, key), || {
                AnimatedValue::new(initial, spec)
            });
            let last = remember_state_with_key(
                format!("anim:{}_last:{}", $prefix, key),
                || None::<$type>,
            );

            let mut a = anim.borrow_mut();
            let mut lt = last.borrow_mut();
            if lt.is_none() || lt.as_ref().unwrap() != &target {
                a.set_spec(spec);
                a.set_target(target);
                *lt = Some(target);
            }
            drop(lt);

            a.update();

            *a.get()
        }
    };
}

animate_from_impl!(animate_color_from, Color, "color");
animate_from_impl!(animate_vec2_from, Vec2, "vec2");
animate_from_impl!(animate_size_from, Size, "size");
animate_from_impl!(animate_rect_from, Rect, "rect");

/// Animate Color to the given target; starts at the target on first mount (legacy behavior).
pub fn animate_color(key: impl Into<String>, target: Color, spec: AnimationSpec) -> Color {
    animate_color_from(key, target, target, spec)
}

/// Animate Vec2 to the given target; starts at the target on first mount.
pub fn animate_vec2(key: impl Into<String>, target: Vec2, spec: AnimationSpec) -> Vec2 {
    animate_vec2_from(key, target, target, spec)
}

/// Animate Size to the given target; starts at the target on first mount.
pub fn animate_size(key: impl Into<String>, target: Size, spec: AnimationSpec) -> Size {
    animate_size_from(key, target, target, spec)
}

/// Animate Rect to the given target; starts at the target on first mount.
pub fn animate_rect(key: impl Into<String>, target: Rect, spec: AnimationSpec) -> Rect {
    animate_rect_from(key, target, target, spec)
}

/// Animate f32 through a sequence of keyframes.
///
/// The keyframe timestamps range from 0.0 to 1.0, mapped across `spec`'s duration.
/// The animation loops if `repeat` is set on the spec.
///
/// Example:
/// ```ignore
/// let val = animate_keyframes("bounce", KeyframesSpec::new(vec![
///     (0.0, 0.0),
///     (0.3, 100.0),
///     (0.6, 80.0),
///     (1.0, 100.0),
/// ]), AnimationSpec::tween(Duration::from_millis(600), Easing::EaseOut));
/// ```
pub fn animate_keyframes(
    key: impl Into<String>,
    keyframes: KeyframesSpec<f32>,
    spec: AnimationSpec,
) -> f32 {
    let key = key.into();
    let anim = remember_state_with_key(format!("anim:kf:{key}"), || AnimatedValue::new(0.0, spec));
    let mut a = anim.borrow_mut();
    if !a.has_keyframes() {
        a.set_keyframes(keyframes);
    }
    a.update();
    *a.get()
}

fn with_infinite_repeat(spec: AnimationSpec) -> AnimationSpec {
    if spec.repeat.is_none() {
        spec.repeated(RepeatableSpec::infinite().reverse())
    } else {
        spec
    }
}

/// A scoped API for continuous/repeating animations.
///
/// All child animations created via this transition loop indefinitely
/// between the given initial and target values (ping-pong with `reverse`).
///
/// If you need a custom repeat configuration, pass an `AnimationSpec` that
/// already has `.repeated(...)` set - the transition will respect it instead
/// of applying the default infinite reverse.
pub struct InfiniteTransition {
    _private: (),
}

/// Creates an `InfiniteTransition` scoped to the current composition slot.
///
/// Use its `animate_float`, `animate_color`, `animate_vec2`, `animate_size`,
/// and `animate_rect` methods for continuous looping animations.
///
/// # Example
///
/// ```ignore
/// let t = remember_infinite_transition();
/// let pulse = t.animate_float("pulse", 0.0, 1.0,
///     AnimationSpec::tween(Duration::from_millis(600), Easing::EaseInOut));
/// ```
pub fn remember_infinite_transition() -> InfiniteTransition {
    InfiniteTransition { _private: () }
}

macro_rules! inf_method {
    ($method:ident, $from_fn:ident, $type:ty) => {
        pub fn $method(
            &self,
            key: impl Into<String>,
            initial: $type,
            target: $type,
            spec: AnimationSpec,
        ) -> $type {
            let key = key.into();
            $from_fn(
                format!("inf:{}", key),
                initial,
                target,
                with_infinite_repeat(spec),
            )
        }
    };
}

impl InfiniteTransition {
    inf_method!(animate_float, animate_f32_from, f32);
    inf_method!(animate_color, animate_color_from, Color);
    inf_method!(animate_vec2, animate_vec2_from, Vec2);
    inf_method!(animate_size, animate_size_from, Size);
    inf_method!(animate_rect, animate_rect_from, Rect);
}

/// A scope for multi-target animations driven by a changing state.
///
/// When the target state changes (detected via `PartialEq`), all child
/// animations registered via `animate_float`, `animate_color`, etc.
/// automatically animate from their current value toward the new mapped
/// target value.
///
/// # Example
///
/// ```ignore
/// let t = update_transition("panel", is_expanded, AnimationSpec::spring_gentle());
/// let h  = t.animate_float("height", |e| if *e { 200.0 } else { 48.0 });
/// let bg = t.animate_color("bg", |e| if *e { theme().primary } else { theme().surface });
/// ```
pub struct TransitionScope<T> {
    key: String,
    spec: AnimationSpec,
    state: Rc<RefCell<T>>,
}

/// Creates a `TransitionScope` keyed to the given state.
///
/// Whenever `target_state` differs from the previous call (via `PartialEq`),
/// all child animation targets are updated and the transition animates toward
/// the new values.
pub fn update_transition<T>(
    key: impl Into<String>,
    target_state: T,
    spec: AnimationSpec,
) -> TransitionScope<T>
where
    T: PartialEq + Clone + 'static,
{
    let key = key.into();
    let state: Rc<RefCell<T>> =
        remember_state_with_key(format!("tr_state:{key}"), || target_state.clone());
    if *state.borrow() != target_state {
        *state.borrow_mut() = target_state;
    }
    TransitionScope { key, spec, state }
}

macro_rules! tr_method {
    ($method:ident, $fn:ident, $type:ty) => {
        pub fn $method<F>(&self, child_key: impl Into<String>, map: F) -> $type
        where
            F: Fn(&T) -> $type,
        {
            let target = map(&*self.state.borrow());
            $fn(
                format!("tr:{}:{}", self.key, child_key.into()),
                target,
                self.spec,
            )
        }
    };
}

impl<T> TransitionScope<T> {
    tr_method!(animate_float, animate_f32, f32);
    tr_method!(animate_color, animate_color, Color);
    tr_method!(animate_vec2, animate_vec2, Vec2);
    tr_method!(animate_size, animate_size, Size);
    tr_method!(animate_rect, animate_rect, Rect);
}
