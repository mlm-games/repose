use rustc_hash::{FxHashMap, FxHashSet};
use std::cell::RefCell;

use crate::View;

thread_local! {
    /// Stack of scope keys currently being composed (set by `scope!`).
    /// A stack (not a single slot) so nested scopes attribute signal reads
    /// to every ancestor: otherwise an outer scope stays `clean` while an
    /// inner scope is dirty, and the outer cache short-circuits the inner
    /// re-execution, swallowing the update.
    static CURRENT_SCOPE_STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Legacy alias kept for the single-key fast path.
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
/// Records against every scope on the stack so ancestor scopes are dirtied
/// when a signal read only inside a nested scope changes.
pub fn record_scope_signal_dep(signal_id: usize) {
    let stack: Vec<String> = CURRENT_SCOPE_STACK.with(|s| s.borrow().clone());
    let stack = if stack.is_empty() {
        match CURRENT_SCOPE_KEY.with(|k| k.borrow().clone()) {
            Some(k) => vec![k],
            None => Vec::new(),
        }
    } else {
        stack
    };
    if stack.is_empty() {
        return;
    }
    SCOPE_SIGNAL_DEPS.with(|deps| {
        let mut deps = deps.borrow_mut();
        for key in &stack {
            deps.entry(signal_id).or_default().insert(key.clone());
        }
    });
    SCOPE_TO_SIGNALS.with(|m| {
        let mut m = m.borrow_mut();
        for key in &stack {
            m.entry(key.clone()).or_default().insert(signal_id);
        }
    });
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
/// Panic-safe: the scope stack is restored via a Drop guard.
pub fn with_scope_key<R>(key: &str, f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            if CURRENT_SCOPE_STACK
                .try_with(|s| {
                    if let Ok(mut s) = s.try_borrow_mut() {
                        s.pop();
                    } else {
                        log::error!(
                            "scope_cache: scope stack busy during scope exit; scope entry leaked"
                        );
                    }
                })
                .is_err()
            {
                log::error!(
                    "scope_cache: scope stack unavailable during scope exit (thread teardown?)"
                );
            }
            let top = CURRENT_SCOPE_STACK
                .try_with(|s| s.try_borrow().ok().and_then(|s| s.last().cloned()))
                .ok()
                .flatten();
            if CURRENT_SCOPE_KEY
                .try_with(|k| {
                    if let Ok(mut k) = k.try_borrow_mut() {
                        *k = top;
                    } else {
                        log::error!(
                            "scope_cache: current scope key busy during scope exit; stale scope key retained"
                        );
                    }
                })
                .is_err()
            {
                log::error!(
                    "scope_cache: current scope key unavailable during scope exit (thread teardown?)"
                );
            }
        }
    }
    CURRENT_SCOPE_STACK.with(|s| s.borrow_mut().push(key.to_string()));
    CURRENT_SCOPE_KEY.with(|k| *k.borrow_mut() = Some(key.to_string()));
    let _guard = Guard;
    let result = f();
    drop(_guard);
    result
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

/// Current innermost `scope!` key, if any. Used to attribute keyed
/// remembers to their owning scope for GC.
pub fn current_scope_key() -> Option<String> {
    CURRENT_SCOPE_KEY.with(|k| k.borrow().clone())
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
        c.live_scope_keys.insert(key.to_string());
        c.live_keyed_owners.insert(key.to_string());
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
        c.live_scope_keys.insert(key.to_string());
        c.live_keyed_owners.insert(key.to_string());
    });
}

/// Remove scope caches (and their keyed remembers) that were not composed
/// this frame. Called at the end of composition from `ComposeGuard::Drop`.
pub fn gc_dead_scopes() {
    crate::runtime::COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        if c.live_scope_keys.is_empty() && c.live_keyed_owners.len() <= 1 {
            return;
        }
        let dead_scopes: Vec<String> = c
            .scope_caches
            .keys()
            .filter(|k| !c.live_scope_keys.contains(*k))
            .cloned()
            .collect();
        for key in dead_scopes {
            c.scope_caches.remove(&key);
            clear_scope_deps_locked(&key);
        }
        let dead_keyed: Vec<String> = c
            .keyed_owner
            .iter()
            .filter(|(_, owner)| !c.live_keyed_owners.contains(*owner))
            .map(|(k, _)| k.clone())
            .collect();
        for key in dead_keyed {
            c.keyed_slots.remove(&key);
            c.keyed_owner.remove(&key);
        }
        c.live_scope_keys.clear();
        c.live_keyed_owners.clear();
        c.live_scope_keys.insert(String::new());
        c.live_keyed_owners.insert(String::new());
    });
}

fn clear_scope_deps_locked(key: &str) {
    SCOPE_TO_SIGNALS.with(|m| {
        let signals = m.borrow_mut().remove(key);
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
    });
}

pub fn clear_all_scope_deps() {
    SCOPE_SIGNAL_DEPS.with(|d| d.borrow_mut().clear());
    SCOPE_TO_SIGNALS.with(|d| d.borrow_mut().clear());
    CURRENT_SCOPE_STACK.with(|s| s.borrow_mut().clear());
    CURRENT_SCOPE_KEY.with(|k| *k.borrow_mut() = None);
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
