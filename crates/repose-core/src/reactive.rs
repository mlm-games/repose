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
    static BATCH_ACTIVE: Cell<bool> = const { Cell::new(false) };
    static RETRY_NEEDED: Cell<bool> = const { Cell::new(false) };
    static PENDING: RefCell<Pending> = RefCell::new(Pending::default());
    static DEFERRED_SIGNALS: RefCell<Vec<SignalId>> = const { RefCell::new(Vec::new()) };
    static DEFERRED_OBSERVERS: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
    static DEFERRED_REMOVALS: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
    static DEFERRED_RUNNING: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
    static DEFERRED_RESTORE: RefCell<Vec<(ObserverId, Option<ObserverId>)>> =
        const { RefCell::new(Vec::new()) };
    static RUN_NOW: RefCell<VecDeque<ObserverId>> = const { RefCell::new(VecDeque::new()) };
    static DEFERRED_SELF_RETRIES: RefCell<Vec<ObserverId>> = const { RefCell::new(Vec::new()) };
}

#[derive(Default)]
struct Pending {
    queue: VecDeque<ObserverId>,
    set: FxHashSet<ObserverId>,
}

#[derive(Default)]
struct DepGraph {
    next_observer: ObserverId,
    edges: FxHashMap<SignalId, FxHashSet<ObserverId>>,
    back: FxHashMap<ObserverId, FxHashSet<SignalId>>,
    observers: FxHashMap<ObserverId, Rc<dyn Fn()>>,
    running: FxHashSet<ObserverId>,
}

impl DepGraph {
    fn remove_all_edges_for(&mut self, observer: ObserverId) {
        if let Some(signals) = self.back.remove(&observer) {
            for signal in signals {
                if let Some(observers) = self.edges.get_mut(&signal) {
                    observers.remove(&observer);
                    if observers.is_empty() {
                        self.edges.remove(&signal);
                    }
                }
            }
        }
    }

    fn remove_observer(&mut self, observer: ObserverId) -> Option<Rc<dyn Fn()>> {
        let callback = self.observers.remove(&observer);
        self.remove_all_edges_for(observer);
        self.running.remove(&observer);
        for observers in self.edges.values_mut() {
            observers.remove(&observer);
        }
        self.edges.retain(|_, observers| !observers.is_empty());
        callback
    }

    fn prepare(&mut self, observer: ObserverId) -> Option<Option<Rc<dyn Fn()>>> {
        if !self.observers.contains_key(&observer) {
            return Some(None);
        }
        if !self.running.insert(observer) {
            return None;
        }
        self.remove_all_edges_for(observer);
        Some(self.observers.get(&observer).cloned())
    }
}

fn schedule_retry() {
    RETRY_NEEDED.with(|retry| retry.set(true));
    crate::request_frame();
}

fn enqueue_observer(observer: ObserverId) -> bool {
    let queued = PENDING.try_with(|pending| match pending.try_borrow_mut() {
        Ok(mut pending) => {
            if pending.set.insert(observer) {
                pending.queue.push_back(observer);
            }
            true
        }
        Err(_) => false,
    });
    if matches!(queued, Ok(true)) {
        return true;
    }
    let deferred = DEFERRED_OBSERVERS.try_with(|deferred| {
        if let Ok(mut deferred) = deferred.try_borrow_mut() {
            deferred.push(observer);
            true
        } else {
            false
        }
    });
    if !matches!(deferred, Ok(true)) {
        schedule_retry();
    }
    schedule_retry();
    false
}

fn requeue_observer(observer: ObserverId, front: bool) -> bool {
    let queued = PENDING.try_with(|pending| match pending.try_borrow_mut() {
        Ok(mut pending) => {
            if pending.set.insert(observer) {
                if front {
                    pending.queue.push_front(observer);
                } else {
                    pending.queue.push_back(observer);
                }
                true
            } else {
                false
            }
        }
        Err(_) => false,
    });
    if matches!(queued, Ok(true)) {
        return true;
    }
    let deferred = DEFERRED_OBSERVERS.try_with(|deferred| {
        if let Ok(mut deferred) = deferred.try_borrow_mut() {
            deferred.push(observer);
            true
        } else {
            false
        }
    });
    if !matches!(deferred, Ok(true)) {
        schedule_retry();
    }
    schedule_retry();
    false
}

