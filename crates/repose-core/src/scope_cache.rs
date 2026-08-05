use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

use crate::View;

thread_local! {
    /// The scope key currently being composed (set by `scope!`).
    static CURRENT_SCOPE_KEY: RefCell<Option<String>> =
        const { RefCell::new(None) };

    /// signal_id -> set of scope keys that read it during composition.
    /// Cleaned up when a scope re-executes (old deps are replaced) or when
    /// the app disposes. Set semantics prevent duplicate keys per signal.
    static SCOPE_SIGNAL_DEPS: RefCell<FxHashMap<usize, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());

    /// scope key -> set of signal ids it read. Reverse map so clearing a
    /// scope's deps is O(deps) instead of a full-map scan.
    static SCOPE_TO_SIGNALS: RefCell<FxHashMap<String, FxHashSet<usize>>> =
        RefCell::new(FxHashMap::default());
}

/// Record that the current composition scope (if any) depends on `signal_id`.
/// Called from `reactive::register_signal_read`.
pub fn record_scope_signal_dep(signal_id: usize) {
    let key = CURRENT_SCOPE_KEY.with(|k| k.borrow().clone());
    if let Some(key) = key {
        SCOPE_SIGNAL_DEPS.with(|deps| {
            deps.borrow_mut()
                .entry(signal_id)
                .or_default()
                .insert(key.clone());
        });
        SCOPE_TO_SIGNALS.with(|m| {
            m.borrow_mut().entry(key).or_default().insert(signal_id);
        });
    }
}

/// Mark all scopes that depend on `signal_id` as dirty.
/// Called from `reactive::signal_changed`.
pub fn mark_scope_deps_dirty(signal_id: usize) {
    let keys = SCOPE_SIGNAL_DEPS.with(|deps| deps.borrow().get(&signal_id).cloned());
    if let Some(keys) = keys {
        for key in keys {
            crate::runtime::COMPOSER.with(|c| {
                let mut c = c.borrow_mut();
                if let Some(cache) = c.scope_caches.get_mut(&key) {
                    cache.clean = false;
                }
            });
        }
    }
}

/// Run `f` with the given scope key tracking any signal reads inside.
pub fn with_scope_key<R>(key: &str, f: impl FnOnce() -> R) -> R {
    CURRENT_SCOPE_KEY.with(|k| {
        let prev = k.borrow_mut().take();
        *k.borrow_mut() = Some(key.to_string());
        let result = f();
        *k.borrow_mut() = prev;
        result
    })
}

/// Clear all signal->scope tracking for the given scope key.
/// Called after the scope body executes, so old deps from a previous run are
/// replaced by the new deps registered during the just-completed run.
pub fn clear_scope_deps(key: &str) {
    let signals = SCOPE_TO_SIGNALS.with(|m| m.borrow_mut().remove(key));
    if let Some(signals) = signals {
        SCOPE_SIGNAL_DEPS.with(|deps| {
            let mut deps = deps.borrow_mut();
            for signal_id in signals {
                if let Some(scopes) = deps.get_mut(&signal_id) {
                    scopes.remove(key);
                    if scopes.is_empty() {
                        deps.remove(&signal_id);
                    }
                }
            }
        });
    }
}

/// Cached state for a single `scope!` invocation.
pub struct ScopeCache {
    /// Combined hash of all scope inputs from the last execution.
    pub input_hash: u64,
    /// The cached View tree produced by the last execution.
    pub view: View,
    /// How many `remember` slots the body consumed.
    pub slot_delta: usize,
    /// `true` if cached output is valid (no signal deps invalidated, inputs unchanged).
    pub clean: bool,
}

/// Check whether a scope should re-execute.
pub fn should_run(key: &str, input_hash: u64) -> bool {
    crate::runtime::COMPOSER.with(|c| {
        let c = c.borrow();
        match c.scope_caches.get(key) {
            Some(cache) => !cache.clean || cache.input_hash != input_hash,
            None => true,
        }
    })
}

/// Retrieve the cached View for a scope being skipped, advancing the remember-slot
/// cursor so sibling scopes remain consistent. IDs are self-contained in the cached
/// View (packed scope-local IDs), so no global ID advance is needed.
pub fn get_cached(key: &str, _s: &mut crate::runtime::Scheduler) -> View {
    crate::runtime::COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let (slot_delta, view) = {
            let cache = c
                .scope_caches
                .get(key)
                .expect("scope_cache::get_cached called but no cache entry found");
            (cache.slot_delta, cache.view.clone())
        };

        c.cursor += slot_delta;
        view
    })
}

/// Store a new or updated cache entry after executing the scope body.
pub fn set_cache(key: &str, input_hash: u64, view: View, slot_delta: usize) {
    crate::runtime::COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        c.scope_caches.insert(
            key.to_string(),
            ScopeCache {
                input_hash,
                view,
                slot_delta,
                clean: true,
            },
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::signal;

    fn reset_maps() {
        SCOPE_SIGNAL_DEPS.with(|d| d.borrow_mut().clear());
        SCOPE_TO_SIGNALS.with(|d| d.borrow_mut().clear());
    }

    #[test]
    fn scope_deps_deduplicate_keys() {
        reset_maps();
        let sig = signal(0);

        // Reading the same signal twice inside one scope registers one dep.
        with_scope_key("dedupe_scope", || {
            let _ = sig.get();
            let _ = sig.get();
        });

        SCOPE_SIGNAL_DEPS.with(|d| {
            let d = d.borrow();
            assert_eq!(
                d.get(&sig.id()).map(|s| s.len()),
                Some(1),
                "duplicate signal reads must collapse to a single scope dep"
            );
        });
        SCOPE_TO_SIGNALS.with(|d| {
            let d = d.borrow();
            assert_eq!(d.get("dedupe_scope").map(|s| s.len()), Some(1));
        });

        // Clearing the scope removes both the reverse entry and the forward entry.
        clear_scope_deps("dedupe_scope");
        SCOPE_TO_SIGNALS.with(|d| assert!(d.borrow().is_empty()));
        SCOPE_SIGNAL_DEPS.with(|d| assert!(d.borrow().is_empty()));
    }

    #[test]
    fn scope_deps_multiple_scopes_share_signal() {
        reset_maps();
        let sig = signal(0);

        with_scope_key("scope_a", || {
            let _ = sig.get();
        });
        with_scope_key("scope_b", || {
            let _ = sig.get();
        });

        SCOPE_SIGNAL_DEPS.with(|d| {
            let d = d.borrow();
            let scopes = d.get(&sig.id()).unwrap();
            assert!(scopes.contains("scope_a"));
            assert!(scopes.contains("scope_b"));
        });

        // Clearing only scope_a leaves scope_b intact.
        clear_scope_deps("scope_a");
        SCOPE_SIGNAL_DEPS.with(|d| {
            let d = d.borrow();
            let scopes = d.get(&sig.id()).unwrap();
            assert!(!scopes.contains("scope_a"));
            assert!(scopes.contains("scope_b"));
        });
        SCOPE_TO_SIGNALS.with(|d| {
            assert!(d.borrow().get("scope_a").is_none());
            assert!(d.borrow().get("scope_b").is_some());
        });
    }
}
