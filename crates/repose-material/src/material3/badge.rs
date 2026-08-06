#![allow(non_snake_case)]

use repose_core::*;
use repose_ui::{Box, ViewExt};

use super::*;

/// Configuration for [`Badge`].
#[derive(Clone, Debug)]
pub struct BadgeConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
}

impl Default for BadgeConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: BadgeDefaults::container_color(),
            content_color: BadgeDefaults::content_color(),
        }
    }
}

/// M3 Badge - a small notification indicator. If `content` is `None`, shows a
/// small 6dp dot; otherwise shows the content inside a 16dp pill.
pub fn Badge(content: Option<View>, config: BadgeConfig) -> View {
    match content {
        None => Box(Modifier::new()
            .size(BadgeDefaults::DOT_SIZE, BadgeDefaults::DOT_SIZE)
            .background(config.container_color)
            .clip_rounded(BadgeDefaults::DOT_SIZE * 0.5)
            .flex_shrink(0.0)
            .then(config.modifier)),
        Some(view) => Box(Modifier::new()
            .min_width(BadgeDefaults::LABEL_MIN_WIDTH)
            .height(BadgeDefaults::LABEL_HEIGHT)
            .background(config.container_color)
            .clip_rounded(BadgeDefaults::LABEL_HEIGHT * 0.5)
            .padding_values(PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            })
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .flex_shrink(0.0)
            .then(config.modifier))
        .child(with_content_color(config.content_color, move || view)),
    }
}

/// Configuration for [`BadgedBox`].
#[derive(Clone, Debug)]
pub struct BadgedBoxConfig {
    pub modifier: Modifier,
    /// Horizontal offset for the badge when it's a small dot.
    pub dot_offset_x: f32,
    /// Vertical offset for the badge when it's a small dot.
    pub dot_offset_y: f32,
    /// Horizontal offset for the badge when it has content.
    pub content_offset_x: f32,
    /// Vertical offset for the badge when it has content.
    pub content_offset_y: f32,
    /// When true, use `content_offset_*` (labeled badge). When false, use `dot_offset_*`.
    pub has_content: bool,
}

impl Default for BadgedBoxConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            dot_offset_x: BadgeDefaults::DOT_OFFSET_X,
            dot_offset_y: BadgeDefaults::DOT_OFFSET_Y,
            content_offset_x: BadgeDefaults::CONTENT_OFFSET_X,
            content_offset_y: BadgeDefaults::CONTENT_OFFSET_Y,
            has_content: false,
        }
    }
}

/// Wraps `content` and shows a `badge` anchored to the top-end corner.
pub fn BadgedBox(badge: View, content: View, config: BadgedBoxConfig) -> View {
    let (top, right) = if config.has_content {
        (
            config.content_offset_y - BadgeDefaults::LABEL_HEIGHT, // 14 - 16 = -2
            config.content_offset_x - BadgeDefaults::LABEL_MIN_WIDTH, // 12 - 16 = -4
        )
    } else {
        (
            config.dot_offset_y - BadgeDefaults::DOT_SIZE, // 6 - 6 = 0
            config.dot_offset_x - BadgeDefaults::DOT_SIZE, // 6 - 6 = 0
        )
    };

    Box(config.modifier.flex_shrink(0.0)).child((
        content,
        Box(Modifier::new()
            .absolute()
            .offset(None, Some(top), Some(right), None)
            .flex_shrink(0.0)
            .hit_passthrough())
        .child(badge),
    ))
}