fn remove_pending_observer(observer: ObserverId) {
    let _ = PENDING.try_with(|pending| {
        if let Ok(mut pending) = pending.try_borrow_mut() {
            pending.set.remove(&observer);
            pending.queue.retain(|queued| *queued != observer);
        } else {
            schedule_retry();
        }
    });
    let _ = DEFERRED_OBSERVERS.try_with(|deferred| {
        if let Ok(mut deferred) = deferred.try_borrow_mut() {
            deferred.retain(|queued| *queued != observer);
        }
    });
    let _ = DEFERRED_SELF_RETRIES.try_with(|deferred| {
        if let Ok(mut deferred) = deferred.try_borrow_mut() {
            deferred.retain(|queued| *queued != observer);
        }
    });
    let _ = RUN_NOW.try_with(|queue| {
        if let Ok(mut queue) = queue.try_borrow_mut() {
            queue.retain(|queued| *queued != observer);
        }
    });
    let _ = DEFERRED_RESTORE.try_with(|restore| {
        if let Ok(mut restore) = restore.try_borrow_mut() {
            restore.retain(|(finished, _)| *finished != observer);
        }
    });
}

fn defer_self_retry(observer: ObserverId) {
    let queued = DEFERRED_SELF_RETRIES.try_with(|deferred| {
        if let Ok(mut deferred) = deferred.try_borrow_mut() {
            if !deferred.contains(&observer) {
                deferred.push(observer);
            }
            true
        } else {
            false
        }
    });
    if !matches!(queued, Ok(true)) {
        schedule_retry();
    }
    schedule_retry();
}

fn enqueue_affected(signal: SignalId) {
    let snapshot = GRAPH
        .try_with(|graph| match graph.try_borrow() {
            Ok(graph) => Some((
                graph
                    .edges
                    .get(&signal)
                    .map(|set| set.iter().copied().collect::<Vec<_>>())
                    .unwrap_or_default(),
                graph.running.clone(),
            )),
            Err(_) => None,
        })
        .ok()
        .flatten();
    let Some((observers, running)) = snapshot else {
        let queued = DEFERRED_SIGNALS.try_with(|deferred| {
            if let Ok(mut deferred) = deferred.try_borrow_mut() {
                deferred.push(signal);
                true
            } else {
                false
            }
        });
        if !matches!(queued, Ok(true)) {
            schedule_retry();
        }
        return;
    };

    for observer in observers {
        if running.contains(&observer) {
            defer_self_retry(observer);
        } else if !enqueue_observer(observer) {
            schedule_retry();
            break;
        }
    }
}

pub fn register_signal_read(signal: SignalId) {
    let observer = CURRENT_OBSERVER
        .try_with(|current| current.try_borrow().ok().and_then(|value| *value))
        .unwrap_or(None);
    if let Some(observer) = observer {
        let _ = GRAPH.try_with(|graph| match graph.try_borrow_mut() {
            Ok(mut graph) => {
                if graph.observers.contains_key(&observer) {
                    graph.edges.entry(signal).or_default().insert(observer);
                    graph.back.entry(observer).or_default().insert(signal);
                }
            }
            Err(_) => schedule_retry(),
        });
    }
    crate::scope_cache::record_scope_signal_dep(signal);
}

fn restore_current_observer(observer: ObserverId, previous: Option<ObserverId>) {
    let applied = CURRENT_OBSERVER
        .try_with(|current| match current.try_borrow_mut() {
            Ok(mut current) => {
                if *current == Some(observer) {
                    *current = previous;
                    true
                } else {
                    *current == previous
                }
            }
            Err(_) => false,
        })
        .unwrap_or(false);
    if !applied {
        let queued = DEFERRED_RESTORE.try_with(|restore| {
            if let Ok(mut restore) = restore.try_borrow_mut() {
                restore.push((observer, previous));
                true
            } else {
                false
            }
        });
        if !matches!(queued, Ok(true)) {
            schedule_retry();
        }
    }
}

