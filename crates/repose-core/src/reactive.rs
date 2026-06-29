use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub type SignalId = usize;
pub type ObserverId = usize;

thread_local! {
    static CURRENT_OBSERVER: RefCell<Option<ObserverId>> = const { RefCell::new(None) };
    static GRAPH: RefCell<DepGraph> = RefCell::new(DepGraph::default());
    static SIGNAL_DEPTH: Cell<u32> = Cell::new(0);
    static PENDING_OBSERVERS: RefCell<VecDeque<ObserverId>> = const { RefCell::new(VecDeque::new()) };
}

#[derive(Default)]
struct DepGraph {
    next_observer: ObserverId,
    // signal_id -> observers that depend on it
    edges: HashMap<SignalId, HashSet<ObserverId>>,
    // observer_id -> signals it depends on
    back: HashMap<ObserverId, HashSet<SignalId>>,
    // recompute closures
    observers: HashMap<ObserverId, Rc<dyn Fn()>>,
    running: HashSet<ObserverId>,
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
        for (_sig, set) in self.edges.iter_mut() {
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

pub fn signal_changed(sig: SignalId) {
    // Mark composition scopes that depend on this signal as dirty
    crate::scope_cache::mark_scope_deps_dirty(sig);

    let is_outer = SIGNAL_DEPTH.with(|depth| {
        let prev = depth.get();
        depth.set(prev + 1);
        if prev > 0 {
            // Re-entrant: defer affected observers for later draining.
            GRAPH.with(|gcell| {
                let g = gcell.borrow();
                if let Some(obs_set) = g.edges.get(&sig) {
                    PENDING_OBSERVERS.with(|q| {
                        let mut queue = q.borrow_mut();
                        for &obs in obs_set {
                            if !g.running.contains(&obs) && !queue.contains(&obs) {
                                queue.push_back(obs);
                            }
                        }
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
                CURRENT_OBSERVER.with(|co| {
                    let prev = *co.borrow();
                    *co.borrow_mut() = Some(obs);
                    f();
                    *co.borrow_mut() = prev;
                });
            }
            g = gcell.borrow_mut();
            g.running.remove(&obs);
        }
    });

    // Drain any observers that were deferred during re-entrant notifications.
    SIGNAL_DEPTH.with(|depth| depth.set(0));
    loop {
        let obs = PENDING_OBSERVERS.with(|q| q.borrow_mut().pop_front());
        match obs {
            None => break,
            Some(obs) => {
                GRAPH.with(|gcell| {
                    let mut g = gcell.borrow_mut();
                    if g.running.contains(&obs) {
                        return;
                    }
                    g.running.insert(obs);
                    g.remove_all_edges_for(obs);
                    let f = g.observers.get(&obs).cloned();
                    drop(g);
                    if let Some(f) = f {
                        CURRENT_OBSERVER.with(|co| {
                            let prev = *co.borrow();
                            *co.borrow_mut() = Some(obs);
                            f();
                            *co.borrow_mut() = prev;
                        });
                    }
                    g = gcell.borrow_mut();
                    g.running.remove(&obs);
                });
            }
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
        let mut g = gcell.borrow_mut();
        if !g.running.insert(id) {
            return None;
        }
        g.remove_all_edges_for(id);
        let f = g.observers.get(&id).cloned();
        drop(g);
        if let Some(f) = f {
            CURRENT_OBSERVER.with(|co| {
                let prev = *co.borrow();
                *co.borrow_mut() = Some(id);
                f();
                *co.borrow_mut() = prev;
            });
        }
        GRAPH.with(|gcell| {
            let mut g = gcell.borrow_mut();
            g.running.remove(&id);
        });
        Some(())
    });
    let _ = f;
}
