#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const TRANSPARENT: Color = Color(0,0,0,0);
    pub const BLACK: Color = Color(0,0,0,255);
    pub const WHITE: Color = Color(255,255,255,255);

    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self { Color(r,g,b,a) }
    pub fn from_hex(hex: &str) -> Self {
        let s = hex.trim_start_matches('#');
        let (r,g,b,a) = match s.len() {
            6 => (u8::from_str_radix(&s[0..2],16).unwrap_or(0),
                  u8::from_str_radix(&s[2..4],16).unwrap_or(0),
                  u8::from_str_radix(&s[4..6],16).unwrap_or(0), 255),
            8 => (u8::from_str_radix(&s[0..2],16).unwrap_or(0),
                  u8::from_str_radix(&s[2..4],16).unwrap_or(0),
                  u8::from_str_radix(&s[4..6],16).unwrap_or(0),
                  u8::from_str_radix(&s[6..8],16).unwrap_or(255)),
            _ => (0,0,0,255)
        };
        Color(r,g,b,a)
    }

    pub fn to_linear(self) -> [f32;4] {
        let [r,g,b,a] = [self.0,self.1,self.2,self.3];
        [r as f32/255.0, g as f32/255.0, b as f32/255.0, a as f32/255.0]
    }
}
