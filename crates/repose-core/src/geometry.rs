#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };
}

impl std::ops::Add for Vec2 {
    type Output = Vec2;
    fn add(self, other: Vec2) -> Vec2 {
        Vec2 {
            x: self.x + other.x,
            y: self.y + other.y,
        }
    }
}

impl std::ops::Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, other: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - other.x,
            y: self.y - other.y,
        }
    }
}

impl std::ops::Neg for Vec2 {
    type Output = Vec2;
    fn neg(self) -> Vec2 {
        Vec2 {
            x: -self.x,
            y: -self.y,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transform {
    pub translate_x: f32,
    pub translate_y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate: f32, // radians
    /// Horizontal shear factor (`x += shear_x * y`), e.g. ASS `\fax`.
    pub shear_x: f32,
    /// Vertical shear factor (`y += shear_y * x`), e.g. ASS `\fay`.
    pub shear_y: f32,
    pub origin_x: f32,
    pub origin_y: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate: 0.0,
            shear_x: 0.0,
            shear_y: 0.0,
            origin_x: 0.5,
            origin_y: 0.5,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            translate_x: x,
            translate_y: y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate: 0.0,
            shear_x: 0.0,
            shear_y: 0.0,
            origin_x: 0.5,
            origin_y: 0.5,
        }
    }

    /// Forward 2x2 linear part (row-major `[m00, m01, m10, m11]`):
    /// `M = R(rotate) * H(shear) * S(scale)` applied before translation.
    pub fn linear(&self) -> [f32; 4] {
        let c = self.rotate.cos();
        let s = self.rotate.sin();
        let hsx = self.scale_x;
        let hsy = self.scale_y;
        let k00 = hsx;
        let k01 = self.shear_x * hsy;
        let k10 = self.shear_y * hsx;
        let k11 = hsy;
        [
            c * k00 - s * k10,
            c * k01 - s * k11,
            s * k00 + c * k10,
            s * k01 + c * k11,
        ]
    }

    /// Inverse of [`linear`](Self::linear), or `None` when singular.
    pub fn inverse_linear(&self) -> Option<[f32; 4]> {
        let m = self.linear();
        let det = m[0] * m[3] - m[1] * m[2];
        if det.abs() < 1e-12 {
            return None;
        }
        Some([m[3] / det, -m[1] / det, -m[2] / det, m[0] / det])
    }

    /// Whether scale/rotation/shear are all identity (translation may apply).
    pub fn linear_is_identity(&self) -> bool {
        self.scale_x == 1.0
            && self.scale_y == 1.0
            && self.rotate == 0.0
            && self.shear_x == 0.0
            && self.shear_y == 0.0
    }

    pub fn apply_to_point(&self, p: Vec2) -> Vec2 {
        let ox = self.origin_x;
        let oy = self.origin_y;
        let m = self.linear();
        let x = p.x - ox;
        let y = p.y - oy;

        Vec2 {
            x: m[0] * x + m[1] * y + ox + self.translate_x,
            y: m[2] * x + m[3] * y + oy + self.translate_y,
        }
    }

    pub fn apply_to_rect(&self, r: Rect) -> Rect {
        let ox = r.x + r.w * self.origin_x;
        let oy = r.y + r.h * self.origin_y;
        let m = self.linear();
        let corners = [
            Vec2 { x: r.x, y: r.y },
            Vec2 {
                x: r.x + r.w,
                y: r.y,
            },
            Vec2 {
                x: r.x,
                y: r.y + r.h,
            },
            Vec2 {
                x: r.x + r.w,
                y: r.y + r.h,
            },
        ];
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for c in corners {
            let x = c.x - ox;
            let y = c.y - oy;
            let tx = m[0] * x + m[1] * y + ox + self.translate_x;
            let ty = m[2] * x + m[3] * y + oy + self.translate_y;
            min_x = min_x.min(tx);
            min_y = min_y.min(ty);
            max_x = max_x.max(tx);
            max_y = max_y.max(ty);
        }
        Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        }
    }

    /// Compose two transforms (`self` outer, `other` inner) for a
    /// transform stack: `current.combine(pushed)` where `pushed` is the
    /// newly pushed (inner) node.
    ///
    /// Returns a transform such that `combined.apply_to_point(p) ==
    /// self.apply_to_point(other.apply_to_point(p))`.
    ///
    /// The linear part is composed exactly (via polar decomposition of the
    /// 2x2 product); origins are inherited from `self`, matching the
    /// previous behaviour for the shear-free cases.
    pub fn combine(&self, other: &Transform) -> Transform {
        let a = self.linear();
        let b = other.linear();
        let m = [
            a[0] * b[0] + a[1] * b[2],
            a[0] * b[1] + a[1] * b[3],
            a[2] * b[0] + a[3] * b[2],
            a[2] * b[1] + a[3] * b[3],
        ];
        let translate_x = a[0] * other.translate_x + a[1] * other.translate_y + self.translate_x;
        let translate_y = a[2] * other.translate_x + a[3] * other.translate_y + self.translate_y;

        let (scale_x, scale_y, rotate, shear_x, shear_y) =
            decompose_linear(m).unwrap_or((1.0, 1.0, 0.0, 0.0, 0.0));

        Transform {
            translate_x,
            translate_y,
            scale_x,
            scale_y,
            rotate,
            shear_x,
            shear_y,
            origin_x: self.origin_x,
            origin_y: self.origin_y,
        }
    }
}

