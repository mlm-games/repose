#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use web_time::Duration;

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
            dismiss_threshold: SwipeToDismissDefaults::DISMISS_THRESHOLD.0,
            dismissed_offset: SwipeToDismissDefaults::DISMISSED_OFFSET.0,
            animation_spec: AnimationSpec::tween(Duration::from_millis(300), Easing::FastOutSlowIn),
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
    swipeable: RefCell<repose_core::SwipeableState<DismissValue>>,
    config: SwipeToDismissConfig,
    dismiss_offset: Cell<f32>,
    dismiss_latch: Cell<bool>,
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

    fn anchors_for(offset: f32, config: &SwipeToDismissConfig) -> Vec<(f32, DismissValue)> {
        let mut anchors = vec![(0.0, DismissValue::Default)];
        if config.enable_dismiss_from_end_to_start {
            anchors.push((-offset, DismissValue::DismissedToStart));
        }
        if config.enable_dismiss_from_start_to_end {
            anchors.push((offset, DismissValue::DismissedToEnd));
        }
        anchors.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
        anchors
    }

    fn build_swipeable(config: &SwipeToDismissConfig) -> repose_core::SwipeableState<DismissValue> {
        let positional_threshold =
            SwipeToDismissDefaults::POSITIONAL_THRESHOLD.0 / config.dismissed_offset.max(1.0);
        let swipeable = repose_core::SwipeableState::new(
            Self::anchors_for(config.dismissed_offset, config),
            repose_core::SwipeableConfig {
                animation_spec: config.animation_spec,
                positional_threshold,
                velocity_threshold: Dp(125.0).to_px().0,
                ..Default::default()
            },
        );
        swipeable.snap_to(0.0);
        swipeable
    }

    pub fn with_config(config: SwipeToDismissConfig) -> Self {
        let dismiss_offset = config.dismissed_offset;
        Self {
            swipeable: RefCell::new(Self::build_swipeable(&config)),
            config,
            dismiss_offset: Cell::new(dismiss_offset),
            dismiss_latch: Cell::new(false),
        }
    }

    /// Currently active dismissed anchor distance.
    pub fn dismiss_offset(&self) -> f32 {
        self.dismiss_offset.get()
    }

    /// Update the dismissed anchors from the measured container width.
    pub fn set_dismiss_offset(&self, offset: f32) {
        if (self.dismiss_offset.get() - offset).abs() < 0.5 {
            return;
        }
        self.dismiss_offset.set(offset);
        let current = self.swipeable.borrow().offset();
        let swipeable = repose_core::SwipeableState::new(
            Self::anchors_for(offset, &self.config),
            repose_core::SwipeableConfig {
                animation_spec: self.config.animation_spec,
                positional_threshold: SwipeToDismissDefaults::POSITIONAL_THRESHOLD.0
                    / offset.max(1.0),
                velocity_threshold: Dp(125.0).to_px().0,
                ..Default::default()
            },
        );
        swipeable.snap_to(current);
        *self.swipeable.borrow_mut() = swipeable;
    }

    /// Current animated offset in pixels.
    pub fn offset(&self) -> f32 {
        self.swipeable.borrow().offset()
    }

    /// Snap instantly to an offset (used during active drag).
    pub fn set_offset_instant(&self, off: f32) {
        self.swipeable.borrow().snap_to(off);
    }

    /// Whether the current position is past the dismiss threshold.
    pub fn is_dismissed(&self) -> bool {
        self.swipeable.borrow().current_value() != DismissValue::Default
    }

    /// Animate to the dismissed position.
    pub fn dismiss(&self) {
        self.swipeable
            .borrow()
            .animate_to(&DismissValue::DismissedToStart);
    }

    /// Animate to the dismissed position with custom offset.
    pub fn dismiss_to(&self, offset: f32) {
        let value = if offset < 0.0 {
            DismissValue::DismissedToStart
        } else {
            DismissValue::DismissedToEnd
        };
        self.swipeable.borrow().animate_to(&value);
    }

    /// Animate back to origin.
    pub fn reset(&self) {
        self.dismiss_latch.set(false);
        self.swipeable.borrow().animate_to(&DismissValue::Default);
    }

    /// Fire the dismiss callback once when the spring settles past a given threshold.
    fn try_handle_dismiss_with_threshold(
        &self,
        on_dismiss: &Option<Rc<dyn Fn()>>,
        _threshold: f32,
    ) {
        let swipeable = self.swipeable.borrow();
        if !swipeable.is_animating() {
            let val = swipeable.current_value();
            if val == DismissValue::Default {
                self.dismiss_latch.set(false);
            } else if let Some(cb) = on_dismiss
                && !self.dismiss_latch.replace(true)
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
    let on_down = { move |e: PointerEvent| s1.borrow().on_pointer_down(e.position.x) };
    let on_move = { move |e: PointerEvent| s2.borrow().on_pointer_move(e.position.x) };
    let on_up = { move |_e: PointerEvent| s3.borrow().on_pointer_up() };

    let active_offset = state.dismiss_offset();
    let display_offset = offset.max(-active_offset).min(active_offset);

    let width_state = state.clone();
    let content_modifier = {
        let mut m = Modifier::new()
            .fill_max_width()
            .translate(display_offset, 0.0)
            .on_size_changed({
                move |size| {
                    if size.x > 0.0 {
                        width_state.set_dismiss_offset(size.x);
                        repose_core::request_frame();
                    }
                }
            });
        if config.gestures_enabled {
            m = m
                .on_pointer_down(on_down)
                .on_pointer_move(on_move)
                .on_pointer_up(on_up);
        }
        if let Some(dismiss) = on_dismiss.clone() {
            m = m.on_dismiss(move || dismiss());
        }
        m
    };

    Column(modifier.fill_max_width()).child((
        Box(Modifier::new().fill_max_size().absolute()).child(background),
        Box(content_modifier).child(content),
    ))
}
