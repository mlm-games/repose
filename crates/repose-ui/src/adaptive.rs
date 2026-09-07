//! Adaptive layouts that respond to the current [`WindowSizeClass`].
//! Currently provides [`ListDetailPaneScaffold`]: the list and the detail
//! render side-by-side on Medium/Expanded widths, and only the selected pane
//! renders on Compact.

use repose_core::prelude::*;
use repose_core::{Modifier, PaddingValues, View, WindowSizeClass};

use crate::{Box, Column, Row, Text, ViewExt};

#[derive(Clone, Copy, Debug)]
pub struct PaneScaffoldDirective {
    pub max_horizontal_partitions: u32,
    pub horizontal_part_spacing: Dp,
    pub list_pane_width: Dp,
    pub content_padding: PaddingValues,
}

impl Default for PaneScaffoldDirective {
    fn default() -> Self {
        Self {
            max_horizontal_partitions: 1,
            horizontal_part_spacing: Dp::ZERO,
            list_pane_width: Dp::ZERO,
            content_padding: PaddingValues::default(),
        }
    }
}

impl PaneScaffoldDirective {
    pub fn from_window_size_class(class: WindowSizeClass) -> Self {
        if class.is_at_least_medium_width() {
            Self {
                max_horizontal_partitions: 2,
                horizontal_part_spacing: Dp::ZERO,
                list_pane_width: Dp(360.0),
                content_padding: PaddingValues::default(),
            }
        } else {
            Self {
                max_horizontal_partitions: 1,
                horizontal_part_spacing: Dp::ZERO,
                list_pane_width: Dp::ZERO,
                content_padding: PaddingValues::default(),
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListDetailPaneValue {
    List,
    Detail,
}

pub fn ListDetailPaneScaffold<F1, F2>(
    directive: PaneScaffoldDirective,
    value: ListDetailPaneValue,
    list: F1,
    detail: F2,
) -> View
where
    F1: Fn() -> View + 'static,
    F2: Fn() -> View + 'static,
{
    if directive.max_horizontal_partitions >= 2 {
        let spacing = directive.horizontal_part_spacing;
        let list_mod = if directive.list_pane_width.0 > 0.0 {
            Modifier::new()
                .width(directive.list_pane_width)
                .fill_max_height()
        } else {
            Modifier::new().flex_grow(1.0).fill_max_height()
        };
        Row(Modifier::new().fill_max_size().gap(spacing)).child((
            Box(list_mod).child(list()),
            Box(Modifier::new().flex_grow(1.0).fill_max_height()).child(detail()),
        ))
    } else {
        match value {
            ListDetailPaneValue::List => list(),
            ListDetailPaneValue::Detail => detail(),
        }
    }
}

pub fn ScaffoldPane(directive: &PaneScaffoldDirective, content: View) -> View {
    Box(Modifier::new()
        .fill_max_size()
        .padding_values(directive.content_padding))
    .child(content)
}

pub fn TwoPaneTopBar(title: &str, leading: Option<View>, trailing: Option<View>) -> View {
    let th = theme();
    let leading = leading.unwrap_or_else(|| Box(Modifier::new().width(Dp::ZERO).height(Dp::ZERO)));
    let trailing =
        trailing.unwrap_or_else(|| Box(Modifier::new().width(Dp::ZERO).height(Dp::ZERO)));
    Box(Modifier::new()
        .fill_max_width()
        .height(Dp(56.0))
        .background(th.surface)
        .padding_values(PaddingValues {
            left: Dp(16.0),
            right: Dp(16.0),
            top: Dp::ZERO,
            bottom: Dp::ZERO,
        }))
    .child((
        leading,
        Column(
            Modifier::new()
                .flex_grow(1.0)
                .padding_values(PaddingValues {
                    left: Dp(16.0),
                    right: Dp(16.0),
                    top: Dp::ZERO,
                    bottom: Dp::ZERO,
                }),
        )
        .child(Text(title)),
        trailing,
    ))
}
