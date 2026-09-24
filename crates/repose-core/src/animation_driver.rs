use std::cell::{Cell, RefCell};
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::request_frame;

type TickFn = Rc<RefCell<dyn FnMut() -> bool>>;
const GLOBAL_OWNER: &str = "\0repose:animation-root";

fn call_tick(callback: &mut dyn FnMut() -> bool) -> bool {
    callback()
}

thread_local! {
    static REGISTRY: RefCell<Vec<(String, TickFn)>> = const { RefCell::new(Vec::new()) };
    static OWNERS: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static TOUCHED: RefCell<FxHashSet<String>> = RefCell::new(FxHashSet::default());
    static PENDING_TOUCHES: RefCell<FxHashSet<String>> =
        RefCell::new(FxHashSet::default());
    static PENDING_REGISTRATIONS: RefCell<Vec<(String, TickFn, String)>> =
        const { RefCell::new(Vec::new()) };
    static LIVE_EPOCH: Cell<u64> = const { Cell::new(0) };
    static TICKING: Cell<bool> = const { Cell::new(false) };
    static SHUTTING_DOWN: Cell<bool> = const { Cell::new(false) };
}

fn current_scope() -> Option<String> {
    crate::scope_cache::current_scope_key()
}

fn current_owner() -> String {
    current_scope().unwrap_or_else(|| GLOBAL_OWNER.to_string())
}

fn add_owner(key: &str, owner: &str) {
    let _ = OWNERS.try_with(|owners| {
        if let Ok(mut owners) = owners.try_borrow_mut() {
            owners
                .entry(key.to_string())
                .or_default()
                .insert(owner.to_string());
        }
    });
}

fn remove_owner(key: &str, owner: &str) -> bool {
    OWNERS
        .try_with(|owners| {
            let mut owners = match owners.try_borrow_mut() {
                Ok(owners) => owners,
                Err(_) => return false,
            };
            let Some(set) = owners.get_mut(key) else {
                return false;
            };
            set.remove(owner);
            let empty = set.is_empty();
            if empty {
                owners.remove(key);
            }
            empty
        })
        .unwrap_or(false)
}

