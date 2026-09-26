use std::rc::Rc;

use crate::{
    Brush, Color, ImageAlignment, ImageFilter, ImageFit, ImageHandle, ImageSourceRect, Px, Rect,
    Scene, SceneNode,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlVisualState {
    pub alpha: f32,
    pub enabled: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub dragged: bool,
    pub focused: bool,
}

impl Default for ControlVisualState {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            enabled: true,
            hovered: false,
            pressed: false,
            dragged: false,
            focused: false,
        }
    }
}

pub type ControlPainter = Rc<dyn Fn(&mut Scene, Rect, ControlVisualState)>;

#[derive(Clone)]
#[non_exhaustive]
pub enum ControlVisual {
    Rect {
        brush: Brush,
        radius: [Px; 4],
    },
    Image {
        handle: ImageHandle,
        source_rect: Option<ImageSourceRect>,
        tint: Color,
        fit: ImageFit,
        filter: ImageFilter,
        alignment: ImageAlignment,
    },
    Custom(ControlPainter),
}

impl ControlVisual {
    pub fn rect(brush: Brush, radius: [Px; 4]) -> Self {
        Self::Rect { brush, radius }
    }

    pub fn image(handle: ImageHandle, source_rect: Option<ImageSourceRect>, tint: Color) -> Self {
        Self::Image {
            handle,
            source_rect,
            tint,
            fit: ImageFit::FillBounds,
            filter: ImageFilter::Linear,
            alignment: ImageAlignment::Center,
        }
    }

    pub fn custom(painter: impl Fn(&mut Scene, Rect, ControlVisualState) + 'static) -> Self {
        Self::Custom(Rc::new(painter))
    }

    pub fn image_fit(mut self, fit: ImageFit) -> Self {
        if let Self::Image { fit: current, .. } = &mut self {
            *current = fit;
        }
        self
    }

    pub fn image_filter(mut self, filter: ImageFilter) -> Self {
        if let Self::Image {
            filter: current, ..
        } = &mut self
        {
            *current = filter;
        }
        self
    }

    pub fn image_alignment(mut self, alignment: ImageAlignment) -> Self {
        if let Self::Image {
            alignment: current, ..
        } = &mut self
        {
            *current = alignment;
        }
        self
    }

    pub fn paint(&self, scene: &mut Scene, rect: Rect, state: ControlVisualState) {
        match self {
            Self::Rect { brush, radius } => scene.nodes.push(SceneNode::Rect {
                rect,
                brush: scale_brush_alpha(*brush, state.alpha),
                radius: *radius,
            }),
            Self::Image {
                handle,
                source_rect,
                tint,
                fit,
                filter,
                alignment,
            } => scene.nodes.push(SceneNode::Image {
                rect,
                handle: *handle,
                tint: scale_color_alpha(*tint, state.alpha),
                fit: *fit,
                filter: *filter,
                source_rect: *source_rect,
                alignment: *alignment,
            }),
            Self::Custom(painter) => painter(scene, rect, state),
        }
    }

    pub fn custom_identity(&self) -> Option<usize> {
        match self {
            Self::Custom(painter) => Some(Rc::as_ptr(painter) as *const () as usize),
            _ => None,
        }
    }
}

impl std::fmt::Debug for ControlVisual {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rect { brush, radius } => f
                .debug_struct("Rect")
                .field("brush", brush)
                .field("radius", radius)
                .finish(),
            Self::Image {
                handle,
                source_rect,
                tint,
                fit,
                filter,
                alignment,
            } => f
                .debug_struct("Image")
                .field("handle", handle)
                .field("source_rect", source_rect)
                .field("tint", tint)
                .field("fit", fit)
                .field("filter", filter)
                .field("alignment", alignment)
                .finish(),
            Self::Custom(_) => f
                .debug_struct("Custom")
                .field("identity", &self.custom_identity())
                .finish(),
        }
    }
}

