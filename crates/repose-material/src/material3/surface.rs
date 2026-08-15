#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, TextStyle, ViewExt};

use super::*;

/// Configuration for [`Surface`].
#[derive(Clone, Debug)]
pub struct SurfaceConfig {
    pub modifier: Modifier,
    pub enabled: bool,
    pub color: Color,
    pub content_color: Color,
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub border: Option<(f32, Color)>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            color: SurfaceDefaults::color(),
            content_color: SurfaceDefaults::content_color(),
            shape_radius: SurfaceDefaults::SHAPE_RADIUS,
            tonal_elevation: SurfaceDefaults::TONAL_ELEVATION,
            shadow_elevation: SurfaceDefaults::SHADOW_ELEVATION,
            border: None,
            interaction_source: None,
        }
    }
}

/// M3 Surface - a basic container with shape, color, elevation, and border.
/// Sets the ContentColor local for children based on the surface color.
pub fn Surface(config: SurfaceConfig, content: impl FnOnce() -> View) -> View {
    let sf_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    let mut m = Modifier::new()
        .background(config.color)
        .clip_rounded(config.shape_radius)
        .interaction_source(&sf_source)
        .then(config.modifier);
    if config.tonal_elevation > 0.0 {
        m = m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            focused: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: 0.0,
        });
    }
    if config.shadow_elevation > 0.0 {
        m = m.shadow(config.shadow_elevation, 0.0);
    }
    if let Some((w, c)) = config.border {
        m = m.border(w, c, config.shape_radius);
    }
    Box(m).color(config.content_color).child(content())
}
