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
    pub horizontal_part_spacing: f32,
    pub list_pane_width: f32,
    pub content_padding: PaddingValues,
}

impl Default for PaneScaffoldDirective {
    fn default() -> Self {
        Self {
            max_horizontal_partitions: 1,
            horizontal_part_spacing: 0.0,
            list_pane_width: 0.0,
            content_padding: PaddingValues::default(),
        }
    }
}

impl PaneScaffoldDirective {
    pub fn from_window_size_class(class: WindowSizeClass) -> Self {
        if class.is_at_least_medium_width() {
            Self {
                max_horizontal_partitions: 2,
                horizontal_part_spacing: 0.0,
                list_pane_width: 360.0,
                content_padding: PaddingValues::default(),
            }
        } else {
            Self {
                max_horizontal_partitions: 1,
                horizontal_part_spacing: 0.0,
                list_pane_width: 0.0,
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
        let list_mod = if directive.list_pane_width > 0.0 {
            Modifier::new().width(directive.list_pane_width).fill_max_height()
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
    let leading = leading.unwrap_or_else(|| Box(Modifier::new().width(0.0).height(0.0)));
    let trailing = trailing.unwrap_or_else(|| Box(Modifier::new().width(0.0).height(0.0)));
    Box(Modifier::new()
        .fill_max_width()
        .height(56.0)
        .background(th.surface)
        .padding_values(PaddingValues {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        }))
    .child((
        leading,
        Column(Modifier::new().flex_grow(1.0).padding_values(PaddingValues {
            left: 16.0,
            right: 16.0,
            top: 0.0,
            bottom: 0.0,
        }))
        .child(Text(title)),
        trailing,
    ))
}
