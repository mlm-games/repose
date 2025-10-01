#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 { pub x: f32, pub y: f32 }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size { pub width: f32, pub height: f32 }

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }

impl Rect {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }
}
