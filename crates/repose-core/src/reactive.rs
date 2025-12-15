use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

pub type SignalId = usize;
pub type ObserverId = usize;

thread_local! {
    static CURRENT_OBSERVER: RefCell<Option<ObserverId>> = const { RefCell::new(None) };
    static GRAPH: RefCell<DepGraph> = RefCell::new(DepGraph::default());
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
}

pub fn signal_changed(sig: SignalId) {
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
            // clear previous deps before recompute
            g.remove_all_edges_for(obs);
            drop(g);
            // run under tracking
            CURRENT_OBSERVER.with(|co| {
                let prev = *co.borrow();
                *co.borrow_mut() = Some(obs);
                GRAPH.with(|g2| {
                    if let Some(f) = g2.borrow().observers.get(&obs).cloned() {
                        f();
                    }
                });
                *co.borrow_mut() = prev;
            });
            g = gcell.borrow_mut();
            g.running.remove(&obs);
        }
    });
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
    GRAPH.with(|g| {
        let mut g = g.borrow_mut();
        g.remove_observer(id);
    });
}

pub fn run_observer_now(id: ObserverId) {
    GRAPH.with(|gcell| {
        let mut g = gcell.borrow_mut();
        g.remove_all_edges_for(id);
        drop(g);
        CURRENT_OBSERVER.with(|co| {
            let prev = *co.borrow();
            *co.borrow_mut() = Some(id);
            GRAPH.with(|g2| {
                if let Some(f) = g2.borrow().observers.get(&id).cloned() {
                    f();
                }
            });
            *co.borrow_mut() = prev;
        });
    });
}
