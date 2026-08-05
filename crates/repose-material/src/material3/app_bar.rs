#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use repose_core::NestedScrollConnection;
use repose_core::*;
use repose_ui::{
    Box, Column, Row,
    ViewExt,
};

use super::*;

use super::util::lerp_color;
/// Color slots for [`TopAppBar`].
#[derive(Clone, Copy, Debug)]
pub struct TopAppBarColors {
    pub container_color: Color,
    pub scrolled_container_color: Color,
    pub navigation_icon_content_color: Color,
    pub title_content_color: Color,
    pub subtitle_content_color: Color,
    pub action_icon_content_color: Color,
}

impl TopAppBarColors {
    pub fn container_color(&self, scroll_fraction: f32) -> Color {
        lerp_color(
            self.container_color,
            self.scrolled_container_color,
            scroll_fraction.clamp(0.0, 1.0),
        )
    }
}

impl Default for TopAppBarColors {
    fn default() -> Self {
        Self {
            container_color: TopAppBarDefaults::container_color(),
            scrolled_container_color: TopAppBarDefaults::scrolled_container_color(),
            navigation_icon_content_color: TopAppBarDefaults::navigation_icon_content_color(),
            title_content_color: TopAppBarDefaults::title_content_color(),
            subtitle_content_color: TopAppBarDefaults::subtitle_content_color(),
            action_icon_content_color: TopAppBarDefaults::action_icon_content_color(),
        }
    }
}

/// Scroll response mode for [`TopAppBarScrollBehavior`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TopAppBarScrollMode {
    /// Always visible, no scroll response.
    Pinned,
    /// Collapses upward when scrolling down, expands when scrolling up.
    EnterAlways,
}

/// Drives scroll-based collapsing/expanding of a TopAppBar.
///
/// Create one, pass its [`nested_scroll_connection`](TopAppBarScrollBehavior::nested_scroll_connection)
/// to a lazy list's [`set_nested_scroll_parent`] method, and set the
/// resulting [`collapsed_offset`](TopAppBarScrollBehavior::collapsed_offset)
/// on the TopAppBar via [`TopAppBarConfig::scroll_offset`].
pub struct TopAppBarScrollBehavior {
    pub collapsed_offset: Signal<f32>,
    pub height: f32,
    pub collapsed_height: f32,
    pub mode: TopAppBarScrollMode,
    _pending: Rc<Cell<f32>>,
}

impl TopAppBarScrollBehavior {
    pub fn new(height: f32, collapsed_height: f32, mode: TopAppBarScrollMode) -> Self {
        Self {
            collapsed_offset: signal(0.0),
            height,
            collapsed_height,
            mode,
            _pending: Rc::new(Cell::new(0.0)),
        }
    }

    /// Returns a [`NestedScrollConnection`] that collapses the bar on
    /// downward scroll and expands on upward scroll.
    pub fn nested_scroll_connection(&self) -> NestedScrollConnection {
        let off = self.collapsed_offset.clone();
        let max_collapse = -(self.height - self.collapsed_height);
        let mode = self.mode;

        NestedScrollConnection::new().on_pre_scroll(move |d: Vec2, _source| -> Vec2 {
            if mode == TopAppBarScrollMode::Pinned {
                return Vec2::ZERO;
            }
            let current = off.get();
            if d.y > 0.0 {
                // Scrolling down -> collapse bar
                if current <= max_collapse {
                    return Vec2::ZERO;
                }
                let collapse_room = current - max_collapse;
                let consume = d.y.min(collapse_room);
                off.set(current - consume);
                repose_core::request_frame();
                Vec2 { x: 0.0, y: consume }
            } else {
                // Scrolling up -> expand bar
                if current >= 0.0 {
                    return Vec2::ZERO;
                }
                let expansion_room = -current;
                let consume = (-d.y).min(expansion_room);
                off.set(current + consume);
                repose_core::request_frame();
                Vec2 { x: 0.0, y: consume }
            }
        })
    }

    /// Returns the current collapsed offset (0 = fully expanded, negative = collapsed).
    pub fn offset(&self) -> f32 {
        self.collapsed_offset.get()
    }
}

/// Configuration for [`TopAppBar`].
#[derive(Clone, Debug)]
pub struct TopAppBarConfig {
    pub modifier: Modifier,
    pub colors: TopAppBarColors,
    pub height: f32,
    pub scroll_fraction: f32,
    /// Vertical translate offset (negative = collapsed upward).
    /// Set this from [`TopAppBarScrollBehavior::collapsed_offset`].
    pub scroll_offset: f32,
    pub window_insets: WindowInsets,
    pub content_padding: PaddingValues,
}

/// System window insets for top app bar padding.
#[derive(Clone, Copy, Debug)]
pub struct WindowInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
}

impl Default for WindowInsets {
    fn default() -> Self {
        Self {
            top: 0.0,
            bottom: 0.0,
            left: 0.0,
            right: 0.0,
        }
    }
}

impl Default for TopAppBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: TopAppBarColors::default(),
            height: TopAppBarDefaults::HEIGHT,
            scroll_fraction: 0.0,
            scroll_offset: 0.0,
            window_insets: WindowInsets::default(),
            content_padding: PaddingValues {
                left: 4.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            },
        }
    }
}

fn top_app_bar_layout(
    title: View,
    subtitle: Option<View>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
    centered: bool,
) -> View {
    let insets = config.window_insets;
    let bg = config.colors.container_color(config.scroll_fraction);
    let mut m = Modifier::new()
        .min_width(200.0)
        .height(config.height + insets.top)
        .background(bg)
        .translate(0.0, config.scroll_offset)
        .padding_values(PaddingValues {
            left: config.content_padding.left + insets.left,
            right: config.content_padding.right + insets.right,
            top: config.content_padding.top + insets.top,
            bottom: config.content_padding.bottom + insets.bottom,
        })
        .align_items(AlignItems::CENTER)
        .then(config.modifier);
    if centered {
        m = m.justify_content(JustifyContent::CENTER);
    }
    Row(m).child((
        navigation_icon.unwrap_or(Box(Modifier::new().width(16.0).fill_max_height())),
        Box(Modifier::new()
            .padding_values(PaddingValues {
                left: 16.0,
                right: 0.0,
                top: 0.0,
                bottom: 0.0,
            })
            .flex_grow(1.0))
        .child(
            Column(Modifier::new().justify_content(JustifyContent::CENTER)).child((
                Box(Modifier::new()).child(with_content_color(
                    config.colors.title_content_color,
                    || title,
                )),
                subtitle
                    .map(|s| {
                        Box(Modifier::new()).child(with_content_color(
                            config.colors.subtitle_content_color,
                            || s,
                        ))
                    })
                    .unwrap_or(Box(Modifier::new())),
            )),
        ),
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .clip_rounded(20.0))
        .child(
            actions
                .into_iter()
                .map(|a| {
                    with_content_color(config.colors.action_icon_content_color, move || a.clone())
                })
                .collect::<Vec<_>>(),
        ),
    ))
}

/// M3 Top App Bar (small). Displays a title with optional navigation icon,
/// subtitle, and trailing action buttons.
pub fn TopAppBar(
    title: View,
    subtitle: Option<View>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
) -> View {
    top_app_bar_layout(title, subtitle, navigation_icon, actions, config, false)
}

/// M3 Center-Aligned Top App Bar - same as TopAppBar but title is centered.
pub fn CenterAlignedTopAppBar(
    title: View,
    subtitle: Option<View>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
) -> View {
    top_app_bar_layout(title, subtitle, navigation_icon, actions, config, true)
}
