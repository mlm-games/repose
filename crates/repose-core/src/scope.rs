use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::effects::Dispose;

thread_local! {
    static CURRENT_SCOPE: RefCell<Option<Weak<ScopeInner>>> = const { RefCell::new(None) };
}

pub struct Scope {
    inner: Rc<ScopeInner>,
}

struct ScopeInner {
    disposers: RefCell<Vec<Box<dyn FnOnce()>>>,
    children: RefCell<Vec<Scope>>,
    memo_cache: RefCell<std::collections::HashMap<String, Box<dyn Any>>>,
    disposed: Cell<bool>,
}

impl Default for Scope {
    fn default() -> Self {
        Self::new()
    }
}

impl Scope {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(ScopeInner {
                disposers: RefCell::new(Vec::new()),
                children: RefCell::new(Vec::new()),
                memo_cache: RefCell::new(std::collections::HashMap::new()),
                disposed: Cell::new(false),
            }),
        }
    }

    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        CURRENT_SCOPE.with(|current| {
            let prev = current.borrow().clone();
            *current.borrow_mut() = Some(Rc::downgrade(&self.inner));
            let result = f();
            *current.borrow_mut() = prev;
            result
        })
    }

    pub fn add_disposer(&self, disposer: impl FnOnce() + 'static) {
        self.inner.disposers.borrow_mut().push(Box::new(disposer));
    }

    /// Returns a cached value from this scope's memo cache, or creates it with
    /// `init` and stores it. The value persists for the lifetime of this scope
    /// (i.e., until the scope key is no longer composed or the root is replaced).
    pub fn memo<T: 'static>(&self, key: &str, init: impl FnOnce() -> T) -> Rc<T> {
        let mut cache = self.inner.memo_cache.borrow_mut();
        if let Some(existing) = cache.get(key) {
            if let Some(v) = existing.downcast_ref::<Rc<T>>() {
                return v.clone();
            }
        }
        let val: Rc<T> = Rc::new(init());
        cache.insert(key.to_string(), Box::new(val.clone()));
        val
    }

    pub fn child(&self) -> Scope {
        let child = Scope::new();
        self.inner.children.borrow_mut().push(child.clone());
        child
    }

    pub fn dispose(self) {
        if self.inner.disposed.replace(true) {
            return; // already disposed (or being dropped)
        }
        // Dispose children first
        let children = std::mem::take(&mut *self.inner.children.borrow_mut());
        for child in children {
            child.dispose();
        }

        // Run disposers
        let disposers = std::mem::take(&mut *self.inner.disposers.borrow_mut());
        for disposer in disposers {
            disposer();
        }
    }
}

impl Clone for Scope {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub fn current_scope() -> Option<Scope> {
    CURRENT_SCOPE.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|weak| weak.upgrade().map(|inner| Scope { inner }))
    })
}

/// Access this scope's memo cache from anywhere inside a `scope!` body.
/// Returns the cached value for `key`, or creates it with `init` and stores it.
/// The value persists until the scope is disposed.
///
/// Unlike `remember_with_key`, this is scoped to the current composition scope
/// and is automatically cleaned up when the scope is no longer composed.
pub fn scope_memo<T: 'static>(key: &str, init: impl FnOnce() -> T) -> Rc<T> {
    match current_scope() {
        Some(scope) => scope.memo(key, init),
        None => Rc::new(init()),
    }
}

/// Scoped effect that auto-cleans up.
///
/// Runs `f()` immediately and registers the returned `Dispose` to run when the
/// current scope is disposed.
pub fn scoped_effect<F>(f: F)
where
    F: FnOnce() -> Dispose + 'static,
{
    if let Some(scope) = current_scope() {
        let cleanup = f();
        scope.add_disposer(move || cleanup.run());
    } else {
        // No scope, run setup now, but drop cleanup (legacy "leak" behavior).
        let _cleanup = f();
    }
}

impl Drop for ScopeInner {
    fn drop(&mut self) {
        if self.disposed.replace(true) {
            return; // already disposed via explicit dispose() call
        }
        let children = std::mem::take(&mut *self.children.borrow_mut());
        for child in children {
            drop(child);
        }

        let disposers = std::mem::take(&mut *self.disposers.borrow_mut());
        for disposer in disposers {
            disposer();
        }
    }
}
