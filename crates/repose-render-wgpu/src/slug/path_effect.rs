use lyon_path::iterator::PathIterator;
use lyon_path::math::Point;
use lyon_path::math::Vector;
use lyon_path::{Builder as PathBuilder, Path, PathEvent};

use repose_core::PathEffect;

/// Apply a path effect to a lyon Path, returning a new Path.
pub fn apply_path_effect(path: &Path, effect: &PathEffect, tolerance: f32) -> Path {
    match effect {
        PathEffect::Corner { radius } => apply_corner_effect(path, *radius, tolerance),
        PathEffect::Dash { intervals, phase } => {
            apply_dash_effect(path, intervals, *phase, tolerance)
        }
    }
}

fn apply_corner_effect(path: &Path, radius: f32, tolerance: f32) -> Path {
    if !radius.is_finite() || radius <= 0.0 {
        return path.clone();
    }
    let events: Vec<PathEvent> = path.iter().flattened(tolerance).collect();
    let mut contours: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut contour_start = Point::new(0.0, 0.0);

    for ev in &events {
        match ev {
            PathEvent::Begin { at } => {
                current.clear();
                current.push(*at);
                contour_start = *at;
            }
            PathEvent::Line { from: _, to } => {
                current.push(*to);
            }
            PathEvent::End {
                last: _,
                first: _,
                close: false,
            } => {
                if !current.is_empty() {
                    contours.push((current.clone(), false));
                }
                current.clear();
            }
            PathEvent::End {
                last: _,
                first: _,
                close: true,
            } => {
                if !current.is_empty() {
                    current.push(contour_start);
                    contours.push((current.clone(), true));
                }
                current.clear();
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        contours.push((current, false));
    }

    let mut builder = Path::builder();
    for (contour, closed) in &contours {
        if contour.len() < 2 {
            continue;
        }
        if contour.len() == 2 {
            builder.begin(contour[0]);
            builder.line_to(contour[1]);
            if *closed {
                builder.close();
            } else {
                builder.end(false);
            }
            continue;
        }

        let n = contour.len();
        let last_idx = n - 1;

        // Rounded output for this contour: an endpoint, plus the control
        // point of the quadratic that rounds into it (`None` for straight
        // runs). Flattened points alone would only chamfer the corner.
        let mut out: Vec<(Point, Option<Point>)> = Vec::new();

        for i in 0..n {
            let p_curr = contour[i];

            // Open contour: first and last points aren't rounded (no adjacent edges).
            if !*closed && (i == 0 || i == last_idx) {
                out.push((p_curr, None));
                continue;
            }

            if i == last_idx && *closed {
                break;
            }

            let p_prev = if i == 0 {
                contour[last_idx - 1]
            } else {
                contour[i - 1]
            };
            let p_next = if i == last_idx {
                contour[1]
            } else {
                contour[i + 1]
            };

            let d1 = Vector::new(p_curr.x - p_prev.x, p_curr.y - p_prev.y); // incoming: prev -> curr
            let d2 = Vector::new(p_next.x - p_curr.x, p_next.y - p_curr.y); // outgoing: curr -> next
            let len1 = d1.length();
            let len2 = d2.length();

            if len1 < 0.0001 || len2 < 0.0001 {
                out.push((p_curr, None));
                continue;
            }

            let u1 = d1 / len1;
            let u2 = d2 / len2;
            let dot = u1.x * u2.x + u1.y * u2.y;
            let angle = dot.clamp(-1.0, 1.0).acos();
            let half_angle = angle * 0.5;

            if angle > std::f32::consts::PI - 0.001 || half_angle.sin().abs() < 0.001 {
                out.push((p_curr, None));
                continue;
            }

            let max_inset_frac = 0.49;
            let inset = radius / half_angle.tan();
            let inset1 = inset.min(len1 * max_inset_frac);
            let inset2 = inset.min(len2 * max_inset_frac);

            let start = Point::new(
                // along incoming edge, inset from curr
                p_curr.x - u1.x * inset1,
                p_curr.y - u1.y * inset1,
            );
            let end = Point::new(
                // along outgoing edge, inset from curr
                p_curr.x + u2.x * inset2,
                p_curr.y + u2.y * inset2,
            );

            out.push((start, None));
            out.push((end, Some(p_curr)));
        }

        if out.is_empty() {
            continue;
        }

        builder.begin(out[0].0);
        for (point, control) in out.iter().skip(1) {
            match control {
                Some(control) => {
                    builder.quadratic_bezier_to(*control, *point);
                }
                None => {
                    builder.line_to(*point);
                }
            }
        }
        if *closed {
            builder.close();
        } else {
            builder.end(false);
        }
    }

    builder.build()
}

/// Dash intervals must be an even, strictly positive, finite sequence. The
/// old `debug_assert!` let odd, zero, negative and NaN intervals reach
/// release builds, where they silently degraded to a solid or a
/// non-advancing dash pattern.
fn dash_intervals_valid(intervals: &[f32]) -> bool {
    intervals.len() >= 2
        && intervals.len().is_multiple_of(2)
        && intervals.iter().all(|len| len.is_finite() && *len > 0.0)
}

/// Validate the requested dash intervals and floor each one at `tolerance`.
///
/// A dash shorter than the flattening tolerance is sub-pixel geometry that
/// the tessellator cannot represent, so raising it to the tolerance costs no
/// visible detail, and it bounds the dash walk to
/// `segment_length / tolerance` steps. Returning `None` means "do not dash".
fn dash_intervals_for(intervals: &[f32], tolerance: f32) -> Option<Vec<f32>> {
    if !dash_intervals_valid(intervals) {
        return None;
    }
    let floor = tolerance.max(f32::MIN_POSITIVE);
    let floored: Vec<f32> = intervals.iter().map(|len| len.max(floor)).collect();
    let total: f32 = floored.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    Some(floored)
}

/// Dash state carried across segments and sub-paths (the pattern continues
/// across contours, as SVG requires).
struct DashState {
    interval_idx: usize,
    dist: f32,
    emitting: bool,
}

/// Walk one segment, emitting dash fragments. Shared by `Line` events and
/// the implicit closing edge of a closed contour, so no edge is left
/// undashed.
///
/// `intervals` must already be floored by [`dash_intervals_for`], which is
/// what bounds this loop: every step advances by at least the floor, so the
/// walk is `segment_length / floor` steps at worst. Without that a
/// sub-tolerance interval needs `length / interval` steps, which is ~10^10
/// for a 1e-9 em dash on a glyph outline.
fn dash_segment(
    from: Point,
    to: Point,
    intervals: &[f32],
    state: &mut DashState,
    builder: &mut PathBuilder,
    in_subpath: &mut bool,
) {
    let seg = to - from;
    let seg_len = seg.length();
    if seg_len <= 0.0 || seg_len.is_nan() {
        return;
    }
    let dir = seg / seg_len;
    let mut remaining = seg_len;
    let mut cur = from;
    while remaining > 0.0 {
        let interval = intervals[state.interval_idx];
        let take = (interval - state.dist).min(remaining);
        // Backstop for float drift: a step that no longer moves the cursor
        // cannot make progress.
        if take <= remaining * f32::EPSILON {
            break;
        }
        let next = Point::new(cur.x + dir.x * take, cur.y + dir.y * take);
        if state.emitting {
            if !*in_subpath {
                builder.begin(cur);
                *in_subpath = true;
            }
            builder.line_to(next);
        } else if *in_subpath {
            builder.end(false);
            *in_subpath = false;
        }
        cur = next;
        remaining -= take;
        state.dist += take;
        if state.dist >= interval {
            state.dist -= interval;
            state.interval_idx = (state.interval_idx + 1) % intervals.len();
            state.emitting = !state.emitting;
        }
    }
}

fn apply_dash_effect(path: &Path, intervals: &[f32], phase: f32, tolerance: f32) -> Path {
    if !phase.is_finite() {
        return path.clone();
    }
    let Some(intervals) = dash_intervals_for(intervals, tolerance) else {
        return path.clone();
    };
    let intervals = intervals.as_slice();
    let dash_len: f32 = intervals.iter().sum();

    let events: Vec<PathEvent> = path.iter().flattened(tolerance).collect();

    let mut builder = Path::builder();

    let norm_phase = {
        let mut p = phase % dash_len;
        if p < 0.0 {
            p += dash_len;
        }
        p
    };

    let mut state = DashState {
        interval_idx: 0,
        dist: 0.0,
        emitting: true,
    };
    {
        let mut acc = 0.0;
        for (i, &len) in intervals.iter().enumerate() {
            if norm_phase < acc + len {
                state.interval_idx = i;
                state.dist = norm_phase - acc;
                state.emitting = i % 2 == 0;
                break;
            }
            acc += len;
        }
    }

    let mut in_subpath = false;

    for ev in &events {
        match ev {
            PathEvent::Begin { at: _ } => {
                if in_subpath {
                    builder.end(false);
                    in_subpath = false;
                }
                // Dash continues across sub-paths; don't reset the state.
            }
            PathEvent::Line { from, to } => {
                dash_segment(
                    *from,
                    *to,
                    intervals,
                    &mut state,
                    &mut builder,
                    &mut in_subpath,
                );
            }
            PathEvent::End {
                last,
                first,
                close: true,
            } => {
                dash_segment(
                    *last,
                    *first,
                    intervals,
                    &mut state,
                    &mut builder,
                    &mut in_subpath,
                );
            }
            PathEvent::End { close: false, .. } => {}
            _ => {}
        }
    }

    // Fragments are open sub-paths: closing one would draw a chord back to
    // the fragment's own start, which is not an edge of the original path.
    if in_subpath {
        builder.end(false);
    }

    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_effect_terminates_open_and_closed_contours() {
        let mut builder = Path::builder();
        builder.begin(Point::new(0.0, 0.0));
        builder.line_to(Point::new(10.0, 0.0));
        builder.line_to(Point::new(0.0, 0.0));
        builder.end(false);
        builder.begin(Point::new(20.0, 0.0));
        builder.line_to(Point::new(30.0, 0.0));
        builder.line_to(Point::new(20.0, 10.0));
        builder.close();
        let path = builder.build();
        let result = apply_path_effect(&path, &PathEffect::Corner { radius: 2.0 }, 0.25);
        let closes: Vec<bool> = result
            .iter()
            .flattened(0.25)
            .filter_map(|event| match event {
                PathEvent::End { close, .. } => Some(close),
                _ => None,
            })
            .collect();
        assert_eq!(closes, vec![false, true]);
    }

    fn square() -> Path {
        let mut builder = Path::builder();
        builder.begin(Point::new(0.0, 0.0));
        builder.line_to(Point::new(10.0, 0.0));
        builder.line_to(Point::new(10.0, 10.0));
        builder.line_to(Point::new(0.0, 10.0));
        builder.close();
        builder.build()
    }

    /// A dashed closed contour must dash its closing edge too, and must not
    /// close a fragment (which would draw a chord back to the fragment's
    /// own first point).
    #[test]
    fn dash_walks_the_closing_edge_of_a_closed_contour() {
        let result = apply_path_effect(
            &square(),
            &PathEffect::Dash {
                intervals: vec![1.0, 1.0],
                phase: 0.0,
            },
            0.25,
        );
        let mut fragments = 0;
        let mut closed = 0;
        let mut longest = 0.0f32;
        for event in result.iter().flattened(0.25) {
            match event {
                PathEvent::Line { from, to } => {
                    longest = longest.max((to - from).length());
                }
                PathEvent::End { close, .. } => {
                    fragments += 1;
                    if close {
                        closed += 1;
                    }
                }
                _ => {}
            }
        }
        assert_eq!(closed, 0, "dash fragments must stay open");
        assert!(
            fragments >= 19,
            "40 units at 1-on/1-off is ~20 fragments, got {fragments}"
        );
        assert!(
            longest <= 1.0 + 1e-3,
            "on-length must respect the interval, got {longest}"
        );
    }

    #[test]
    fn dash_phase_shifts_the_whole_contour() {
        let first_fragment_end = |phase: f32| {
            let result = apply_path_effect(
                &square(),
                &PathEffect::Dash {
                    intervals: vec![1.0, 1.0],
                    phase,
                },
                0.25,
            );
            let mut begun = false;
            let mut end = None;
            for event in result.iter().flattened(0.25) {
                match event {
                    PathEvent::Begin { .. } => begun = true,
                    PathEvent::Line { to, .. } if begun => {
                        end = Some(to);
                        break;
                    }
                    _ => {}
                }
            }
            end.expect("a dash always emits something")
        };
        assert_eq!(first_fragment_end(0.0), Point::new(1.0, 0.0));
        assert_eq!(first_fragment_end(0.5), Point::new(0.5, 0.0));
    }

    /// A dash interval far below the flatten tolerance used to need
    /// `segment_length / interval` walk steps, around 10^10 for a 1e-9 em
    /// dash on a glyph outline, which pinned the render thread and grew the
    /// path buffer until the process died.
    #[test]
    fn sub_ulp_dash_intervals_terminate() {
        for intervals in [
            vec![1e-9, 1e-9],
            vec![f32::MIN_POSITIVE, f32::MIN_POSITIVE],
            vec![1e-30, 1e-30],
        ] {
            let result = apply_path_effect(
                &square(),
                &PathEffect::Dash {
                    intervals: intervals.clone(),
                    phase: 0.0,
                },
                0.25,
            );
            let count = result.iter().flattened(0.25).count();
            assert!(
                count < 10_000,
                "intervals {intervals:?} produced {count} events"
            );
        }
    }

    /// An interval sum that overflows to infinity would drop the phase.
    #[test]
    fn overflowing_dash_length_returns_the_path_unchanged() {
        let result = apply_path_effect(
            &square(),
            &PathEffect::Dash {
                intervals: vec![3e38, 3e38],
                phase: 1.0,
            },
            0.25,
        );
        assert_eq!(
            result.iter().flattened(0.25).count(),
            square().iter().flattened(0.25).count()
        );
    }

    /// Invalid dash input must be rejected in release, not asserted away.
    #[test]
    fn invalid_dash_input_returns_the_path_unchanged() {
        let cases: Vec<Vec<f32>> = vec![
            vec![],
            vec![1.0],
            vec![1.0, 1.0, 1.0],
            vec![0.0, 5.0],
            vec![1.0, -1.0],
            vec![f32::NAN, 1.0],
            vec![f32::INFINITY, 1.0],
        ];
        for intervals in cases {
            let result = apply_path_effect(
                &square(),
                &PathEffect::Dash {
                    intervals: intervals.clone(),
                    phase: 0.0,
                },
                0.25,
            );
            assert_eq!(
                result.iter().flattened(0.25).count(),
                square().iter().flattened(0.25).count(),
                "intervals {intervals:?} should pass through untouched"
            );
        }
        let nan_phase = apply_path_effect(
            &square(),
            &PathEffect::Dash {
                intervals: vec![1.0, 1.0],
                phase: f32::NAN,
            },
            0.25,
        );
        assert_eq!(
            nan_phase.iter().flattened(0.25).count(),
            square().iter().flattened(0.25).count()
        );
    }

    /// The corner effect must round, not chamfer.
    #[test]
    fn corner_effect_emits_curves() {
        let result = apply_path_effect(&square(), &PathEffect::Corner { radius: 1.0 }, 0.25);
        // Read the verbs unflattened: flattening a quadratic turns it back
        // into the chamfer this test exists to catch.
        let curves = result
            .iter()
            .filter(|event| matches!(event, PathEvent::Quadratic { .. } | PathEvent::Cubic { .. }))
            .count();
        assert_eq!(curves, 4, "a square has four corners to round");
    }
}