fn clear_running(observer: ObserverId) {
    let cleared = GRAPH
        .try_with(|graph| match graph.try_borrow_mut() {
            Ok(mut graph) => {
                graph.running.remove(&observer);
                true
            }
            Err(_) => false,
        })
        .unwrap_or(false);
    if !cleared {
        let queued = DEFERRED_RUNNING.try_with(|deferred| {
            if let Ok(mut deferred) = deferred.try_borrow_mut() {
                if !deferred.contains(&observer) {
                    deferred.push(observer);
                }
                true
            } else {
                false
            }
        });
        if !matches!(queued, Ok(true)) {
            schedule_retry();
        }
    }
}

fn run_observer_guarded(
    observer: ObserverId,
    function: Rc<dyn Fn()>,
) -> Result<(), Box<dyn std::any::Any + Send>> {
    struct ObserverGuard {
        observer: ObserverId,
        previous: Option<ObserverId>,
    }

    impl Drop for ObserverGuard {
        fn drop(&mut self) {
            restore_current_observer(self.observer, self.previous);
            clear_running(self.observer);
        }
    }

    let previous = CURRENT_OBSERVER
        .try_with(|current| match current.try_borrow_mut() {
            Ok(mut current) => {
                let previous = *current;
                *current = Some(observer);
                previous
            }
            Err(_) => {
                schedule_retry();
                None
            }
        })
        .unwrap_or(None);
    let _guard = ObserverGuard { observer, previous };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| function()));
    match result {
        Ok(()) => Ok(()),
        Err(payload) => {
            log::error!("reactive observer {observer} panicked");
            if !cfg!(target_arch = "wasm32") {
                let message = payload
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| payload.downcast_ref::<&str>().copied())
                    .unwrap_or("unknown panic payload");
                log::error!("observer panic payload: {message}");
            }
            Err(payload)
        }
    }
}

pub fn signal_changed(signal: SignalId) {
    crate::scope_cache::mark_current_scope_dirty_for_signal(signal);
    crate::scope_cache::mark_scope_deps_dirty(signal);

    if in_batch() {
        enqueue_affected(signal);
        return;
    }

    enqueue_affected(signal);
    if SIGNAL_DEPTH.with(Cell::get) == 0 {
        drain_pending();
    }
}

pub fn batch<R>(function: impl FnOnce() -> R) -> R {
    struct BatchGuard {
        outer: bool,
    }

    impl Drop for BatchGuard {
        fn drop(&mut self) {
            if self.outer {
                BATCH_ACTIVE.with(|active| active.set(false));
                if SIGNAL_DEPTH.with(Cell::get) == 0 {
                    drain_pending();
                }
            }
        }
    }

    let outer = !in_batch();
    if outer {
        BATCH_ACTIVE.with(|active| active.set(true));
    }
    let _guard = BatchGuard { outer };
    function()
}

fn in_batch() -> bool {
    BATCH_ACTIVE.with(Cell::get)
}

fn retry_deferred_housekeeping() {
    let restores = match DEFERRED_RESTORE.try_with(|queue| match queue.try_borrow_mut() {
        Ok(mut queue) => Some(std::mem::take(&mut *queue)),
        Err(_) => None,
    }) {
        Ok(Some(restores)) => restores,
        Ok(None) => {
            schedule_retry();
            Vec::new()
        }
        Err(_) => {
            schedule_retry();
            Vec::new()
        }
    };
    if !restores.is_empty() {
        let applied = CURRENT_OBSERVER
            .try_with(|current| match current.try_borrow_mut() {
                Ok(mut current) => {
                    let mut result = Vec::new();
                    for (observer, previous) in restores {
                        if *current == Some(observer) {
                            *current = previous;
                        } else if *current != previous {
                            result.push((observer, previous));
                        }
                    }
                    result
                }
                Err(_) => restores,
            })
            .unwrap_or_default();
        if !applied.is_empty() {
            schedule_retry();
            let _ = DEFERRED_RESTORE.try_with(|queue| {
                if let Ok(mut queue) = queue.try_borrow_mut() {
                    queue.extend(applied);
                }
            });
        }
    }

    let running = match DEFERRED_RUNNING.try_with(|queue| match queue.try_borrow_mut() {
        Ok(mut queue) => Some(std::mem::take(&mut *queue)),
        Err(_) => None,
    }) {
        Ok(Some(running)) => running,
        Ok(None) | Err(_) => {
            schedule_retry();
            Vec::new()
        }
    };
    if !running.is_empty() {
        let applied = GRAPH
            .try_with(|graph| match graph.try_borrow_mut() {
                Ok(mut graph) => {
                    for observer in &running {
                        graph.running.remove(observer);
                    }
                    true
                }
                Err(_) => false,
            })
            .unwrap_or(false);
        if !applied {
            schedule_retry();
            let _ = DEFERRED_RUNNING.try_with(|queue| {
                if let Ok(mut queue) = queue.try_borrow_mut() {
                    queue.extend(running);
                }
            });
        }
    }

    let removals = match DEFERRED_REMOVALS.try_with(|queue| match queue.try_borrow_mut() {
        Ok(mut queue) => Some(std::mem::take(&mut *queue)),
        Err(_) => None,
    }) {
        Ok(Some(removals)) => removals,
        Ok(None) | Err(_) => {
            schedule_retry();
            Vec::new()
        }
    };
    if !removals.is_empty() {
        let mut callbacks = Vec::new();
        let applied = GRAPH
            .try_with(|graph| match graph.try_borrow_mut() {
                Ok(mut graph) => {
                    for observer in &removals {
                        if let Some(callback) = graph.remove_observer(*observer) {
                            callbacks.push(callback);
                        }
                        remove_pending_observer(*observer);
                    }
                    true
                }
                Err(_) => false,
            })
            .unwrap_or(false);
        drop(callbacks);
        if !applied {
            schedule_retry();
            let _ = DEFERRED_REMOVALS.try_with(|queue| {
                if let Ok(mut queue) = queue.try_borrow_mut() {
                    queue.extend(removals);
                }
            });
        }
    }
}

