#![allow(non_snake_case)]

use std::rc::Rc;

use crate::{Icon, Symbol};
use repose_core::*;
use repose_ui::{
    Box, Column, Row, Spacer, Text, TextStyle, ViewExt,
    anim::animate_f32_from,
    overlay::{SnackbarAction, SnackbarController, SnackbarRequest},
};

use super::*;

/// Configuration for [`Snackbar`].
#[derive(Clone)]
pub struct SnackbarConfig {
    pub container_color: Color,
    pub content_color: Color,
    pub action_color: Color,
    pub dismiss_action_content_color: Color,
    pub action_on_new_line: bool,
    /// Whether a close (dismiss) icon button is shown at the end.
    pub show_dismiss_action: bool,
    /// Called when the dismiss icon is clicked.
    pub on_dismiss: Option<Rc<dyn Fn()>>,
    pub shape_radius: f32,
    pub min_height: f32,
    pub min_width: f32,
    pub max_width: f32,
}

impl std::fmt::Debug for SnackbarConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SnackbarConfig")
            .field("container_color", &self.container_color)
            .field("content_color", &self.content_color)
            .field("action_color", &self.action_color)
            .field(
                "dismiss_action_content_color",
                &self.dismiss_action_content_color,
            )
            .field("action_on_new_line", &self.action_on_new_line)
            .field("show_dismiss_action", &self.show_dismiss_action)
            .field("on_dismiss", &self.on_dismiss.as_ref().map(|_| ".."))
            .field("shape_radius", &self.shape_radius)
            .field("min_height", &self.min_height)
            .field("min_width", &self.min_width)
            .field("max_width", &self.max_width)
            .finish()
    }
}

impl Default for SnackbarConfig {
    fn default() -> Self {
        Self {
            container_color: SnackbarDefaults::container_color(),
            content_color: SnackbarDefaults::content_color(),
            action_color: SnackbarDefaults::action_color(),
            dismiss_action_content_color: SnackbarDefaults::dismiss_action_content_color(),
            action_on_new_line: false,
            show_dismiss_action: false,
            on_dismiss: None,
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
    dismissing: bool,
) -> View {
    let msg = message.into();
    let th = theme();
    let bg = config.container_color;
    let fg = config.content_color;
    let action_color = config.action_color;

    let slide_target = if dismissing { 80.0 } else { 0.0 };
    let slide = animate_f32_from("snackbar_slide", 80.0, slide_target, th.motion.overlay);

    let alpha_target = if dismissing { 0.0 } else { 1.0 };
    let alpha = animate_f32_from("snackbar_alpha", 0.0, alpha_target, th.motion.overlay);

    let snackbar = Box(Modifier::new()
        .translate(0.0, slide)
        .alpha(alpha)
        .min_height(config.min_height)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .background(bg)
        .clip_rounded(config.shape_radius)
        .shadow(th.elevation.level3, 0.0));

    let dismiss_btn = if config.show_dismiss_action {
        let d = config.on_dismiss.clone();
        Some(IconButton(
            Icon(Symbol::new("close", '\u{E5CD}')).size(24.0),
            move || {
                if let Some(cb) = &d {
                    cb();
                }
            },
            IconButtonConfig {
                colors: IconButtonColors {
                    container_color: Color::TRANSPARENT,
                    content_color: config.dismiss_action_content_color,
                    disabled_container_color: Color::TRANSPARENT,
                    disabled_content_color: config
                        .dismiss_action_content_color
                        .with_alpha_f32(0.38),
                },
                container_size: Some(48.0),
                ..Default::default()
            },
        ))
    } else {
        None
    };

    let content = if config.action_on_new_line {
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
                        ButtonConfig {
                            colors: Some(ButtonColors {
                                container_color: Color::TRANSPARENT,
                                content_color: action_color,
                                disabled_container_color: Color::TRANSPARENT,
                                disabled_content_color: action_color.with_alpha_f32(0.38),
                            }),
                            ..Default::default()
                        },
                        || Text(label).size(th.typography.label_large).single_line(),
                    ))
                })
                .unwrap_or(Box(Modifier::new())),
            dismiss_btn.unwrap_or(Box(Modifier::new())),
        ))
    } else {
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
                        ButtonConfig {
                            colors: Some(ButtonColors {
                                container_color: Color::TRANSPARENT,
                                content_color: action_color,
                                disabled_container_color: Color::TRANSPARENT,
                                disabled_content_color: action_color.with_alpha_f32(0.38),
                            }),
                            ..Default::default()
                        },
                        || Text(label).size(th.typography.label_large).single_line(),
                    )
                })
                .unwrap_or(Box(Modifier::new())),
            dismiss_btn.unwrap_or(Box(Modifier::new())),
        ))
    };
    let snackbar = snackbar.child(with_content_color(fg, move || content));

    Box(Modifier::new()
        .absolute()
        .offset_bottom(0.0)
        .fill_max_width()
        .justify_content(repose_core::JustifyContent::CENTER)
        .then(modifier))
    .child(snackbar)
}

/// Show a standard text snackbar without hand-building a [`SnackbarRequest`].
///
/// The controller owns dismissal (timeout, action tap), so `on_action` only
/// needs the action's own effect. The rendered view uses [`SnackbarConfig::default()`].
pub fn show_simple_snackbar(
    controller: &SnackbarController,
    message: impl Into<String>,
    action_label: Option<String>,
    on_action: Option<Rc<dyn Fn()>>,
    duration_ms: u32,
) {
    let message = message.into();
    let view_message = message.clone();
    let action = action_label.map(|label| SnackbarAction {
        label,
        on_click: on_action.unwrap_or_else(|| Rc::new(|| {})),
    });
    let view_action = action.clone();
    controller.show(SnackbarRequest {
        message,
        action,
        duration_ms,
        builder: Rc::new(move |dismissing| {
            Snackbar(
                view_message.clone(),
                view_action.clone(),
                Modifier::new(),
                SnackbarConfig::default(),
                dismissing,
            )
        }),
    });
}
