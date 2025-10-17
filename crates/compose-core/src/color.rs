#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const TRANSPARENT: Color = Color(0, 0, 0, 0);
    pub const BLACK: Color = Color(0, 0, 0, 255);
    pub const WHITE: Color = Color(255, 255, 255, 255);

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
