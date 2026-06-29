use std::cell::RefCell;
use std::collections::HashMap;

use crate::View;

thread_local! {
    /// The scope key currently being composed (set by `scope!`).
    static CURRENT_SCOPE_KEY: RefCell<Option<String>> =
        const { RefCell::new(None) };

    /// signal_id → set of scope keys that read it during composition.
    /// Cleaned up when a scope re-executes (old deps are replaced) or when
    /// the app disposes.
    static SCOPE_SIGNAL_DEPS: RefCell<HashMap<usize, Vec<String>>> =
        RefCell::new(HashMap::new());
}

/// Record that the current composition scope (if any) depends on `signal_id`.
/// Called from `reactive::register_signal_read`.
pub fn record_scope_signal_dep(signal_id: usize) {
    CURRENT_SCOPE_KEY.with(|key| {
        if let Some(ref key) = *key.borrow() {
            SCOPE_SIGNAL_DEPS.with(|deps| {
                let mut deps = deps.borrow_mut();
                deps.entry(signal_id).or_default().push(key.clone());
            });
        }
    });
}

/// Mark all scopes that depend on `signal_id` as dirty.
/// Called from `reactive::signal_changed`.
pub fn mark_scope_deps_dirty(signal_id: usize) {
    SCOPE_SIGNAL_DEPS.with(|deps| {
        let deps = deps.borrow();
        if let Some(keys) = deps.get(&signal_id) {
            for key in keys {
                crate::runtime::COMPOSER.with(|c| {
                    let mut c = c.borrow_mut();
                    if let Some(cache) = c.scope_caches.get_mut(key) {
                        cache.clean = false;
                    }
                });
            }
        }
    });
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

/// Clear all signal→scope tracking for the given scope key.
/// Called after the scope body executes, so old deps from a previous run are
/// replaced by the new deps registered during the just-completed run.
pub fn clear_scope_deps(key: &str) {
    SCOPE_SIGNAL_DEPS.with(|deps| {
        let mut deps = deps.borrow_mut();
        deps.retain(|_, keys| {
            keys.retain(|k| k != key);
            !keys.is_empty()
        });
    });
}

/// Cached state for a single `scope!` invocation.
pub struct ScopeCache {
    /// Combined hash of all scope inputs from the last execution.
    pub input_hash: u64,
    /// The cached View tree produced by the last execution.
    pub view: View,
    /// Number of view IDs the body consumed.
    pub id_count: u32,
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

/// Retrieve the cached View for a scope being skipped, advancing cursor and ID
/// counter so subsequent sibling scopes remain consistent.
pub fn get_cached(key: &str, s: &mut crate::runtime::Scheduler) -> View {
    crate::runtime::COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let (slot_delta, id_count, view) = {
            let cache = c
                .scope_caches
                .get(key)
                .expect("scope_cache::get_cached called but no cache entry found");
            (cache.slot_delta, cache.id_count, cache.view.clone())
        };

        c.cursor += slot_delta;
        s.advance_id(id_count);
        view
    })
}

/// Store a new or updated cache entry after executing the scope body.
pub fn set_cache(key: &str, input_hash: u64, view: View, id_count: u32, slot_delta: usize) {
    crate::runtime::COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        c.scope_caches.insert(
            key.to_string(),
            ScopeCache {
                input_hash,
                view,
                id_count,
                slot_delta,
                clean: true,
            },
        );
    });
}
