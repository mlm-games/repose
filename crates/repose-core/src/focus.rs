use crate::runtime::{FocusDirection, Frame, Scheduler, spatial_focus_next};
use crate::semantics::Role;
use crate::shortcuts::Action;

/// Handle a focus-related action.
/// Returns `Some(new_focus_id)` if focus moved, `None` otherwise.
pub fn handle_action(action: &Action, sched: &mut Scheduler, frame: &Frame) -> Option<u64> {
    let dir = match action {
        Action::FocusNext => FocusDirection::Next,
        Action::FocusPrevious => FocusDirection::Previous,
        Action::FocusLeft => FocusDirection::Left,
        Action::FocusRight => FocusDirection::Right,
        Action::FocusUp => FocusDirection::Up,
        Action::FocusDown => FocusDirection::Down,
        _ => return None,
    };

    // For spatial arrows, skip if focused element is a text field (arrows are for cursor)
    if matches!(
        dir,
        FocusDirection::Left | FocusDirection::Right | FocusDirection::Up | FocusDirection::Down
    ) {
        let is_textfield = frame
            .semantics_nodes
            .iter()
            .any(|n| sched.focused == Some(n.id) && n.role == Role::TextField);
        if is_textfield {
            return None;
        }
    }

    let next = spatial_focus_next(&frame.focus_chain, &frame.hit_regions, sched.focused, dir)?;
    sched.focused = Some(next);
    Some(next)
}
