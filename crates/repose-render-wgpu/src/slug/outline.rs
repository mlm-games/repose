use cosmic_text::Command;

/// A single quadratic Bézier curve in em-space coordinates.
///
/// Coordinates use a font Y‑up convention (matching Swash/cosmic-text output).
/// The shader flips Y when mapping to screen space.
#[derive(Clone, Debug)]
pub struct QuadCurve {
    pub p1: [f32; 2],
    pub p2: [f32; 2],
    pub p3: [f32; 2],
}

const MAX_CUBIC_DEPTH: u32 = 8;
/// Maximum allowed pixel-space error from cubic→quadratic approximation.
const MAX_PIXEL_ERROR: f32 = 0.5;

/// Convert raw outline commands into em-space quadratic Bézier curves.
/// `font_size` is the pixel size used when the commands were scaled.
pub fn commands_to_curves(commands: &[Command], font_size: f32) -> Option<Vec<QuadCurve>> {
    if commands.is_empty() {
        return None;
    }
    let inv_size = 1.0 / font_size;
    let tolerance = (MAX_PIXEL_ERROR / font_size).max(0.002);
    let mut curves = Vec::new();
    let mut cur = [0.0f32; 2];
    let mut contour_start = [0.0f32; 2];

    for cmd in commands {
        match *cmd {
            Command::MoveTo(p) => {
                cur = [p.x * inv_size, p.y * inv_size];
                contour_start = cur;
            }
            Command::LineTo(p) => {
                let p = [p.x * inv_size, p.y * inv_size];
                curves.push(QuadCurve {
                    p1: cur,
                    p2: [(cur[0] + p[0]) * 0.5, (cur[1] + p[1]) * 0.5],
                    p3: p,
                });
                cur = p;
            }
            Command::QuadTo(c, p) => {
                let c = [c.x * inv_size, c.y * inv_size];
                let p = [p.x * inv_size, p.y * inv_size];
                curves.push(QuadCurve {
                    p1: cur,
                    p2: c,
                    p3: p,
                });
                cur = p;
            }
            Command::CurveTo(c1, c2, p) => {
                let c1 = [c1.x * inv_size, c1.y * inv_size];
                let c2 = [c2.x * inv_size, c2.y * inv_size];
                let p = [p.x * inv_size, p.y * inv_size];
                cubic_to_quadratics(&cur, &c1, &c2, &p, &mut curves, tolerance, 0);
                cur = p;
            }
            Command::Close => {
                if point_dist2(&cur, &contour_start) > 1e-12 {
                    curves.push(QuadCurve {
                        p1: cur,
                        p2: [
                            (cur[0] + contour_start[0]) * 0.5,
                            (cur[1] + contour_start[1]) * 0.5,
                        ],
                        p3: contour_start,
                    });
                    cur = contour_start;
                }
            }
        }
    }

    if curves.is_empty() {
        None
    } else {
        Some(curves)
    }
}

/// Recursively subdivide a cubic until flat, then emit quadratic(s).
fn cubic_to_quadratics(
    p0: &[f32; 2],
    p1: &[f32; 2],
    p2: &[f32; 2],
    p3: &[f32; 2],
    out: &mut Vec<QuadCurve>,
    tolerance: f32,
    depth: u32,
) {
    if depth < MAX_CUBIC_DEPTH && !is_flat_enough(p0, p1, p2, p3, tolerance) {
        let p0p1 = midpoint(p0, p1);
        let p1p2 = midpoint(p1, p2);
        let p2p3 = midpoint(p2, p3);
        let left_c2 = midpoint(&p0p1, &p1p2);
        let right_c1 = midpoint(&p1p2, &p2p3);
        let mid = midpoint(&left_c2, &right_c1);

        cubic_to_quadratics(p0, &p0p1, &left_c2, &mid, out, tolerance, depth + 1);
        cubic_to_quadratics(&mid, &right_c1, &p2p3, p3, out, tolerance, depth + 1);
    } else {
        // Flat enough — single midpoint split produces 2 quadratics
        let mid = [
            (p0[0] + 3.0 * p1[0] + 3.0 * p2[0] + p3[0]) * 0.125,
            (p0[1] + 3.0 * p1[1] + 3.0 * p2[1] + p3[1]) * 0.125,
        ];
        let q1_c = midpoint(p0, p1);
        out.push(QuadCurve {
            p1: *p0,
            p2: q1_c,
            p3: mid,
        });
        let q2_c = midpoint(p2, p3);
        out.push(QuadCurve {
            p1: mid,
            p2: q2_c,
            p3: *p3,
        });
    }
}

fn midpoint(a: &[f32; 2], b: &[f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

fn point_dist2(a: &[f32; 2], b: &[f32; 2]) -> f32 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    dx * dx + dy * dy
}

/// Check whether the cubic P0–P3 is within `tolerance` of a quadratic approximation.
fn is_flat_enough(
    p0: &[f32; 2],
    p1: &[f32; 2],
    p2: &[f32; 2],
    p3: &[f32; 2],
    tolerance: f32,
) -> bool {
    let ux = p3[0] - p0[0];
    let uy = p3[1] - p0[1];
    let denom = ux * ux + uy * uy;
    if denom < 1e-12 {
        return point_dist2(p1, p0).max(point_dist2(p2, p0)) < tolerance * tolerance;
    }
    let inv_len = denom.sqrt().recip();
    let d1 = ((p1[0] - p0[0]) * uy - (p1[1] - p0[1]) * ux).abs() * inv_len;
    let d2 = ((p2[0] - p0[0]) * uy - (p2[1] - p0[1]) * ux).abs() * inv_len;
    d1.max(d2) < tolerance
}
