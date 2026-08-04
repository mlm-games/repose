use std::cell::RefCell;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::{Box, ViewExt, ZStack};
use repose_core::*;

use crate::anim::animate_f32_from;
use crate::anim::animate_vec2_from;

/// Vertical/horizontal alignment for expand/shrink transitions (0 = start, 1 = end).
pub type ExpandFrom = f32;

/// Describes how content enters the screen.
#[derive(Clone, Debug)]
pub enum EnterTransition {
    /// Fade from alpha 0 to 1.
    FadeIn,
    /// Slide from the given offset (dx, dy in dp) to position (0,0).
    SlideIn { offset_x: f32, offset_y: f32 },
    /// Scale from initial to 1.0 combined with fade from 0 to 1.
    ScaleIn { initial: f32 },
    /// Animate layout height 0 -> full and clip (Compose `expandVertically`).
    /// Participates in layout: siblings reflow as the height animates.
    ExpandVertically {
        /// Clip content to the animated bounds (Compose default: true).
        clip: bool,
        /// 0.0 = top, 0.5 = center, 1.0 = bottom.
        expand_from: ExpandFrom,
    },
    /// Animate layout width 0 -> full and clip (Compose `expandHorizontally`).
    ExpandHorizontally {
        clip: bool,
        /// 0.0 = start/left, 1.0 = end/right.
        expand_from: ExpandFrom,
    },
    /// Expand both axes (Compose `expandIn`).
    ExpandIn { clip: bool },
    /// Multiple enter transitions applied together (Compose `+`).
    Composite(Vec<EnterTransition>),
}

impl Default for EnterTransition {
    fn default() -> Self {
        Self::fade_in().and(Self::expand_vertically())
    }
}

impl EnterTransition {
    pub fn fade_in() -> Self {
        Self::FadeIn
    }
    pub fn expand_vertically() -> Self {
        Self::ExpandVertically {
            clip: true,
            expand_from: 0.0,
        }
    }
    pub fn expand_horizontally() -> Self {
        Self::ExpandHorizontally {
            clip: true,
            expand_from: 0.0,
        }
    }
    pub fn expand_in() -> Self {
        Self::ExpandIn { clip: true }
    }
    pub fn slide_in(offset_x: f32, offset_y: f32) -> Self {
        Self::SlideIn { offset_x, offset_y }
    }
    pub fn scale_in(initial: f32) -> Self {
        Self::ScaleIn { initial }
    }
    /// Compose-style `this + other`.
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Composite(mut a), Self::Composite(b)) => {
                a.extend(b);
                Self::Composite(a)
            }
            (Self::Composite(mut a), b) => {
                a.push(b);
                Self::Composite(a)
            }
            (a, Self::Composite(mut b)) => {
                let mut v = vec![a];
                v.append(&mut b);
                Self::Composite(v)
            }
            (a, b) => Self::Composite(vec![a, b]),
        }
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
    /// Animate layout height full -> 0 and clip (Compose `shrinkVertically`).
    ShrinkVertically {
        clip: bool,
        /// 0.0 = toward top, 1.0 = toward bottom.
        shrink_towards: ExpandFrom,
    },
    /// Animate layout width full -> 0 and clip (Compose `shrinkHorizontally`).
    ShrinkHorizontally {
        clip: bool,
        shrink_towards: ExpandFrom,
    },
    /// Shrink both axes (Compose `shrinkOut`).
    ShrinkOut { clip: bool },
    /// Multiple exit transitions applied together (Compose `+`).
    Composite(Vec<ExitTransition>),
}

impl Default for ExitTransition {
    fn default() -> Self {
        Self::fade_out().and(Self::shrink_vertically())
    }
}

