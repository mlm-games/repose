//! Currently holds small, platform-agnostic utilities.
//!
//! Next commit: Larger `App` deduplication is left for a follow-up (requires trait for `ApplicationHandler`).

use repose_app::ReposeRuntime;
use repose_core::Vec2;
use repose_core::input::Modifiers;
use winit::keyboard::ModifiersState;

/// Update `Modifiers` from winit state
pub fn update_modifiers(modifiers: &mut Modifiers, state: &ModifiersState) {
    crate::common::update_modifiers(modifiers, state)
}

/// Common scroll dispatch wrapper (handles `handle_scroll` wake).
pub fn handle_scroll(rt: &mut ReposeRuntime, delta: Vec2) -> bool {
    rt.handle_scroll(delta)
}
