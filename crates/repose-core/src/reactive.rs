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
    /// Observers whose `running` flag could not be cleared because the graph
    /// was borrowed (`try_borrow_mut` failed in a guard `Drop`). Retried on
    /// the next drain instead of pinning the observer forever.
    static PENDING_RUNNING_CLEANUP: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
    /// Observer removals deferred for the same reason. Retried on next drain.
    static PENDING_REMOVALS: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
    /// `CURRENT_OBSERVER` restores deferred for the same reason. Each entry is
    /// `(finished_observer, prev_value)`; applied only if the cell still holds
    /// the stale `finished_observer`.
    static PENDING_OBSERVER_RESTORE: RefCell<Vec<(ObserverId, Option<ObserverId>)>> =
        const { RefCell::new(Vec::new()) };
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
            CURRENT_OBSERVER.with(|co| {
                match co.try_borrow_mut() {
                    Ok(mut b) => {
                        if *b == Some(self.obs) {
                            *b = self.prev;
                        }
                    }
                    Err(_) => {
                        PENDING_OBSERVER_RESTORE.with(|q| {
                            if let Ok(mut q) = q.try_borrow_mut() {
                                q.push((self.obs, self.prev));
                            } else {
                                log::error!(
                                    "reactive: CURRENT_OBSERVER and restore queue busy while finishing observer {}; stale observer reference retained",
                                    self.obs
                                );
                            }
                        });
                    }
                }
            });
            GRAPH.with(|gcell| {
                match gcell.try_borrow_mut() {
                    Ok(mut g) => {
                        g.running.remove(&self.obs);
                    }
                    Err(_) => {
                        PENDING_RUNNING_CLEANUP.with(|q| {
                            if let Ok(mut q) = q.try_borrow_mut() {
                                if !q.contains(&self.obs) {
                                    q.push(self.obs);
                                }
                            } else {
                                log::error!(
                                    "reactive: dependency graph and cleanup queue busy while finishing observer {}; running flag retained",
                                    self.obs
                                );
                            }
                        });
                    }
                }
            });
        }
    }

    let prev = CURRENT_OBSERVER.with(|co| {
        let prev = co.try_borrow().map(|b| *b).unwrap_or(None);
        if let Ok(mut b) = co.try_borrow_mut() {
            *b = Some(obs);
        } else {
            log::error!(
                "reactive: CURRENT_OBSERVER busy while starting observer {obs}; dependency attribution may be incomplete"
            );
        }
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

    if in_batch() {
        enqueue_affected(sig);
        return;
    }

    enqueue_affected(sig);

    if SIGNAL_DEPTH.with(|d| d.get()) != 0 {
        return;
    }

    drain_pending();
}

/// Explicit batching scope: notifications inside `f` are coalesced and
/// drained once with dedup when `f` returns (long-term fix for redundant +
/// torn recomputes on multi-signal updates, e.g. `A.set(); B.set();` inside
/// `batch` recomputes a shared observer once on the final state).
/// Nested `batch` calls collapse into the outermost drain. Panic-safe: the
/// drain still runs on unwind.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    struct BatchGuard {
        outer: bool,
    }
    impl Drop for BatchGuard {
        fn drop(&mut self) {
            if self.outer {
                BATCH_ACTIVE.with(|b| b.set(false));
                if SIGNAL_DEPTH.with(|d| d.get()) == 0 {
                    drain_pending();
                }
            }
        }
    }

    let outer = !in_batch();
    if outer {
        BATCH_ACTIVE.with(|b| b.set(true));
    }
    let _guard = BatchGuard { outer };
    f()
}

fn in_batch() -> bool {
    BATCH_ACTIVE.with(|b| b.get())
}

