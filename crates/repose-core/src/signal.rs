use std::cell::RefCell;
use std::rc::Rc;

pub type SubId = usize;

#[derive(Clone)]
pub struct Signal<T: 'static>(Rc<RefCell<Inner<T>>>);

struct Inner<T> {
    value: T,
    subs: Vec<Box<dyn Fn(&T)>>,
}

impl<T> Signal<T> {
    pub fn new(value: T) -> Self {
        Self(Rc::new(RefCell::new(Inner {
            value,
            subs: Vec::new(),
        })))
    }
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.0.borrow().value.clone()
    }
    pub fn set(&self, v: T) {
        let mut inner = self.0.borrow_mut();
        inner.value = v;
        let vref = &inner.value;
        for s in &inner.subs {
            s(vref);
        }
    }
    pub fn update<F: FnOnce(&mut T)>(&self, f: F) {
        let mut inner = self.0.borrow_mut();
        f(&mut inner.value);
        let vref = &inner.value;
        for s in &inner.subs {
            s(vref);
        }
    }
    pub fn subscribe(&self, f: impl Fn(&T) + 'static) -> SubId {
        self.0.borrow_mut().subs.push(Box::new(f));
        self.0.borrow().subs.len() - 1
    }
}

pub fn signal<T>(t: T) -> Signal<T> {
    Signal::new(t)
}
