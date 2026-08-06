use lyon_path::iterator::PathIterator;
use lyon_path::math::Point;
use lyon_path::math::Vector;
use lyon_path::{Path, PathEvent};

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
    if radius <= 0.0 {
        return path.clone();
    }
    let events: Vec<PathEvent> = path.iter().flattened(tolerance).collect();
    let mut contours: Vec<Vec<Point>> = Vec::new();
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
                    contours.push(current.clone());
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
                    contours.push(current.clone());
                }
                current.clear();
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        contours.push(current);
    }

    let mut builder = Path::builder();
    for contour in &contours {
        if contour.len() < 2 {
            continue;
        }
        if contour.len() == 2 {
            builder.begin(contour[0]);
            builder.line_to(contour[1]);
            continue;
        }

        let n = contour.len();
        let last_idx = n - 1;
        let closed = (contour[0] - contour[last_idx]).square_length() < 0.0001;

        // Collect rounded output points for this contour.
        let mut out_pts: Vec<Point> = Vec::new();

        for i in 0..n {
            let p_curr = contour[i];

            // Open contour: first and last points aren't rounded (no adjacent edges).
            if !closed && (i == 0 || i == last_idx) {
                out_pts.push(p_curr);
                continue;
            }

            if i == last_idx && closed {
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
                out_pts.push(p_curr);
                continue;
            }

            let u1 = d1 / len1;
            let u2 = d2 / len2;
            let dot = u1.x * u2.x + u1.y * u2.y;
            let angle = dot.clamp(-1.0, 1.0).acos();
            let half_angle = angle * 0.5;

            if angle > std::f32::consts::PI - 0.001 || half_angle.sin().abs() < 0.001 {
                out_pts.push(p_curr);
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

            out_pts.push(start);
            out_pts.push(end);
        }

        if out_pts.is_empty() {
            continue;
        }

        builder.begin(out_pts[0]);
        for pt in out_pts.iter().skip(1) {
            builder.line_to(*pt);
        }
        if closed {
            builder.close();
        }
    }

    builder.build()
}

fn apply_dash_effect(path: &Path, intervals: &[f32], phase: f32, tolerance: f32) -> Path {
    if intervals.is_empty() || intervals.iter().sum::<f32>() <= 0.0 {
        return path.clone();
    }
    debug_assert!(
        intervals.len() >= 2 && intervals.len().is_multiple_of(2),
        "Dash intervals must have even length (>=2), got {}",
        intervals.len()
    );

    let events: Vec<PathEvent> = path.iter().flattened(tolerance).collect();
    let dash_len: f32 = intervals.iter().sum();

    let mut builder = Path::builder();

    let norm_phase = {
        let mut p = phase % dash_len;
        if p < 0.0 {
            p += dash_len;
        }
        p
    };

    let mut interval_idx = 0;
    let mut offset_in_interval = 0.0;
    let mut emitting = true;
    {
        let mut acc = 0.0;
        for (i, &len) in intervals.iter().enumerate() {
            if norm_phase < acc + len {
                interval_idx = i;
                offset_in_interval = norm_phase - acc;
                emitting = i % 2 == 0;
                break;
            }
            acc += len;
        }
    }

    let mut dash_dist = offset_in_interval;
    let mut in_subpath = false;

    for ev in &events {
        match ev {
            PathEvent::Begin { at: _ } => {
                if in_subpath {
                    builder.close();
                    in_subpath = false;
                }
                // Dash continues across sub-paths; don't reset dash_dist.
            }
            PathEvent::Line { from, to } => {
                let seg = *to - *from;
                let seg_len = seg.length();
                if seg_len < 0.0001 {
                    continue;
                }
                let dir = seg / seg_len;

                let mut remaining = seg_len;
                let mut cur = *from;

                while remaining > 0.0 {
                    let seg_avail = intervals[interval_idx] - dash_dist;
                    let take = seg_avail.min(remaining);

                    if take > 0.0 {
                        let next = Point::new(cur.x + dir.x * take, cur.y + dir.y * take);
                        if emitting {
                            if !in_subpath {
                                builder.begin(cur);
                                in_subpath = true;
                            }
                            builder.line_to(next);
                        } else if in_subpath {
                            builder.close();
                            in_subpath = false;
                        }
                        cur = next;
                    }

                    remaining -= take;
                    dash_dist += take;

                    if (dash_dist - intervals[interval_idx]).abs() < 0.0001
                        || dash_dist >= intervals[interval_idx]
                    {
                        dash_dist = 0.0;
                        interval_idx = (interval_idx + 1) % intervals.len();
                        emitting = !emitting;
                    }
                }
            }
            PathEvent::End {
                last: _,
                first: _,
                close,
            } if in_subpath => {
                if *close {
                    builder.close();
                }
                in_subpath = false;
            }
            _ => {}
        }
    }

    if in_subpath {
        builder.close();
    }

    builder.build()
}
