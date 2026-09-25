use std::cell::RefCell;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::View;

struct ScopeFrame {
    key: String,
    children: FxHashSet<String>,
    locals: FxHashMap<usize, u64>,
    animations: FxHashSet<String>,
}

thread_local! {
    static CURRENT_SCOPE_STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static CURRENT_SCOPE_KEY: RefCell<Option<String>> = const { RefCell::new(None) };
    static SCOPE_SIGNAL_DEPS: RefCell<FxHashMap<usize, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_TO_SIGNALS: RefCell<FxHashMap<String, FxHashSet<usize>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_DIRTY_EPOCHS: RefCell<FxHashMap<String, u64>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_PENDING_DIRTY: RefCell<FxHashSet<String>> =
        RefCell::new(FxHashSet::default());
    static SCOPE_CACHE_CHILDREN: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_CACHE_LOCALS: RefCell<FxHashMap<String, FxHashMap<usize, u64>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_CACHE_ANIMATIONS: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_LAST_CHILDREN: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_LAST_LOCALS: RefCell<FxHashMap<String, FxHashMap<usize, u64>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_LAST_ANIMATIONS: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_RUN_EPOCHS: RefCell<FxHashMap<String, u64>> =
        RefCell::new(FxHashMap::default());
    static SCOPE_FRAMES: RefCell<Vec<ScopeFrame>> = const { RefCell::new(Vec::new()) };
}

fn current_scope_stack() -> Vec<String> {
    let stack = CURRENT_SCOPE_STACK.with(|stack| stack.borrow().clone());
    if stack.is_empty() {
        CURRENT_SCOPE_KEY
            .with(|key| key.borrow().clone())
            .into_iter()
            .collect()
    } else {
        stack
    }
}

fn for_each_current_scope(mut f: impl FnMut(&str)) {
    let has_stack = CURRENT_SCOPE_STACK.with(|stack| !stack.borrow().is_empty());
    if has_stack {
        CURRENT_SCOPE_STACK.with(|stack| {
            for key in stack.borrow().iter() {
                f(key);
            }
        });
    } else if let Some(key) = CURRENT_SCOPE_KEY.with(|key| key.borrow().clone()) {
        f(&key);
    }
}

pub fn record_scope_signal_dep(signal: usize) {
    for_each_current_scope(|key| {
        SCOPE_SIGNAL_DEPS.with(|deps| {
            deps.borrow_mut()
                .entry(signal)
                .or_default()
                .insert(key.to_string());
        });
        SCOPE_TO_SIGNALS.with(|map| {
            map.borrow_mut()
                .entry(key.to_string())
                .or_default()
                .insert(signal);
        });
    });
}

fn mark_dirty(key: &str) {
    SCOPE_DIRTY_EPOCHS.with(|epochs| {
        let mut epochs = epochs.borrow_mut();
        let epoch = epochs.entry(key.to_string()).or_insert(0);
        *epoch = epoch.wrapping_add(1);
    });
    let marked = crate::runtime::COMPOSER
        .try_with(|composer| {
            composer
                .try_borrow_mut()
                .ok()
                .map(|mut composer| {
                    if let Some(cache) = composer.scope_caches.get_mut(key) {
                        cache.clean = false;
                    }
                })
                .is_some()
        })
        .unwrap_or(false);
    if !marked {
        let _ = SCOPE_PENDING_DIRTY.try_with(|pending| {
            if let Ok(mut pending) = pending.try_borrow_mut() {
                pending.insert(key.to_string());
                true
            } else {
                false
            }
        });
        crate::request_frame();
    }
}

pub fn mark_current_scope_dirty() {
    for_each_current_scope(mark_dirty);
}

pub fn mark_current_scope_dirty_for_signal(signal: usize) {
    for_each_current_scope(|key| {
        let dependent = SCOPE_SIGNAL_DEPS.with(|deps| {
            deps.borrow()
                .get(&signal)
                .is_some_and(|keys| keys.contains(key))
        });
        if !dependent {
            mark_dirty(key);
        }
    });
}

pub fn mark_scope_dirty(key: &str) {
    mark_dirty(key);
}

pub fn mark_scope_deps_dirty(signal: usize) {
    SCOPE_SIGNAL_DEPS.with(|deps| {
        let deps = deps.borrow();
        if let Some(keys) = deps.get(&signal) {
            for key in keys {
                mark_dirty(key);
            }
        }
    });
}

