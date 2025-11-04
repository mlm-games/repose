use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::reactive;

pub type SubId = usize;

static NEXT_SIGNAL_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone)]
pub struct Signal<T: 'static>(Rc<RefCell<Inner<T>>>);

struct Inner<T> {
    id: usize,
    value: T,
    subs: Vec<Box<dyn Fn(&T)>>,
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
    pub fn set(&self, v: T) {
        let mut inner = self.0.borrow_mut();
        inner.value = v;
        let vref = &inner.value;
        for s in &inner.subs {
            s(vref);
        }
        // notify reactive graph
        reactive::signal_changed(inner.id);
    }
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let mut inner = self.0.borrow_mut();
        f(&mut inner.value);
        let vref = &inner.value;
        for s in &inner.subs {
            s(vref);
        }
        reactive::signal_changed(inner.id);
    }
    pub fn subscribe(&self, f: impl Fn(&T) + 'static) -> SubId {
        self.0.borrow_mut().subs.push(Box::new(f));
        self.0.borrow().subs.len() - 1
    }
}

pub fn signal<T>(t: T) -> Signal<T> {
    Signal::new(t)
}
