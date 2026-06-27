use std::cell::RefCell;

thread_local! {
    static CLIPBOARD: RefCell<Option<Box<dyn Fn(&str)>>> = RefCell::new(None);
    static PRIMARY: RefCell<Option<Box<dyn Fn(&str)>>> = RefCell::new(None);
}

/// Register a global clipboard write function (Ctrl+C / system clipboard).
pub fn set_clipboard_fn(f: Box<dyn Fn(&str)>) {
    CLIPBOARD.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Copy text to the system clipboard via the registered setter.
pub fn copy_to_clipboard(text: &str) {
    CLIPBOARD.with(|slot| {
        if let Some(f) = slot.borrow().as_ref() {
            f(text);
        }
    });
}

/// Register a global primary selection write function (X11 middle-click buffer).
pub fn set_primary_fn(f: Box<dyn Fn(&str)>) {
    PRIMARY.with(|slot| *slot.borrow_mut() = Some(f));
}

/// Write text to the primary selection (middle-click paste on Linux/X11).
pub fn set_primary_selection(text: &str) {
    PRIMARY.with(|slot| {
        if let Some(f) = slot.borrow().as_ref() {
            f(text);
        }
    });
}