pub fn with_scope_key<R>(key: &str, function: impl FnOnce() -> R) -> R {
    struct Guard {
        key: String,
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            let frame = SCOPE_FRAMES
                .try_with(|frames| match frames.try_borrow_mut() {
                    Ok(mut frames) => frames
                        .iter()
                        .rposition(|frame| frame.key == self.key)
                        .map(|index| frames.remove(index)),
                    Err(_) => None,
                })
                .ok()
                .flatten();
            if let Some(frame) = frame {
                let _ = SCOPE_LAST_CHILDREN.try_with(|map| {
                    if let Ok(mut map) = map.try_borrow_mut() {
                        map.insert(self.key.clone(), frame.children);
                    }
                });
                let _ = SCOPE_LAST_LOCALS.try_with(|map| {
                    if let Ok(mut map) = map.try_borrow_mut() {
                        map.insert(self.key.clone(), frame.locals);
                    }
                });
                let _ = SCOPE_LAST_ANIMATIONS.try_with(|map| {
                    if let Ok(mut map) = map.try_borrow_mut() {
                        map.insert(self.key.clone(), frame.animations);
                    }
                });
            }
            let _ = CURRENT_SCOPE_STACK.try_with(|stack| {
                if let Ok(mut stack) = stack.try_borrow_mut()
                    && stack.last() == Some(&self.key)
                {
                    stack.pop();
                }
            });
            let top = CURRENT_SCOPE_STACK
                .try_with(|stack| {
                    stack
                        .try_borrow()
                        .ok()
                        .and_then(|stack| stack.last().cloned())
                })
                .ok()
                .flatten();
            let _ = CURRENT_SCOPE_KEY.try_with(|current| {
                if let Ok(mut current) = current.try_borrow_mut() {
                    *current = top;
                }
            });
        }
    }

    let key = key.to_string();
    let parent = CURRENT_SCOPE_STACK.with(|stack| stack.borrow().last().cloned());
    if let Some(parent) = parent {
        let _ = SCOPE_FRAMES.try_with(|frames| {
            if let Ok(mut frames) = frames.try_borrow_mut()
                && let Some(frame) = frames.last_mut()
                && frame.key == parent
            {
                frame.children.insert(key.clone());
            }
        });
    }
    CURRENT_SCOPE_STACK.with(|stack| stack.borrow_mut().push(key.clone()));
    CURRENT_SCOPE_KEY.with(|current| *current.borrow_mut() = Some(key.clone()));
    SCOPE_FRAMES.with(|frames| {
        frames.borrow_mut().push(ScopeFrame {
            key: key.clone(),
            children: FxHashSet::default(),
            locals: FxHashMap::default(),
            animations: FxHashSet::default(),
        })
    });
    let _guard = Guard { key };
    function()
}

pub fn record_scope_local_read(local: usize, value: u64) {
    let _ = SCOPE_FRAMES.try_with(|frames| {
        if let Ok(mut frames) = frames.try_borrow_mut()
            && let Some(frame) = frames.last_mut()
        {
            frame.locals.insert(local, value);
        }
    });
}

pub fn record_scope_animation_key(key: &str) {
    let _ = SCOPE_FRAMES.try_with(|frames| {
        if let Ok(mut frames) = frames.try_borrow_mut()
            && let Some(frame) = frames.last_mut()
        {
            frame.animations.insert(key.to_string());
        }
    });
}

pub fn clear_scope_deps(key: &str) {
    let signals = SCOPE_TO_SIGNALS.with(|map| map.borrow_mut().remove(key));
    if let Some(signals) = signals {
        SCOPE_SIGNAL_DEPS.with(|deps| {
            let mut deps = deps.borrow_mut();
            for signal in signals {
                if let Some(scopes) = deps.get_mut(&signal) {
                    scopes.remove(key);
                    if scopes.is_empty() {
                        deps.remove(&signal);
                    }
                }
            }
        });
    }
    let epoch = SCOPE_DIRTY_EPOCHS.with(|epochs| epochs.borrow().get(key).copied().unwrap_or(0));
    SCOPE_RUN_EPOCHS.with(|epochs| epochs.borrow_mut().insert(key.to_string(), epoch));
}

pub struct ScopeCache {
    pub input_hash: u64,
    pub view: View,
    pub slot_delta: usize,
    pub clean: bool,
}

fn collect_view_scope_keys(view: &View, keys: &mut FxHashSet<String>) {
    if let Some(key) = &view.scope_key {
        keys.insert(key.clone());
    }
    for child in &view.children {
        collect_view_scope_keys(child, keys);
    }
}

fn view_has_nested_scope(view: &View, parent_key: &str) -> bool {
    if view
        .scope_key
        .as_deref()
        .is_some_and(|scope_key| scope_key != parent_key)
    {
        return true;
    }
    view.children
        .iter()
        .any(|child| view_has_nested_scope(child, parent_key))
}

