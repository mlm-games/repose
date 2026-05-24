#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
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
            Vec2 { x: r.x + r.w, y: r.y },
            Vec2 { x: r.x, y: r.y + r.h },
            Vec2 { x: r.x + r.w, y: r.y + r.h },
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

    pub fn combine(&self, other: &Transform) -> Transform {
        Transform {
            translate_x: self.translate_x + other.translate_x,
            translate_y: self.translate_y + other.translate_y,
            scale_x: self.scale_x * other.scale_x,
            scale_y: self.scale_y * other.scale_y,
            rotate: self.rotate + other.rotate,
        }
    }
}
