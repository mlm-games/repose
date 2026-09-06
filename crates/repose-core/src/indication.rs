use crate::{InteractionSource, Rect, Scene};

/// Marker trait for indication implementations.
pub trait Indication: std::fmt::Debug {}

/// Factory that creates a drawable indication node bound to an interaction source.
pub trait IndicationNodeFactory: Indication {
    fn create(&self, interaction_source: &InteractionSource) -> Box<dyn IndicationDrawNode>;
}

/// A drawable indication node. The layout engine calls `draw()` during the paint
/// pass to emit scene nodes for visual feedback (ripple, overlay, focus ring).
pub trait IndicationDrawNode {
    /// Draw the indication into `scene` at the given `rect` (in physical pixels).
    /// `radius` is the component's corner radii (px) for shape-matched overlays.
    /// `alpha` is the accumulated compositing alpha from ancestor modifiers.
    fn draw(&self, scene: &mut Scene, rect: Rect, radius: [f32; 4], alpha: f32);
}
