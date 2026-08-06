#![allow(non_snake_case)]

use std::cell::Cell;
use std::rc::Rc;

use repose_core::NestedScrollConnection;
use repose_core::*;
use repose_ui::{
    Box, Column, Row, ZStack,
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
    /// Collapses upward when scrolling down, expands as soon as scrolling up.
    EnterAlways,
    /// Collapses when scrolling down, but only expands once the nested content
    /// has been scrolled back to the very top. Used by medium/large bars.
    ExitUntilCollapsed,
}

/// Drives scroll-based collapsing/expanding of a TopAppBar.
///
/// Create one, pass its [`nested_scroll_connection`](TopAppBarScrollBehavior::nested_scroll_connection)
/// to a lazy list's [`set_nested_scroll_parent`] method, and either set the
/// resulting [`collapsed_offset`](TopAppBarScrollBehavior::collapsed_offset)
/// on the TopAppBar via [`TopAppBarConfig::scroll_offset`], or attach it via
/// [`TopAppBarConfig::scroll_behavior`] so the bar wires offset + color itself.
#[derive(Clone)]
pub struct TopAppBarScrollBehavior {
    pub collapsed_offset: Signal<f32>,
    pub height: f32,
    pub collapsed_height: f32,
    pub mode: TopAppBarScrollMode,
    _pending: Rc<Cell<f32>>,
}

impl std::fmt::Debug for TopAppBarScrollBehavior {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TopAppBarScrollBehavior")
            .field("offset", &self.offset())
            .field("height", &self.height)
            .field("collapsed_height", &self.collapsed_height)
            .field("mode", &self.mode)
            .finish()
    }
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
    /// downward scroll and expands on upward scroll. `ExitUntilCollapsed`
    /// only expands once the nested content is back at the top.
    pub fn nested_scroll_connection(&self) -> NestedScrollConnection {
        let off = self.collapsed_offset.clone();
        let max_collapse = -(self.height - self.collapsed_height);

        match self.mode {
            TopAppBarScrollMode::Pinned => NestedScrollConnection::new(),
            TopAppBarScrollMode::EnterAlways => {
                NestedScrollConnection::new().on_pre_scroll(move |d: Vec2, _source| -> Vec2 {
                    let mut consumed = Vec2::ZERO;
                    let current = off.get();
                    if d.y > 0.0 {
                        // Scrolling down -> collapse bar
                        if current > max_collapse {
                            let consume = d.y.min(current - max_collapse);
                            off.set(current - consume);
                            consumed.y = consume;
                        }
                    } else if current < 0.0 {
                        // Scrolling up -> expand bar
                        let consume = (-d.y).min(-current);
                        off.set(current + consume);
                        consumed.y = consume;
                    }
                    if consumed.y != 0.0 {
                        repose_core::request_frame();
                    }
                    consumed
                })
            }
            TopAppBarScrollMode::ExitUntilCollapsed => NestedScrollConnection::new()
                .on_pre_scroll({
                    let off = off.clone();
                    move |d: Vec2, _source| -> Vec2 {
                        let mut consumed = Vec2::ZERO;
                        if d.y > 0.0 {
                            // Scrolling down -> collapse bar
                            let current = off.get();
                            if current > max_collapse {
                                let consume = d.y.min(current - max_collapse);
                                off.set(current - consume);
                                consumed.y = consume;
                                repose_core::request_frame();
                            }
                        }
                        consumed
                    }
                })
                .on_post_scroll(
                    move |_consumed: Vec2, available: Vec2, _source| -> Vec2 {
                        // Upward scroll leftover means the content is at the top,
                        // so the bar may expand.
                        let mut expanded = Vec2::ZERO;
                        if available.y < 0.0 {
                            let current = off.get();
                            if current < 0.0 {
                                let consume = (-available.y).min(-current);
                                off.set(current + consume);
                                expanded.y = consume;
                                repose_core::request_frame();
                            }
                        }
                        expanded
                    },
                ),
        }
    }

    /// Returns the current collapsed offset (0 = fully expanded, negative = collapsed).
    pub fn offset(&self) -> f32 {
        self.collapsed_offset.get()
    }

    /// Collapse progress in `0.0..=1.0` (`0` = expanded, `1` = fully collapsed).
    /// Drives the container color lerp so the scrolled color tracks the offset.
    pub fn collapsed_fraction(&self) -> f32 {
        let range = (self.height - self.collapsed_height).max(f32::EPSILON);
        ((-self.collapsed_offset.get()) / range).clamp(0.0, 1.0)
    }
}

