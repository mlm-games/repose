use std::{any::Any, path::PathBuf, rc::Rc};

/// Opaque payload moved during internal drag & drop.
/// Use [`downcast_drag_payload`] on the receiver side to recover a typed value.
pub type DragPayload = Rc<dyn Any>;

/// Wrap a typed value into a [`DragPayload`] for a drag source.
///
/// ```ignore
/// Modifier::new().on_drag_start(|_start| Some(drag_payload(MyItem { id: 1 })))
/// ```
pub fn drag_payload<T: 'static>(value: T) -> DragPayload {
    Rc::new(value)
}

/// Try to downcast a drag payload to a typed reference. Used on the drop side.
///
/// ```ignore
/// if let Some(item) = downcast_drag_payload::<MyItem>(&ev.payload) {
///     // handle item
/// }
/// ```
pub fn downcast_drag_payload<T: 'static>(payload: &DragPayload) -> Option<&T> {
    payload.as_ref().downcast_ref::<T>()
}

/// Block-style convenience for [`Modifier::on_drag_start`] with a typed payload.
///
/// ```ignore
/// use repose_core::{Modifier, drag_and_drop_source};
/// struct MyItem { id: i32 }
/// let m = drag_and_drop_source(Modifier::new(), |_start| Some(MyItem { id: 1 }));
/// ```
///
/// is equivalent to:
///
/// ```ignore
/// Modifier::new().on_drag_start(|_start| Some(drag_payload(MyItem { id: 1 })))
/// ```
pub fn drag_and_drop_source<T, F>(mut modifier: crate::Modifier, on_start: F) -> crate::Modifier
where
    T: 'static,
    F: Fn(DragStart) -> Option<T> + 'static,
{
    modifier = modifier.on_drag_start(move |start| on_start(start).map(drag_payload::<T>));
    modifier
}

/// Block-style convenience for [`Modifier::on_drop`] with a typed payload. The
/// drop is accepted when the closure returns `true`; the typed payload is
/// downcast before the closure is invoked.
///
/// ```ignore
/// use repose_core::{Modifier, drag_and_drop_target};
/// struct MyItem { id: i32 }
/// let m = drag_and_drop_target(Modifier::new(), |_ev, item: &MyItem| {
///     println!("got id {}", item.id);
///     true
/// });
/// ```
pub fn drag_and_drop_target<T, F>(mut modifier: crate::Modifier, on_drop: F) -> crate::Modifier
where
    T: 'static,
    F: Fn(&DropEvent, &T) -> bool + 'static,
{
    modifier = modifier.on_drop(move |ev| {
        match downcast_drag_payload::<T>(&ev.payload) {
            Some(v) => on_drop(&ev, v),
            None => false,
        }
    });
    modifier
}

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
