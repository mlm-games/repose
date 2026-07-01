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
    /// `alpha` is the accumulated compositing alpha from ancestor modifiers.
    fn draw(&self, scene: &mut Scene, rect: Rect, alpha: f32);
}

/// A debug indication that draws colored overlays for hover/press/focus.
#[deprecated]
#[derive(Clone, Debug)]
pub struct DebugIndication {
    pub press_color: crate::Color,
    pub hover_color: crate::Color,
    pub focus_color: crate::Color,
}

impl Default for DebugIndication {
    fn default() -> Self {
        Self {
            press_color: crate::Color(0, 0, 255, 40),
            hover_color: crate::Color::TRANSPARENT,
            focus_color: crate::Color::TRANSPARENT,
        }
    }
}

impl Indication for DebugIndication {}

impl IndicationNodeFactory for DebugIndication {
    fn create(&self, interaction_source: &InteractionSource) -> Box<dyn IndicationDrawNode> {
        Box::new(DebugIndicationDrawNode {
            interaction_source: interaction_source.clone(),
            press_color: self.press_color,
            hover_color: self.hover_color,
            focus_color: self.focus_color,
        })
    }
}

struct DebugIndicationDrawNode {
    interaction_source: InteractionSource,
    press_color: crate::Color,
    hover_color: crate::Color,
    focus_color: crate::Color,
}

impl IndicationDrawNode for DebugIndicationDrawNode {
    fn draw(&self, scene: &mut Scene, rect: Rect, alpha: f32) {
        let pressed = self.interaction_source.collect_is_pressed();
        let hovered = self.interaction_source.collect_is_hovered();
        let _focused = self.interaction_source.collect_is_focused();

        // Draw press overlay
        if pressed {
            scene.nodes.push(crate::SceneNode::Rect {
                rect,
                brush: self
                    .press_color
                    .with_alpha_f32(self.press_color.3 as f32 / 255.0 * alpha)
                    .into(),
                radius: [0.0; 4],
            });
        }
        // Draw hover overlay
        if hovered && !pressed {
            scene.nodes.push(crate::SceneNode::Rect {
                rect,
                brush: self
                    .hover_color
                    .with_alpha_f32(self.hover_color.3 as f32 / 255.0 * alpha)
                    .into(),
                radius: [0.0; 4],
            });
        }
    }
}
