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
            stroke_width: ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH,
            stroke_cap: StrokeCap::Round,
            gap_size: Dp::ZERO,
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
            animation_key,
            Rc::new(RefCell::new(move || {
                animation_for_driver.borrow_mut().update()
            })),
        );
    }
    request_frame();
    animation
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
    let sz = ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE.to_px().0;
    let stroke_px = config.stroke_width.to_px().0;
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

    // Pre-compute gap angular size in radians
    let indicator_size_dp = ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE;
    let adjusted_gap_dp = (if config.stroke_cap == StrokeCap::Butt {
        config.gap_size
    } else {
        config.gap_size + config.stroke_width
    })
    .max(Dp::ZERO);
    let circle_dia_dp = (indicator_size_dp - config.stroke_width).max(Dp(1.0));
    let gap_sweep_rad = (adjusted_gap_dp / circle_dia_dp) * 2.0;

    Box(Modifier::new()
        .size(Dp(sz), Dp(sz))
        .then(config.modifier)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
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
                    let start_angle = -std::f32::consts::FRAC_PI_2 + radians;
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
                            brush: Brush::Solid(mul_c(config.track_color)),
                            cap: config.stroke_cap,
                        });
                    }
                }
            }
        }))
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
        .then(|| indeterminate_animation(&identity, Duration::from_millis(1800)));

    Box(Modifier::new()
        .fill_max_width()
        .height(ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT)
        .then(config.modifier)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let (head, tail) = if let Some(animation) = &animation {
                let t = *animation.borrow().get();
                ((t * 1.5).fract(), ((t * 1.5) - 0.4).fract().max(0.0))
            } else {
                (0.0, 0.0)
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

            // Full track background
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: rect.x,
                    y: cy - corner,
                    w: rect.w,
                    h: track_h,
                },
                brush: Brush::Solid(mul_c(config.track_color)),
                radius: [Px(cap_radius); 4],
            });

            if let Some(t) = value {
                let cap_ofs = cap_radius;
                let ind_end = (t * rect.w - gap_px).clamp(cap_ofs, rect.w - cap_ofs);
                let ind_w = (ind_end - cap_ofs).max(0.0);

                if t > 0.0 && ind_w > 0.0 {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: rect.x + cap_ofs,
                            y: cy - corner,
                            w: ind_w,
                            h: track_h,
                        },
                        brush: Brush::Solid(mul_c(config.color)),
                        radius: [Px(cap_radius); 4],
                    });
                }

                // Stop indicator (M3 determinate)
                let sx = rect.x + rect.w - dot_r;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(config.color)),
                });
            } else {
                // Indeterminate: two sliding segments (head leading, tail trailing)
                let w = rect.w.max(1.0);
                for (start_frac, end_frac) in
                    [(tail, head), ((tail + 0.5).fract(), (head + 0.5).fract())]
                {
                    let a = start_frac.min(end_frac);
                    let b = start_frac.max(end_frac);
                    if b - a < 0.05 {
                        continue; // too small
                    }
                    let x0 = rect.x + a * w;
                    let x1 = rect.x + b * w;
                    let ww = (x1 - x0).max(0.0);
                    if ww > 1.0 {
                        scene.nodes.push(SceneNode::Rect {
                            rect: Rect {
                                x: x0,
                                y: cy - corner,
                                w: ww,
                                h: track_h,
                            },
                            brush: Brush::Solid(mul_c(config.color)),
                            radius: [Px(cap_radius); 4],
                        });
                    }
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