fn drain_pending() {
    struct DepthGuard {
        previous: u32,
    }

    impl Drop for DepthGuard {
        fn drop(&mut self) {
            SIGNAL_DEPTH.with(|depth| depth.set(self.previous));
        }
    }

    let previous_depth = SIGNAL_DEPTH.with(Cell::get);
    SIGNAL_DEPTH.with(|depth| depth.set(previous_depth + 1));
    let _depth_guard = DepthGuard {
        previous: previous_depth,
    };
    if previous_depth == 0 {
        let retries = match DEFERRED_SELF_RETRIES.try_with(|deferred| {
            deferred
                .try_borrow_mut()
                .ok()
                .map(|mut deferred| std::mem::take(&mut *deferred))
        }) {
            Ok(Some(retries)) => retries,
            Ok(None) | Err(_) => {
                schedule_retry();
                Vec::new()
            }
        };
        for observer in retries {
            enqueue_observer(observer);
        }
    }

    loop {
        retry_deferred_housekeeping();
        if !drain_run_now_queue() {
            break;
        }

        let deferred_observers = DEFERRED_OBSERVERS
            .try_with(|deferred| match deferred.try_borrow_mut() {
                Ok(mut deferred) => std::mem::take(&mut *deferred),
                Err(_) => Vec::new(),
            })
            .unwrap_or_default();
        for observer in deferred_observers {
            enqueue_observer(observer);
        }

        let deferred = DEFERRED_SIGNALS
            .try_with(|signals| match signals.try_borrow_mut() {
                Ok(mut signals) => std::mem::take(&mut *signals),
                Err(_) => Vec::new(),
            })
            .unwrap_or_default();
        for signal in deferred {
            enqueue_affected(signal);
        }

        let next = PENDING
            .try_with(|pending| match pending.try_borrow_mut() {
                Ok(mut pending) => pending
                    .queue
                    .pop_front()
                    .filter(|observer| pending.set.remove(observer)),
                Err(_) => {
                    schedule_retry();
                    None
                }
            })
            .unwrap_or(None);
        let Some(observer) = next else { break };

        let prepared = GRAPH
            .try_with(|graph| match graph.try_borrow_mut() {
                Ok(mut graph) => graph.prepare(observer),
                Err(_) => {
                    schedule_retry();
                    None
                }
            })
            .unwrap_or(None);
        let Some(function) = prepared else {
            requeue_observer(observer, true);
            schedule_retry();
            if DEFERRED_RUNNING
                .try_with(|queue| {
                    queue
                        .try_borrow()
                        .ok()
                        .is_some_and(|q| q.contains(&observer))
                })
                .unwrap_or(false)
            {
                break;
            }
            break;
        };
        let Some(function) = function else {
            continue;
        };
        let _ = run_observer_guarded(observer, function);
    }
}

