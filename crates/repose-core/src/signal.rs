use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::reactive;

pub type SubId = usize;

static NEXT_SIGNAL_ID: AtomicUsize = AtomicUsize::new(1);

pub struct Signal<T: 'static>(Rc<RefCell<Inner<T>>>);

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

struct Inner<T> {
    id: usize,
    value: T,
    subs: Vec<Option<Box<dyn Fn(&T)>>>,
    free_list: Vec<SubId>,
}

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        let id = NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed);
        Self(Rc::new(RefCell::new(Inner {
            id,
            value,
            subs: Vec::new(),
            free_list: Vec::new(),
        })))
    }

    pub fn id(&self) -> usize {
        self.0.borrow().id
    }

    pub fn get(&self) -> T
    where
        T: Clone,
    {
        let inner = self.0.borrow();
        reactive::register_signal_read(inner.id);
        inner.value.clone()
    }

    /// Read the current value without cloning it, tracking the read in the
    /// reactive graph. Prefer over `get` for large/expensive-to-clone types.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let inner = self.0.borrow();
        reactive::register_signal_read(inner.id);
        f(&inner.value)
    }

    /// Set the signal value only if it changed, skipping subscribers, the
    /// reactive graph, and the frame request when the value is unchanged.
    pub fn set_neq(&self, v: T)
    where
        T: PartialEq,
    {
        let id = {
            let mut inner = self.0.borrow_mut();
            if inner.value == v {
                return;
            }
            inner.value = v;
            inner.id
        };
        self.notify_and_request_frame(id);
    }

    /// Set the signal value and notify subscribers + the reactive graph.
    pub fn set(&self, v: T) {
        let id = {
            let mut inner = self.0.borrow_mut();
            inner.value = v;
            inner.id
        };
        self.notify_and_request_frame(id);
    }

    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let id = {
            let mut inner = self.0.borrow_mut();
            f(&mut inner.value);
            inner.id
        };
        self.notify_and_request_frame(id);
    }

    fn notify_and_request_frame(&self, id: usize) {
        let subs_snapshot: Vec<(SubId, *const T)> = Vec::new();
        let callbacks: Vec<Box<dyn Fn(&T)>> = Vec::new();
        let _ = subs_snapshot;
        let _ = callbacks;
        reactive::without_observer(|| {
            let indices: Vec<SubId> = {
                let inner = self.0.borrow();
                inner
                    .subs
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| if s.is_some() { Some(i) } else { None })
                    .collect()
            };
            for idx in indices {
                let cb_opt = {
                    let inner = match self.0.try_borrow() {
                        Ok(b) => b,
                        Err(_) => {
                            log::warn!("Signal notify: inner already borrowed, skipping idx {idx}");
                            continue;
                        }
                    };
                    inner.subs[idx]
                        .as_ref()
                        .map(|b| b.as_ref() as *const dyn Fn(&T))
                };
                if let Some(ptr) = cb_opt {
                    // Need value ref; get it via try_borrow (may fail if cb mutated, but we already dropped)
                    let val_ptr = {
                        let inner = match self.0.try_borrow() {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        &inner.value as *const T
                    };
                    // Safety: ptr and val_ptr are valid for this call (no mutation of Vec during iteration
                    // except via free_list push which doesn't reallocate subs Vec middle).
                    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
                        (*ptr)(&*val_ptr)
                    }));
                    if let Err(e) = res {
                        let msg = e
                            .downcast_ref::<String>()
                            .map(|s| s.as_str())
                            .or_else(|| e.downcast_ref::<&str>().copied())
                            .unwrap_or("unknown");
                        log::error!("Signal subscriber panicked: {msg}");
                    }
                }
            }
        });

        reactive::signal_changed(id);
        crate::signal_fired();
        crate::request_frame();
    }

    pub fn subscribe(&self, f: impl Fn(&T) + 'static) -> SubId {
        let mut inner = self.0.borrow_mut();
        if let Some(free_id) = inner.free_list.pop() {
            inner.subs[free_id] = Some(Box::new(f));
            free_id
        } else {
            inner.subs.push(Some(Box::new(f)));
            inner.subs.len() - 1
        }
    }

    /// Remove a subscriber by id. Returns true if removed.
    pub fn unsubscribe(&self, id: SubId) -> bool {
        let mut inner = self.0.borrow_mut();
        if id < inner.subs.len() && inner.subs[id].is_some() {
            inner.subs[id] = None;
            inner.free_list.push(id);
            while inner.subs.last().is_some_and(|s| s.is_none()) {
                let popped = inner.subs.len() - 1;
                inner.subs.pop();
                // Remove from free_list if it was the tail we just popped
                if let Some(pos) = inner.free_list.iter().position(|&x| x == popped) {
                    inner.free_list.swap_remove(pos);
                }
            }
            true
        } else {
            false
        }
    }

    /// Subscribe and get a guard that auto-unsubscribes on drop.
    pub fn subscribe_guard(&self, f: impl Fn(&T) + 'static) -> SubGuard<T> {
        let id = self.subscribe(f);
        SubGuard {
            sig: self.clone(),
            id,
        }
    }
}

pub fn signal<T>(t: T) -> Signal<T> {
    Signal::new(t)
}

/// RAII guard for a Signal subscription. Unsubscribes on drop.
pub struct SubGuard<T: 'static> {
    sig: crate::Signal<T>,
    id: SubId,
}
impl<T> Drop for SubGuard<T> {
    fn drop(&mut self) {
        let _ = self.sig.unsubscribe(self.id);
    }
}
