use crate::effects::{Dispose, on_unmount};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    SwipeLeft,
    SwipeRight,
    /// delta_scale > 1 => zoom in; < 1 => zoom out
    Pinch {
        delta_scale: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,

    Back,
    Find,
    Save,

    Gesture(Gesture),
}

pub type Handler = Rc<dyn Fn(Action) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = RefCell::new(None);
}

/// Set/clear the global handler (prefer InstallShortcutHandler + scoped_effect).
pub fn set(handler: Option<Handler>) {
    HANDLER.with(|h| *h.borrow_mut() = handler);
}

/// Dispatch an action to the global handler. Returns true if consumed.
pub fn handle(action: Action) -> bool {
    HANDLER.with(|h| h.borrow().as_ref().map(|f| f(action)).unwrap_or(false))
}

/// Install/uninstall a global shortcut handler for the current scope.
#[allow(non_snake_case)]
pub fn InstallShortcutHandler(handler: Handler) -> Dispose {
    set(Some(handler));
    on_unmount(|| set(None))
}
