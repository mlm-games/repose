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
        let p = self.apply_to_point(Vec2 { x: r.x, y: r.y });
        Rect {
            x: p.x,
            y: p.y,
            w: r.w * self.scale_x,
            h: r.h * self.scale_y,
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
