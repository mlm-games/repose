use std::cell::RefCell;

use crate::{Box, Stack, ViewExt};
use repose_core::*;

use crate::anim::animate_f32;
use crate::anim::animate_f32_from;

/// Describes how content enters the screen.
#[derive(Clone, Debug)]
pub enum EnterTransition {
    /// Fade from alpha 0 to 1.
    FadeIn,
    /// Slide from the given offset (dx, dy in dp) to position (0,0).
    SlideIn { offset_x: f32, offset_y: f32 },
    /// Scale from initial to 1.0 combined with fade from 0 to 1.
    ScaleIn { initial: f32 },
    /// Composite: multiple enter transitions applied together.
    Composite(Vec<EnterTransition>),
}

impl Default for EnterTransition {
    fn default() -> Self {
        EnterTransition::FadeIn
    }
}

/// Describes how content exits the screen.
#[derive(Clone, Debug)]
pub enum ExitTransition {
    /// Fade from alpha 1 to 0.
    FadeOut,
    /// Slide from position (0,0) to the given offset (dx, dy in dp).
    SlideOut { offset_x: f32, offset_y: f32 },
    /// Scale from 1.0 to target combined with fade from 1 to 0.
    ScaleOut { target: f32 },
    /// Composite: multiple exit transitions applied together.
    Composite(Vec<ExitTransition>),
}

impl Default for ExitTransition {
    fn default() -> Self {
        ExitTransition::FadeOut
    }
}

/// Crossfades between two pieces of content when `target` changes.
///
/// When the target state changes, the old content fades out while the new
/// content fades in, with no other transforms applied.
pub fn Crossfade<T, F>(key: impl Into<String>, target: T, spec: AnimationSpec, content: F) -> View
where
    T: PartialEq + Clone + 'static,
    F: Fn(T) -> View + 'static,
{
    let key = key.into();

    let prev = remember_with_key(format!("cf_prev:{key}"), || RefCell::new(target.clone()));
    let old_content =
        remember_with_key(format!("cf_old_view:{key}"), || RefCell::new(None::<View>));
    let version = remember_with_key(format!("cf_version:{key}"), || RefCell::new(0u64));

    let is_new = *prev.borrow() != target;
    if is_new {
        let prev_view = content(prev.borrow().clone());
        old_content.borrow_mut().replace(prev_view);
        prev.borrow_mut().clone_from(&target);
        *version.borrow_mut() += 1;
    }

    let v = *version.borrow();
    let new_view = content(target.clone());

    // Exit: versioned key ensures fresh animation state per transition.
    let old_view = {
        let mut oc = old_content.borrow_mut();
        if let Some(ref ov) = *oc {
            let exit_alpha = animate_f32_from(format!("cf_exit:{key}:v{v}"), 1.0, 0.0, spec);
            if exit_alpha > 0.005 {
                Some(Box(Modifier::new().fill_max_size().alpha(exit_alpha)).child(ov.clone()))
            } else {
                *oc = None;
                None
            }
        } else {
            None
        }
    };

    // Enter: versioned key ensures fade-in starts at 0 on each transition.
    let enter_alpha = animate_f32_from(format!("cf_enter:{key}:v{v}"), 0.0, 1.0, spec);

    match old_view {
        Some(ov) => Stack(Modifier::new().fill_max_size()).child((
            ov,
            Box(Modifier::new().fill_max_size().alpha(enter_alpha)).child(new_view),
        )),
        None => Box(Modifier::new().fill_max_size().alpha(enter_alpha)).child(new_view),
    }
}

/// Animates between different content based on the `target_state`, with
/// configurable enter and exit transitions.
///
/// When the target state changes, the old content animates out using the
/// `exit` transition while the new content animates in using the `enter`
/// transition. During the transition both are stacked on top of each other.
///
/// # Defaults
///
/// If no `enter`/`exit` is given, defaults to `FadeIn` / `FadeOut` with
/// the provided `spec` (or `AnimationSpec::default()`).
pub fn AnimatedContent<T, F>(
    key: impl Into<String>,
    target_state: T,
    spec: AnimationSpec,
    enter: EnterTransition,
    exit: ExitTransition,
    content: F,
) -> View
where
    T: PartialEq + Clone + 'static,
    F: Fn(T) -> View + 'static,
{
    let key = key.into();

    let prev = remember_with_key(format!("ac_prev:{key}"), || {
        RefCell::new(target_state.clone())
    });
    let old_content =
        remember_with_key(format!("ac_old_view:{key}"), || RefCell::new(None::<View>));
    let version = remember_with_key(format!("ac_version:{key}"), || RefCell::new(0u64));

    let is_new = *prev.borrow() != target_state;
    if is_new {
        let prev_view = content(prev.borrow().clone());
        old_content.borrow_mut().replace(prev_view);
        prev.borrow_mut().clone_from(&target_state);
        *version.borrow_mut() += 1;
    }

    let v = *version.borrow();
    let new_view = content(target_state.clone());
    let new_view = apply_enter(&key, v, &enter, &spec, new_view);

    let old_view = {
        let mut oc = old_content.borrow_mut();
        if let Some(ref ov) = *oc {
            // Check if exit is already done (read-only, no side effects).
            if exit_animation_done(&key, v, &exit) {
                *oc = None;
                None
            } else {
                Some(apply_exit(&key, v, &exit, &spec, ov.clone()))
            }
        } else {
            None
        }
    };

    match old_view {
        Some(ov) => Stack(Modifier::new().fill_max_size()).child((ov, new_view)),
        None => new_view,
    }
}

