use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::{Signal, remember_with_key, scoped_effect, signal};

pub struct MutableState<T: Clone + 'static> {
    inner: Signal<T>,
    saver: Option<Box<dyn StateSaver<T>>>,
}

pub trait StateSaver<T>: 'static {
    fn save(&self, value: &T) -> Box<dyn Any>;
    fn restore(&self, saved: &dyn Any) -> Option<T>;
}

pub struct DerivedState<T: Clone + 'static> {
    compute: Rc<dyn Fn() -> T>,
    cached: RefCell<Option<T>>,
    dependencies: Vec<Weak<dyn Any>>,
}

impl<T: Clone + 'static> DerivedState<T> {
    pub fn new(compute: impl Fn() -> T + 'static) -> Self {
        Self {
            compute: Rc::new(compute),
            cached: RefCell::new(None),
            dependencies: Vec::new(),
        }
    }

    pub fn get(&self) -> T {
        // Check dependencies for changes
        let needs_recompute = self.cached.borrow().is_none();
        if needs_recompute {
            let value = (self.compute)();
            *self.cached.borrow_mut() = Some(value.clone());
            value
        } else {
            self.cached.borrow().as_ref().unwrap().clone()
        }
    }
}

// State holder pattern
pub trait StateHolder: 'static {
    type State: Clone;
    type Event;

    fn initial_state() -> Self::State;
    fn reduce(state: &Self::State, event: Self::Event) -> Self::State;
}

// pub fn produce_state<T: Clone + 'static>(
//     key: impl Into<String>,
//     producer: impl Fn() -> T + 'static,
// ) -> Rc<Signal<T>> {
//     remember_with_key(key, || {
//         let sig = signal(producer());
//         scoped_effect(|| {
//             let value = producer();
//             sig.set(value);
//             Box::new(|| {})
//         });
//         sig
//     })
//     .clone()
// }
