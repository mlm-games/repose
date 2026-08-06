#![allow(non_snake_case)]

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
    pub stroke_width: f32,
    pub stroke_cap: StrokeCap,
    pub gap_size: f32,
}

impl Default for CircularProgressIndicatorConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            color: ProgressIndicatorDefaults::circular_color(),
            track_color: ProgressIndicatorDefaults::circular_track_color(),
            stroke_width: ProgressIndicatorDefaults::CIRCULAR_STROKE_WIDTH,
            stroke_cap: StrokeCap::Round,
            gap_size: 0.0,
        }
    }
}

/// M3 Circular Progress Indicator.
///
/// Determinate (`Some(0..1)`): draws arc from 12 o'clock clockwise.
/// Indeterminate (`None`): animates a spinning 270° arc.
pub fn CircularProgressIndicator(
    value: Option<f32>,
    config: CircularProgressIndicatorConfig,
) -> View {
    let sz = dp_to_px(ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE);
    let stroke_px = dp_to_px(config.stroke_width);
    let val = value.map(|v| v.clamp(0.0, 1.0));

    // Three concurrent animations matching Compose Material3 indeterminate spec:
    //   1. Global rotation -> 1080° linear over 6000ms
    //   2. Additional rotation -> 90° stepped jumps with EmphasizedDecelerate
    //   3. Sweep -> oscillates 0.1 -> 0.87 -> 0.1 over 6000ms
    let (global_rotation, additional_rotation, sweep_val) = if value.is_none() {
        let shared = remember_state_with_key("circ_ind_shared", || {
            let mut a = AnimatedValue::new(
                0.0f32,
                AnimationSpec::tween(Duration::from_millis(6000), Easing::Linear)
                    .repeated(RepeatableSpec::infinite()),
            );
            a.set_target(1.0);
            a
        });
        let mut s = shared.borrow_mut();
        s.update();
        let t = *s.get();
        drop(s);

        let gv = t * 1080.0;

        let emph = Easing::Custom(CubicBezier::new(0.05, 0.7, 0.1, 1.0));
        let add_kf = remember_state_with_key("circ_ind_add_kf", || KeyframesSpec {
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
        });
        let av = add_kf.borrow().evaluate(t);

        let std_dec = Easing::Custom(CubicBezier::new(0.2, 0.0, 0.0, 1.0));
        let sweep_kf = remember_state_with_key("circ_ind_sweep_kf", || KeyframesSpec {
            keyframes: vec![
                (0.0, 0.1, None),
                (0.5, 0.87, Some(std_dec)),
                (1.0, 0.1, None),
            ],
        });
        let sv = sweep_kf.borrow().evaluate(t);

        (gv, av, sv)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Pre-compute gap angular size in radians
    let indicator_size_dp = ProgressIndicatorDefaults::CIRCULAR_INDICATOR_SIZE;
    let adjusted_gap_dp = if config.stroke_cap == StrokeCap::Butt {
        config.gap_size
    } else {
        config.gap_size + config.stroke_width
    };
    let circle_dia_dp = indicator_size_dp - config.stroke_width;
    let gap_sweep_rad = 2.0 * adjusted_gap_dp / circle_dia_dp;

    Box(Modifier::new().size(sz, sz).then(config.modifier).painter(
        move |scene: &mut Scene, rect: Rect, alpha: f32| {
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
                            stroke_width: stroke_px,
                            color: mul_c(config.color),
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
                            stroke_width: stroke_px,
                            color: mul_c(config.track_color),
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
                        stroke_width: stroke_px,
                        color: mul_c(config.color),
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
                            stroke_width: stroke_px,
                            color: mul_c(config.track_color),
                            cap: config.stroke_cap,
                        });
                    }
                }
            }
        },
    ))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
        selectable_group: false,
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
    /// Gap between indicator and track, in dp.
    pub gap_size: f32,
    /// Diameter of the stop indicator dot, in dp.
    pub stop_size: f32,
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
/// Pass `LinearProgressIndicatorConfig::default()` for standard M3 appearance,
/// or override individual fields via struct-update syntax.
pub fn LinearProgressIndicator(value: Option<f32>, config: LinearProgressIndicatorConfig) -> View {
    Box(Modifier::new()
        .fill_max_width()
        .height(ProgressIndicatorDefaults::LINEAR_INDICATOR_HEIGHT)
        .then(config.modifier)
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
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
            let dot_r = dp_to_px(config.stop_size) * 0.5;
            let cy = rect.y + rect.h * 0.5;
            let t = value.unwrap_or(0.0).clamp(0.0, 1.0);

            let cap_radius = if config.stroke_cap == StrokeCap::Butt {
                0.0
            } else {
                corner
            };

            let gap = dp_to_px(config.gap_size)
                - if config.stroke_cap == StrokeCap::Butt {
                    0.0
                } else {
                    cap_radius
                };

            let cap_ofs = cap_radius;
            let ind_end = (t * rect.w).clamp(cap_ofs, rect.w - cap_ofs);
            let ind_w = (ind_end - cap_ofs).max(0.0);

            // Indicator (active portion from left)
            if t > 0.0 && ind_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x + cap_ofs,
                        y: cy - corner,
                        w: ind_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(config.color)),
                    radius: [cap_radius; 4],
                });
            }

            // Track (inactive portion after gap)
            let track_start = (rect.x + ind_end + gap).min(rect.x + rect.w);
            let track_w = (rect.x + rect.w - track_start).max(0.0);
            if t < 1.0 && track_w > 0.0 {
                let track_left = track_start + cap_ofs;
                let track_right = rect.x + rect.w;
                if track_right > track_left {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: track_left,
                            y: cy - corner,
                            w: track_right - track_left,
                            h: track_h,
                        },
                        brush: Brush::Solid(mul_c(config.track_color)),
                        radius: [cap_radius; 4],
                    });
                }
            }

            // Stop indicator at right end circle
            {
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
            }
        }))
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
        selectable_group: false,
    })
}
