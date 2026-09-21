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

/// Runs `f()` immediately and returns its `Dispose`, registering cleanup on
/// the current scope when one exists. Like `scoped_effect`, this runs on every
/// call - for mount-once semantics use `scoped_effect_once` or
/// `disposable_effect`.
pub fn effect<F>(f: F) -> Dispose
where
    F: FnOnce() -> Dispose + 'static,
{
    let d = f();

    if let Some(scope) = crate::scope::current_scope() {
        let d2 = d.clone();
        scope.add_disposer(move || d2.run());
    } else {
        debug_assert!(
            false,
            "effect called without a current Scope; cleanup cannot be tracked"
        );
        log::error!("effect called without a current Scope; cleanup untracked");
    }

    d
}

/// Mount-once effect: runs `f` only the first time this call site composes.
/// Later recompositions return the original `Dispose` without re-running setup.
#[track_caller]
pub fn effect_once(f: impl FnOnce() -> Dispose + 'static) -> Dispose {
    let loc = std::panic::Location::caller();
    let key = format!("effect:{}:{}:{}", loc.file(), loc.line(), loc.column());
    let slot = crate::remember_with_key(key, || RefCell::new(None::<Dispose>));
    if let Some(d) = slot.borrow().as_ref() {
        return d.clone();
    }
    let d = effect(f);
    *slot.borrow_mut() = Some(d.clone());
    d
}
/// Helper to register cleanup inside effect.
pub fn on_unmount(f: impl FnOnce() + 'static) -> Dispose {
    Dispose::new(f)
}
