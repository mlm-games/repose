use repose_core::*;

pub fn AnimatedVisibility(
    visible: bool,
    enter: EnterTransition,
    exit: ExitTransition,
    content: View,
) -> View {
    let alpha = animate_f32(
        "visibility_alpha",
        if visible { 1.0 } else { 0.0 },
        AnimationSpec::default(),
    );

    let scale = animate_f32(
        "visibility_scale",
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
    target: T,
    content: impl Fn(T) -> View + 'static,
) -> View {
    let key = format!("crossfade_{:?}", std::ptr::addr_of!(&target));
    let prev = remember_with_key(key.clone(), || RefCell::new(target.clone()));

    let alpha = if *prev.borrow() != target {
        prev.replace(target.clone());
        animate_f32(key, 1.0, AnimationSpec::fast())
    } else {
        1.0
    };

    Box(Modifier::new().alpha(alpha)).child(content(target))
}

pub fn AnimatedContent(key: String, transition: Option<Transition>, content: View) -> View {
    match transition {
        Some(Transition::Push { .. }) => {
            let offset = animate_f32(format!("push_{}", key), 0.0, AnimationSpec::default());
            Box(Modifier::new().translate(offset, 0.0)).child(content)
        }
        Some(Transition::Pop { .. }) => {
            let offset = animate_f32(format!("pop_{}", key), 0.0, AnimationSpec::default());
            Box(Modifier::new().translate(-offset, 0.0)).child(content)
        }
        _ => content,
    }
}
