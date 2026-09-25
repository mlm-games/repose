#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use web_time::Duration;

use repose_core::animation::{AnimationSpec, CubicBezier, Easing, KeyframesSpec, RepeatableSpec};
use repose_core::*;
use repose_ui::Box;

use super::*;

/// Configuration for [`CircularProgressIndicator`].
#[derive(Clone, Debug)]
pub struct CircularProgressIndicatorConfig {
    pub modifier: Modifier,
    pub color: Color,
    pub track_color: Color,
    pub indeterminate_track_color: Color,
    pub stroke_width: Dp,
    pub stroke_cap: StrokeCap,
    pub gap_size: Dp,
}

impl Default for CircularProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: ProgressIndicatorDefaults::circular_color(),
            track_color: ProgressIndicatorDefaults::circular_track_color(),
            indeterminate_track_color:
                ProgressIndicatorDefaults::circular_indeterminate_track_color(),
            stroke_width: ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH,
            stroke_cap: StrokeCap::Round,
            gap_size: ProgressIndicatorDefaults::CIRCULAR_TRACK_ACTIVE_SPACE,
        }
    }
}

fn progress_identity(modifier: &Modifier, kind: &str, instance_id: u64) -> String {
    match modifier.key {
        Some(key) => format!("progress:{kind}:key:{key}"),
        None => format!("progress:{kind}:instance:{instance_id}"),
    }
}

fn indeterminate_animation(key: &str, duration: Duration) -> Rc<RefCell<AnimatedValue<f32>>> {
    let animation_key = format!("progress:driver:{key}");
    let animation = remember_state_with_key(animation_key.clone(), || {
        let mut animation = AnimatedValue::new(
            0.0,
            AnimationSpec::tween(duration, Easing::Linear).repeated(RepeatableSpec::infinite()),
        );
        animation.set_target(1.0);
        animation
    });
    repose_core::animation_driver::touch(&animation_key);
    if !repose_core::animation_driver::is_registered(&animation_key) {
        let animation_for_driver = animation.clone();
        repose_core::animation_driver::register(
            animation_key.clone(),
            Rc::new(RefCell::new(move || {
                animation_for_driver.borrow_mut().update()
            })),
        );
    }
    let cleanup_key = format!("{animation_key}:cleanup");
    let cleanup_animation_key = animation_key.clone();
    effect_once_with_key(cleanup_key, move || {
        on_unmount(move || animation_driver::unregister(&cleanup_animation_key))
    });
    request_frame();
    animation
}

const LINEAR_INDETERMINATE_DURATION_MS: f32 = 1750.0;

fn linear_progress_keyframe(delay_ms: f32, duration_ms: f32) -> KeyframesSpec<f32> {
    let start = (delay_ms / LINEAR_INDETERMINATE_DURATION_MS).clamp(0.0, 1.0);
    let end = ((delay_ms + duration_ms) / LINEAR_INDETERMINATE_DURATION_MS).clamp(0.0, 1.0);
    let easing = Easing::Custom(CubicBezier::new(0.3, 0.0, 0.8, 0.15));
    let mut keyframes = Vec::with_capacity(4);
    if start > 0.0 {
        keyframes.push((0.0, 0.0, None));
    }
    keyframes.push((start, 0.0, Some(easing)));
    keyframes.push((end, 1.0, None));
    if end < 1.0 {
        keyframes.push((1.0, 1.0, None));
    }
    KeyframesSpec { keyframes }
}

fn draw_linear_progress_segment(
    scene: &mut Scene,
    rect: Rect,
    start: f32,
    end: f32,
    color: Color,
    cap_radius: f32,
) {
    let start = start.clamp(0.0, 1.0);
    let end = end.clamp(0.0, 1.0);
    if end <= start {
        return;
    }
    let min_x = rect.x + cap_radius;
    let max_x = rect.x + rect.w - cap_radius;
    let start_x = (rect.x + start * rect.w).clamp(min_x, max_x);
    let end_x = (rect.x + end * rect.w).clamp(min_x, max_x);
    if end_x <= start_x {
        return;
    }
    scene.nodes.push(SceneNode::Rect {
        rect: Rect {
            x: start_x,
            y: rect.y,
            w: end_x - start_x,
            h: rect.h,
        },
        brush: Brush::Solid(color),
        radius: [Px(cap_radius); 4],
    });
}