fn has_cached_scope_children(key: &str) -> bool {
    let cached = SCOPE_CACHE_CHILDREN.with(|map| {
        map.borrow()
            .get(key)
            .is_some_and(|children| !children.is_empty())
    });
    if cached {
        return true;
    }
    crate::runtime::COMPOSER
        .try_with(|composer| {
            composer.try_borrow().ok().is_some_and(|composer| {
                composer
                    .scope_caches
                    .get(key)
                    .is_some_and(|cache| view_has_nested_scope(&cache.view, key))
            })
        })
        .ok()
        .unwrap_or(false)
}

fn cached_scope_children(key: &str) -> FxHashSet<String> {
    let mut children =
        SCOPE_CACHE_CHILDREN.with(|map| map.borrow().get(key).cloned().unwrap_or_default());
    if children.is_empty() {
        let _ = crate::runtime::COMPOSER.try_with(|composer| {
            if let Ok(composer) = composer.try_borrow()
                && let Some(cache) = composer.scope_caches.get(key)
            {
                let mut keys = FxHashSet::default();
                collect_view_scope_keys(&cache.view, &mut keys);
                keys.remove(key);
                children.extend(keys);
            }
        });
    }
    children
}

fn cached_locals_changed(key: &str) -> bool {
    SCOPE_CACHE_LOCALS.with(|map| {
        map.borrow().get(key).is_some_and(|values| {
            values
                .iter()
                .any(|(local, value)| crate::locals::local_fingerprint(*local) != *value)
        })
    })
}

pub fn should_run(key: &str, input_hash: u64) -> bool {
    if SCOPE_PENDING_DIRTY.with(|pending| pending.borrow().contains(key)) {
        return true;
    }
    if has_cached_scope_children(key) {
        return true;
    }
    let locals_changed = cached_locals_changed(key);
    crate::runtime::COMPOSER.with(|composer| {
        let composer = composer.borrow();
        match composer.scope_caches.get(key) {
            Some(cache) => !cache.clean || cache.input_hash != input_hash || locals_changed,
            None => true,
        }
    })
}

pub fn current_scope_key() -> Option<String> {
    CURRENT_SCOPE_KEY.with(|key| key.borrow().clone())
}

pub fn validate_scope_key(key: &str) {
    assert!(!key.is_empty(), "scope keys must not be empty");
    assert!(
        crate::runtime::scope_owner_token(Some(key)) != crate::runtime::scope_owner_token(None),
        "scope keys must not use the private root owner sentinel"
    );
}

fn record_cached_child(key: &str) {
    let _ = SCOPE_FRAMES.try_with(|frames| {
        if let Ok(mut frames) = frames.try_borrow_mut()
            && let Some(frame) = frames.last_mut()
        {
            frame.children.insert(key.to_string());
        }
    });
}

fn restore_cached_deps(key: &str) {
    fn visit(key: &str, ancestors: &[String], seen: &mut FxHashSet<String>) {
        if !seen.insert(key.to_string()) {
            return;
        }
        let mut path = ancestors.to_vec();
        if !path.iter().any(|entry| entry == key) {
            path.push(key.to_string());
        }
        let signals = SCOPE_TO_SIGNALS.with(|map| map.borrow().get(key).cloned());
        if let Some(signals) = signals {
            for signal in signals {
                record_signal_for_keys(signal, &path);
            }
        }
        for child in cached_scope_children(key) {
            visit(&child, &path, seen);
        }
    }

    fn record_signal_for_keys(signal: usize, keys: &[String]) {
        SCOPE_SIGNAL_DEPS.with(|deps| {
            let mut deps = deps.borrow_mut();
            for key in keys {
                deps.entry(signal).or_default().insert(key.clone());
            }
        });
        SCOPE_TO_SIGNALS.with(|map| {
            let mut map = map.borrow_mut();
            for key in keys {
                map.entry(key.clone()).or_default().insert(signal);
            }
        });
    }

    let ancestors = current_scope_stack();
    visit(key, &ancestors, &mut FxHashSet::default());
}

fn mark_scope_live(key: &str) {
    let mut pending = vec![key.to_string()];
    let mut seen = FxHashSet::default();
    while let Some(current) = pending.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        let children = cached_scope_children(&current);
        let exists = crate::runtime::COMPOSER.with(|composer| {
            let mut composer = composer.borrow_mut();
            let exists = composer.scope_caches.contains_key(&current);
            if exists {
                composer.live_scope_keys.insert(current.clone());
                composer
                    .live_keyed_owners
                    .insert(crate::runtime::scope_owner_token(Some(&current)));
            }
            exists
        });
        if exists {
            let animations = SCOPE_CACHE_ANIMATIONS
                .with(|map| map.borrow().get(&current).cloned().unwrap_or_default());
            for animation in animations {
                crate::animation_driver::touch_cached(&animation, &current);
            }
            pending.extend(children);
        }
    }
}