thread_local! {
    static BATCH_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

fn enqueue_affected(sig: SignalId) {
    GRAPH.with(|gcell| {
        let g = match gcell.try_borrow() {
            Ok(g) => g,
            Err(_) => {
                log::error!(
                    "reactive: dependency graph busy while enqueueing signal {sig}; notification deferred to next drain"
                );
                return;
            }
        };
        if let Some(obs_set) = g.edges.get(&sig) {
            PENDING_SET.with(|set_cell| {
                PENDING_OBSERVERS.with(|q| {
                    match (set_cell.try_borrow_mut(), q.try_borrow_mut()) {
                        (Ok(mut set), Ok(mut queue)) => {
                            for &obs in obs_set {
                                if !g.running.contains(&obs) && set.insert(obs) {
                                    queue.push_back(obs);
                                }
                            }
                        }
                        _ => {
                            log::error!(
                                "reactive: pending queue busy while enqueueing signal {sig}"
                            );
                        }
                    }
                });
            });
        }
    });
}

/// Best-effort retry of housekeeping deferred by earlier `try_borrow_mut`
/// contention. Runs at the head of every drain so a transient contention
/// never permanently pins an observer (`running`), leaks a removal, or
/// leaves a stale `CURRENT_OBSERVER`.
fn retry_deferred_housekeeping() {
    PENDING_OBSERVER_RESTORE.with(|q| {
        if let Ok(mut q) = q.try_borrow_mut() {
            let mut i = 0;
            while i < q.len() {
                let (finished, prev) = q[i];
                let applied = CURRENT_OBSERVER.with(|co| {
                    if let Ok(mut b) = co.try_borrow_mut() {
                        if *b == Some(finished) {
                            *b = prev;
                            true
                        } else if *b == prev {
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                });
                if applied {
                    q.swap_remove(i);
                } else {
                    i += 1;
                }
            }
        }
    });
    PENDING_RUNNING_CLEANUP.with(|q| {
        if let Ok(pending) = q.try_borrow_mut().map(|mut q| std::mem::take(&mut *q)) {
            GRAPH.with(|gcell| {
                if let Ok(mut g) = gcell.try_borrow_mut() {
                    for obs in pending {
                        g.running.remove(&obs);
                    }
                } else if let Ok(mut q) = q.try_borrow_mut() {
                    q.extend(pending);
                }
            });
        }
    });
    PENDING_REMOVALS.with(|q| {
        if let Ok(pending) = q.try_borrow_mut().map(|mut q| std::mem::take(&mut *q)) {
            GRAPH.with(|gcell| {
                if let Ok(mut g) = gcell.try_borrow_mut() {
                    for obs in pending {
                        g.remove_observer(obs);
                    }
                } else if let Ok(mut q) = q.try_borrow_mut() {
                    q.extend(pending);
                }
            });
        }
    });
}

/// Clear an observer's `running` flag, deferring on contention (with retry
/// via [`retry_deferred_housekeeping`]) instead of pinning it forever.
fn clear_running(obs: ObserverId) {
    GRAPH.with(|gcell| match gcell.try_borrow_mut() {
        Ok(mut g) => {
            g.running.remove(&obs);
        }
        Err(_) => {
            PENDING_RUNNING_CLEANUP.with(|q| {
                if let Ok(mut q) = q.try_borrow_mut() {
                    if !q.contains(&obs) {
                        q.push(obs);
                    }
                } else {
                    log::error!(
                        "reactive: dependency graph and cleanup queue busy while clearing observer {obs}; running flag retained"
                    );
                }
            });
        }
    });
}

/// Drain the coalesced pending queue until empty. Runs observers outside the
/// graph borrow; re-entrant `signal_changed` calls during an observer simply
/// enqueue and are picked up by this loop.
fn drain_pending() {
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
    SIGNAL_DEPTH.with(|d| d.set(d.get() + 1));
    let _depth_guard = DepthGuard;

    loop {
        retry_deferred_housekeeping();
        let obs =
            PENDING_OBSERVERS.with(|q| q.try_borrow_mut().ok().and_then(|mut q| q.pop_front()));
        let Some(obs) = obs else { break };
        let still_queued = PENDING_SET.with(|s| {
            s.try_borrow_mut()
                .map(|mut s| s.remove(&obs))
                .unwrap_or(true)
        });
        if !still_queued {
            continue;
        }
        let f = GRAPH.with(|gcell| match gcell.try_borrow_mut() {
            Ok(mut g) => {
                if g.running.contains(&obs) {
                    None
                } else {
                    g.running.insert(obs);
                    g.remove_all_edges_for(obs);
                    g.observers.get(&obs).cloned()
                }
            }
            Err(_) => None,
        });
        let Some(f) = f else {
            PENDING_SET.with(|s| {
                PENDING_OBSERVERS.with(|q| {
                    if let (Ok(mut s), Ok(mut q)) =
                        (s.try_borrow_mut(), q.try_borrow_mut())
                    {
                        if s.insert(obs) {
                            q.push_front(obs);
                        }
                    } else {
                        log::error!(
                            "reactive: graph and pending queue busy; deferred observer {obs} retained in set for next drain"
                        );
                    }
                });
            });
            break;
        };
        run_observer_guarded(obs, f);
        clear_running(obs);
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

/// Remove an observer and all of its dependency edges. Idempotent: removing
/// an unknown or already-removed id is a no-op. Uses `try_borrow_mut` so
/// disposal from `Drop` (e.g. `ProduceHandle`, scope teardown) can never
/// panic while the graph is borrowed; on contention the removal is deferred
/// and retried by the next drain instead of silently leaking the observer.
pub fn remove_observer(id: ObserverId) {
    let _ = GRAPH.try_with(|g| match g.try_borrow_mut() {
        Ok(mut g) => g.remove_observer(id),
        Err(e) => {
            log::error!(
                "reactive: dependency graph busy while removing observer {id}, deferring removal: {e}"
            );
            PENDING_REMOVALS.with(|q| {
                if let Ok(mut q) = q.try_borrow_mut()
                    && !q.contains(&id)
                {
                    q.push(id);
                }
            });
        }
    });
}

/// Run a closure with `CURRENT_OBSERVER` cleared (panic-safe restore).
/// Re-entrancy safe: if the cell is already borrowed elsewhere, the closure
/// still runs (untracked) and the restore is skipped because there is no
/// locally-held previous value to corrupt.
pub fn without_observer<R>(f: impl FnOnce() -> R) -> R {
    struct Guard {
        prev: Option<Option<ObserverId>>,
    }
    impl Drop for Guard {
        fn drop(&mut self) {
            if let Some(prev) = self.prev.take() {
                CURRENT_OBSERVER.with(|co| {
                    match co.try_borrow_mut() {
                        Ok(mut b) => {
                            if b.is_none() {
                                *b = prev;
                            }
                        }
                        Err(_) => {
                            log::error!(
                                "reactive: CURRENT_OBSERVER busy after without_observer block; stale observer reference retained"
                            );
                        }
                    }
                });
            }
        }
    }
    let prev = CURRENT_OBSERVER.with(|co| {
        co.try_borrow().ok().map(|b| *b).and_then(|prev| {
            co.try_borrow_mut()
                .map(|mut b| {
                    *b = None;
                    prev
                })
                .ok()
        })
    });
    let _guard = Guard { prev };
    f()
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
            } else {
                log::error!(
                    "reactive: dependency graph busy after running observer {id}; running flag retained"
                );
            }
        });
        Some(())
    });
    let _ = f;
}
