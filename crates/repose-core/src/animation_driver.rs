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
    static REGISTRY: RefCell<FxHashMap<String, TickFn>> =
        RefCell::new(FxHashMap::default());
    static OWNERS: RefCell<FxHashMap<String, FxHashSet<String>>> =
        RefCell::new(FxHashMap::default());
    static TOUCHED: RefCell<FxHashSet<String>> = RefCell::new(FxHashSet::default());
    static PENDING_TOUCHES: RefCell<FxHashSet<String>> =
        RefCell::new(FxHashSet::default());
    static PENDING_REGISTRATIONS: RefCell<Vec<(String, TickFn, String)>> =
        const { RefCell::new(Vec::new()) };
    static REGISTRATION_GENERATIONS: RefCell<FxHashMap<String, u64>> =
        RefCell::new(FxHashMap::default());
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

fn has_owner(key: &str) -> bool {
    OWNERS
        .try_with(|owners| {
            owners
                .try_borrow()
                .ok()
                .map(|owners| owners.get(key).is_some_and(|set| !set.is_empty()))
        })
        .ok()
        .flatten()
        .unwrap_or(false)
}

fn for_each_owner(key: &str, mut f: impl FnMut(&str)) {
    let _ = OWNERS.try_with(|owners| {
        if let Ok(owners) = owners.try_borrow()
            && let Some(set) = owners.get(key)
        {
            for owner in set {
                f(owner);
            }
        }
    });
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
    let _ = REGISTRATION_GENERATIONS.try_with(|generations| {
        if let Ok(mut generations) = generations.try_borrow_mut() {
            generations.remove(key);
        }
    });
}

fn remove_empty_registration(key: &str) {
    let removed = REGISTRY
        .try_with(|registry| {
            registry
                .try_borrow_mut()
                .ok()
                .and_then(|mut registry| registry.remove(key))
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
    let (old, changed, same) = REGISTRY
        .try_with(|registry| match registry.try_borrow_mut() {
            Ok(mut registry) => {
                let same = registry
                    .get(&key)
                    .is_some_and(|registered| Rc::ptr_eq(registered, &tick));
                bump_registration_generation(&key);
                if same {
                    (None, false, true)
                } else {
                    let old = registry.insert(key.clone(), tick.clone());
                    (old, true, false)
                }
            }
            Err(_) => {
                queue_registration(key.clone(), tick.clone(), owner.clone());
                (None, false, false)
            }
        })
        .unwrap_or((None, false, false));
    drop(old);
    if changed || is_registered(&key) {
        add_owner(&key, &owner);
    }
    let re_registered = same && TICKING.with(Cell::get);
    if let Some(scope) = current {
        crate::scope_cache::record_scope_animation_key(&key);
        if changed || re_registered {
            crate::scope_cache::mark_scope_dirty(&scope);
        }
    }
    if changed || re_registered {
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
        match owners.len() {
            1 => release_owner(key, &owners[0]),
            0 => remove_empty_registration(key),
            _ => {}
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
                .map(|registry| registry.contains_key(key))
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

fn bump_registration_generation(key: &str) -> u64 {
    REGISTRATION_GENERATIONS.with(|generations| {
        let mut generations = generations.borrow_mut();
        let generation = generations.entry(key.to_string()).or_insert(0);
        *generation = generation.wrapping_add(1);
        *generation
    })
}

fn registration_generation(key: &str) -> Option<u64> {
    REGISTRATION_GENERATIONS.with(|generations| generations.borrow().get(key).copied())
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
                let mut next = FxHashMap::default();
                let mut removed = Vec::new();
                for (key, tick) in old {
                    if touched.contains(&key) || has_owner(&key) {
                        next.insert(key, tick);
                    } else {
                        removed.push(key);
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
    for key in removed {
        let _ = OWNERS.try_with(|owners| {
            if let Ok(mut owners) = owners.try_borrow_mut() {
                owners.remove(&key);
            }
        });
        clear_key_state(&key);
    }

    let entries = REGISTRY
        .try_with(|registry| {
            registry.try_borrow().ok().map(|registry| {
                registry
                    .iter()
                    .map(|(key, tick)| (key.clone(), tick.clone(), registration_generation(key)))
                    .collect::<Vec<_>>()
            })
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    let mut results = Vec::with_capacity(entries.len());
    for (key, tick_fn, generation) in entries {
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
        for_each_owner(&key, |owner| crate::scope_cache::mark_scope_dirty(owner));
        results.push((key, tick_fn, still, generation));
    }

    let any_still = results.iter().any(|(_, _, still, _)| *still);
    let removed_after = REGISTRY
        .try_with(|registry| match registry.try_borrow_mut() {
            Ok(mut registry) => {
                let mut removed = Vec::new();
                for (key, callback, still, generation) in &results {
                    if *still {
                        continue;
                    }
                    let matches_snapshot = registry
                        .get(key)
                        .is_some_and(|registered| Rc::ptr_eq(registered, callback))
                        && registration_generation(key) == *generation;
                    if matches_snapshot && let Some(removed_callback) = registry.remove(key) {
                        removed.push((key.clone(), removed_callback));
                    }
                }
                removed
            }
            Err(_) => {
                request_frame();
                Vec::new()
            }
        })
        .unwrap_or_default();
    let removed_after_any = !removed_after.is_empty();
    for (key, _) in &removed_after {
        let _ = OWNERS.try_with(|owners| {
            if let Ok(mut owners) = owners.try_borrow_mut() {
                owners.remove(key);
            }
        });
        clear_key_state(key);
    }
    drop(removed_after);

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
    let registration_generations = REGISTRATION_GENERATIONS
        .try_with(|generations| {
            generations
                .try_borrow_mut()
                .ok()
                .map(|mut generations| std::mem::take(&mut *generations))
        })
        .ok()
        .flatten();
    drop(registry);
    drop(owners);
    drop(touched);
    drop(pending_touches);
    drop(pending_registrations);
    drop(registration_generations);
    LIVE_EPOCH.with(|epoch| epoch.set(epoch.get().wrapping_add(1)));
    TICKING.with(|ticking| ticking.set(false));
    SHUTTING_DOWN.with(|shutting| shutting.set(false));
}
