use crate::Vec2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const TRANSPARENT: Color = Color(0, 0, 0, 0);
    pub const BLACK: Color = Color(0, 0, 0, 255);
    pub const WHITE: Color = Color(255, 255, 255, 255);

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color(r, g, b, 255)
    }
    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color(r, g, b, a)
    }
    pub fn from_hex(hex: &str) -> Self {
        let s = hex.trim_start_matches('#');
        let (r, g, b, a) = match s.len() {
            6 => (
                u8::from_str_radix(&s[0..2], 16).unwrap_or(0),
                u8::from_str_radix(&s[2..4], 16).unwrap_or(0),
                u8::from_str_radix(&s[4..6], 16).unwrap_or(0),
                255,
            ),
            8 => (
                u8::from_str_radix(&s[0..2], 16).unwrap_or(0),
                u8::from_str_radix(&s[2..4], 16).unwrap_or(0),
                u8::from_str_radix(&s[4..6], 16).unwrap_or(0),
                u8::from_str_radix(&s[6..8], 16).unwrap_or(255),
            ),
            _ => (0, 0, 0, 255),
        };
        Color(r, g, b, a)
    }
    pub fn with_alpha(self, a: u8) -> Self {
        Color(self.0, self.1, self.2, a)
    }

    pub fn to_linear(self) -> [f32; 4] {
        fn srgb_to_linear(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        let r = srgb_to_linear(self.0 as f32 / 255.0);
        let g = srgb_to_linear(self.1 as f32 / 255.0);
        let b = srgb_to_linear(self.2 as f32 / 255.0);
        let a = self.3 as f32 / 255.0;
        [r, g, b, a]
    }
}

/// Brush for filling shapes.
///
/// This can be a solid color or a gradient. Higher‑level APIs (Modifier,
/// widgets) should talk in terms of `Brush` rather than raw `Color` so that
/// gradients and future brush types (radial, image) can share the same path.
#[derive(Clone, Copy, Debug)]
pub enum Brush {
    /// Solid color fill
    Solid(Color),

    /// Linear gradient from `start` to `end` in local coordinates.
    ///
    /// The gradient is defined in the local space of the node being drawn
    /// (e.g. Rect's top‑left is (0,0), bottom‑right is (w,h)).
    Linear {
        start: Vec2,
        end: Vec2,
        start_color: Color,
        end_color: Color,
    },
    // Later can add Radial, Image, etc...
}

impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Brush::Solid(c)
    }
}

pub struct LinearGradient {
    pub start: Vec2,
    pub end: Vec2,
    pub start_color: Color,
    pub end_color: Color,
}

impl LinearGradient {
    pub fn vertical(top: Color, bottom: Color) -> Brush {
        Brush::Linear {
            start: Vec2 { x: 0.0, y: 0.0 },
            end: Vec2 { x: 0.0, y: 1.0 }, // normalized; interpreted in rect size
            start_color: top,
            end_color: bottom,
        }
    }

    pub fn horizontal(left: Color, right: Color) -> Brush {
        Brush::Linear {
            start: Vec2 { x: 0.0, y: 0.0 },
            end: Vec2 { x: 1.0, y: 0.0 },
            start_color: left,
            end_color: right,
        }
    }
}