pub fn get_cached(key: &str, _scheduler: &mut crate::runtime::Scheduler) -> View {
    let (slot_delta, view) = crate::runtime::COMPOSER.with(|composer| {
        let composer = composer.borrow();
        let cache = composer
            .scope_caches
            .get(key)
            .expect("scope_cache::get_cached called but no cache entry found");
        (cache.slot_delta, cache.view.clone())
    });
    crate::runtime::COMPOSER.with(|composer| composer.borrow_mut().cursor += slot_delta);
    record_cached_child(key);
    restore_cached_deps(key);
    mark_scope_live(key);
    view
}

pub fn set_cache(key: &str, input_hash: u64, view: View, slot_delta: usize) {
    let dirty_before = SCOPE_RUN_EPOCHS.with(|epochs| epochs.borrow().get(key).copied());
    let clean = dirty_before.is_none_or(|before| dirty_generation(key) == before);
    set_cache_inner(key, input_hash, view, slot_delta, clean);
}

pub fn dirty_generation(key: &str) -> u64 {
    SCOPE_DIRTY_EPOCHS.with(|epochs| epochs.borrow().get(key).copied().unwrap_or(0))
}

pub fn set_cache_preserving_dirty(
    key: &str,
    input_hash: u64,
    view: View,
    slot_delta: usize,
    dirty_before: u64,
) {
    let clean = dirty_generation(key) == dirty_before;
    set_cache_inner(key, input_hash, view, slot_delta, clean);
}

fn set_cache_inner(key: &str, input_hash: u64, view: View, slot_delta: usize, clean: bool) {
    let children = SCOPE_LAST_CHILDREN
        .with(|map| map.borrow_mut().remove(key))
        .unwrap_or_default();
    let locals = SCOPE_LAST_LOCALS
        .with(|map| map.borrow_mut().remove(key))
        .unwrap_or_default();
    let animations = SCOPE_LAST_ANIMATIONS
        .with(|map| map.borrow_mut().remove(key))
        .unwrap_or_default();
    let old_cache = crate::runtime::COMPOSER.with(|composer| {
        let mut composer = composer.borrow_mut();
        composer.scope_caches.insert(
            key.to_string(),
            ScopeCache {
                input_hash,
                view,
                slot_delta,
                clean,
            },
        )
    });
    drop(old_cache);
    SCOPE_CACHE_CHILDREN.with(|map| {
        let mut map = map.borrow_mut();
        if children.is_empty() {
            map.remove(key);
        } else {
            map.insert(key.to_string(), children);
        }
    });
    SCOPE_CACHE_LOCALS.with(|map| {
        let mut map = map.borrow_mut();
        if locals.is_empty() {
            map.remove(key);
        } else {
            map.insert(key.to_string(), locals);
        }
    });
    SCOPE_CACHE_ANIMATIONS.with(|map| {
        let mut map = map.borrow_mut();
        if animations.is_empty() {
            map.remove(key);
        } else {
            map.insert(key.to_string(), animations);
        }
    });
    SCOPE_RUN_EPOCHS.with(|epochs| {
        epochs.borrow_mut().remove(key);
    });
    SCOPE_PENDING_DIRTY.with(|pending| {
        pending.borrow_mut().remove(key);
    });
    mark_scope_live(key);
}

