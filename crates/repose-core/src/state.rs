use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use crate::{Signal, on_unmount, reactive, remember_with_key, request_frame, scoped_effect, signal};

pub struct MutableState<T: Clone + 'static> {
    inner: Signal<T>,
    saver: Option<Box<dyn StateSaver<T>>>,
}pub trait StateSaver<T>: 'static {
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
    produce_state_inner(key.into(), producer, |out, v| out.set(v))
}

/// Like [`produce_state`], but only writes the output signal when the computed
/// value actually changed (`T: PartialEq`), skipping invalidations/frame
/// requests when the derived value is unchanged.
pub fn produce_state_eq<T: Clone + PartialEq + 'static>(
    key: impl Into<String>,
    producer: impl Fn() -> T + 'static + Clone,
) -> Rc<Signal<T>> {
    produce_state_inner(key.into(), producer, |out, v| out.set_neq(v))
}

fn produce_state_inner<T: Clone + 'static>(
    key: String,
    producer: impl Fn() -> T + 'static + Clone,
    write: impl Fn(Signal<T>, T) + 'static + Copy,
) -> Rc<Signal<T>> {
    remember_with_key(format!("produce:{key}"), || {
        let out: Signal<T> = signal(producer());
        let out_clone = out.clone();

        let obs_id = reactive::new_observer({
            let producer = producer.clone();
            move || {
                let v = producer();
                write(out_clone.clone(), v);
            }
        });

        // Establish initial deps and value
        reactive::run_observer_now(obs_id);

        scoped_effect(move || {
            on_unmount(move || {
                reactive::remove_observer(obs_id);
            })
        });

        out
    })
}

/// Local widget state that drives recomposition on every write.
///
/// Unlike [`crate::remember_state`] (a bare `Rc<RefCell<T>>` that never requests
/// a frame), `Mutable` calls [`request_frame`] on `set`/`update` so async /
/// timer / layout-callback mutations reliably re-render. Prefer [`Signal`] for
/// shared/derived state; use `Mutable` for widget-local state that should
/// always recompose.
pub struct Mutable<T: 'static>(Rc<RefCell<T>>);

// Manual impl: `#[derive(Clone)]` would require `T: Clone`, but `Rc<RefCell<T>>`
// is unconditionally cloneable and local widget state must not need `T: Clone`.
impl<T: 'static> Clone for Mutable<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> Mutable<T> {
    pub fn new(v: T) -> Self {
        Self(Rc::new(RefCell::new(v)))
    }

    pub fn get(&self) -> Ref<'_, T> {
        self.0.borrow()
    }

    pub fn set(&self, v: T) {
        *self.0.borrow_mut() = v;
        crate::signal_fired();
        request_frame();
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        f(&mut *self.0.borrow_mut());
        crate::signal_fired();
        request_frame();
    }

    /// Escape hatch when batching many writes; call [`request_frame`] yourself.
    pub fn borrow_mut_silent(&self) -> RefMut<'_, T> {
        self.0.borrow_mut()
    }

    pub fn as_rc(&self) -> Rc<RefCell<T>> {
        self.0.clone()
    }
}

/// Remember a [`Mutable`] in the current composition slot.
#[track_caller]
pub fn remember_mutable<T: 'static>(init: impl FnOnce() -> T) -> Mutable<T> {
    crate::remember(|| Mutable::new(init())).as_ref().clone()
}