impl ExitTransition {
    pub fn fade_out() -> Self {
        Self::FadeOut
    }
    pub fn shrink_vertically() -> Self {
        Self::ShrinkVertically {
            clip: true,
            shrink_towards: 0.0,
        }
    }
    pub fn shrink_horizontally() -> Self {
        Self::ShrinkHorizontally {
            clip: true,
            shrink_towards: 0.0,
        }
    }
    pub fn shrink_out() -> Self {
        Self::ShrinkOut { clip: true }
    }
    pub fn slide_out(offset_x: f32, offset_y: f32) -> Self {
        Self::SlideOut { offset_x, offset_y }
    }
    pub fn scale_out(target: f32) -> Self {
        Self::ScaleOut { target }
    }
    /// Compose-style `this + other`.
    pub fn and(self, other: Self) -> Self {
        match (self, other) {
            (Self::Composite(mut a), Self::Composite(b)) => {
                a.extend(b);
                Self::Composite(a)
            }
            (Self::Composite(mut a), b) => {
                a.push(b);
                Self::Composite(a)
            }
            (a, Self::Composite(mut b)) => {
                let mut v = vec![a];
                v.append(&mut b);
                Self::Composite(v)
            }
            (a, b) => Self::Composite(vec![a, b]),
        }
    }
}

#[derive(Clone)]
pub struct CrossfadeConfig {
    pub key: String,
    pub spec: AnimationSpec,
}

impl Default for CrossfadeConfig {
    fn default() -> Self {
        Self {
            key: "crossfade".into(),
            spec: AnimationSpec::default(),
        }
    }
}