pub fn gc_dead_scopes() {
    let (dead_scopes, dead_keyed, removed_caches) = crate::runtime::COMPOSER.with(|composer| {
        let mut composer = composer.borrow_mut();
        let dead_scopes: Vec<String> = composer
            .scope_caches
            .keys()
            .filter(|key| !composer.live_scope_keys.contains(*key))
            .cloned()
            .collect();
        let mut dead_keyed = crate::runtime::take_dead_keyed_slots(&mut composer);
        dead_keyed
            .sort_by_key(|(key, _)| std::cmp::Reverse(crate::runtime::keyed_disposer_order(key)));
        let mut removed_caches = Vec::with_capacity(dead_scopes.len());
        for key in &dead_scopes {
            if let Some(cache) = composer.scope_caches.remove(key) {
                removed_caches.push(cache);
            }
        }
        composer.live_scope_keys.clear();
        composer.live_keyed_owners.clear();
        composer.live_scope_keys.insert(String::new());
        composer
            .live_keyed_owners
            .insert(crate::runtime::scope_owner_token(None));
        (dead_scopes, dead_keyed, removed_caches)
    });

    drop(removed_caches);

    for key in dead_scopes {
        clear_scope_deps(&key);
        SCOPE_DIRTY_EPOCHS.with(|epochs| {
            epochs.borrow_mut().remove(&key);
        });
        SCOPE_PENDING_DIRTY.with(|pending| {
            pending.borrow_mut().remove(&key);
        });
        SCOPE_CACHE_CHILDREN.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_CACHE_LOCALS.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_CACHE_ANIMATIONS.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_LAST_CHILDREN.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_LAST_LOCALS.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_LAST_ANIMATIONS.with(|map| {
            map.borrow_mut().remove(&key);
        });
        SCOPE_RUN_EPOCHS.with(|epochs| {
            epochs.borrow_mut().remove(&key);
        });
        crate::animation_driver::release_scope(&key);
    }

    for (key, _slot) in &dead_keyed {
        clear_scope_deps(key);
        if let Some(disposer) = crate::runtime::take_keyed_disposer(key) {
            crate::runtime::run_keyed_disposer(disposer);
        }
    }
    drop(dead_keyed);
}

pub fn touch_live_cached_animations() {
    let live = crate::runtime::COMPOSER
        .try_with(|composer| {
            composer
                .try_borrow()
                .ok()
                .map(|composer| composer.live_scope_keys.clone())
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    for key in live {
        for animation in cached_animation_keys(&key) {
            crate::animation_driver::touch_cached(&animation, &key);
        }
    }
}

pub fn cached_animation_keys(key: &str) -> Vec<String> {
    SCOPE_CACHE_ANIMATIONS
        .with(|map| {
            map.borrow()
                .get(key)
                .map(|keys| keys.iter().cloned().collect())
        })
        .unwrap_or_default()
}

pub fn clear_all_scope_deps() {
    SCOPE_SIGNAL_DEPS.with(|map| map.borrow_mut().clear());
    SCOPE_TO_SIGNALS.with(|map| map.borrow_mut().clear());
    SCOPE_DIRTY_EPOCHS.with(|map| map.borrow_mut().clear());
    SCOPE_PENDING_DIRTY.with(|map| map.borrow_mut().clear());
    SCOPE_CACHE_CHILDREN.with(|map| map.borrow_mut().clear());
    SCOPE_CACHE_LOCALS.with(|map| map.borrow_mut().clear());
    SCOPE_CACHE_ANIMATIONS.with(|map| map.borrow_mut().clear());
    SCOPE_LAST_CHILDREN.with(|map| map.borrow_mut().clear());
    SCOPE_LAST_LOCALS.with(|map| map.borrow_mut().clear());
    SCOPE_LAST_ANIMATIONS.with(|map| map.borrow_mut().clear());
    SCOPE_RUN_EPOCHS.with(|map| map.borrow_mut().clear());
    SCOPE_FRAMES.with(|frames| frames.borrow_mut().clear());
    CURRENT_SCOPE_STACK.with(|stack| stack.borrow_mut().clear());
    CURRENT_SCOPE_KEY.with(|key| *key.borrow_mut() = None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::signal;

    fn reset_maps() {
        SCOPE_SIGNAL_DEPS.with(|map| map.borrow_mut().clear());
        SCOPE_TO_SIGNALS.with(|map| map.borrow_mut().clear());
        SCOPE_DIRTY_EPOCHS.with(|map| map.borrow_mut().clear());
        SCOPE_PENDING_DIRTY.with(|map| map.borrow_mut().clear());
    }

    #[test]
    fn scope_deps_deduplicate_keys() {
        reset_maps();
        let signal = signal(0);
        with_scope_key("dedupe_scope", || {
            let _ = signal.get();
            let _ = signal.get();
        });
        SCOPE_SIGNAL_DEPS.with(|deps| {
            assert_eq!(deps.borrow().get(&signal.id()).map(FxHashSet::len), Some(1));
        });
        clear_scope_deps("dedupe_scope");
        SCOPE_SIGNAL_DEPS.with(|deps| assert!(deps.borrow().is_empty()));
    }

    #[test]
    fn nested_scope_reads_dirty_ancestors() {
        reset_maps();
        let signal = signal(0);
        with_scope_key("outer", || {
            with_scope_key("inner", || {
                let _ = signal.get();
            });
        });
        signal.set(1);
        assert_eq!(dirty_generation("outer"), 1);
        assert_eq!(dirty_generation("inner"), 1);
    }
}
