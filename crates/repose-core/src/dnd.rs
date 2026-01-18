use std::{any::Any, path::PathBuf, rc::Rc};

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

/// A single dropped file descriptor.
/// - On desktop: `path` is `Some(PathBuf)`.
/// - On web: `path` is usually `None` (browser doesn't expose local paths).
#[derive(Clone, Debug)]
pub struct DroppedFile {
    pub name: String,
    pub path: Option<PathBuf>,
}

/// Payload type for file drag/drop coming from the OS/browser.
#[derive(Clone, Debug)]
pub struct DroppedFiles {
    pub files: Vec<DroppedFile>,
}