fn owner_list(key: &str) -> Vec<String> {
    OWNERS
        .try_with(|owners| {
            owners
                .try_borrow()
                .ok()
                .and_then(|owners| owners.get(key).map(|set| set.iter().cloned().collect()))
        })
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn clear_key_state(key: &str) {
    let _ = TOUCHED.try_with(|touched| {
        if let Ok(mut touched) = touched.try_borrow_mut() {
            touched.remove(key);
        }
    });
    let _ = PENDING_TOUCHES.try_with(|pending| {
        if let Ok(mut pending) = pending.try_borrow_mut() {
            pending.remove(key);
        }
    });
    let _ = PENDING_REGISTRATIONS.try_with(|pending| {
        if let Ok(mut pending) = pending.try_borrow_mut() {
            pending.retain(|(registered, _, _)| registered != key);
        }
    });
}

fn remove_empty_registration(key: &str) {
    let removed = REGISTRY
        .try_with(|registry| {
            registry.try_borrow_mut().ok().and_then(|mut registry| {
                let position = registry
                    .iter()
                    .position(|(registered, _)| registered == key);
                position.map(|index| registry.remove(index).1)
            })
        })
        .ok()
        .flatten();
    if let Some(tick) = removed {
        drop(tick);
        clear_key_state(key);
    }
}

fn mark_touched(key: &str) {
    let inserted = TOUCHED
        .try_with(|touched| {
            touched
                .try_borrow_mut()
                .ok()
                .map(|mut touched| touched.insert(key.to_string()))
        })
        .ok()
        .flatten();
    if inserted != Some(true) {
        let queued = PENDING_TOUCHES
            .try_with(|pending| {
                pending
                    .try_borrow_mut()
                    .ok()
                    .map(|mut pending| pending.insert(key.to_string()))
            })
            .ok()
            .flatten();
        if queued != Some(true) {
            request_frame();
        }
    }
}

pub fn touch(key: &str) {
    if SHUTTING_DOWN.with(Cell::get) {
        return;
    }
    mark_touched(key);
    if let Some(scope) = current_scope() {
        crate::scope_cache::record_scope_animation_key(key);
        if is_registered(key) {
            add_owner(key, &scope);
            crate::scope_cache::mark_scope_dirty(&scope);
        }
    }
}

pub fn touch_cached(key: &str, scope: &str) {
    if SHUTTING_DOWN.with(Cell::get) {
        return;
    }
    mark_touched(key);
    if is_registered(key) {
        add_owner(key, scope);
        crate::scope_cache::mark_scope_dirty(scope);
    }
}

fn queue_registration(key: String, tick: TickFn, owner: String) {
    let queued = PENDING_REGISTRATIONS
        .try_with(|pending| {
            pending.try_borrow_mut().ok().map(|mut pending| {
                pending.push((key, tick, owner));
                true
            })
        })
        .ok()
        .flatten();
    if queued != Some(true) {
        request_frame();
    }
}

fn register_for_owner(key: String, tick: TickFn, owner: String) {
    if SHUTTING_DOWN.with(Cell::get) {
        return;
    }
    let current = current_scope();
    let (old, changed) = REGISTRY
        .try_with(|registry| match registry.try_borrow_mut() {
            Ok(mut registry) => {
                let position = registry
                    .iter()
                    .position(|(registered, _)| registered == &key);
                if let Some(index) = position {
                    if Rc::ptr_eq(&registry[index].1, &tick) {
                        (None, false)
                    } else {
                        let old = registry.remove(index);
                        registry.push((key.clone(), tick.clone()));
                        (Some(old.1), true)
                    }
                } else {
                    registry.push((key.clone(), tick.clone()));
                    (None, true)
                }
            }
            Err(_) => {
                queue_registration(key.clone(), tick.clone(), owner.clone());
                (None, false)
            }
        })
        .unwrap_or((None, false));
    drop(old);
    if changed || is_registered(&key) {
        add_owner(&key, &owner);
    }
    if let Some(scope) = current {
        crate::scope_cache::record_scope_animation_key(&key);
        if changed {
            crate::scope_cache::mark_scope_dirty(&scope);
        }
    }
    if changed {
        mark_touched(&key);
        LIVE_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
        request_frame();
    }
}

fn flush_pending_registrations() {
    let pending = PENDING_REGISTRATIONS
        .try_with(|pending| {
            pending
                .try_borrow_mut()
                .ok()
                .map(|mut pending| std::mem::take(&mut *pending))
        })
        .ok()
        .flatten();
    if let Some(pending) = pending {
        for (key, tick, owner) in pending {
            register_for_owner(key, tick, owner);
        }
    } else {
        request_frame();
    }
}

pub fn register(key: String, tick: TickFn) {
    register_for_owner(key, tick, current_owner());
}

fn release_owner(key: &str, owner: &str) {
    if remove_owner(key, owner) {
        remove_empty_registration(key);
    }
}

pub fn unregister(key: &str) {
    if SHUTTING_DOWN.with(Cell::get) {
        return;
    }
    if let Some(scope) = current_scope() {
        release_owner(key, &scope);
        return;
    }
    let had_global = OWNERS
        .try_with(|owners| {
            owners
                .try_borrow()
                .ok()
                .and_then(|owners| owners.get(key).map(|set| set.contains(GLOBAL_OWNER)))
        })
        .ok()
        .flatten()
        .unwrap_or(false);
    if had_global {
        release_owner(key, GLOBAL_OWNER);
    } else {
        let owners = owner_list(key);
        if owners.len() == 1 {
            release_owner(key, &owners[0]);
        } else if owners.len() > 1 {
            return;
        } else {
            remove_empty_registration(key);
        }
    }
}

pub fn release(key: &str) {
    unregister(key);
}

pub fn release_scope(scope: &str) {
    let keys = OWNERS
        .try_with(|owners| {
            owners
                .try_borrow()
                .ok()
                .map(|owners| {
                    owners
                        .iter()
                        .filter(|(_, set)| set.contains(scope))
                        .map(|(key, _)| key.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .ok()
        .unwrap_or_default();
    for key in keys {
        release_owner(&key, scope);
    }
}

pub fn is_registered(key: &str) -> bool {
    REGISTRY
        .try_with(|registry| {
            registry
                .try_borrow()
                .map(|registry| registry.iter().any(|(registered, _)| registered == key))
                .unwrap_or(false)
        })
        .ok()
        .unwrap_or(false)
}

fn take_touched() -> Option<FxHashSet<String>> {
    let mut touched = TOUCHED
        .try_with(|touched| {
            touched
                .try_borrow_mut()
                .ok()
                .map(|mut touched| std::mem::take(&mut *touched))
        })
        .ok()
        .flatten()?;
    let pending = PENDING_TOUCHES
        .try_with(|pending| {
            pending
                .try_borrow_mut()
                .ok()
                .map(|mut pending| std::mem::take(&mut *pending))
        })
        .ok()
        .flatten();
    if let Some(pending) = pending {
        touched.extend(pending);
    } else {
        request_frame();
    }
    Some(touched)
}

pub fn tick() -> bool {
    if SHUTTING_DOWN.with(Cell::get) {
        return false;
    }
    if TICKING.with(Cell::get) {
        return true;
    }
    TICKING.with(|ticking| ticking.set(true));
    struct TickGuard;
    impl Drop for TickGuard {
        fn drop(&mut self) {
            TICKING.with(|ticking| ticking.set(false));
        }
    }
    let _guard = TickGuard;
    flush_pending_registrations();

    let Some(touched) = take_touched() else {
        return is_active();
    };

    let removed = REGISTRY
        .try_with(|registry| match registry.try_borrow_mut() {
            Ok(mut registry) => {
                let old = std::mem::take(&mut *registry);
                let mut next = Vec::with_capacity(old.len());
                let mut removed = Vec::new();
                for entry in old {
                    if touched.contains(&entry.0) || !owner_list(&entry.0).is_empty() {
                        next.push(entry);
                    } else {
                        removed.push(entry);
                    }
                }
                *registry = next;
                removed
            }
            Err(_) => {
                request_frame();
                Vec::new()
            }
        })
        .unwrap_or_default();
    let removed_any = !removed.is_empty();
    let removed_keys: Vec<String> = removed.iter().map(|(key, _)| key.clone()).collect();
    drop(removed);
    for key in removed_keys {
        let _ = OWNERS.try_with(|owners| {
            if let Ok(mut owners) = owners.try_borrow_mut() {
                owners.remove(&key);
            }
        });
    }

    let entries = REGISTRY
        .try_with(|registry| registry.try_borrow().ok().map(|registry| registry.clone()))
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut results = Vec::with_capacity(entries.len());
    for (key, tick_fn) in entries {
        let still = {
            let callback = tick_fn.try_borrow_mut();
            match callback {
                Ok(mut callback) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    call_tick(&mut *callback)
                }))
                .unwrap_or(true),
                Err(_) => {
                    request_frame();
                    true
                }
            }
        };
        for owner in owner_list(&key) {
            crate::scope_cache::mark_scope_dirty(&owner);
        }
        results.push((key, tick_fn, still));
    }

    let removed_after = REGISTRY
        .try_with(|registry| match registry.try_borrow_mut() {
            Ok(mut registry) => {
                let old = std::mem::take(&mut *registry);
                let mut next = Vec::with_capacity(old.len());
                let mut removed = Vec::new();
                for entry in old {
                    let keep = results
                        .iter()
                        .find(|(key, callback, _)| {
                            key == &entry.0 && Rc::ptr_eq(callback, &entry.1)
                        })
                        .is_none_or(|(_, _, still)| *still);
                    if keep {
                        next.push(entry);
                    } else {
                        removed.push(entry);
                    }
                }
                *registry = next;
                removed
            }
            Err(_) => {
                request_frame();
                Vec::new()
            }
        })
        .unwrap_or_default();
    let removed_after_any = !removed_after.is_empty();
    let removed_keys: Vec<String> = removed_after.iter().map(|(key, _)| key.clone()).collect();
    drop(removed_after);
    for key in removed_keys {
        let _ = OWNERS.try_with(|owners| {
            if let Ok(mut owners) = owners.try_borrow_mut() {
                owners.remove(&key);
            }
        });
        clear_key_state(&key);
    }

    let any_still = results.iter().any(|(_, _, still)| *still);
    if any_still || removed_any || removed_after_any {
        LIVE_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
    }
    if any_still {
        request_frame();
    }
    any_still
}

pub fn is_active() -> bool {
    REGISTRY
        .try_with(|registry| {
            registry
                .try_borrow()
                .map(|registry| !registry.is_empty())
                .unwrap_or(false)
        })
        .ok()
        .unwrap_or(false)
}

pub fn live_epoch() -> u64 {
    LIVE_EPOCH.with(Cell::get)
}

pub fn shutdown() {
    if SHUTTING_DOWN.with(|shutting| shutting.replace(true)) {
        return;
    }
    let registry = REGISTRY
        .try_with(|registry| {
            registry
                .try_borrow_mut()
                .ok()
                .map(|mut registry| std::mem::take(&mut *registry))
        })
        .ok()
        .flatten();
    let owners = OWNERS
        .try_with(|owners| {
            owners
                .try_borrow_mut()
                .ok()
                .map(|mut owners| std::mem::take(&mut *owners))
        })
        .ok()
        .flatten();
    let touched = TOUCHED
        .try_with(|touched| {
            touched
                .try_borrow_mut()
                .ok()
                .map(|mut touched| std::mem::take(&mut *touched))
        })
        .ok()
        .flatten();
    let pending_touches = PENDING_TOUCHES
        .try_with(|pending| {
            pending
                .try_borrow_mut()
                .ok()
                .map(|mut pending| std::mem::take(&mut *pending))
        })
        .ok()
        .flatten();
    let pending_registrations = PENDING_REGISTRATIONS
        .try_with(|pending| {
            pending
                .try_borrow_mut()
                .ok()
                .map(|mut pending| std::mem::take(&mut *pending))
        })
        .ok()
        .flatten();
    drop(registry);
    drop(owners);
    drop(touched);
    drop(pending_touches);
    drop(pending_registrations);
    LIVE_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
    TICKING.with(|ticking| ticking.set(false));
    SHUTTING_DOWN.with(|shutting| shutting.set(false));
}
