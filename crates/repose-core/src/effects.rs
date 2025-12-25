use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Dispose(Rc<RefCell<Option<Box<dyn FnOnce()>>>>);

impl Dispose {
    pub fn new(f: impl FnOnce() + 'static) -> Self {
        Self(Rc::new(RefCell::new(Some(Box::new(f)))))
    }

    /// Runs at most once (safe to call multiple times).
    pub fn run(&self) {
        if let Some(f) = self.0.borrow_mut().take() {
            f()
        }
    }
}

/// Runs `f()` immediately and returns its `Dispose`.
pub fn effect<F>(f: F) -> Dispose
where
    F: FnOnce() -> Dispose + 'static,
{
    // run now
    let d = f();

    // auto-register cleanup in the current scope if one exists
    if let Some(scope) = crate::scope::current_scope() {
        let d2 = d.clone();
        scope.add_disposer(move || d2.run());
    }

    d
}
/// Helper to register cleanup inside effect.
pub fn on_unmount(f: impl FnOnce() + 'static) -> Dispose {
    Dispose::new(f)
}