/// M3 Circular Progress Indicator.
///
/// Determinate (`Some(0..1)`): draws arc from 12 o'clock clockwise.
/// Indeterminate (`None`): animates a spinning 270° arc.
pub fn CircularProgressIndicator(
    value: Option<f32>,
    config: CircularProgressIndicatorConfig,
) -> View {
    let instance_id = remember(unique_component_id);
    let identity = progress_identity(&config.modifier, "circular", *instance_id);
    let sz = ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE;
    let val = value.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    let animation = value
        .is_none()
        .then(|| indeterminate_animation(&identity, Duration::from_millis(6000)));
    let add_kf = value.is_none().then(|| {
        remember_state_with_key(format!("{identity}:circular-add"), || {
            let emph = Easing::Custom(CubicBezier::new(0.05, 0.7, 0.1, 1.0));
            KeyframesSpec {
                keyframes: vec![
                    (0.0, 0.0, None),
                    (0.05, 90.0, Some(emph)),
                    (0.25, 90.0, None),
                    (0.30, 180.0, None),
                    (0.50, 180.0, None),
                    (0.55, 270.0, None),
                    (0.75, 270.0, None),
                    (0.80, 360.0, None),
                    (1.0, 360.0, None),
                ],
            }
        })
    });
    let sweep_kf = value.is_none().then(|| {
        remember_state_with_key(format!("{identity}:circular-sweep"), || {
            let std_dec = Easing::Custom(CubicBezier::new(0.2, 0.0, 0.0, 1.0));
            KeyframesSpec {
                keyframes: vec![
                    (0.0, 0.1, None),
                    (0.5, 0.87, Some(std_dec)),
                    (1.0, 0.1, None),
                ],
            }
        })
    });

    Box(Modifier::new().size(sz, sz).then(config.modifier).painter(
        move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let stroke_px = config.stroke_width.to_px().0;
            let gap_px = config.gap_size.to_px().0;
            let outer_diameter_px = rect.w.max(1.0);
            let adjusted_gap_px = (if config.stroke_cap == StrokeCap::Butt || rect.h > rect.w {
                gap_px
            } else {
                gap_px + stroke_px
            })
            .max(0.0);
            let gap_sweep_rad = adjusted_gap_px / outer_diameter_px * 2.0;
            let (global_rotation, additional_rotation, sweep_val) =
                if let Some(animation) = &animation {
                    let t = *animation.borrow().get();
                    let av = add_kf
                        .as_ref()
                        .map(|keyframe| keyframe.borrow().evaluate(t))
                        .unwrap_or(0.0);
                    let sv = sweep_kf
                        .as_ref()
                        .map(|keyframe| keyframe.borrow().evaluate(t))
                        .unwrap_or(0.0);
                    (t * 1080.0, av, sv)
                } else {
                    (0.0, 0.0, 0.0)
                };
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let cx = rect.x + rect.w * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let r = (rect.w.min(rect.h)) * 0.5 - stroke_px * 0.5;
            let circle = Rect {
                x: cx - r,
                y: cy - r,
                w: r * 2.0,
                h: r * 2.0,
            };

            match val {
                Some(p) => {
                    let sweep_rad = p * std::f32::consts::TAU;
                    let start_angle = -std::f32::consts::FRAC_PI_2;
                    let effective_gap = gap_sweep_rad.min(sweep_rad);

                    // Indicator arc
                    if p > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle,
                            sweep_angle: sweep_rad,
                            stroke_width: Px(stroke_px),
                            brush: Brush::Solid(mul_c(config.color)),
                            cap: config.stroke_cap,
                        });
                    }

                    // Track arc (with gap from indicator)
                    let track_start = start_angle + sweep_rad + effective_gap;
                    let track_sweep = std::f32::consts::TAU - sweep_rad - 2.0 * effective_gap;
                    if track_sweep > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle: track_start,
                            sweep_angle: track_sweep,
                            stroke_width: Px(stroke_px),
                            brush: Brush::Solid(mul_c(config.track_color)),
                            cap: config.stroke_cap,
                        });
                    }
                }
                None => {
                    let radians =
                        (global_rotation + additional_rotation) * std::f32::consts::PI / 180.0;
                    let start_angle = radians;
                    let sweep_rad = sweep_val * std::f32::consts::TAU;
                    let effective_gap = gap_sweep_rad.min(sweep_rad);

                    // Indicator arc
                    scene.nodes.push(SceneNode::Arc {
                        rect: circle,
                        start_angle,
                        sweep_angle: sweep_rad,
                        stroke_width: Px(stroke_px),
                        brush: Brush::Solid(mul_c(config.color)),
                        cap: config.stroke_cap,
                    });

                    // Track arc (with gap from indicator)
                    let track_start = start_angle + sweep_rad + effective_gap;
                    let track_sweep = std::f32::consts::TAU - sweep_rad - 2.0 * effective_gap;
                    if track_sweep > 0.0 {
                        scene.nodes.push(SceneNode::Arc {
                            rect: circle,
                            start_angle: track_start,
                            sweep_angle: track_sweep,
                            stroke_width: Px(stroke_px),
                            brush: Brush::Solid(mul_c(config.indeterminate_track_color)),
                            cap: config.stroke_cap,
                        });
                    }
                }
            }
        },
    ))
    .semantics(Semantics {
        role: Role::ProgressBar,
        value: val.map(|v| format!("{}%", (v * 100.0).round() as i32)),
        ..Default::default()
    })
}

