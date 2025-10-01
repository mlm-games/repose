use crate::{Color, Size};

#[derive(Clone, Debug)]
pub struct Border { pub width: f32, pub color: Color, pub radius: f32 }

#[derive(Clone, Debug, Default)]
pub struct Modifier {
    pub padding: Option<f32>,
    pub size: Option<Size>,
    pub fill_max: bool,
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub click: bool,
    pub semantics_label: Option<String>,
    pub z_index: f32,
    pub clip_rounded: Option<f32>,
}

impl Default for Border {
    fn default() -> Self { Border { width: 1.0, color: Color::WHITE, radius: 0.0 } }
}

impl Modifier {
    pub fn new() -> Self { Self::default() }
    pub fn padding(mut self, px: f32) -> Self { self.padding = Some(px); self }
    pub fn size(mut self, w: f32, h: f32) -> Self { self.size = Some(Size{width:w, height:h}); self }
    pub fn fill_max_size(mut self) -> Self { self.fill_max = true; self }
    pub fn background(mut self, color: Color) -> Self { self.background = Some(color); self }
    pub fn border(mut self, width: f32, color: Color, radius: f32) -> Self {
        self.border = Some(Border{ width, color, radius }); self
    }
    pub fn clickable(mut self) -> Self { self.click = true; self }
    pub fn semantics(mut self, label: impl Into<String>) -> Self { self.semantics_label = Some(label.into()); self }
    pub fn z_index(mut self, z: f32) -> Self { self.z_index = z; self }
    pub fn clip_rounded(mut self, r: f32) -> Self { self.clip_rounded = Some(r); self }
}
