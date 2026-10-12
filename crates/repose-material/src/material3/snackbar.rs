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
    pub shape_radius: Dp,
    pub min_height: Dp,
    pub min_width: Dp,
    pub max_width: Dp,
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

    let instance_id = remember(unique_component_id);
    let identity = match modifier.key {
        Some(key) => format!("snackbar:key:{key}"),
        None => format!("snackbar:instance:{instance_id}"),
    };
    let slide_target = if dismissing { 80.0 } else { 0.0 };
    let slide = animate_f32_from(
        format!("{identity}:slide"),
        80.0,
        slide_target,
        th.motion.overlay,
    );

    let alpha_target = if dismissing { 0.0 } else { 1.0 };
    let alpha = animate_f32_from(
        format!("{identity}:alpha"),
        0.0,
        alpha_target,
        th.motion.overlay,
    );

    let snackbar = Box(Modifier::new()
        .translate(0.0, slide)
        .alpha(alpha)
        .min_height(config.min_height)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .background(bg)
        .clip_rounded(config.shape_radius)
        .shadow(th.elevation.level3, Dp::ZERO));

    let dismiss_btn = if config.show_dismiss_action {
        let d = config.on_dismiss.clone();
        Some(IconButton(
            Icon(Symbol::new("close", '\u{E5CD}')).size(Sp(24.0)),
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
                container_size: Some(Dp(48.0)),
                ..Default::default()
            },
        ))
    } else {
        None
    };

    let content = if config.action_on_new_line {
        Column(Modifier::new().padding_values(PaddingValues {
            left: Dp(16.0),
            right: Dp(8.0),
            top: Dp(0.0),
            bottom: Dp(0.0),
        }))
        .child((
            Text(msg)
                .modifier(Modifier::new().padding_values(PaddingValues {
                    left: Dp(0.0),
                    right: Dp(0.0),
                    top: Dp(14.0),
                    bottom: Dp(14.0),
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
                left: Dp(16.0),
                right: Dp(8.0),
                top: Dp(0.0),
                bottom: Dp(0.0),
            })
            .align_items(repose_core::AlignItems::CENTER))
        .child((
            Text(msg)
                .modifier(Modifier::new().padding_values(PaddingValues {
                    left: Dp(0.0),
                    right: Dp(0.0),
                    top: Dp(14.0),
                    bottom: Dp(14.0),
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
        .offset_bottom(Dp::ZERO)
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
    let action = action_label.map(|label| {
        let on_click = on_action.unwrap_or_else(|| Rc::new(|| {}));
        let controller = controller.clone();
        SnackbarAction {
            label,
            on_click: Rc::new(move || {
                on_click();
                controller.dismiss();
            }),
        }
    });
    let view_action = action.clone();
    let animation_id = unique_component_id();
    controller.show(SnackbarRequest {
        message,
        action,
        duration_ms,
        on_finished: None,
        builder: Rc::new(move |dismissing| {
            Snackbar(
                view_message.clone(),
                view_action.clone(),
                Modifier::new().key(animation_id),
                SnackbarConfig::default(),
                dismissing,
            )
        }),
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackbarDuration {
    Short,
    Long,
    Indefinite,
}

impl SnackbarDuration {
    pub fn to_millis(self) -> u32 {
        match self {
            SnackbarDuration::Short => 4_000,
            SnackbarDuration::Long => 10_000,
            SnackbarDuration::Indefinite => 86_400_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnackbarResult {
    Dismissed,
    ActionPerformed,
}

#[derive(Clone)]
pub struct SnackbarHostState {
    controller: SnackbarController,
}

impl SnackbarHostState {
    pub fn new(controller: &SnackbarController) -> Self {
        Self {
            controller: controller.clone(),
        }
    }

    pub fn ambient() -> Self {
        Self {
            controller: SnackbarController::ambient(),
        }
    }

    pub fn show_snackbar(
        &self,
        message: impl Into<String>,
        action_label: Option<String>,
        duration: SnackbarDuration,
        on_result: Option<Rc<dyn Fn(SnackbarResult)>>,
    ) {
        let message = message.into();
        let view_message = message.clone();
        let performed = Rc::new(std::cell::Cell::new(false));
        let action = action_label.map(|label| {
            let controller = self.controller.clone();
            let performed = performed.clone();
            let on_result = on_result.clone();
            SnackbarAction {
                label,
                on_click: Rc::new(move || {
                    performed.set(true);
                    if let Some(cb) = &on_result {
                        cb(SnackbarResult::ActionPerformed);
                    }
                    controller.dismiss();
                }),
            }
        });
        let view_action = action.clone();
        let animation_id = unique_component_id();
        let finished_performed = performed.clone();
        self.controller.show(SnackbarRequest {
            message,
            action,
            duration_ms: duration.to_millis(),
            on_finished: Some(Rc::new(move || {
                if !finished_performed.get()
                    && let Some(cb) = &on_result
                {
                    cb(SnackbarResult::Dismissed);
                }
            })),
            builder: Rc::new(move |dismissing| {
                Snackbar(
                    view_message.clone(),
                    view_action.clone(),
                    Modifier::new().key(animation_id),
                    SnackbarConfig::default(),
                    dismissing,
                )
            }),
        });
    }

    pub fn dismiss(&self) {
        self.controller.dismiss();
    }
}

pub fn SnackbarHost(host_state: &SnackbarHostState, modifier: Modifier) -> View {
    let _ = host_state;
    Box(modifier.semantics(Semantics::new(Role::Container).with_live_region(LiveRegion::Polite)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snackbar_duration_mapping() {
        assert_eq!(SnackbarDuration::Short.to_millis(), 4_000);
        assert_eq!(SnackbarDuration::Long.to_millis(), 10_000);
        assert!(SnackbarDuration::Indefinite.to_millis() > 0);
    }
}
