use std::any::Any;
use std::cell::RefCell;
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

    pub fn child(&self) -> Scope {
        let child = Scope::new();
        self.inner.children.borrow_mut().push(child.clone());
        child
    }

    pub fn dispose(self) {
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
