//! # Scroll model
//!
//! Repose separates visual scroll containers from scroll state.
//!
//! This file implements scroll composables (wrappers that attach scroll bindings
//! as modifiers). The actual scroll state types live in `repose_core::scroll`.
//!
//! Velocities are expressed in px/sec and integrated with dt,
//! so behavior is frame-rate independent.

pub use repose_core::scroll::{HorizontalScrollState, ScrollPhysics, ScrollState, ScrollStateXY};
use repose_core::*;
use std::rc::Rc;

/// Remembered ScrollState (requires unique key).
pub fn remember_scroll_state(key: impl Into<String>) -> Rc<ScrollState> {
    repose_core::remember_with_key(key.into(), ScrollState::new)
}

pub fn remember_horizontal_scroll_state(key: impl Into<String>) -> Rc<HorizontalScrollState> {
    repose_core::remember_with_key(key.into(), HorizontalScrollState::new)
}

pub fn remember_scroll_state_xy(key: impl Into<String>) -> Rc<ScrollStateXY> {
    repose_core::remember_with_key(key.into(), ScrollStateXY::new)
}

/// Scroll container with inertia, like verticalScroll.
pub fn ScrollArea(modifier: Modifier, state: Rc<ScrollState>, content: View) -> View {
    let binding = state.to_binding();
    View::new(0, ViewKind::Box)
        .modifier(modifier.vertical_scroll(match binding {
            ScrollBinding::Vertical(b) => b,
            _ => unreachable!(),
        }))
        .with_children(vec![content])
}

pub fn HorizontalScrollArea(
    modifier: Modifier,
    state: Rc<HorizontalScrollState>,
    content: View,
) -> View {
    let binding = state.to_binding();
    View::new(0, ViewKind::Box)
        .modifier(modifier.horizontal_scroll(match binding {
            ScrollBinding::Horizontal(b) => b,
            _ => unreachable!(),
        }))
        .with_children(vec![content])
}

pub fn ScrollAreaXY(modifier: Modifier, state: Rc<ScrollStateXY>, content: View) -> View {
    let binding = state.to_binding();
    View::new(0, ViewKind::Box)
        .modifier(modifier.scrollable(match binding {
            ScrollBinding::Both(b) => b,
            _ => unreachable!(),
        }))
        .with_children(vec![content])
}
