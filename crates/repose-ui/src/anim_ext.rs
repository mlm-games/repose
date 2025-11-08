use std::cell::RefCell;

use crate::{Box, ViewExt, navigation::Transition};
use repose_core::*;

use crate::anim::{self, animate_f32};

pub fn AnimatedVisibility(
    key: impl Into<String>,
    visible: bool,
    _enter: EnterTransition,
    _exit: ExitTransition,
    content: View,
) -> View {
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

pub enum EnterTransition {
    FadeIn,
    SlideIn,
    ScaleIn,
    ExpandIn,
}

pub enum ExitTransition {
    FadeOut,
    SlideOut,
    ScaleOut,
    ShrinkOut,
}

pub fn Crossfade<T: PartialEq + Clone + 'static>(
    key: impl Into<String>,
    target: T,
    content: impl Fn(T) -> View + 'static,
) -> View {
    let key = key.into();
    let prev = remember_with_key(format!("crossfade_prev:{key}"), || {
        RefCell::new(target.clone())
    });

    let alpha = if *prev.borrow() != target {
        prev.replace(target.clone());
        // restart animation to 1.0 each change (UI can layer content if desired)
        animate_f32(format!("crossfade_alpha:{key}"), 1.0, AnimationSpec::fast())
    } else {
        1.0
    };

    Box(Modifier::new().alpha(alpha)).child(content(target))
}

pub fn AnimatedContent(key: String, transition: Option<Transition>, content: View) -> View {
    match transition {
        Some(Transition::Push { .. }) => {
            let offset = animate_f32(format!("push_{key}"), 0.0, AnimationSpec::default());
            Box(Modifier::new().translate(offset, 0.0)).child(content)
        }
        Some(Transition::Pop { .. }) => {
            let offset = animate_f32(format!("pop_{key}"), 0.0, AnimationSpec::default());
            Box(Modifier::new().translate(-offset, 0.0)).child(content)
        }
        _ => content,
    }
}
