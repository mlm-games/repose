#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;
use repose_ui::{Box, Column, ViewExt};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum FabPosition {
    #[default]
    End,
    Center,
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
    pub owns_window_insets: bool,
    pub floating_action_button_fallback_height: Dp,
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
            owns_window_insets: true,
            floating_action_button_fallback_height: FABDefaults::SIZE,
        }
    }
}

pub fn Scaffold(content: impl Fn(PaddingValues) -> View, config: ScaffoldConfig) -> View {
    with_material_indication(|| scaffold_inner(content, config))
}

fn max_dp(a: Dp, b: Dp) -> Dp {
    if a > b { a } else { b }
}

fn scaffold_identity(modifier: &Modifier, instance_id: u64) -> String {
    match modifier.key {
        Some(key) => format!("scaffold:key:{key}"),
        None => format!("scaffold:instance:{instance_id}"),
    }
}

fn measured_bar(bar: View, measured: Rc<Signal<f32>>, placement: Modifier) -> View {
    let measured_for_callback = measured;
    Box(placement.fill_max_width().on_size_changed(move |size| {
        let height = if size.y.is_finite() {
            size.y.max(0.0)
        } else {
            0.0
        };
        measured_for_callback.set_neq(height);
    }))
    .child(bar)
}

fn scaffold_inner(content: impl Fn(PaddingValues) -> View, config: ScaffoldConfig) -> View {
    let instance_id = remember(unique_component_id);
    let insets = window_insets();
    let (itop, ibottom, iime, ileft, iright) = if config.owns_window_insets {
        (
            Px(insets.top).to_dp(),
            Px(insets.bottom).to_dp(),
            Px(insets.ime_bottom).to_dp(),
            Px(insets.left).to_dp(),
            Px(insets.right).to_dp(),
        )
    } else {
        (Dp::ZERO, Dp::ZERO, Dp::ZERO, Dp::ZERO, Dp::ZERO)
    };
    let has_top = config.top_bar.is_some();
    let has_bottom = config.bottom_bar.is_some();
    let has_fab = config.floating_action_button.is_some();
    let identity = scaffold_identity(&config.modifier, *instance_id);
    let top_height = remember_with_key(format!("{identity}:top"), || signal(0.0));
    let bottom_height = remember_with_key(format!("{identity}:bottom"), || signal(0.0));
    let fab_height = remember_with_key(format!("{identity}:fab"), || signal(0.0));
    if !has_top {
        top_height.set_neq(0.0);
    }
    if !has_bottom {
        bottom_height.set_neq(0.0);
    }
    if !has_fab {
        fab_height.set_neq(0.0);
    }
    let measured_top = top_height.get();
    let measured_bottom = bottom_height.get();
    let measured_fab: f32 = fab_height.get();
    let top_reserved = if has_top {
        max_dp(itop, itop + Dp(measured_top))
    } else {
        itop
    };
    let bottom_reserved = if has_bottom {
        max_dp(ibottom + iime, ibottom + iime + Dp(measured_bottom))
    } else {
        ibottom + iime
    };
    let content_padding = PaddingValues {
        top: top_reserved,
        bottom: bottom_reserved,
        left: ileft,
        right: iright,
    };

    let top_bar = config.top_bar.map(|bar| {
        measured_bar(
            bar,
            top_height,
            Modifier::new()
                .absolute()
                .offset(Some(Dp::ZERO), Some(itop), Some(Dp::ZERO), None),
        )
    });
    let bottom_bar = config.bottom_bar.map(|bar| {
        measured_bar(
            bar,
            bottom_height,
            Modifier::new().absolute().offset(
                Some(Dp::ZERO),
                None,
                Some(ibottom + iime),
                Some(Dp::ZERO),
            ),
        )
    });

    let content = Box(Modifier::new().fill_max_size())
        .child(with_content_color(config.content_color, move || {
            content(content_padding)
        }));

    let fab = config.floating_action_button.map(|fab| {
        let bottom = bottom_reserved + ScaffoldDefaults::FAB_MARGIN;
        let modifier = match config.fab_position {
            FabPosition::End => Modifier::new().absolute().offset(
                None,
                None,
                Some(bottom),
                Some(ScaffoldDefaults::FAB_MARGIN + iright),
            ),
            FabPosition::Center => Modifier::new()
                .absolute()
                .fill_max_width()
                .align_self(AlignSelf::CENTER)
                .offset(None, None, Some(bottom), None),
        };
        Box(modifier.on_size_changed({
            let measured = fab_height.clone();
            move |size| {
                let height = if size.y.is_finite() {
                    size.y.max(0.0)
                } else {
                    0.0
                };
                measured.set_neq(height);
            }
        }))
        .child(fab)
    });
    let snackbar_bottom = if has_fab {
        let fab_height = measured_fab.max(config.floating_action_button_fallback_height.0);
        bottom_reserved
            + ScaffoldDefaults::FAB_MARGIN
            + Dp(fab_height)
            + ScaffoldDefaults::FAB_MARGIN
    } else {
        bottom_reserved + ScaffoldDefaults::FAB_MARGIN
    };
    let snackbar = config.snackbar_host.map(|host| {
        Box(Modifier::new()
            .absolute()
            .fill_max_size()
            .z_index(2.0)
            .padding_values(PaddingValues {
                left: ileft,
                right: iright,
                top: Dp::ZERO,
                bottom: snackbar_bottom,
            })
            .justify_content(JustifyContent::FLEX_END)
            .align_items(AlignItems::CENTER))
        .child(host)
    });

    Column(
        config
            .modifier
            .fill_max_size()
            .background(config.container_color),
    )
    .child((
        content,
        top_bar.unwrap_or_else(|| Box(Modifier::new())),
        bottom_bar.unwrap_or_else(|| Box(Modifier::new())),
        fab.unwrap_or_else(|| Box(Modifier::new())),
        snackbar.unwrap_or_else(|| Box(Modifier::new())),
    ))
}
