use crate::units::Px;
use crate::view::Scene;

#[derive(Clone, Copy)]
pub struct GlyphRasterConfig {
    /// Rasterization size in physical pixels.
    pub px: Px,
}

pub trait RenderBackend {
    fn configure_surface(&mut self, width: u32, height: u32);
    fn frame(&mut self, scene: &Scene, glyph_cfg: GlyphRasterConfig);
}
