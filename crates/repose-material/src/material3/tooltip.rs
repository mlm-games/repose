#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{
    Box, Text, TextStyle,
    ViewExt,
    anim::animate_f32,
};

use super::*;

static TOOLTIP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Configuration for tooltip.
#[derive(Clone, Debug)]
pub struct TooltipConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub offset_y: f32,
    pub horizontal_padding: f32,
    pub vertical_padding: f32,
    pub has_action: bool,
    pub enable_user_input: bool,
    pub focusable: bool,
    pub max_width: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
}

impl Default for TooltipConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: TooltipDefaults::container_color(),
            content_color: TooltipDefaults::content_color(),
            offset_y: TooltipDefaults::OFFSET_Y,
            horizontal_padding: TooltipDefaults::HORIZONTAL_PADDING,
            vertical_padding: TooltipDefaults::VERTICAL_PADDING,
            has_action: false,
            enable_user_input: true,
            focusable: false,
            max_width: TooltipDefaults::MAX_WIDTH,
            tonal_elevation: 0.0,
            shadow_elevation: 0.0,
        }
    }
}

/// State controlling tooltip visibility.
pub struct TooltipState {
    visible: Signal<bool>,
}

impl TooltipState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            visible: signal(false),
        })
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// Wraps `content` with a tooltip label shown above it when `state` is visible.
///
/// When [`TooltipConfig::enable_user_input`] is true (default), the tooltip is
/// shown on pointer hover and dismissed on leave.
///
/// Usage:
/// ```ignore
/// let tip = TooltipState::new();
/// TooltipBox("I'm a tooltip", tip.clone(), Modifier::new(), Button("Hover me", {
///     let tip = tip.clone();
///     move || tip.show()
/// }));
/// ```
pub fn TooltipBox(
    text: impl Into<String>,
    state: Rc<TooltipState>,
    content: View,
    config: TooltipConfig,
) -> View {
    let text: Rc<str> = Rc::from(text.into());
    let th = theme();
    let spec = th.motion.overlay;
    let id = remember(|| TOOLTIP_COUNTER.fetch_add(1, Ordering::Relaxed));

    let alpha = animate_f32(
        format!("tooltip_alpha_{id}"),
        if state.is_visible() { 1.0 } else { 0.0 },
        spec,
    );

    let tooltip_visible = state.is_visible() || alpha > 0.01;
    let scale = 0.92 + 0.08 * alpha;

    let mut host = config
        .modifier
        .align_self(AlignSelf::FLEX_START)
        .flex_shrink(0.0);

    if config.enable_user_input {
        let enter = state.clone();
        let leave = state.clone();
        host = host.hoverable(
            move || enter.show(),
            move || leave.dismiss(),
        );
    }

    Box(host).child((
        content,
        if tooltip_visible {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(config.offset_y), Some(0.0), None)
                .justify_content(JustifyContent::CENTER)
                .align_items(AlignItems::CENTER)
                .hit_passthrough()
                .render_z_index(10_000.0)
                .alpha(alpha))
            .child(
                Box(Modifier::new()
                    .background(config.container_color)
                    .clip_rounded(th.shapes.extra_small)
                    .padding_values(PaddingValues {
                        left: config.horizontal_padding,
                        right: config.horizontal_padding,
                        top: config.vertical_padding,
                        bottom: config.vertical_padding,
                    })
                    .max_width(config.max_width)
                    .flex_shrink(0.0)
                    .scale(scale)
                    .hit_passthrough()
                    .then({
                        let mut m = Modifier::new();
                        if config.shadow_elevation > 0.0 {
                            m = m.shadow(config.shadow_elevation, 0.0);
                        }
                        if config.tonal_elevation > 0.0 {
                            m = m.state_elevation(StateElevation {
                                default: config.tonal_elevation,
                                hovered: config.tonal_elevation,
                                pressed: config.tonal_elevation,
                                dragged: config.tonal_elevation,
                                disabled: 0.0,
                            });
                        }
                        m
                    }))
                .child(
                    Text((*text).to_string())
                        .color(config.content_color)
                        .size(th.typography.label_medium),
                ),
            )
        } else {
            Box(Modifier::new())
        },
    ))
}