/// Configuration for [`LinearProgressIndicator`].
#[derive(Clone, Debug)]
pub struct LinearProgressIndicatorConfig {
    pub modifier: Modifier,
    pub color: Color,
    pub track_color: Color,
    /// Stroke cap style for the indicator ends. Default: `StrokeCap::Round`
    pub stroke_cap: StrokeCap,
    /// Gap between indicator and track, in [`Dp`].
    pub gap_size: Dp,
    /// Diameter of the stop indicator dot, in [`Dp`].
    pub stop_size: Dp,
}

impl Default for LinearProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: ProgressIndicatorDefaults::linear_color(),
            track_color: ProgressIndicatorDefaults::linear_track_color(),
            stroke_cap: StrokeCap::Round,
            gap_size: ProgressIndicatorDefaults::LINEAR_INDICATOR_GAP_SIZE,
            stop_size: ProgressIndicatorDefaults::LINEAR_TRACK_STOP_SIZE,
        }
    }
}

/// M3 Linear Progress Indicator.
///
/// Determinate (`Some(0..1)`): active track + gap + stop indicator (M3).
/// Indeterminate (`None`): sliding indicator matching Compose Material3 timing.
pub fn LinearProgressIndicator(value: Option<f32>, config: LinearProgressIndicatorConfig) -> View {
    let instance_id = remember(unique_component_id);
    let identity = progress_identity(&config.modifier, "linear", *instance_id);
    let value = value.map(|v| {
        if v.is_finite() {
            v.clamp(0.0, 1.0)
        } else {
            0.0
        }
    });
    let animation = value
        .is_none()
        .then(|| indeterminate_animation(&identity, Duration::from_millis(1750)));
    let motion = value.is_none().then(|| {
        (
            remember_state_with_key(format!("{identity}:linear-head-1"), || {
                linear_progress_keyframe(0.0, 1000.0)
            }),
            remember_state_with_key(format!("{identity}:linear-tail-1"), || {
                linear_progress_keyframe(250.0, 1000.0)
            }),
            remember_state_with_key(format!("{identity}:linear-head-2"), || {
                linear_progress_keyframe(650.0, 850.0)
            }),
            remember_state_with_key(format!("{identity}:linear-tail-2"), || {
                linear_progress_keyframe(900.0, 850.0)
            }),
        )
    });

    Box(Modifier::new()
        .fill_max_width()
        .height(ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT)
        .then(config.modifier)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let (first_head, first_tail, second_head, second_tail) =
                if let (Some(animation), Some((head_1, tail_1, head_2, tail_2))) =
                    (&animation, &motion)
                {
                    let t = *animation.borrow().get();
                    (
                        head_1.borrow().evaluate(t),
                        tail_1.borrow().evaluate(t),
                        head_2.borrow().evaluate(t),
                        tail_2.borrow().evaluate(t),
                    )
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = rect.h;
            let corner = track_h * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let cap_radius = if config.stroke_cap == StrokeCap::Butt {
                0.0
            } else {
                corner
            };
            let dot_r = (config.stop_size.to_px().0 * 0.5).max(0.0);
            let gap_px = config.gap_size.to_px().0.max(0.0);

            let gap_fraction = if config.stroke_cap == StrokeCap::Butt || track_h > rect.w {
                gap_px / rect.w.max(1.0)
            } else {
                (gap_px + track_h) / rect.w.max(1.0)
            }
            .clamp(0.0, 1.0);
            let track = mul_c(config.track_color);
            let indicator = mul_c(config.color);

            if let Some(t) = value {
                let track_start = t + t.min(gap_fraction);
                draw_linear_progress_segment(scene, rect, track_start, 1.0, track, cap_radius);
                draw_linear_progress_segment(scene, rect, 0.0, t, indicator, cap_radius);

                let sx = rect.x + rect.w - dot_r;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(indicator),
                });
            } else {
                if first_head < 1.0 - gap_fraction {
                    let start = if first_head > 0.0 {
                        first_head + gap_fraction
                    } else {
                        0.0
                    };
                    draw_linear_progress_segment(scene, rect, start, 1.0, track, cap_radius);
                }
                draw_linear_progress_segment(
                    scene, rect, first_tail, first_head, indicator, cap_radius,
                );
                if first_tail > gap_fraction {
                    let start = if second_head > 0.0 {
                        second_head + gap_fraction
                    } else {
                        0.0
                    };
                    let end = if first_tail < 1.0 {
                        first_tail - gap_fraction
                    } else {
                        1.0
                    };
                    draw_linear_progress_segment(scene, rect, start, end, track, cap_radius);
                }
                draw_linear_progress_segment(
                    scene,
                    rect,
                    second_tail,
                    second_head,
                    indicator,
                    cap_radius,
                );
                if second_tail > gap_fraction {
                    let end = if second_tail < 1.0 {
                        second_tail - gap_fraction
                    } else {
                        1.0
                    };
                    draw_linear_progress_segment(scene, rect, 0.0, end, track, cap_radius);
                }
            }
        }))
    .semantics(Semantics {
        role: Role::ProgressBar,
        value: value.map(|v| {
            let v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            };
            format!("{}%", (v * 100.0).round() as i32)
        }),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::locals::{Density, with_density};

    #[test]
    fn circular_indeterminate_track_defaults_to_transparent() {
        assert_eq!(
            CircularProgressIndicatorConfig::default().indeterminate_track_color,
            Color::TRANSPARENT
        );
    }

    #[test]
    fn linear_motion_uses_compose_delays() {
        let first_head = linear_progress_keyframe(0.0, 1000.0);
        let first_tail = linear_progress_keyframe(250.0, 1000.0);
        let second_head = linear_progress_keyframe(650.0, 850.0);
        let second_tail = linear_progress_keyframe(900.0, 850.0);
        let total = LINEAR_INDETERMINATE_DURATION_MS;
        assert_eq!(first_head.evaluate(0.0), 0.0);
        assert_eq!(first_tail.evaluate(250.0 / total), 0.0);
        assert!(first_tail.evaluate(1250.0 / total) > 0.0);
        assert_eq!(second_head.evaluate(650.0 / total), 0.0);
        assert!(second_head.evaluate(1500.0 / total) > 0.0);
        assert_eq!(second_tail.evaluate(900.0 / total), 0.0);
        assert!(second_tail.evaluate(1750.0 / total) > 0.0);
    }

    #[test]
    fn circular_progress_uses_material_track_active_space() {
        assert_eq!(
            CircularProgressIndicatorConfig::default().gap_size,
            ProgressIndicatorDefaults::CIRCULAR_TRACK_ACTIVE_SPACE
        );
    }

    #[test]
    fn circular_progress_keeps_dp_size_until_layout() {
        let (view, stroke) = with_density(Density { scale: 2.0 }, || {
            let view =
                CircularProgressIndicator(Some(0.5), CircularProgressIndicatorConfig::default());
            let painter = view.modifier.painter.as_ref().expect("progress painter");
            let mut scene = Scene::default();
            painter(
                &mut scene,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 80.0,
                    h: 80.0,
                },
                1.0,
            );
            let stroke = scene
                .nodes
                .iter()
                .find_map(|node| match node {
                    SceneNode::Arc { stroke_width, .. } => Some(*stroke_width),
                    _ => None,
                })
                .expect("progress arc");
            (view, stroke)
        });
        assert_eq!(
            view.modifier.size,
            Some(DpSize {
                width: ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE,
                height: ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE,
            })
        );
        assert_eq!(stroke, Px(8.0));
    }
}