/// Configuration for [`TopAppBar`].
#[derive(Clone, Debug)]
pub struct TopAppBarConfig {
    pub modifier: Modifier,
    pub colors: TopAppBarColors,
    pub height: f32,
    /// Collapse progress in `0.0..=1.0` driving the container color lerp.
    /// Ignored when [`scroll_behavior`](TopAppBarConfig::scroll_behavior) is set.
    pub scroll_fraction: f32,
    /// Vertical translate offset (negative = collapsed upward).
    /// Ignored when [`scroll_behavior`](TopAppBarConfig::scroll_behavior) is set.
    pub scroll_offset: f32,
    /// Optional shared scroll behavior. When set, the bar reads
    /// [`TopAppBarScrollBehavior::offset`] and
    /// [`TopAppBarScrollBehavior::collapsed_fraction`] reactively itself,
    /// so translate and container color stay in sync without manual wiring.
    pub scroll_behavior: Option<Rc<TopAppBarScrollBehavior>>,
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
            scroll_behavior: None,
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
    // When a behavior is attached, read its offset/fraction reactively so the
    // bar's translate and container color track collapse automatically.
    let (scroll_offset, scroll_fraction) = if let Some(ref sb) = config.scroll_behavior {
        (sb.offset(), sb.collapsed_fraction())
    } else {
        (config.scroll_offset, config.scroll_fraction)
    };
    let bg = config.colors.container_color(scroll_fraction);
    let colors = config.colors;

    let root_m = Modifier::new()
        .fill_max_width()
        .height(config.height + insets.top)
        .background(bg)
        .translate(0.0, scroll_offset)
        .semantics(Semantics::new(Role::Container));

    let nav = navigation_icon
        .map(|icon| with_content_color(colors.navigation_icon_content_color, move || icon))
        .unwrap_or(Box(Modifier::new().width(16.0).fill_max_height()));

    let actions_row = Row(Modifier::new()
        .align_items(AlignItems::CENTER)
        .flex_shrink(0.0))
    .child(
        actions
            .into_iter()
            .map(|a| {
                with_content_color(colors.action_icon_content_color, move || a.clone())
            })
            .collect::<Vec<_>>(),
    );

    let title_column = Column(Modifier::new().justify_content(JustifyContent::CENTER)).child((
        Box(Modifier::new()).child(with_content_color(
            colors.title_content_color,
            || title,
        )),
        subtitle
            .map(|s| {
                Box(Modifier::new()).child(with_content_color(
                    colors.subtitle_content_color,
                    || s,
                ))
            })
            .unwrap_or(Box(Modifier::new())),
    ));

    let content_padding = PaddingValues {
        left: config.content_padding.left + insets.left,
        right: config.content_padding.right + insets.right,
        top: config.content_padding.top + insets.top,
        bottom: config.content_padding.bottom + insets.bottom,
    };

    if centered {
        // True center alignment: nav/actions sit at the edges while the title
        // overlays the bar, centered across the FULL width (not the leftover
        // space between nav and actions), matching Compose's optical centering.
        ZStack(root_m.then(config.modifier)).child((
            Row(Modifier::new()
                .fill_max_width()
                .align_items(AlignItems::CENTER)
                .padding_values(content_padding))
            .child((
                nav,
                Box(Modifier::new().flex_grow(1.0)),
                actions_row,
            )),
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(0.0), Some(0.0), None)
                .fill_max_width()
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER))
            .child(title_column),
        ))
    } else {
        Row(root_m
            .padding_values(content_padding)
            .then(config.modifier))
        .child((
            nav,
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .flex_grow(1.0))
            .child(title_column),
            actions_row,
        ))
    }
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

/// M3 Center-Aligned Top App Bar - same as TopAppBar but the title is truly
/// centered across the full bar width (nav/actions sit at the edges).
pub fn CenterAlignedTopAppBar(
    title: View,
    subtitle: Option<View>,
    navigation_icon: Option<View>,
    actions: Vec<View>,
    config: TopAppBarConfig,
) -> View {
    top_app_bar_layout(title, subtitle, navigation_icon, actions, config, true)
}