fn apply_enter(
    key: &str,
    version: u64,
    enter: &EnterTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match enter {
        EnterTransition::FadeIn => {
            let val = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(Modifier::new().fill_max_size().alpha(val)).child(view)
        }
        EnterTransition::SlideIn { offset_x, offset_y } => {
            let val_x = animate_f32_from(
                format!("{key}:v{version}:enter:slide_x"),
                dp_to_px(*offset_x),
                0.0,
                *spec,
            );
            let val_y = animate_f32_from(
                format!("{key}:v{version}:enter:slide_y"),
                dp_to_px(*offset_y),
                0.0,
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate(val_x, val_y)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            let a = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(Modifier::new().fill_max_size().scale(s).alpha(a)).child(view)
        }
        EnterTransition::Composite(transitions) => {
            let mut v = view;
            for t in transitions {
                v = apply_enter_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_enter_single(
    key: &str,
    version: u64,
    enter: &EnterTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match enter {
        EnterTransition::FadeIn => {
            let val = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(Modifier::new().fill_max_size().alpha(val)).child(view)
        }
        EnterTransition::SlideIn { offset_x, offset_y } => {
            let val_x = animate_f32_from(
                format!("{key}:v{version}:enter:slide_x"),
                dp_to_px(*offset_x),
                0.0,
                *spec,
            );
            let val_y = animate_f32_from(
                format!("{key}:v{version}:enter:slide_y"),
                dp_to_px(*offset_y),
                0.0,
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate(val_x, val_y)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            Box(Modifier::new().fill_max_size().scale(s)).child(view)
        }
        EnterTransition::Composite(inner) => {
            let mut v = view;
            for t in inner {
                v = apply_enter_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_exit(
    key: &str,
    version: u64,
    exit: &ExitTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match exit {
        ExitTransition::FadeOut => {
            let val = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(Modifier::new().fill_max_size().alpha(val)).child(view)
        }
        ExitTransition::SlideOut { offset_x, offset_y } => {
            let val_x = animate_f32_from(
                format!("{key}:v{version}:exit:slide_x"),
                0.0,
                dp_to_px(*offset_x),
                *spec,
            );
            let val_y = animate_f32_from(
                format!("{key}:v{version}:exit:slide_y"),
                0.0,
                dp_to_px(*offset_y),
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate(val_x, val_y)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            let a = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(Modifier::new().fill_max_size().scale(s).alpha(a)).child(view)
        }
        ExitTransition::Composite(transitions) => {
            let mut v = view;
            for t in transitions {
                v = apply_exit_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_exit_single(
    key: &str,
    version: u64,
    exit: &ExitTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match exit {
        ExitTransition::FadeOut => {
            let val = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(Modifier::new().fill_max_size().alpha(val)).child(view)
        }
        ExitTransition::SlideOut { offset_x, offset_y } => {
            let val_x = animate_f32_from(
                format!("{key}:v{version}:exit:slide_x"),
                0.0,
                dp_to_px(*offset_x),
                *spec,
            );
            let val_y = animate_f32_from(
                format!("{key}:v{version}:exit:slide_y"),
                0.0,
                dp_to_px(*offset_y),
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate(val_x, val_y)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            Box(Modifier::new().fill_max_size().scale(s)).child(view)
        }
        ExitTransition::Composite(inner) => {
            let mut v = view;
            for t in inner {
                v = apply_exit_single(key, version, t, spec, v);
            }
            v
        }
    }
}

/// Read the current value of an `animate_f32` animation without advancing it.
/// Returns `None` if the animation is still running or the entry doesn't exist.
fn read_anim_value(key: &str, default: f32) -> Option<f32> {
    let anim = remember_state_with_key::<AnimatedValue<f32>>(format!("anim:f32:{key}"), || {
        AnimatedValue::new(default, AnimationSpec::default())
    });
    let a = anim.borrow();
    if a.is_animating() {
        None
    } else {
        Some(*a.get())
    }
}

/// Check whether an exit animation has completed (read-only, no advancement).
fn exit_animation_done(key: &str, version: u64, exit: &ExitTransition) -> bool {
    match exit {
        ExitTransition::FadeOut => read_anim_value(&format!("{key}:v{version}:exit:fade"), 1.0)
            .map(|v| v < 0.005)
            .unwrap_or(false),
        ExitTransition::SlideOut { offset_x, offset_y } => {
            let done_x = read_anim_value(&format!("{key}:v{version}:exit:slide_x"), 0.0)
                .map(|v| (v - dp_to_px(*offset_x)).abs() < 0.5)
                .unwrap_or(false);
            let done_y = read_anim_value(&format!("{key}:v{version}:exit:slide_y"), 0.0)
                .map(|v| (v - dp_to_px(*offset_y)).abs() < 0.5)
                .unwrap_or(false);
            done_x && done_y
        }
        ExitTransition::ScaleOut { target } => {
            let done_scale = read_anim_value(&format!("{key}:v{version}:exit:scale"), 1.0)
                .map(|v| (v - target).abs() < 0.005)
                .unwrap_or(false);
            let done_fade = read_anim_value(&format!("{key}:v{version}:exit:fade"), 1.0)
                .map(|v| v < 0.005)
                .unwrap_or(false);
            done_scale && done_fade
        }
        ExitTransition::Composite(ts) => ts.iter().all(|t| exit_animation_done(key, version, t)),
    }
}

pub fn AnimatedVisibility(key: impl Into<String>, visible: bool, content: View) -> View {
    let key = key.into();
    let alpha = animate_f32(
        format!("visibility_alpha:{key}"),
        if visible { 1.0 } else { 0.0 },
        AnimationSpec::default(),
    );

    let scale = animate_f32(
        format!("visibility_scale:{key}"),
        if visible { 1.0 } else { 0.8 },
        AnimationSpec::default(),
    );

    if alpha > 0.01 {
        Box(Modifier::new().alpha(alpha).scale(scale)).child(content)
    } else {
        Box(Modifier::new())
    }
}