impl PartialEq for ControlVisual {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Rect {
                    brush: a,
                    radius: ra,
                },
                Self::Rect {
                    brush: b,
                    radius: rb,
                },
            ) => a == b && ra == rb,
            (
                Self::Image {
                    handle: ah,
                    source_rect: ar,
                    tint: at,
                    fit: af,
                    filter: ax,
                    alignment: aa,
                },
                Self::Image {
                    handle: bh,
                    source_rect: br,
                    tint: bt,
                    fit: bf,
                    filter: bx,
                    alignment: ba,
                },
            ) => ah == bh && ar == br && at == bt && af == bf && ax == bx && aa == ba,
            (Self::Custom(a), Self::Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ControlVisualSet {
    pub normal: Option<ControlVisual>,
    pub hovered: Option<ControlVisual>,
    pub pressed: Option<ControlVisual>,
    pub dragged: Option<ControlVisual>,
    pub focused: Option<ControlVisual>,
    pub disabled: Option<ControlVisual>,
}

impl ControlVisualSet {
    pub fn resolve(&self, state: ControlVisualState) -> Option<&ControlVisual> {
        let selected = if !state.enabled {
            self.disabled.as_ref()
        } else if state.dragged {
            self.dragged.as_ref()
        } else if state.pressed {
            self.pressed.as_ref()
        } else if state.focused {
            self.focused.as_ref()
        } else if state.hovered {
            self.hovered.as_ref()
        } else {
            None
        };
        selected.or(self.normal.as_ref())
    }

    pub fn with_normal(mut self, visual: ControlVisual) -> Self {
        self.normal = Some(visual);
        self
    }

    pub fn with_hovered(mut self, visual: ControlVisual) -> Self {
        self.hovered = Some(visual);
        self
    }

    pub fn with_pressed(mut self, visual: ControlVisual) -> Self {
        self.pressed = Some(visual);
        self
    }

    pub fn with_dragged(mut self, visual: ControlVisual) -> Self {
        self.dragged = Some(visual);
        self
    }

    pub fn with_focused(mut self, visual: ControlVisual) -> Self {
        self.focused = Some(visual);
        self
    }

    pub fn with_disabled(mut self, visual: ControlVisual) -> Self {
        self.disabled = Some(visual);
        self
    }
}

fn scale_color_alpha(color: Color, alpha: f32) -> Color {
    Color(
        color.0,
        color.1,
        color.2,
        (color.3 as f32 * alpha.clamp(0.0, 1.0)) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_visual_state_priority_is_stable() {
        let set = ControlVisualSet {
            normal: Some(ControlVisual::image(1, None, Color::WHITE)),
            hovered: Some(ControlVisual::image(2, None, Color::WHITE)),
            focused: Some(ControlVisual::image(3, None, Color::WHITE)),
            dragged: Some(ControlVisual::image(4, None, Color::WHITE)),
            disabled: Some(ControlVisual::image(5, None, Color::WHITE)),
            ..Default::default()
        };
        let focused_hovered = ControlVisualState {
            hovered: true,
            focused: true,
            ..Default::default()
        };
        assert!(matches!(
            set.resolve(focused_hovered),
            Some(ControlVisual::Image { handle: 3, .. })
        ));
        let disabled_dragged = ControlVisualState {
            enabled: false,
            dragged: true,
            ..Default::default()
        };
        assert!(matches!(
            set.resolve(disabled_dragged),
            Some(ControlVisual::Image { handle: 5, .. })
        ));
    }
}

fn scale_brush_alpha(brush: Brush, alpha: f32) -> Brush {
    match brush {
        Brush::Solid(color) => Brush::Solid(scale_color_alpha(color, alpha)),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start,
            end,
            start_color: scale_color_alpha(start_color, alpha),
            end_color: scale_color_alpha(end_color, alpha),
        },
        Brush::LinearNormalized {
            start,
            end,
            start_color,
            end_color,
        } => Brush::LinearNormalized {
            start,
            end,
            start_color: scale_color_alpha(start_color, alpha),
            end_color: scale_color_alpha(end_color, alpha),
        },
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => Brush::Radial {
            center,
            radius,
            start_color: scale_color_alpha(start_color, alpha),
            end_color: scale_color_alpha(end_color, alpha),
        },
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => Brush::Sweep {
            center,
            start_color: scale_color_alpha(start_color, alpha),
            end_color: scale_color_alpha(end_color, alpha),
        },
    }
}
