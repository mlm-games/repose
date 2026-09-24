use crate::{Dispose, on_unmount, scoped_effect};
use std::cell::RefCell;

/// Callsite-key for branch-stable effect storage.
macro_rules! effect_key {
    ($prefix:literal) => {{
        let loc = std::panic::Location::caller();
        format!("{}:{}:{}:{}", $prefix, loc.file(), loc.line(), loc.column())
    }};
}

/// cleanup on key change or unmount. Storage is callsite-keyed, so conditional
/// branches above the call site cannot shift slots.
#[track_caller]
pub fn disposable_effect<K: PartialEq + Clone + 'static>(
    key: K,
    effect: impl FnOnce() -> Dispose + 'static,
) {
    let callsite = effect_key!("de");
    let last_key = crate::remember_with_key(format!("{callsite}:last"), || RefCell::new(None::<K>));
    let cleanup_key = format!("{callsite}:cleanup");
    let cleanup_slot =
        crate::remember_with_key(cleanup_key.clone(), || RefCell::new(None::<Dispose>));
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    let installed = crate::remember_with_key(format!("{callsite}:installed:{owner}"), || {
        RefCell::new(false)
    });

    if !*installed.borrow() {
        *installed.borrow_mut() = true;
        let cleanup_slot = cleanup_slot.clone();
        let cleanup_key = cleanup_key.clone();
        let cleanup_owner = owner.clone();
        scoped_effect(move || {
            on_unmount(move || {
                let disposer = cleanup_slot.borrow().as_ref().cloned();
                if let Some(disposer) = disposer.as_ref() {
                    crate::runtime::remove_keyed_disposer_for_owner(
                        &cleanup_key,
                        disposer,
                        &cleanup_owner,
                    );
                }
                let (_, removed) =
                    crate::runtime::release_keyed_owner(&cleanup_key, &cleanup_owner);
                drop(removed);
            })
        });
    }

    let changed = last_key.borrow().as_ref() != Some(&key);
    if changed {
        let old_key = {
            let mut last = last_key.borrow_mut();
            last.replace(key.clone())
        };
        drop(old_key);

        let previous = {
            let mut slot = cleanup_slot.borrow_mut();
            slot.take()
        };
        if let Some(previous) = previous {
            crate::runtime::remove_keyed_disposer_for_owner(&cleanup_key, &previous, &owner);
        }

        let d = effect();
        let old = {
            let mut slot = cleanup_slot.borrow_mut();
            slot.replace(d.clone())
        };
        drop(old);
        crate::runtime::register_keyed_disposer_for_owner(cleanup_key.clone(), d, owner.clone());
    }
}

/// runs on every recomposition
pub fn side_effect(effect: impl Fn()) {
    effect();
}

/// Internal implementation: keyed by a per-callsite id string.
/// Runs the effect when `key` changes and runs the returned `Dispose` on key
/// change or unmount (cancellable launched effect).
pub fn launched_effect_internal<K: PartialEq + Clone + 'static>(
    callsite: &'static str,
    key: K,
    effect: impl FnOnce() -> Dispose + 'static,
) {
    disposable_effect_with_callsite(format!("launched:{callsite}"), key, effect);
}

fn disposable_effect_with_callsite<K: PartialEq + Clone + 'static>(
    callsite: String,
    key: K,
    effect: impl FnOnce() -> Dispose + 'static,
) {
    let last_key = crate::remember_with_key(format!("{callsite}:last"), || RefCell::new(None::<K>));
    let cleanup_key = format!("{callsite}:cleanup");
    let cleanup_slot =
        crate::remember_with_key(cleanup_key.clone(), || RefCell::new(None::<Dispose>));
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    let installed = crate::remember_with_key(format!("{callsite}:installed:{owner}"), || {
        RefCell::new(false)
    });

    if !*installed.borrow() {
        *installed.borrow_mut() = true;
        let cleanup_slot = cleanup_slot.clone();
        let cleanup_key = cleanup_key.clone();
        let cleanup_owner = owner.clone();
        scoped_effect(move || {
            on_unmount(move || {
                let disposer = cleanup_slot.borrow().as_ref().cloned();
                if let Some(disposer) = disposer.as_ref() {
                    crate::runtime::remove_keyed_disposer_for_owner(
                        &cleanup_key,
                        disposer,
                        &cleanup_owner,
                    );
                }
                let (_, removed) =
                    crate::runtime::release_keyed_owner(&cleanup_key, &cleanup_owner);
                drop(removed);
            })
        });
    }

    let changed = last_key.borrow().as_ref() != Some(&key);
    if changed {
        let old_key = {
            let mut last = last_key.borrow_mut();
            last.replace(key.clone())
        };
        drop(old_key);
        let previous = {
            let mut slot = cleanup_slot.borrow_mut();
            slot.take()
        };
        if let Some(previous) = previous {
            crate::runtime::remove_keyed_disposer_for_owner(&cleanup_key, &previous, &owner);
        }
        let d = effect();
        let old = {
            let mut slot = cleanup_slot.borrow_mut();
            slot.replace(d.clone())
        };
        drop(old);
        crate::runtime::register_keyed_disposer_for_owner(cleanup_key.clone(), d, owner.clone());
    }
}

/// Fire-and-forget launched effect: runs `effect` on key change with no
/// cleanup on unmount. Prefer `launched_effect!` (cancellable) unless the work
/// is intentionally uncancellable.
pub fn launched_effect_uncancelled_internal<K: PartialEq + Clone + 'static>(
    callsite: &'static str,
    key: K,
    effect: impl FnOnce() + 'static,
) {
    let last_key =
        crate::remember_with_key(format!("launched:{callsite}"), || RefCell::new(None::<K>));

    let changed = last_key.borrow().as_ref() != Some(&key);
    if changed {
        let old = {
            let mut last = last_key.borrow_mut();
            last.replace(key.clone())
        };
        drop(old);
        effect();
    }
}

#[macro_export] // Should probably move this to macros (might want to move the above part too?)
macro_rules! launched_effect {
    ($key:expr, $effect:expr) => {
        $crate::effects_ext::launched_effect_internal(
            concat!(module_path!(), ":", line!(), ":", column!()),
            $key,
            $effect,
        )
    };
}

#[macro_export]
macro_rules! launched_effect_uncancelled {
    ($key:expr, $effect:expr) => {
        $crate::effects_ext::launched_effect_uncancelled_internal(
            concat!(module_path!(), ":", line!(), ":", column!()),
            $key,
            $effect,
        )
    };
}
