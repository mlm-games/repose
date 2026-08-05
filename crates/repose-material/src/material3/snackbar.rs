#![allow(non_snake_case)]


use repose_core::*;
use repose_ui::{
    Box, Column, Row, Spacer, Text, TextStyle,
    ViewExt,
    anim::animate_f32_from,
    overlay::SnackbarAction,
    overlay::snackbar_is_dismissing,
};

use super::*;

/// Configuration for [`Snackbar`].
#[derive(Clone, Debug)]
pub struct SnackbarConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub action_color: Color,
    pub dismiss_action_content_color: Color,
    pub action_on_new_line: bool,
    pub shape_radius: f32,
    pub min_height: f32,
    pub min_width: f32,
    pub max_width: f32,
}

impl Default for SnackbarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: SnackbarDefaults::container_color(),
            content_color: SnackbarDefaults::content_color(),
            action_color: SnackbarDefaults::action_color(),
            dismiss_action_content_color: SnackbarDefaults::dismiss_action_content_color(),
            action_on_new_line: false,
            shape_radius: SnackbarDefaults::SHAPE_RADIUS,
            min_height: SnackbarDefaults::MIN_HEIGHT,
            min_width: SnackbarDefaults::MIN_WIDTH,
            max_width: SnackbarDefaults::MAX_WIDTH,
        }
    }
}

pub fn Snackbar(
    message: impl Into<String>,
    action: Option<SnackbarAction>,
    modifier: Modifier,
    config: SnackbarConfig,
) -> View {
    let msg = message.into();
    let th = theme();
    let bg = config.container_color;
    let fg = config.content_color;
    let action_color = config.action_color;

    let dismissing = snackbar_is_dismissing();

    let slide_target = if dismissing { 80.0 } else { 0.0 };
    let slide = animate_f32_from("snackbar_slide", 80.0, slide_target, th.motion.overlay);

    let alpha_target = if dismissing { 0.0 } else { 1.0 };
    let alpha = animate_f32_from("snackbar_alpha", 0.0, alpha_target, th.motion.overlay);

    let snackbar = Box(Modifier::new()
        .translate(0.0, slide)
        .alpha(alpha)
        .min_height(48.0)
        .min_width(280.0)
        .max_width(600.0)
        .background(bg)
        .clip_rounded(config.shape_radius));

    let snackbar = if config.action_on_new_line {
        snackbar.child(
            Column(Modifier::new().padding_values(PaddingValues {
                left: 16.0,
                right: 8.0,
                top: 0.0,
                bottom: 0.0,
            }))
            .child((
                Text(msg)
                    .modifier(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 0.0,
                        top: 14.0,
                        bottom: 14.0,
                    }))
                    .color(fg)
                    .size(th.typography.body_medium)
                    .max_lines(2)
                    .overflow_ellipsize(),
                action
                    .map(|a| {
                        let label = a.label.clone();
                        Row(Modifier::new()
                            .fill_max_width()
                            .justify_content(repose_core::JustifyContent::END))
                        .child(TextButton(
                            Modifier::new(),
                            move || (a.on_click)(),
                            ButtonConfig::default(),
                            || {
                                Text(label)
                                    .color(action_color)
                                    .size(th.typography.label_large)
                                    .single_line()
                            },
                        ))
                    })
                    .unwrap_or(Box(Modifier::new())),
            )),
        )
    } else {
        snackbar.child(
            Row(Modifier::new()
                .fill_max_width()
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                })
                .align_items(repose_core::AlignItems::CENTER))
            .child((
                Text(msg)
                    .modifier(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 0.0,
                        top: 14.0,
                        bottom: 14.0,
                    }))
                    .color(fg)
                    .size(th.typography.body_medium)
                    .max_lines(2)
                    .overflow_ellipsize(),
                Spacer(),
                action
                    .map(|a| {
                        let label = a.label.clone();
                        TextButton(
                            Modifier::new(),
                            move || (a.on_click)(),
                            ButtonConfig::default(),
                            || {
                                Text(label)
                                    .color(action_color)
                                    .size(th.typography.label_large)
                                    .single_line()
                            },
                        )
                    })
                    .unwrap_or(Box(Modifier::new())),
            )),
        )
    };

    Box(Modifier::new()
        .absolute()
        .offset_bottom(0.0)
        .fill_max_width()
        .justify_content(repose_core::JustifyContent::CENTER)
        .then(modifier))
    .child(snackbar)
}
