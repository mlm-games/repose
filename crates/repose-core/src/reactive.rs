use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use rustc_hash::{FxHashMap, FxHashSet};

pub type SignalId = usize;
pub type ObserverId = usize;

thread_local! {
    static CURRENT_OBSERVER: RefCell<Option<ObserverId>> = const { RefCell::new(None) };
    static GRAPH: RefCell<DepGraph> = RefCell::new(DepGraph::default());
    static SIGNAL_DEPTH: Cell<u32> = const { Cell::new(0) };
    static PENDING_OBSERVERS: RefCell<VecDeque<ObserverId>> = const { RefCell::new(VecDeque::new()) };
    static PENDING_SET: RefCell<FxHashSet<ObserverId>> = RefCell::new(FxHashSet::default());
}

#[derive(Default)]
struct DepGraph {
    next_observer: ObserverId,
    // signal_id -> observers that depend on it
    edges: FxHashMap<SignalId, FxHashSet<ObserverId>>,
    // observer_id -> signals it depends on
    back: FxHashMap<ObserverId, FxHashSet<SignalId>>,
    // recompute closures
    observers: FxHashMap<ObserverId, Rc<dyn Fn()>>,
    running: FxHashSet<ObserverId>,
}

impl DepGraph {
    fn remove_all_edges_for(&mut self, obs: ObserverId) {
        if let Some(signals) = self.back.remove(&obs) {
            for s in signals {
                if let Some(set) = self.edges.get_mut(&s) {
                    set.remove(&obs);
                }
            }
        }
    }
    fn remove_observer(&mut self, obs: ObserverId) {
        self.observers.remove(&obs);
        self.remove_all_edges_for(obs);
        // scrub forward maps just in case
        for set in self.edges.values_mut() {
            set.remove(&obs);
        }
        self.running.remove(&obs);
    }
}

pub fn register_signal_read(sig: SignalId) {
    CURRENT_OBSERVER.with(|co| {
        if let Some(obs) = *co.borrow() {
            GRAPH.with(|g| {
                let mut g = g.borrow_mut();
                g.edges.entry(sig).or_default().insert(obs);
                g.back.entry(obs).or_default().insert(sig);
            });
        }
    });
    // track also against the current composition scope (if in a `scope!` body)
    crate::scope_cache::record_scope_signal_dep(sig);
}

fn run_observer_guarded(obs: ObserverId, f: Rc<dyn Fn()>) {
    struct ObserverGuard {
        obs: ObserverId,
        prev: Option<ObserverId>,
    }
    impl Drop for ObserverGuard {
        fn drop(&mut self) {
            CURRENT_OBSERVER.with(|co| *co.borrow_mut() = self.prev);
            GRAPH.with(|gcell| {
                if let Ok(mut g) = gcell.try_borrow_mut() {
                    g.running.remove(&self.obs);
                }
            });
        }
    }

    let prev = CURRENT_OBSERVER.with(|co| {
        let prev = *co.borrow();
        *co.borrow_mut() = Some(obs);
        prev
    });
    let _guard = ObserverGuard { obs, prev };

    // Catch unwind so one failing observer does not kill the graph.
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f()));
    if let Err(payload) = res {
        log::error!("reactive observer {obs} panicked");
        if !cfg!(target_arch = "wasm32") {
            let msg = payload
                .downcast_ref::<String>()
                .map(|s| s.as_str())
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown panic payload");
            log::error!("observer panic payload: {msg}");
        }
    }
    // guard drops here, restoring CURRENT_OBSERVER and running
}

