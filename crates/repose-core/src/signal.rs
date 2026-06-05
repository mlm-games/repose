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
}

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        let id = NEXT_SIGNAL_ID.fetch_add(1, Ordering::Relaxed);
        Self(Rc::new(RefCell::new(Inner {
            id,
            value,
            subs: Vec::new(),
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

    /// Set the signal value and notify subscribers + the reactive graph.
    pub fn set(&self, v: T) {
        let id = {
            let mut inner = self.0.borrow_mut();
            inner.value = v;
            inner.id
        };

        // Call subscribers with CURRENT_OBSERVER cleared so subscriber reads
        // don't register edges against the wrong observer.
        reactive::without_observer(|| {
            let inner = self.0.borrow();
            let vref = &inner.value;
            for s in &inner.subs {
                if let Some(cb) = s.as_ref() {
                    cb(vref);
                }
            }
        });

        // Notify reactive graph after all borrows are dropped.
        reactive::signal_changed(id);

        crate::request_frame();
    }

    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let id = {
            let mut inner = self.0.borrow_mut();
            f(&mut inner.value);
            inner.id
        };

        reactive::without_observer(|| {
            let inner = self.0.borrow();
            let vref = &inner.value;
            for s in &inner.subs {
                if let Some(cb) = s.as_ref() {
                    cb(vref);
                }
            }
        });

        reactive::signal_changed(id);

        crate::request_frame();
    }

    pub fn subscribe(&self, f: impl Fn(&T) + 'static) -> SubId {
        let mut inner = self.0.borrow_mut();
        inner.subs.push(Some(Box::new(f)));
        inner.subs.len() - 1
    }

    /// Remove a subscriber by id. Returns true if removed.
    pub fn unsubscribe(&self, id: SubId) -> bool {
        let mut inner = self.0.borrow_mut();
        if id < inner.subs.len() {
            inner.subs[id] = None;
            // prevents unbounded tombstone growth
            while inner.subs.last().is_some_and(|s| s.is_none()) {
                inner.subs.pop();
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

/// RAII guard for a Signal subscription; unsubscribes on drop.
pub struct SubGuard<T: 'static> {
    sig: crate::Signal<T>,
    id: SubId,
}
impl<T> Drop for SubGuard<T> {
    fn drop(&mut self) {
        let _ = self.sig.unsubscribe(self.id);
    }
}