/// Crossfades between two pieces of content when `target` changes.
///
/// When the target state changes, the old content fades out while the new
/// content fades in, with no other transforms applied.
pub fn Crossfade<T, F>(target: T, config: CrossfadeConfig, content: F) -> View
where
    T: PartialEq + Clone + 'static,
    F: Fn(T) -> View + 'static,
{
    let key = config.key;
    let spec = config.spec;

    let prev = remember_with_key(format!("cf_prev:{key}"), || RefCell::new(target.clone()));
    let old_content =
        remember_with_key(format!("cf_old_view:{key}"), || RefCell::new(None::<View>));
    let version = remember_with_key(format!("cf_version:{key}"), || RefCell::new(0u64));

    let is_new = *prev.borrow() != target;
    if is_new {
        let old_ver = *version.borrow();
        let mut prev_view = content(prev.borrow().clone());
        prev_view.scope_key = Some(format!("cf_{key}_old_v{old_ver}"));
        prev_view.modifier.repaint_boundary = true;
        old_content.borrow_mut().replace(prev_view);
        prev.borrow_mut().clone_from(&target);
        *version.borrow_mut() += 1;
    }

    let v = *version.borrow();
    let mut new_view = content(target.clone());
    new_view.scope_key = Some(format!("cf_{key}_v{v}"));
    new_view.modifier.repaint_boundary = true;

    // Exit: versioned key ensures fresh animation state per transition.
    let old_view = {
        let mut oc = old_content.borrow_mut();
        if let Some(ref ov) = *oc {
            let exit_alpha = animate_f32_from(format!("cf_exit:{key}:v{v}"), 1.0, 0.0, spec);
            if exit_alpha > 0.005 {
                let mut exit_box =
                    Box(Modifier::new().fill_max_size().alpha(exit_alpha)).child(ov.clone());
                exit_box.modifier.key = Some(transition_child_key(&key, v, "cf_exit"));
                Some(exit_box)
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
    let mut enter_box = Box(Modifier::new().fill_max_size().alpha(enter_alpha)).child(new_view);
    enter_box.modifier.key = Some(transition_child_key(&key, v, "cf_enter"));

    match old_view {
        Some(ov) => ZStack(Modifier::new().fill_max_size()).child((ov, enter_box)),
        None => enter_box,
    }
}

#[derive(Clone)]
pub struct AnimatedContentConfig {
    pub key: String,
    pub spec: AnimationSpec,
    pub enter: EnterTransition,
    pub exit: ExitTransition,
}

impl Default for AnimatedContentConfig {
    fn default() -> Self {
        Self {
            key: "anim_content".into(),
            spec: AnimationSpec::default(),
            enter: EnterTransition::FadeIn,
            exit: ExitTransition::FadeOut,
        }
    }
}

/// Stable child key for tree reconciliation during animated transitions.
fn transition_child_key(key: &str, version: u64, tag: &str) -> u64 {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    version.hash(&mut h);
    tag.hash(&mut h);
    h.finish()
}

/// In-flow wrapper modifier for enter/exit transitions. Intentionally does NOT
/// `fill_max_size()`: that would fight the parent `Column` (fill-max height on a
/// flex child makes it grow to consume leftover space).
fn flow_mod() -> Modifier {
    Modifier::new().fill_max_width()
}

/// Which axis an expand/shrink transition animates.
#[derive(Clone, Copy)]
enum SizeAxis {
    Vertical,
    Horizontal,
    Both,
}

/// Layout-true expand/shrink: the outer node reports an animated size (so
/// siblings in the parent `Column` reflow smoothly) while the content keeps its
/// natural measured size and is clipped to the animated bounds.
///
/// The natural size is measured via `on_size_changed` the first time the content
/// is composed at full size; afterwards it is remembered (`measure_key`) and
/// reused by both enter and exit animations.
#[allow(clippy::too_many_arguments)]
fn apply_size_fraction(
    measure_key: &str,
    anim_key: &str,
    axis: SizeAxis,
    from: f32,
    to: f32,
    clip: bool,
    align: f32,
    spec: AnimationSpec,
    view: View,
) -> View {
    let full_w = remember_mutable_with_key(format!("{measure_key}:w"), || 0.0f32);
    let full_h = remember_mutable_with_key(format!("{measure_key}:h"), || 0.0f32);

    if measure_key.contains("online_advanced") {
        eprintln!(
            "[SIZE] key={measure_key} full_w={} full_h={}",
            *full_w.get(),
            *full_h.get()
        );
    }

    let have = match axis {
        SizeAxis::Vertical => *full_h.get() > 0.5,
        SizeAxis::Horizontal => *full_w.get() > 0.5,
        SizeAxis::Both => *full_w.get() > 0.5 && *full_h.get() > 0.5,
    };

    let progress = if have {
        animate_f32_from(anim_key, from, to, spec)
    } else {
        from
    };

    let capture = {
        let fw = full_w.clone();
        let fh = full_h.clone();
        move |sz: Vec2| {
            if sz.x > 0.5 {
                fw.set_neq(sz.x);
            }
            if sz.y > 0.5 {
                fh.set_neq(sz.y);
            }
        }
    };

    let settled = from < to && progress >= 0.999;

    let full_w = *full_w.get();
    let full_h = *full_h.get();

    let shown_w = match axis {
        SizeAxis::Vertical => full_w,
        SizeAxis::Horizontal | SizeAxis::Both => full_w * progress,
    };
    let shown_h = match axis {
        SizeAxis::Horizontal => full_h,
        SizeAxis::Vertical | SizeAxis::Both => full_h * progress,
    };

    let mut outer = Modifier::new().fill_max_width().flex_shrink(0.0);
    if !have || settled {
        // Not measured yet (first composition): lay out at natural size so
        // `on_size_changed` can capture the true size, staying clipped. The
        // companion fade keeps this frame invisible. When settled (enter done),
        // release the fixed size and keep refreshing the measure while open.
        outer = outer.on_size_changed(capture);
    } else {
        match axis {
            SizeAxis::Vertical => {
                outer = outer.height(shown_h.max(0.0));
            }
            SizeAxis::Horizontal => {
                outer = outer.width(shown_w.max(0.0));
            }
            SizeAxis::Both => {
                outer = outer.size(shown_w.max(0.0), shown_h.max(0.0));
            }
        }
    }
    if clip {
        outer = outer.overflow(repose_core::Overflow::Clip);
    }

    // Inner keeps its full natural size and must not shrink into the animating
    // window; the outer bounds clip hides the overflow. This is what makes the
    // content get *clipped* (Compose `expandVertically`) instead of squashing.
    let align = align.clamp(0.0, 1.0);
    let ox = match axis {
        SizeAxis::Vertical => 0.0,
        SizeAxis::Horizontal | SizeAxis::Both => (shown_w - full_w) * align,
    };
    let oy = match axis {
        SizeAxis::Horizontal => 0.0,
        SizeAxis::Vertical | SizeAxis::Both => (shown_h - full_h) * align,
    };
    let mut inner = Modifier::new().fill_max_width().flex_shrink(0.0);
    if have && !settled {
        match axis {
            SizeAxis::Vertical => {
                inner = inner.height(full_h);
            }
            SizeAxis::Horizontal => {
                inner = inner.width(full_w);
            }
            SizeAxis::Both => {
                inner = inner.size(full_w, full_h);
            }
        }
    }
    if ox.abs() > 0.01 || oy.abs() > 0.01 {
        inner = inner.translate(dp_to_px(ox), dp_to_px(oy));
    }

    Box(outer).child(Box(inner).child(view))
}

/// Animates between different content based on the `target_state`, with
/// configurable enter and exit transitions.
///
/// When the target state changes, the old content animates out using the
/// `exit` transition while the new content animates in using the `enter`
/// transition. During the transition both are stacked on top of each other.
pub fn AnimatedContent<T, F>(target_state: T, content: F, config: AnimatedContentConfig) -> View
where
    T: PartialEq + Clone + 'static,
    F: Fn(T) -> View + 'static,
{
    let key = config.key;
    let spec = config.spec;
    let enter = config.enter;
    let exit = config.exit;

    let prev = remember_with_key(format!("ac_prev:{key}"), || {
        RefCell::new(target_state.clone())
    });
    let old_content =
        remember_with_key(format!("ac_old_view:{key}"), || RefCell::new(None::<View>));
    let version = remember_with_key(format!("ac_version:{key}"), || RefCell::new(0u64));

    let is_new = *prev.borrow() != target_state;
    if is_new {
        let old_ver = *version.borrow();
        let mut prev_view = content(prev.borrow().clone());
        prev_view.scope_key = Some(format!("ac_{key}_old_v{old_ver}"));
        prev_view.modifier.repaint_boundary = true;
        old_content.borrow_mut().replace(prev_view);
        prev.borrow_mut().clone_from(&target_state);
        *version.borrow_mut() += 1;
    }

    let v = *version.borrow();
    let mut new_view = content(target_state.clone());
    new_view.scope_key = Some(format!("ac_{key}_v{v}"));
    new_view.modifier.repaint_boundary = true;
    let mut new_view = apply_enter(&key, v, &enter, &spec, new_view);
    new_view.modifier.key = Some(transition_child_key(&key, v, "ac_enter"));

    let old_view = {
        let mut oc = old_content.borrow_mut();
        if let Some(ref ov) = *oc {
            // Check if exit is already done (read-only, no side effects).
            if exit_animation_done(&key, v, &exit) {
                *oc = None;
                None
            } else {
                let mut exit_view = apply_exit(&key, v, &exit, &spec, ov.clone());
                exit_view.modifier.key = Some(transition_child_key(&key, v, "ac_exit"));
                Some(exit_view)
            }
        } else {
            None
        }
    };

    match old_view {
        Some(ov) => ZStack(Modifier::new().fill_max_size()).child((ov, new_view)),
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
            let offset = animate_vec2_from(
                format!("{key}:v{version}:enter:slide"),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                Vec2::default(),
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate_vec2(offset)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            let a = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(Modifier::new().fill_max_size().transform_origin(0.5, 0.5).scale(s).alpha(a)).child(view)
        }
        EnterTransition::ExpandVertically { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_v"),
            SizeAxis::Vertical,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandHorizontally { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_h"),
            SizeAxis::Horizontal,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandIn { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_in"),
            SizeAxis::Both,
            0.0,
            1.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
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
            let offset = animate_vec2_from(
                format!("{key}:v{version}:enter:slide"),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                Vec2::default(),
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate_vec2(offset)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            Box(Modifier::new().fill_max_size().transform_origin(0.5, 0.5).scale(s)).child(view)
        }
        EnterTransition::ExpandVertically { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_v"),
            SizeAxis::Vertical,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandHorizontally { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_h"),
            SizeAxis::Horizontal,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandIn { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_in"),
            SizeAxis::Both,
            0.0,
            1.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
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
            let offset = animate_vec2_from(
                format!("{key}:v{version}:exit:slide"),
                Vec2::default(),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate_vec2(offset)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            let a = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(Modifier::new().fill_max_size().transform_origin(0.5, 0.5).scale(s).alpha(a)).child(view)
        }
        ExitTransition::ShrinkVertically { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_v"),
            SizeAxis::Vertical,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkHorizontally { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_h"),
            SizeAxis::Horizontal,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkOut { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_in"),
            SizeAxis::Both,
            1.0,
            0.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
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
            let offset = animate_vec2_from(
                format!("{key}:v{version}:exit:slide"),
                Vec2::default(),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                *spec,
            );
            Box(Modifier::new().fill_max_size().translate_vec2(offset)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            Box(Modifier::new().fill_max_size().transform_origin(0.5, 0.5).scale(s)).child(view)
        }
        ExitTransition::ShrinkVertically { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_v"),
            SizeAxis::Vertical,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkHorizontally { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_h"),
            SizeAxis::Horizontal,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkOut { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_in"),
            SizeAxis::Both,
            1.0,
            0.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
        ExitTransition::Composite(inner) => {
            let mut v = view;
            for t in inner {
                v = apply_exit_single(key, version, t, spec, v);
            }
            v
        }
    }
}

/// In-flow variant of `apply_enter`: fade/slide/scale wrappers use
/// `fill_max_width` instead of `fill_max_size` so the entering view participates
/// in its parent `Column`/`Row` instead of trying to fill it.
fn apply_enter_inflow(
    key: &str,
    version: u64,
    enter: &EnterTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match enter {
        EnterTransition::FadeIn => {
            let val = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(flow_mod().alpha(val)).child(view)
        }
        EnterTransition::SlideIn { offset_x, offset_y } => {
            let offset = animate_vec2_from(
                format!("{key}:v{version}:enter:slide"),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                Vec2::default(),
                *spec,
            );
            Box(flow_mod().translate_vec2(offset)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            let a = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(flow_mod().transform_origin(0.5, 0.5).scale(s).alpha(a)).child(view)
        }
        EnterTransition::ExpandVertically { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_v"),
            SizeAxis::Vertical,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandHorizontally { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_h"),
            SizeAxis::Horizontal,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandIn { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_in"),
            SizeAxis::Both,
            0.0,
            1.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
        EnterTransition::Composite(transitions) => {
            let mut v = view;
            for t in transitions {
                v = apply_enter_inflow_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_enter_inflow_single(
    key: &str,
    version: u64,
    enter: &EnterTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match enter {
        EnterTransition::FadeIn => {
            let val = animate_f32_from(format!("{key}:v{version}:enter:fade"), 0.0, 1.0, *spec);
            Box(flow_mod().alpha(val)).child(view)
        }
        EnterTransition::SlideIn { offset_x, offset_y } => {
            let offset = animate_vec2_from(
                format!("{key}:v{version}:enter:slide"),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                Vec2::default(),
                *spec,
            );
            Box(flow_mod().translate_vec2(offset)).child(view)
        }
        EnterTransition::ScaleIn { initial } => {
            let s = animate_f32_from(
                format!("{key}:v{version}:enter:scale"),
                *initial,
                1.0,
                *spec,
            );
            Box(flow_mod().transform_origin(0.5, 0.5).scale(s)).child(view)
        }
        EnterTransition::ExpandVertically { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_v"),
            SizeAxis::Vertical,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandHorizontally { clip, expand_from } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_h"),
            SizeAxis::Horizontal,
            0.0,
            1.0,
            *clip,
            *expand_from,
            *spec,
            view,
        ),
        EnterTransition::ExpandIn { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:enter:expand_in"),
            SizeAxis::Both,
            0.0,
            1.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
        EnterTransition::Composite(inner) => {
            let mut v = view;
            for t in inner {
                v = apply_enter_inflow_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_exit_inflow(
    key: &str,
    version: u64,
    exit: &ExitTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match exit {
        ExitTransition::FadeOut => {
            let val = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(flow_mod().alpha(val)).child(view)
        }
        ExitTransition::SlideOut { offset_x, offset_y } => {
            let offset = animate_vec2_from(
                format!("{key}:v{version}:exit:slide"),
                Vec2::default(),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                *spec,
            );
            Box(flow_mod().translate_vec2(offset)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            let a = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(flow_mod().transform_origin(0.5, 0.5).scale(s).alpha(a)).child(view)
        }
        ExitTransition::ShrinkVertically { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_v"),
            SizeAxis::Vertical,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkHorizontally { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_h"),
            SizeAxis::Horizontal,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkOut { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_in"),
            SizeAxis::Both,
            1.0,
            0.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
        ExitTransition::Composite(transitions) => {
            let mut v = view;
            for t in transitions {
                v = apply_exit_inflow_single(key, version, t, spec, v);
            }
            v
        }
    }
}

fn apply_exit_inflow_single(
    key: &str,
    version: u64,
    exit: &ExitTransition,
    spec: &AnimationSpec,
    view: View,
) -> View {
    match exit {
        ExitTransition::FadeOut => {
            let val = animate_f32_from(format!("{key}:v{version}:exit:fade"), 1.0, 0.0, *spec);
            Box(flow_mod().alpha(val)).child(view)
        }
        ExitTransition::SlideOut { offset_x, offset_y } => {
            let offset = animate_vec2_from(
                format!("{key}:v{version}:exit:slide"),
                Vec2::default(),
                Vec2 {
                    x: dp_to_px(*offset_x),
                    y: dp_to_px(*offset_y),
                },
                *spec,
            );
            Box(flow_mod().translate_vec2(offset)).child(view)
        }
        ExitTransition::ScaleOut { target } => {
            let s = animate_f32_from(format!("{key}:v{version}:exit:scale"), 1.0, *target, *spec);
            Box(flow_mod().transform_origin(0.5, 0.5).scale(s)).child(view)
        }
        ExitTransition::ShrinkVertically { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_v"),
            SizeAxis::Vertical,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkHorizontally { clip, shrink_towards } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_h"),
            SizeAxis::Horizontal,
            1.0,
            0.0,
            *clip,
            *shrink_towards,
            *spec,
            view,
        ),
        ExitTransition::ShrinkOut { clip } => apply_size_fraction(
            &format!("{key}:meas"),
            &format!("{key}:v{version}:exit:expand_in"),
            SizeAxis::Both,
            1.0,
            0.0,
            *clip,
            0.0,
            *spec,
            view,
        ),
        ExitTransition::Composite(inner) => {
            let mut v = view;
            for t in inner {
                v = apply_exit_inflow_single(key, version, t, spec, v);
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

fn read_anim_vec2_value(key: &str, default: Vec2) -> Option<Vec2> {
    let anim = remember_state_with_key::<AnimatedValue<Vec2>>(format!("anim:vec2:{key}"), || {
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
            let target = Vec2 {
                x: dp_to_px(*offset_x),
                y: dp_to_px(*offset_y),
            };
            read_anim_vec2_value(&format!("{key}:v{version}:exit:slide"), Vec2::default())
                .map(|v| (v.x - target.x).abs() < 0.5 && (v.y - target.y).abs() < 0.5)
                .unwrap_or(false)
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
        ExitTransition::ShrinkVertically { .. } => read_anim_value(
            &format!("{key}:v{version}:exit:expand_v"),
            1.0,
        )
        .map(|v| v < 0.005)
        .unwrap_or(false),
        ExitTransition::ShrinkHorizontally { .. } => read_anim_value(
            &format!("{key}:v{version}:exit:expand_h"),
            1.0,
        )
        .map(|v| v < 0.005)
        .unwrap_or(false),
        ExitTransition::ShrinkOut { .. } => read_anim_value(
            &format!("{key}:v{version}:exit:expand_in"),
            1.0,
        )
        .map(|v| v < 0.005)
        .unwrap_or(false),
        ExitTransition::Composite(ts) => ts.iter().all(|t| exit_animation_done(key, version, t)),
    }
}

#[derive(Clone)]
pub struct AnimatedVisibilityConfig {
    pub key: String,
    pub spec: AnimationSpec,
    pub enter: EnterTransition,
    pub exit: ExitTransition,
}

impl Default for AnimatedVisibilityConfig {
    fn default() -> Self {
        Self {
            key: "anim_vis".into(),
            spec: AnimationSpec::default(),
            enter: EnterTransition::default(),
            exit: ExitTransition::default(),
        }
    }
}

impl AnimatedVisibilityConfig {
    pub fn with_key(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Default::default()
        }
    }
}

/// Shows or hides content with animated enter/exit transitions.
///
/// When `visible` becomes `true`, the content enters using the specified `enter`
/// transition. When it becomes `false`, the content exits using the specified `exit`
/// transition.
pub fn AnimatedVisibility(visible: bool, content: View, config: AnimatedVisibilityConfig) -> View {
    let key = config.key;
    let spec = config.spec;
    let enter = config.enter;
    let exit = config.exit;

    let old_content = remember_with_key(format!("av_old:{key}"), || RefCell::new(None::<View>));
    let version = remember_with_key(format!("av_ver:{key}"), || RefCell::new(0u64));
    let prev = remember_with_key(format!("av_prev:{key}"), || RefCell::new(visible));

    if key == "online_advanced" {
        eprintln!(
            "[AV] key={key} visible={visible} prev={:?}",
            *prev.borrow()
        );
    }

    // Detect transition
    if *prev.borrow() != visible {
        if !visible {
            // Going hidden: capture current content for exit animation
            let mut captured = content.clone();
            captured.scope_key = Some(format!("av_{key}_old"));
            captured.modifier.repaint_boundary = true;
            old_content.borrow_mut().replace(captured);
        } else {
            // Re-opening: drop any pending exit snapshot so enter starts clean.
            *old_content.borrow_mut() = None;
        }
        *version.borrow_mut() += 1;
        prev.borrow_mut().clone_from(&visible);
    }

    let v = *version.borrow();

    // Handle exiting old content
    let exiting = {
        let mut oc = old_content.borrow_mut();
        if let Some(ref old) = *oc {
            if exit_animation_done(&key, v, &exit) {
                *oc = None;
                None
            } else {
                let mut exit_view = apply_exit_inflow(&key, v, &exit, &spec, old.clone());
                exit_view.modifier.key = Some(transition_child_key(&key, v, "av_exit"));
                Some(exit_view)
            }
        } else {
            None
        }
    };

    if visible {
        let mut content = content;
        content.scope_key = Some(format!("av_{key}_content"));
        content.modifier.repaint_boundary = true;
        let mut entering = if v > 0 {
            apply_enter_inflow(&key, v, &enter, &spec, content)
        } else {
            content
        };
        entering.modifier.key = Some(transition_child_key(&key, v, "av_enter"));
        entering
    } else {
        exiting.unwrap_or_else(|| Box(Modifier::new().height(0.0)))
    }
}