pub fn signal_changed(sig: SignalId) {
    // Mark composition scopes that depend on this signal as dirty
    crate::scope_cache::mark_scope_deps_dirty(sig);

    let is_outer = SIGNAL_DEPTH.with(|depth| {
        let prev = depth.get();
        depth.set(prev + 1);
        if prev > 0 {
            // Re-entrant: defer affected observers for later draining (O(1) dedup).
            GRAPH.with(|gcell| {
                let g = gcell.borrow();
                if let Some(obs_set) = g.edges.get(&sig) {
                    PENDING_SET.with(|set_cell| {
                        PENDING_OBSERVERS.with(|q| {
                            let mut set = set_cell.borrow_mut();
                            let mut queue = q.borrow_mut();
                            for &obs in obs_set {
                                if !g.running.contains(&obs) && set.insert(obs) {
                                    queue.push_back(obs);
                                }
                            }
                        });
                    });
                }
            });
            false
        } else {
            true
        }
    });

    if !is_outer {
        SIGNAL_DEPTH.with(|d| d.set(d.get() - 1));
        return;
    }

    struct DepthGuard;
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            SIGNAL_DEPTH.with(|depth| {
                if depth.get() != 0 {
                    depth.set(0);
                }
            });
        }
    }
    let _depth_guard = DepthGuard;

    GRAPH.with(|gcell| {
        let mut g = gcell.borrow_mut();
        let mut queue: VecDeque<ObserverId> = g
            .edges
            .get(&sig)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();
        while let Some(obs) = queue.pop_front() {
            if g.running.contains(&obs) {
                continue;
            }
            g.running.insert(obs);
            g.remove_all_edges_for(obs);
            let f = g.observers.get(&obs).cloned();
            drop(g);
            if let Some(f) = f {
                run_observer_guarded(obs, f);
            }
            match gcell.try_borrow_mut() {
                Ok(mut new_g) => {
                    new_g.running.remove(&obs);
                    g = new_g;
                }
                Err(_) => {
                    log::error!("GRAPH poisoned after observer {obs} panic — resetting");
                    *gcell.borrow_mut() = DepGraph::default();
                    break;
                }
            }
        }
    });

    // Drain any observers that were deferred during re-entrant notifications.
    SIGNAL_DEPTH.with(|depth| depth.set(0));
    loop {
        let obs = PENDING_OBSERVERS.with(|q| q.borrow_mut().pop_front());
        let Some(obs) = obs else { break };
        PENDING_SET.with(|s| {
            s.borrow_mut().remove(&obs);
        });
        let should_run = GRAPH.with(|gcell| {
            if let Ok(mut g) = gcell.try_borrow_mut() {
                if g.running.contains(&obs) {
                    return false;
                }
                g.running.insert(obs);
                g.remove_all_edges_for(obs);
                true
            } else {
                false
            }
        });
        if !should_run {
            continue;
        }
        let f = GRAPH.with(|gcell| gcell.borrow().observers.get(&obs).cloned());
        if let Some(f) = f {
            run_observer_guarded(obs, f);
        }
    }
}

pub fn new_observer(f: impl Fn() + 'static) -> ObserverId {
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        let id = g.next_observer;
        g.next_observer += 1;
        g.observers.insert(id, Rc::new(f));
        id
    })
}

/// Remove an observer and all of its dependency edges.
pub fn remove_observer(id: ObserverId) {
    let _ = GRAPH.try_with(|g| {
        let mut g = g.borrow_mut();
        g.remove_observer(id);
    });
}

/// Run a closure with `CURRENT_OBSERVER` cleared
pub fn without_observer<R>(f: impl FnOnce() -> R) -> R {
    CURRENT_OBSERVER.with(|co| {
        let prev = *co.borrow();
        *co.borrow_mut() = None;
        let result = f();
        *co.borrow_mut() = prev;
        result
    })
}

pub fn run_observer_now(id: ObserverId) {
    let f = GRAPH.with(|gcell| {
        let mut g = match gcell.try_borrow_mut() {
            Ok(g) => g,
            Err(_) => return None,
        };
        if !g.running.insert(id) {
            return None;
        }
        g.remove_all_edges_for(id);
        let f = g.observers.get(&id).cloned();
        drop(g);
        if let Some(f) = f.clone() {
            run_observer_guarded(id, f);
        }
        GRAPH.with(|gcell| {
            if let Ok(mut g) = gcell.try_borrow_mut() {
                g.running.remove(&id);
            }
        });
        Some(())
    });
    let _ = f;
}
