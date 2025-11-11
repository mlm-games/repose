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

pub fn remember_derived<T: Clone + 'static>(
    key: impl Into<String>,
    producer: impl Fn() -> T + 'static + Clone,
) -> std::rc::Rc<crate::Signal<T>> {
    let key: String = key.into();
    produce_state(format!("derived:{key}"), producer)
}

// State holder pattern
pub trait StateHolder: 'static {
    type State: Clone;
    type Event;

    fn initial_state() -> Self::State;
    fn reduce(state: &Self::State, event: Self::Event) -> Self::State;
}

/// Lazily produces a Signal<T> (remembered by key) and keeps it up to date
/// by re-running `producer` under the reactive graph whenever its dependencies change.
///
/// - Runs an initial compute immediately to establish dependencies.
pub fn produce_state<T: Clone + 'static>(
    key: impl Into<String>,
    producer: impl Fn() -> T + 'static + Clone,
) -> Rc<Signal<T>> {
    let key = key.into();
    remember_with_key(format!("produce:{key}"), || {
        let out: Signal<T> = signal(producer());
        let out_clone = out.clone();

        let obs_id = reactive::new_observer({
            let producer = producer.clone();
            move || {
                let v = producer();
                out_clone.set(v);
            }
        });

        // Establish initial deps and value
        reactive::run_observer_now(obs_id);

        scoped_effect(move || {
            // cleanup
            Box::new(move || {
                reactive::remove_observer(obs_id);
            })
        });
        out
    })
}
