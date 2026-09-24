use std::any::Any;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use crate::{Signal, reactive, remember_with_key, request_frame};

#[allow(dead_code)]
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

/// Owner of a `produce_state` observer. Stored in the keyed slot; when the
/// slot is replaced (type change or key reuse) or explicitly cleared, `Drop`
/// removes the reactive observer so zombie recomputes cannot accumulate.
/// Navigation-entry scopes additionally dispose via their own `Scope`, which
/// is idempotent with this path.
struct ProduceHandle {
    obs: reactive::ObserverId,
}

impl Drop for ProduceHandle {
    fn drop(&mut self) {
        reactive::remove_observer(self.obs);
    }
}

fn produce_state_inner<T: Clone + 'static>(
    key: String,
    producer: impl Fn() -> T + 'static + Clone,
    write: impl Fn(Signal<T>, T) + 'static + Copy,
) -> Rc<Signal<T>> {
    let full_key = format!("produce:{key}");
    let rc: Rc<(Signal<T>, ProduceHandle)> = remember_with_key(full_key.clone(), || {
        let out_cell: Rc<RefCell<Option<Signal<T>>>> = Rc::new(RefCell::new(None));
        let out_cell_c = out_cell.clone();
        let producer_c = producer.clone();
        let obs_id = reactive::new_observer(move || {
            let value = producer_c();
            let output = {
                let cell = out_cell_c.borrow();
                cell.as_ref().cloned()
            };
            if let Some(output) = output {
                write(output, value);
            } else {
                let old = {
                    let mut cell = out_cell_c.borrow_mut();
                    cell.replace(Signal::new(value))
                };
                drop(old);
            }
        });

        if let Err(payload) = reactive::try_run_observer_now(obs_id) {
            reactive::remove_observer(obs_id);
            std::panic::resume_unwind(payload);
        }
        let initialized = out_cell.borrow().is_some();
        if !initialized {
            reactive::remove_observer(obs_id);
        }
        let out = out_cell
            .borrow()
            .as_ref()
            .cloned()
            .unwrap_or_else(|| Signal::new(producer()));
        (out, ProduceHandle { obs: obs_id })
    });
    if let Some(scope) = crate::scope::current_scope() {
        let owner =
            crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
        let installed = crate::remember_with_key(
            format!("produce-cleanup-installed:{full_key}:{owner}"),
            || RefCell::new(false),
        );
        if !*installed.borrow() {
            *installed.borrow_mut() = true;
            let key = full_key.clone();
            let cleanup_owner = owner.clone();
            let cleanup = crate::Dispose::new(move || {
                let (_released, removed) =
                    crate::runtime::release_keyed_owner(&key, &cleanup_owner);
                drop(removed);
            });
            let registered = cleanup.clone();
            crate::runtime::register_keyed_disposer_for_owner(
                full_key.clone(),
                registered.clone(),
                owner.clone(),
            );
            let key = full_key.clone();
            let scope_owner = owner;
            scope.add_disposer(move || {
                crate::runtime::remove_keyed_disposer_for_owner(&key, &registered, &scope_owner);
            });
        }
    }
    Rc::new(rc.0.clone())
}

/// Local widget state that drives recomposition on every write.
///
/// Unlike [`crate::remember_state`] (a bare `Rc<RefCell<T>>` that never requests
/// a frame), `Mutable` calls [`request_frame`] on `set`/`update` so async /
/// timer / layout-callback mutations reliably re-render. Prefer [`Signal`] for
/// shared/derived state; use `Mutable` for widget-local state that should
/// always recompose.
pub struct Mutable<T: 'static> {
    value: Rc<RefCell<T>>,
    dependency: Signal<()>,
}

impl<T: 'static> Clone for Mutable<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            dependency: self.dependency.clone(),
        }
    }
}

impl<T: 'static> Mutable<T> {
    pub fn new(v: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(v)),
            dependency: Signal::new(()),
        }
    }

    fn track_read(&self) {
        self.dependency.get();
    }

    fn changed(&self) {
        reactive::signal_changed(self.dependency.id());
        crate::signal_fired();
        request_frame();
    }

    pub fn get(&self) -> Ref<'_, T> {
        self.track_read();
        self.value.borrow()
    }

    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.track_read();
        let value = self.value.borrow();
        let result = f(&value);
        drop(value);
        result
    }

    pub fn set(&self, v: T) {
        let old = {
            let mut value = self.value.borrow_mut();
            std::mem::replace(&mut *value, v)
        };
        drop(old);
        self.changed();
    }

    pub fn set_neq(&self, v: T)
    where
        T: PartialEq,
    {
        let old = {
            let mut value = self.value.borrow_mut();
            if *value == v {
                return;
            }
            std::mem::replace(&mut *value, v)
        };
        drop(old);
        self.changed();
    }

    pub fn update(&self, f: impl FnOnce(&mut T)) {
        {
            let mut value = self.value.borrow_mut();
            f(&mut value);
        }
        self.changed();
    }

    pub fn update_neq(&self, f: impl FnOnce(&mut T))
    where
        T: PartialEq + Clone,
    {
        let changed = {
            let mut value = self.value.borrow_mut();
            let before = value.clone();
            f(&mut value);
            *value != before
        };
        if changed {
            self.changed();
        }
    }

    pub fn borrow_mut_silent(&self) -> RefMut<'_, T> {
        self.value.borrow_mut()
    }

    pub fn as_rc(&self) -> Rc<RefCell<T>> {
        self.value.clone()
    }
}

/// Remember a [`Mutable`] in the current composition slot.
#[track_caller]
pub fn remember_mutable<T: 'static>(init: impl FnOnce() -> T) -> Mutable<T> {
    crate::remember(|| Mutable::new(init())).as_ref().clone()
}

/// Key-based variant of [`remember_mutable`]. Stable across conditional branches.
#[track_caller]
pub fn remember_mutable_with_key<T: 'static>(
    key: impl Into<String>,
    init: impl FnOnce() -> T,
) -> Mutable<T> {
    remember_with_key(key, || Mutable::new(init()))
        .as_ref()
        .clone()
}

/// Remember a reducer-backed local state. Returns a [`Mutable`] snapshot reader
/// plus a dispatch closure that runs `H::reduce` and writes the result back.
///
/// Prefer this for multi-field widget state over many loose `Mutable`s. It keeps
/// the state shape and all mutations in one place.
#[track_caller]
pub fn remember_reducer<H: StateHolder>() -> (Mutable<H::State>, impl Fn(H::Event) + Clone)
where
    H::State: 'static,
    H::Event: 'static,
{
    let state = remember_mutable(|| H::initial_state());
    let dispatch = {
        let state = state.clone();
        move |ev: H::Event| {
            state.update(|s| *s = H::reduce(s, ev));
        }
    };
    (state, dispatch)
}

/// Key-based variant of [`remember_reducer`]. Stable across conditional branches.
#[track_caller]
pub fn remember_reducer_with_key<H: StateHolder>(
    key: impl Into<String>,
) -> (Mutable<H::State>, impl Fn(H::Event) + Clone)
where
    H::State: 'static,
    H::Event: 'static,
{
    let state = remember_mutable_with_key(key, || H::initial_state());
    let dispatch = {
        let state = state.clone();
        move |ev: H::Event| {
            state.update(|s| *s = H::reduce(s, ev));
        }
    };
    (state, dispatch)
}
