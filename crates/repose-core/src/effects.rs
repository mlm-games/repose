use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Dispose(Rc<RefCell<Option<Box<dyn FnOnce()>>>>);

impl Dispose {
    pub fn new(f: impl FnOnce() + 'static) -> Self {
        Self(Rc::new(RefCell::new(Some(Box::new(f)))))
    }

    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    pub fn run(&self) {
        let function = {
            let mut function = self.0.borrow_mut();
            function.take()
        };
        if let Some(function) = function {
            function();
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
    effect_once_with_key(
        format!("effect:{}:{}:{}", loc.file(), loc.line(), loc.column()),
        f,
    )
}

pub fn effect_once_with_key(
    key: impl Into<String>,
    f: impl FnOnce() -> Dispose + 'static,
) -> Dispose {
    let key = key.into();
    let slot = crate::remember_with_key(key.clone(), || RefCell::new(None::<Dispose>));
    let existing = {
        let slot = slot.borrow();
        slot.as_ref().cloned()
    };
    if let Some(existing) = existing {
        return register_scope_owner(&key, &existing);
    }

    let disposer = f();
    let old = {
        let mut slot = slot.borrow_mut();
        slot.replace(disposer.clone())
    };
    drop(old);
    register_scope_owner(&key, &disposer)
}

fn register_scope_owner(key: &str, disposer: &Dispose) -> Dispose {
    let Some(scope) = crate::scope::current_scope() else {
        crate::runtime::register_keyed_disposer(key.to_string(), disposer.clone());
        return disposer.clone();
    };
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    let token_key = key.to_string();
    let token_disposer = disposer.clone();
    let token_owner = owner.clone();
    if !crate::runtime::keyed_disposer_has_owner(&token_key, &token_disposer, &owner) {
        crate::runtime::register_keyed_disposer_for_owner(
            token_key.clone(),
            token_disposer.clone(),
            owner.clone(),
        );
        let registered = token_disposer.clone();
        let scope_key = token_key.clone();
        let scope_owner = owner.clone();
        scope.add_disposer(move || {
            crate::runtime::remove_keyed_disposer_for_owner(&scope_key, &registered, &scope_owner);
        });
    }
    Dispose::new(move || {
        crate::runtime::remove_keyed_disposer_for_owner(&token_key, &token_disposer, &token_owner);
    })
}
/// Helper to register cleanup inside effect.
pub fn on_unmount(f: impl FnOnce() + 'static) -> Dispose {
    Dispose::new(f)
}
