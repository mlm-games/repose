use repose_core::Color;
use repose_core::{
    animation::{AnimatedValue, AnimationSpec},
    remember_state_with_key,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Animate f32 to the given target; returns the current value each frame.
pub fn animate_f32(key: impl Into<String>, target: f32, spec: AnimationSpec) -> f32 {
    let key = key.into();
    let anim = remember_state_with_key(format!("anim:f32:{key}"), || {
        RefCell::new(AnimatedValue::new(target, spec))
    });
    {
        let a_ref = anim.borrow_mut();

        let mut a = a_ref.borrow_mut();
        if *a.get() != target {
            a.set_target(target);
        }
        a.update();
        *a.get()
    }
}

/// Animate Color to the given target; returns the current value each frame.
pub fn animate_color(key: impl Into<String>, target: Color, spec: AnimationSpec) -> Color {
    let key = key.into();
    let anim = remember_state_with_key(format!("anim:color:{key}"), || {
        RefCell::new(AnimatedValue::new(target, spec))
    });
    {
        let a_ref = anim.borrow_mut();
        let mut a = a_ref.borrow_mut();
        if *a.get() != target {
            a.set_target(target);
        }
        a.update();
        *a.get()
    }
}
