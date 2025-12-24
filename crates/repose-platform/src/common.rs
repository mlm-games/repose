use crate::*;
use repose_core::input::{PointerButton, PointerEvent, PointerEventKind, PointerId, PointerKind};

use std::cell::RefCell;
use std::rc::Rc;

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

/// Shared state for runner-provided "auto root scroll".
#[derive(Default)]
pub(crate) struct RootScrollState {
    pub viewport_h: f32,
    pub content_h: f32,
    pub offset_y: f32,
}

impl RootScrollState {
    #[inline]
    pub fn max_offset(&self) -> f32 {
        (self.content_h - self.viewport_h).max(0.0)
    }
}

/// Wrap any `child` view in a vertical scroll container backed by `RootScrollState`.
/// This is how the runner can provide "overflow-y: auto" semantics for the whole app.
pub(crate) fn wrap_root_scroll(child: View, st: Rc<RefCell<RootScrollState>>) -> View {
    let st_get = st.clone();
    let get_scroll_offset = Some(Rc::new(move || st_get.borrow().offset_y) as Rc<dyn Fn() -> f32>);

    let st_set = st.clone();
    let set_scroll_offset = Some(Rc::new(move |y: f32| {
        let mut s = st_set.borrow_mut();
        s.offset_y = y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_vp = st.clone();
    let set_viewport_height = Some(Rc::new(move |h: f32| {
        let mut s = st_vp.borrow_mut();
        s.viewport_h = h.max(0.0);
        s.offset_y = s.offset_y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_ch = st.clone();
    let set_content_height = Some(Rc::new(move |h: f32| {
        let mut s = st_ch.borrow_mut();
        s.content_h = h.max(0.0);
        s.offset_y = s.offset_y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_scroll = st.clone();
    let on_scroll = Some(Rc::new(move |delta: Vec2| -> Vec2 {
        let mut s = st_scroll.borrow_mut();
        let max_off = s.max_offset();
        if max_off <= 0.5 {
            return delta; // nothing to consume
        }

        let before = s.offset_y;
        let target = (s.offset_y - delta.y).clamp(0.0, max_off);
        s.offset_y = target;

        let consumed = before - target;
        Vec2 {
            x: delta.x,
            y: delta.y - consumed, // leftover
        }
    }) as Rc<dyn Fn(Vec2) -> Vec2>);

    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll,
            set_viewport_height,
            set_content_height,
            get_scroll_offset,
            set_scroll_offset,
        },
    )
    .modifier(Modifier::new().fill_max_size())
    .with_children(vec![child])
}