/// Split a 2x2 matrix `[m00, m01, m10, m11]` into
/// `(scale_x, scale_y, rotate, shear_x, shear_y)` such that
/// `M = R(rotate) * H(shear) * S(scale)`, or `None` when degenerate.
///
/// Uses polar decomposition (`M = R * K` with symmetric `K`), then reads
/// scale/shear off `K`. Reflections fold their sign into `scale_x`.
fn decompose_linear(m: [f32; 4]) -> Option<(f32, f32, f32, f32, f32)> {
    let (mut a, mut b, c, d) = (m[0], m[1], m[2], m[3]);
    let mut angle_sign = 1.0;
    if a * d - b * c < 0.0 {
        a = -a;
        b = -b;
        angle_sign = -1.0;
    }
    let e = a * a + c * c;
    let f = a * b + c * d;
    let g = b * b + d * d;
    let det_p = (e * g - f * f).max(0.0);
    let s = (e + g + 2.0 * det_p.sqrt()).sqrt();
    if !(s > 1e-12) {
        return None;
    }
    let root_det = det_p.sqrt();
    let k00 = (e + root_det) / s;
    let k01 = f / s;
    let k10 = f / s;
    let k11 = (g + root_det) / s;
    let det_k = (k00 * k11 - k01 * k10).max(1e-24);
    let r00 = (a * k11 - b * k10) / det_k;
    let r10 = (c * k11 - d * k10) / det_k;
    let rotate = r10.atan2(r00);
    let (sx, sy) = (k00, k11);
    if sx.abs() < 1e-12 || sy.abs() < 1e-12 {
        return None;
    }
    if angle_sign < 0.0 {
        return Some((-sx, sy, -rotate, -(k01 / sy), -(k10 / sx)));
    }
    Some((sx, sy, rotate, k01 / sy, k10 / sx))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    fn transform(tx: f32, ty: f32, sx: f32, sy: f32, rot: f32, hx: f32, hy: f32) -> Transform {
        Transform {
            translate_x: tx,
            translate_y: ty,
            scale_x: sx,
            scale_y: sy,
            rotate: rot,
            shear_x: hx,
            shear_y: hy,
            origin_x: 0.0,
            origin_y: 0.0,
        }
    }

    #[test]
    fn shear_free_matches_legacy_formula() {
        let t = transform(5.0, -3.0, 2.0, 0.5, 0.7, 0.0, 0.0);
        let p = Vec2 { x: 11.0, y: 7.0 };
        let got = t.apply_to_point(p);
        let (c, s) = (0.7f32.cos(), 0.7f32.sin());
        let x = (p.x) * 2.0;
        let y = (p.y) * 0.5;
        assert!(approx(got.x, x * c - y * s + 5.0));
        assert!(approx(got.y, x * s + y * c - 3.0));
    }

    #[test]
    fn shear_order_is_scale_then_shear_then_rotate() {
        let t = transform(0.0, 0.0, 2.0, 3.0, 0.0, 1.0, 0.0);
        let got = t.apply_to_point(Vec2 { x: 1.0, y: 1.0 });
        assert!(approx(got.x, 2.0 + 3.0));
        assert!(approx(got.y, 3.0));
    }

    #[test]
    fn combine_round_trips_through_apply() {
        let cases = [
            (
                transform(3.0, 4.0, 1.0, 1.0, 0.0, 0.0, 0.0),
                transform(0.0, 0.0, 2.0, 2.0, 0.0, 0.0, 0.0),
            ),
            (
                transform(1.0, 2.0, 1.5, 0.5, 0.6, 0.0, 0.0),
                transform(-2.0, 1.0, 0.7, 1.3, -0.4, 0.0, 0.0),
            ),
            (
                transform(0.0, 0.0, 2.0, 0.5, 0.9, 0.0, 0.0),
                transform(4.0, -1.0, 1.0, 1.0, 0.3, 0.0, 0.0),
            ),
            (
                transform(2.0, 0.0, 1.0, 1.0, 0.2, 0.8, -0.3),
                transform(0.0, 5.0, 1.2, 0.9, -0.5, 0.4, 0.1),
            ),
            (
                transform(0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0),
                transform(7.0, 7.0, 1.0, 1.0, 1.1, 0.2, 0.0),
            ),
        ];
        let points = [
            Vec2 { x: 0.0, y: 0.0 },
            Vec2 { x: 10.0, y: -4.0 },
            Vec2 { x: -3.5, y: 8.25 },
            Vec2 { x: 100.0, y: 200.0 },
        ];
        for (a, b) in cases {
            let combined = a.combine(&b);
            for p in points {
                let expect = a.apply_to_point(b.apply_to_point(p));
                let got = combined.apply_to_point(p);
                assert!(
                    approx(got.x, expect.x) && approx(got.y, expect.y),
                    "combine mismatch: {a:?} then {b:?} at {p:?}: got {got:?}, want {expect:?}"
                );
            }
        }
    }

    #[test]
    fn inverse_linear_round_trips() {
        let t = transform(3.0, -2.0, 1.5, 0.75, 0.8, 0.6, -0.2);
        let m = t.linear();
        let n = t.inverse_linear().expect("invertible");
        let id = [
            n[0] * m[0] + n[1] * m[2],
            n[0] * m[1] + n[1] * m[3],
            n[2] * m[0] + n[3] * m[2],
            n[2] * m[1] + n[3] * m[3],
        ];
        assert!(approx(id[0], 1.0) && approx(id[3], 1.0));
        assert!(approx(id[1], 0.0) && approx(id[2], 0.0));
    }
}
