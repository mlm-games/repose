use crate::units::Px;
use crate::view::Scene;

#[derive(Clone, Copy)]
pub struct GlyphRasterConfig {
    /// Rasterization size in physical pixels.
    pub px: Px,
}

pub trait RenderBackend {
    /// Configure the framebuffer for `width` x `height` and return the size
    /// actually used.
    ///
    /// Device texture limits can make the framebuffer smaller than the
    /// viewport, so the caller must adopt the returned size for layout and
    /// hit testing. Anything else lets the scene project against one size
    /// while input is resolved against another.
    fn configure_surface(&mut self, width: u32, height: u32) -> (u32, u32);
    fn frame(&mut self, scene: &Scene, glyph_cfg: GlyphRasterConfig) -> bool;
}
