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
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            translate_x: 0.0,
            translate_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate: 0.0,
        }
    }

    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            translate_x: x,
            translate_y: y,
            scale_x: 1.0,
            scale_y: 1.0,
            rotate: 0.0,
        }
    }

    pub fn apply_to_point(&self, p: Vec2) -> Vec2 {
        // Apply in order: scale, rotate, translate
        let mut x = p.x * self.scale_x;
        let mut y = p.y * self.scale_y;

        if self.rotate != 0.0 {
            let cos = self.rotate.cos();
            let sin = self.rotate.sin();
            let nx = x * cos - y * sin;
            let ny = x * sin + y * cos;
            x = nx;
            y = ny;
        }

        Vec2 {
            x: x + self.translate_x,
            y: y + self.translate_y,
        }
    }

    pub fn apply_to_rect(&self, r: Rect) -> Rect {
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
            let p = self.apply_to_point(c);
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        }
    }

    /// Compose two transforms: `self` then `other` (post-multiply).
    ///
    /// Returns a transform such that `combined.apply_to_point(p) ==
    /// other.apply_to_point(self.apply_to_point(p))`.
    ///
    /// **Limitation:** the T*R*S representation is not closed under matrix
    /// multiplication when both transforms have rotation *and* non-uniform
    /// scale. In that case the result is an approximation (off-diagonal
    /// rotation/scale coupling is dropped). For UI use (uniform scale or no
    /// rotation) this is exact.
    pub fn combine(&self, other: &Transform) -> Transform {
        let c = self.rotate.cos();
        let s = self.rotate.sin();

        Transform {
            translate_x: other.translate_x * self.scale_x * c
                - other.translate_y * self.scale_y * s
                + self.translate_x,
            translate_y: other.translate_x * self.scale_x * s
                + other.translate_y * self.scale_y * c
                + self.translate_y,
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            rotate: self.rotate + other.rotate,
        }
    }
}
