#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{Box, Column, ViewExt};

use super::*;

/// Configuration for swipe-to-dismiss.
#[derive(Clone, Debug)]
pub struct SwipeToDismissConfig {
    pub modifier: Modifier,
    pub dismiss_threshold: f32,
    pub dismissed_offset: f32,
    pub animation_spec: AnimationSpec,
    pub gestures_enabled: bool,
    pub enable_dismiss_from_start_to_end: bool,
    pub enable_dismiss_from_end_to_start: bool,
}

impl Default for SwipeToDismissConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            dismiss_threshold: SwipeToDismissDefaults::DISMISS_THRESHOLD,
            dismissed_offset: SwipeToDismissDefaults::DISMISSED_OFFSET,
            animation_spec: AnimationSpec::spring_gentle(),
            gestures_enabled: true,
            enable_dismiss_from_start_to_end: true,
            enable_dismiss_from_end_to_start: true,
        }
    }
}

/// Direction for the dismiss action.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DismissDirection {
    StartToEnd,
    EndToStart,
    Both,
}

/// Resolved state for swipe-to-dismiss.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DismissValue {
    Default,
    DismissedToStart,
    DismissedToEnd,
}

/// State for `SwipeToDismiss` - backed by a generic `SwipeableState<DismissValue>`.
pub struct SwipeToDismissState {
    swipeable: repose_core::SwipeableState<DismissValue>,
}

impl Default for SwipeToDismissState {
    fn default() -> Self {
        Self::new()
    }
}

impl SwipeToDismissState {
    pub fn new() -> Self {
        Self::with_config(SwipeToDismissConfig::default())
    }

    pub fn with_config(config: SwipeToDismissConfig) -> Self {
        let one_third = 1.0 / 3.0;
        let positional_threshold = (config.dismiss_threshold * one_third) / config.dismissed_offset;
        let mut anchors = vec![(0.0, DismissValue::Default)];
        if config.enable_dismiss_from_end_to_start {
            anchors.push((-config.dismissed_offset, DismissValue::DismissedToStart));
        }
        if config.enable_dismiss_from_start_to_end {
            anchors.push((config.dismissed_offset, DismissValue::DismissedToEnd));
        }
        // Sort by offset for correct clamp/nearest/next-anchor logic.
        anchors.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
        let swipeable = repose_core::SwipeableState::new(
            anchors,
            repose_core::SwipeableConfig {
                animation_spec: config.animation_spec,
                positional_threshold,
                ..Default::default()
            },
        );
        // Start at the default position (not anchors[0], which may be negative).
        swipeable.snap_to(0.0);
        Self {
            swipeable,
        }
    }

    /// Current animated offset in pixels.
    pub fn offset(&self) -> f32 {
        self.swipeable.offset()
    }

    /// Snap instantly to an offset (used during active drag).
    pub fn set_offset_instant(&self, off: f32) {
        self.swipeable.snap_to(off);
    }

    /// Whether the current position is past the dismiss threshold.
    pub fn is_dismissed(&self) -> bool {
        self.swipeable.current_value() != DismissValue::Default
    }

    /// Animate to the dismissed position.
    pub fn dismiss(&self) {
        self.swipeable.animate_to(&DismissValue::DismissedToStart);
    }

    /// Animate to the dismissed position with custom offset.
    pub fn dismiss_to(&self, offset: f32) {
        let value = if offset < 0.0 {
            DismissValue::DismissedToStart
        } else {
            DismissValue::DismissedToEnd
        };
        self.swipeable.animate_to(&value);
    }

    /// Animate back to origin.
    pub fn reset(&self) {
        self.swipeable.animate_to(&DismissValue::Default);
    }

    /// Fire the dismiss callback once when the spring settles past a given threshold.
    fn try_handle_dismiss_with_threshold(
        &self,
        on_dismiss: &Option<Rc<dyn Fn()>>,
        _threshold: f32,
    ) {
        if !self.swipeable.is_animating() {
            let val = self.swipeable.current_value();
            if val != DismissValue::Default
                && let Some(cb) = on_dismiss
            {
                cb();
            }
        }
    }
}

/// M3 SwipeToDismiss - wraps content that can be swiped to reveal
/// a `background` action view. On release past the threshold the content
/// springs to the dismissed position and `on_dismiss` fires **once**.
///
/// The gesture logic uses `SwipeableState<DismissValue>` internally, so it
/// supports both left and right dismiss directions based on the config.
pub fn SwipeToDismiss(
    state: Rc<SwipeToDismissState>,
    on_dismiss: Option<Rc<dyn Fn()>>,
    background: View,
    content: View,
    modifier: Modifier,
    config: SwipeToDismissConfig,
) -> View {
    let offset = state.offset();
    state.try_handle_dismiss_with_threshold(&on_dismiss, config.dismiss_threshold);

    let s1 = state.swipeable.clone();
    let s2 = state.swipeable.clone();
    let s3 = state.swipeable.clone();
    let on_down = { move |e: PointerEvent| s1.on_pointer_down(e.position.x) };
    let on_move = { move |e: PointerEvent| s2.on_pointer_move(e.position.x) };
    let on_up = { move |_e: PointerEvent| s3.on_pointer_up() };

    let display_offset = offset
        .max(-config.dismissed_offset)
        .min(config.dismissed_offset);

    let content_modifier = {
        let mut m = Modifier::new()
            .fill_max_width()
            .translate(display_offset, 0.0);
        if config.gestures_enabled {
            m = m
                .on_pointer_down(on_down)
                .on_pointer_move(on_move)
                .on_pointer_up(on_up);
        }
        m
    };

    Column(modifier.fill_max_width()).child((
        Box(Modifier::new().fill_max_size().absolute()).child(background),
        Box(content_modifier).child(content),
    ))
}
