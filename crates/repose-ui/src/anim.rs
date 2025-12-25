use repose_core::Color;
use repose_core::{
    animation::{AnimatedValue, AnimationSpec},
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

    let mut a = anim.borrow_mut();
    let cur = *a.get();
    if (cur - target).abs() > 1e-3 {
        a.set_target(target);
    }
    a.update();
    *a.get()
}

/// Animate f32 to the given target; starts at the target on first mount (legacy behavior).
pub fn animate_f32(key: impl Into<String>, target: f32, spec: AnimationSpec) -> f32 {
    animate_f32_from(key, target, target, spec)
}

/// Animate Color from an explicit initial value to a target.
pub fn animate_color_from(
    key: impl Into<String>,
    initial: Color,
    target: Color,
    spec: AnimationSpec,
) -> Color {
    let key = key.into();
    let anim = remember_state_with_key(format!("anim:color:{key}"), || {
        AnimatedValue::new(initial, spec)
    });

    let mut a = anim.borrow_mut();
    if *a.get() != target {
        a.set_target(target);
    }
    a.update();
    *a.get()
}

/// Animate Color to the given target; starts at the target on first mount (legacy behavior).
pub fn animate_color(key: impl Into<String>, target: Color, spec: AnimationSpec) -> Color {
    animate_color_from(key, target, target, spec)
}
