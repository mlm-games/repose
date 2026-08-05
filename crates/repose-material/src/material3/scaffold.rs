#![allow(non_snake_case)]


use repose_core::*;
use repose_ui::{
    Box, Column,
    ViewExt,
};

use super::*;

/// Position of the floating action button within a Scaffold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FabPosition {
    End,
    Center,
}

impl Default for FabPosition {
    fn default() -> Self {
        Self::End
    }
}

#[derive(Clone)]
pub struct ScaffoldConfig {
    pub modifier: Modifier,
    pub top_bar: Option<View>,
    pub bottom_bar: Option<View>,
    pub floating_action_button: Option<View>,
    pub snackbar_host: Option<View>,
    pub container_color: Color,
    pub content_color: Color,
    pub fab_position: FabPosition,
}

impl Default for ScaffoldConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            top_bar: None,
            bottom_bar: None,
            floating_action_button: None,
            snackbar_host: None,
            container_color: ScaffoldDefaults::container_color(),
            content_color: ScaffoldDefaults::content_color(),
            fab_position: FabPosition::End,
        }
    }
}

pub fn Scaffold(content: impl Fn(PaddingValues) -> View, config: ScaffoldConfig) -> View {
    let insets = window_insets();
    let itop = px_to_dp(insets.top);
    let ibottom = px_to_dp(insets.bottom);
    let iime = px_to_dp(insets.ime_bottom);
    let ileft = px_to_dp(insets.left);
    let iright = px_to_dp(insets.right);

    let content_padding = PaddingValues {
        top: if config.top_bar.is_some() {
            64.0
        } else {
            itop
        },
        bottom: if config.bottom_bar.is_some() {
            80.0 + ibottom + iime
        } else {
            ibottom + iime
        },
        left: ileft,
        right: iright,
    };

    Column(
        config
            .modifier
            .fill_max_size()
            .background(config.container_color),
    )
    .child((
        Box(Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                top: if config.top_bar.is_some() {
                    64.0 + itop
                } else {
                    0.0
                },
                bottom: if config.bottom_bar.is_some() {
                    80.0 + ibottom + iime
                } else {
                    ibottom + iime
                },
                ..Default::default()
            }))
        .child(content(content_padding)),
        if let Some(bar) = config.top_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(itop), Some(0.0), None))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(bar) = config.bottom_bar {
            Box(Modifier::new().absolute().offset(
                Some(0.0),
                None,
                Some(ibottom + iime),
                Some(0.0),
            ))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(fab) = config.floating_action_button {
            let mut fab_m = Modifier::new().absolute();
            match config.fab_position {
                FabPosition::End => {
                    fab_m = fab_m.offset(
                        None,
                        None,
                        Some(16.0 + ibottom + iime),
                        Some(16.0),
                    );
                }
                FabPosition::Center => {
                    fab_m = fab_m.fill_max_width().align_self(AlignSelf::CENTER).offset(
                        None,
                        None,
                        Some(16.0 + ibottom + iime),
                        None,
                    );
                }
            }
            Box(fab_m).child(fab)
        } else {
            Box(Modifier::new())
        },
        config.snackbar_host.unwrap_or_else(|| Box(Modifier::new())),
    ))
}
