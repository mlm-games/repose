use crate::*;
use repose_core::input::{PointerButton, PointerEvent, PointerEventKind, PointerId, PointerKind};

/// Find the top-most hit region index under `pos` (reverse iteration).
pub(crate) fn top_hit_index(frame: &Frame, pos: Vec2) -> Option<usize> {
    frame
        .hit_regions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, h)| h.rect.contains(pos))
        .map(|(i, _)| i)
}

pub(crate) fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
}

pub(crate) fn pe_mouse(event: PointerEventKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind: PointerKind::Mouse,
        event,
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_touch(event: PointerEventKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind: PointerKind::Touch,
        event,
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_down_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind,
        event: PointerEventKind::Down(PointerButton::Primary),
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_up_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind,
        event: PointerEventKind::Up(PointerButton::Primary),
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

/// Dispatch wheel/touch-scroll to the top-most scroll consumer under `pos`.
/// Returns `true` if something consumed the scroll.
pub(crate) fn dispatch_scroll(frame: &Frame, pos: Vec2, delta: Vec2) -> bool {
    for hit in frame
        .hit_regions
        .iter()
        .rev()
        .filter(|h| h.rect.contains(pos))
    {
        if let Some(cb) = &hit.on_scroll {
            let before = delta;
            let leftover = cb(before);
            let consumed_x = (before.x - leftover.x).abs() > 0.001;
            let consumed_y = (before.y - leftover.y).abs() > 0.001;
            if consumed_x || consumed_y {
                return true;
            }
        }
    }
    false
}