pub fn new_observer(function: impl Fn() + 'static) -> ObserverId {
    GRAPH.with(|graph| {
        let mut graph = graph.borrow_mut();
        let observer = graph.next_observer;
        graph.next_observer = graph.next_observer.wrapping_add(1);
        graph.observers.insert(observer, Rc::new(function));
        observer
    })
}

pub fn remove_observer(observer: ObserverId) {
    remove_pending_observer(observer);
    let callback = GRAPH
        .try_with(|graph| match graph.try_borrow_mut() {
            Ok(mut graph) => graph.remove_observer(observer),
            Err(_) => None,
        })
        .unwrap_or(None);
    let removed = callback.is_some()
        || GRAPH
            .try_with(|graph| {
                graph
                    .try_borrow()
                    .map(|graph| !graph.observers.contains_key(&observer))
                    .unwrap_or(false)
            })
            .unwrap_or(false);
    drop(callback);
    if !removed {
        let queued = DEFERRED_REMOVALS.try_with(|queue| {
            if let Ok(mut queue) = queue.try_borrow_mut() {
                if !queue.contains(&observer) {
                    queue.push(observer);
                }
                true
            } else {
                false
            }
        });
        if !matches!(queued, Ok(true)) {
            schedule_retry();
        }
    }
}

pub fn without_observer<R>(function: impl FnOnce() -> R) -> R {
    struct RestoreGuard {
        previous: Option<Option<ObserverId>>,
    }

    impl Drop for RestoreGuard {
        fn drop(&mut self) {
            let Some(previous) = self.previous.take() else {
                return;
            };
            let restored = CURRENT_OBSERVER
                .try_with(|current| match current.try_borrow_mut() {
                    Ok(mut current) => {
                        if current.is_none() {
                            *current = previous;
                            true
                        } else {
                            false
                        }
                    }
                    Err(_) => false,
                })
                .unwrap_or(false);
            if !restored {
                schedule_retry();
            }
        }
    }

    let previous = CURRENT_OBSERVER
        .try_with(|current| match current.try_borrow_mut() {
            Ok(mut current) => {
                let previous = *current;
                *current = None;
                Some(previous)
            }
            Err(_) => None,
        })
        .unwrap_or(None);
    let _guard = RestoreGuard { previous };
    function()
}

pub(crate) fn try_run_observer_now(
    observer: ObserverId,
) -> Result<(), Box<dyn std::any::Any + Send>> {
    struct DepthGuard {
        previous: u32,
    }

    impl Drop for DepthGuard {
        fn drop(&mut self) {
            SIGNAL_DEPTH.with(|depth| depth.set(self.previous));
        }
    }

    let previous = SIGNAL_DEPTH.with(Cell::get);
    SIGNAL_DEPTH.with(|depth| depth.set(previous.wrapping_add(1)));
    let _depth_guard = DepthGuard { previous };

    let prepared = GRAPH
        .try_with(|graph| match graph.try_borrow_mut() {
            Ok(mut graph) => graph.prepare(observer),
            Err(_) => {
                schedule_retry();
                None
            }
        })
        .unwrap_or(None);
    match prepared {
        Some(Some(function)) => run_observer_guarded(observer, function),
        Some(None) => Ok(()),
        None => {
            let queued = RUN_NOW.try_with(|queue| {
                if let Ok(mut queue) = queue.try_borrow_mut() {
                    queue.push_back(observer);
                    true
                } else {
                    false
                }
            });
            if !matches!(queued, Ok(true)) {
                schedule_retry();
            }
            schedule_retry();
            Ok(())
        }
    }
}

pub fn run_observer_now(observer: ObserverId) {
    let _ = try_run_observer_now(observer);
}

fn drain_run_now_queue() -> bool {
    let observer = RUN_NOW
        .try_with(|queue| {
            queue
                .try_borrow_mut()
                .ok()
                .and_then(|mut queue| queue.pop_front())
        })
        .unwrap_or(None);
    let Some(observer) = observer else {
        return true;
    };
    run_observer_now(observer);
    RUN_NOW
        .try_with(|queue| {
            queue
                .try_borrow()
                .map(|queue| queue.is_empty())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

pub fn take_retry_request() -> bool {
    RETRY_NEEDED.with(|retry| retry.replace(false))
}
