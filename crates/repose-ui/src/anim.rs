use repose_core::Color;
use repose_core::{
    animation::{AnimatedValue, AnimationSpec},
    remember_state_with_key, request_frame,
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

    let still_animating = a.update();
    if still_animating {
        request_frame();
    }

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
    let last = remember_state_with_key(format!("anim:color_last:{key}"), || None::<Color>);

    let mut a = anim.borrow_mut();
    let mut lt = last.borrow_mut();
    if lt.is_none() || lt.as_ref().unwrap() != &target {
        a.set_spec(spec);
        a.set_target(target);
        *lt = Some(target);
    }
    drop(lt);

    let still_animating = a.update();
    if still_animating {
        request_frame();
    }

    *a.get()
}

/// Animate Color to the given target; starts at the target on first mount (legacy behavior).
pub fn animate_color(key: impl Into<String>, target: Color, spec: AnimationSpec) -> Color {
    animate_color_from(key, target, target, spec)
}
