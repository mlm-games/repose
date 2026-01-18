use std::{any::Any, rc::Rc};

/// Opaque payload moved during internal drag & drop.
/// Use `payload.as_ref().downcast_ref::<T>()` on the receiver side.
pub type DragPayload = Rc<dyn Any>;

#[derive(Clone, Debug)]
pub struct DragStart {
    pub source_id: u64,
    pub position: crate::Vec2,
    pub modifiers: crate::Modifiers,
}

#[derive(Clone, Debug)]
pub struct DragOver {
    pub source_id: u64,
    pub target_id: u64,
    pub position: crate::Vec2,
    pub modifiers: crate::Modifiers,
    pub payload: DragPayload,
}

#[derive(Clone, Debug)]
pub struct DropEvent {
    pub source_id: u64,
    pub target_id: u64,
    pub position: crate::Vec2,
    pub modifiers: crate::Modifiers,
    pub payload: DragPayload,
}

/// Sent to the drag source when the drag ends (drop or cancel).
#[derive(Clone, Copy, Debug)]
pub struct DragEnd {
    pub accepted: bool,
}
