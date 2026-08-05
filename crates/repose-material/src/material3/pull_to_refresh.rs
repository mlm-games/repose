#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use web_time::Duration;

use crate::{Icon, Symbol};
use repose_core::animation::{AnimationSpec, Easing, RepeatableSpec};
use repose_core::*;
use repose_ui::{
    Box, Column, TextStyle,
    ViewExt,
    anim::animate_f32_from,
};

use super::*;

/// Configuration for pull-to-refresh.
#[derive(Clone, Debug)]
pub struct PullToRefreshConfig {
    pub modifier: Modifier,
    pub indicator_color: Color,
    pub threshold: f32,
    pub content_alignment: AlignItems,
}

impl Default for PullToRefreshConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            indicator_color: PullToRefreshDefaults::indicator_color(),
            threshold: PullToRefreshDefaults::THRESHOLD,
            content_alignment: AlignItems::FLEX_START,
        }
    }
}

/// State for `PullToRefresh` - tracks pull progress and refresh trigger.
///
/// Connect to a [`ScrollState`](repose_ui::scroll::ScrollState) via
/// [`set_scroll_state`](PullToRefreshState::set_scroll_state) so that the
/// pull offset is automatically driven by scroll overscroll.
pub struct PullToRefreshState {
    refreshing: Signal<bool>,
    scroll_state: RefCell<Option<Rc<repose_ui::scroll::ScrollState>>>,
    threshold: f32,
    triggered: Cell<bool>,
}

impl Default for PullToRefreshState {
    fn default() -> Self {
        Self::new()
    }
}

impl PullToRefreshState {
    pub fn new() -> Self {
        Self {
            refreshing: signal(false),
            scroll_state: RefCell::new(None),
            threshold: 64.0,
            triggered: Cell::new(false),
        }
    }

    /// Connect this PullToRefresh state to a scroll state.
    /// The pull offset is then derived from the scroll state's overscroll.
    pub fn set_scroll_state(&self, state: Rc<repose_ui::scroll::ScrollState>) {
        *self.scroll_state.borrow_mut() = Some(state);
    }

    /// Set the overscroll threshold that triggers a refresh (default 64px).
    pub fn set_threshold(&mut self, px: f32) {
        self.threshold = px;
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.get()
    }

    pub fn set_refreshing(&self, v: bool) {
        self.refreshing.set(v);
        if !v && let Some(sc) = self.scroll_state.borrow().as_ref() {
            sc.set_overscroll(0.0);
        }
    }

    /// Read the current pull offset from the connected scroll state's overscroll.
    pub fn pull_offset(&self) -> f32 {
        if let Some(sc) = self.scroll_state.borrow().as_ref() {
            let os = sc.overscroll_offset();
            if os < 0.0 { -os } else { 0.0 }
        } else {
            0.0
        }
    }
}

/// Wraps scrollable content with a pull-to-refresh indicator.
///
/// Renders a small spinner at the top when the user pulls down past a threshold,
/// or shows the current pull offset as a visual indicator.
///
/// The `state` must be connected to a [`ScrollState`](repose_ui::scroll::ScrollState)
/// via [`set_scroll_state`](PullToRefreshState::set_scroll_state) for the pull
/// offset to be derived from the scroll overscroll automatically.
pub fn PullToRefresh(
    state: Rc<PullToRefreshState>,
    modifier: Modifier,
    on_refresh: Rc<dyn Fn()>,
    content: View,
    config: PullToRefreshConfig,
) -> View {
    let pull = state.pull_offset();
    let refreshing = state.is_refreshing();
    let threshold = config.threshold;

    if state.triggered.get() && !refreshing && pull < threshold {
        state.triggered.set(false);
    }

    if !refreshing && !state.triggered.get() && pull >= threshold {
        state.triggered.set(true);
        state.refreshing.set(true);
        (on_refresh)();
    }

    let frac_key = format!("ptr_frac_{}", Rc::as_ptr(&state) as u64);
    let raw_frac = if refreshing {
        1.0
    } else if pull > 0.0 {
        pull / threshold
    } else {
        0.0
    };
    let distance_fraction = animate_f32_from(frac_key, 0.0, raw_frac, theme().motion.color);

    let adjusted_percent = (distance_fraction.min(1.0) - 0.4).max(0.0) * 5.0 / 3.0;
    let overshoot_percent = (distance_fraction - 1.0).max(0.0);
    let linear_tension = overshoot_percent.min(2.0);
    let tension_percent = linear_tension - linear_tension.powi(2) / 4.0;
    let rotation_turns = (-0.25 + 0.4 * adjusted_percent + tension_percent) * 0.5;
    // rotate by 360° to convert turns → degrees, then to radians for the modifier
    let spinner_rotation_rad = rotation_turns * std::f32::consts::TAU;

    // Indicator at top (pushed into view by overscroll) + content below.
    let indicator_h = distance_fraction * threshold;
    let comp_scale = adjusted_percent.min(1.0);
    let icon_size = if refreshing {
        24.0
    } else {
        (16.0 + comp_scale * 8.0).min(24.0)
    };
    let rotation = if refreshing {
        animate_f32_from(
            "ptr_spin",
            0.0,
            std::f32::consts::TAU,
            AnimationSpec::tween(Duration::from_millis(1000), Easing::Linear)
                .repeated(RepeatableSpec::infinite()),
        )
    } else {
        spinner_rotation_rad
    };
    let alpha = if refreshing {
        1.0
    } else if distance_fraction >= 1.0 {
        1.0
    } else {
        0.3
    };
    Column(modifier.align_items(config.content_alignment)).child((
        if distance_fraction > 0.01 {
            Box(Modifier::new()
                .fill_max_width()
                .height(indicator_h)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER))
            .child(
                Box(Modifier::new()
                    .size(icon_size, icon_size)
                    .translate(icon_size * 0.5, icon_size * 0.5)
                    .rotate(rotation)
                    .translate(-icon_size * 0.5, -icon_size * 0.5))
                .child(if refreshing {
                    Icon(Symbol::new("refresh", '\u{E5D5}'))
                        .size(24.0)
                        .color(config.indicator_color)
                } else {
                    Icon(Symbol::new("arrow_downward", '\u{E5DB}'))
                        .size(icon_size)
                        .color(config.indicator_color.with_alpha_f32(alpha))
                }),
            )
        } else {
            Box(Modifier::new())
        },
        content,
    ))
}
