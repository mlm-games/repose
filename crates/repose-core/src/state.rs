use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::{Signal, reactive, remember_with_key, scoped_effect, signal};

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
}

impl<T: Clone + 'static> DerivedState<T> {
    pub fn new(compute: impl Fn() -> T + 'static) -> Self {
        Self {
            compute: Rc::new(compute),
            cached: RefCell::new(None),
        }
    }

    pub fn invalidate(&self) {
        *self.cached.borrow_mut() = None;
    }

    pub fn get(&self) -> T {
        if let Some(v) = self.cached.borrow().as_ref() {
            return v.clone();
        }
        let v = (self.compute)();
        *self.cached.borrow_mut() = Some(v.clone());
        v
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
// ) -> Rc<Rc<signal::Signal<_>>> {
//     let key = key.into();
//     remember_with_key(format!("produce:{key}"), || {
//         let out = Rc::new(signal(producer()));
//         let out_weak: Weak<Signal<T>> = Rc::downgrade(&out);

//         let producer_rc = Rc::new(producer);
//         let obs_id = reactive::new_observer({
//             let producer_rc = producer_rc.clone();
//             move || {
//                 if let Some(out) = out_weak.upgrade() {
//                     let v = producer_rc();
//                     out.set(v);
//                 }
//             }
//         });

//         // Initial compute under tracking to establish dependencies
//         reactive::run_observer_now(obs_id);

//         out
//     })
// }
