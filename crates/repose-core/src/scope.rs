use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use rustc_hash::FxHashMap;

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
    memo_cache: RefCell<FxHashMap<String, Box<dyn Any>>>,
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
                memo_cache: RefCell::new(FxHashMap::default()),
                disposed: Cell::new(false),
            }),
        }
    }

    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        struct Guard {
            prev: Option<Weak<ScopeInner>>,
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                CURRENT_SCOPE.with(|current| {
                    if let Ok(mut b) = current.try_borrow_mut() {
                        *b = self.prev.take();
                    } else {
                        log::error!(
                            "scope: CURRENT_SCOPE busy during scope exit; stale scope reference retained"
                        );
                    }
                });
            }
        }
        let prev = CURRENT_SCOPE.with(|current| current.borrow().clone());
        CURRENT_SCOPE.with(|current| {
            *current.borrow_mut() = Some(Rc::downgrade(&self.inner));
        });
        let _guard = Guard { prev };
        f()
    }

    pub fn add_disposer(&self, disposer: impl FnOnce() + 'static) {
        self.inner.disposers.borrow_mut().push(Box::new(disposer));
    }

    /// Returns a cached value from this scope's memo cache, or creates it with
    /// `init` and stores it. The value persists for the lifetime of this scope
    /// (i.e., until the scope key is no longer composed or the root is replaced).
    pub fn memo<T: 'static>(&self, key: &str, init: impl FnOnce() -> T) -> Rc<T> {
        let mut cache = self.inner.memo_cache.borrow_mut();
        if let Some(existing) = cache.get(key)
            && let Some(v) = existing.downcast_ref::<Rc<T>>()
        {
            return v.clone();
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
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child.dispose()));
            if let Err(e) = res {
                let msg = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                log::error!("Scope child dispose panicked: {msg}");
            }
        }

        let disposers = std::mem::take(&mut *self.inner.disposers.borrow_mut());
        for disposer in disposers {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(disposer));
            if let Err(e) = res {
                let msg = e
                    .downcast_ref::<String>()
                    .map(|s| s.as_str())
                    .or_else(|| e.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown");
                log::error!("Scope disposer panicked: {msg}");
            }
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

/// Access the current `Scope`'s memo cache (`Scope::memo`).
/// The value lives until that `Scope` is disposed (navigation pop, root
/// shutdown) - not merely until a `scope!` cache key stops composing,
/// since `scope!` is a memo cache, not a `Scope`.
pub fn scope_memo<T: 'static>(key: &str, init: impl FnOnce() -> T) -> Rc<T> {
    match current_scope() {
        Some(scope) => scope.memo(key, init),
        None => Rc::new(init()),
    }
}

/// Mount-once scoped effect: runs `f` only the first time this call site
/// composes, registering cleanup on the current scope. Later recompositions
/// are no-ops. Requires composition context.
#[track_caller]
pub fn scoped_effect_once(f: impl FnOnce() -> Dispose + 'static) {
    let loc = std::panic::Location::caller();
    let key = format!(
        "scoped_effect:{}:{}:{}",
        loc.file(),
        loc.line(),
        loc.column()
    );
    let installed = crate::remember_with_key(key, || std::cell::Cell::new(false));
    if !installed.get() {
        installed.set(true);
        scoped_effect(f);
    }
}

/// Scoped effect that auto-cleans up.
///
/// Runs `f()` immediately and registers the returned `Dispose` to run when the
/// current scope is disposed. This runs on every call - for mount-once
/// semantics use `scoped_effect_once`, `disposable_effect`, or keyed variants.
pub fn scoped_effect<F>(f: F)
where
    F: FnOnce() -> Dispose + 'static,
{
    if let Some(scope) = current_scope() {
        let cleanup = f();
        scope.add_disposer(move || cleanup.run());
    } else {
        debug_assert!(
            false,
            "scoped_effect called without a current Scope; setup skipped so cleanup cannot leak"
        );
        log::error!("scoped_effect called without a current Scope; setup skipped");
    }
}

impl Drop for ScopeInner {
    fn drop(&mut self) {
        if self.disposed.replace(true) {
            return; // already disposed via explicit dispose() call
        }
        let children = std::mem::take(&mut *self.children.borrow_mut());
        for child in children {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(child)));
            if let Err(e) = res {
                log::error!(
                    "ScopeInner drop child panicked: {}",
                    e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown")
                );
            }
        }

        let disposers = std::mem::take(&mut *self.disposers.borrow_mut());
        for disposer in disposers {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(disposer));
            if let Err(e) = res {
                log::error!(
                    "ScopeInner drop disposer panicked: {}",
                    e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown")
                );
            }
        }
    }
}
