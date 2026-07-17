use std::cell::RefCell;

thread_local! {
    static CLIPBOARD: RefCell<Option<Box<dyn Fn(&str)>>> = RefCell::new(None);
    static CLIPBOARD_OBSERVER: RefCell<Option<Box<dyn Fn(&str)>>> = RefCell::new(None);
    static PRIMARY: RefCell<Option<Box<dyn Fn(&str)>>> = RefCell::new(None);
    static CLIPBOARD_READ: RefCell<Option<Box<dyn Fn() -> Option<String>>>> = RefCell::new(None);
}

/// Register a global clipboard write function (Ctrl+C / system clipboard).
pub fn set_clipboard_fn(f: Box<dyn Fn(&str)>) {
    CLIPBOARD.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Register a transient clipboard observer that fires alongside the primary
/// callback.  Used by embeddable runtimes to capture clipboard text produced
/// during frame composition into `PlatformOutput`.
pub fn set_clipboard_observer(f: Box<dyn Fn(&str)>) {
    CLIPBOARD_OBSERVER.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Remove the clipboard observer.
pub fn clear_clipboard_observer() {
    CLIPBOARD_OBSERVER.with(|slot| *slot.borrow_mut() = None);
}

/// Copy text to the system clipboard via the registered setter.
pub fn copy_to_clipboard(text: &str) {
    let _ = CLIPBOARD.try_with(|slot| {
        if let Some(f) = slot.borrow().as_ref() {
            f(text);
        }
    });
    let _ = CLIPBOARD_OBSERVER.try_with(|slot| {
        if let Some(f) = slot.borrow().as_ref() {
            f(text);
        }
    });
}

/// Register a global clipboard read function (Ctrl+V / system clipboard paste).
pub fn set_clipboard_read_fn(f: Box<dyn Fn() -> Option<String>>) {
    CLIPBOARD_READ.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Read text from the system clipboard via the registered getter.
pub fn paste_text() -> Option<String> {
    CLIPBOARD_READ.with(|slot| slot.borrow().as_ref().and_then(|f| f()))
}

/// Register a global primary selection write function (X11 middle-click buffer).
pub fn set_primary_fn(f: Box<dyn Fn(&str)>) {
    PRIMARY.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Write text to the primary selection (middle-click paste on Linux/X11).
pub fn set_primary_selection(text: &str) {
    let _ = PRIMARY.try_with(|slot| {
        if let Some(f) = slot.borrow().as_ref() {
            f(text);
        }
    });
}
