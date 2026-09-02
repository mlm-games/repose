use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use web_time::{Duration, Instant};

use crate::{Signal, reactive, request_frame, signal};

thread_local! {
    static REGISTRY: RefCell<HashMap<usize, Entry>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<usize> = const { RefCell::new(1usize) };
}

struct Entry {
    deadline: Instant,
    callback: Rc<dyn Fn()>,
}

fn next_id() -> usize {
    NEXT_ID.with(|c| {
        let mut v = c.borrow_mut();
        let id = *v;
        *v += 1;
        id
    })
}

/// Earliest debounced deadline, if any. Used by `ReposeRuntime::next_wakeup_deadline`
/// so `platform` can `WaitUntil` without knowing about debounce.
pub fn next_deadline() -> Option<Instant> {
    REGISTRY.with(|r| r.borrow().values().map(|e| e.deadline).min())
}

/// Fire all debounced callbacks whose deadline is due. Called from
/// `ReposeRuntime::tick_overlays` (each frame) - ensures `request_frame` wakeup
/// at `deadline` actually propagates the value.
pub fn poll() {
    let now = Instant::now();
    let due: Vec<Rc<dyn Fn()>> = REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        let mut due = Vec::new();
        let mut to_remove = Vec::new();
        for (id, entry) in reg.iter() {
            if entry.deadline <= now {
                due.push(entry.callback.clone());
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            reg.remove(&id);
        }
        due
    });
    for cb in due {
        cb();
    }
}

fn schedule(id: usize, deadline: Instant, cb: Rc<dyn Fn()>) {
    REGISTRY.with(|r| {
        r.borrow_mut().insert(
            id,
            Entry {
                deadline,
                callback: cb,
            },
        );
    });
    request_frame();
}

fn cancel(id: usize) {
    REGISTRY.with(|r| {
        r.borrow_mut().remove(&id);
    });
}

/// Debounce `source` by `delay`. Returns a new `Signal<T>` that follows `source`
/// after `delay` of inactivity. Resets on every `source` change.
///
/// Must be called inside composition.
///
/// ```ignore
/// let search = signal(String::new());
/// let debounced = debounced_signal(search.clone(), Duration::from_millis(300));
/// // `debounced.get()` updates 300ms after last `search.set`
/// ```
pub fn debounced_signal<T>(source: Signal<T>, delay: Duration) -> Signal<T>
where
    T: Clone + PartialEq + 'static,
{
    let out = signal(source.get());
    let out_clone = out.clone();
    let pending: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let delay_c = delay;

    // unique id for this debounced instance
    let id = next_id();

    let obs_id = reactive::new_observer({
        let source = source.clone();
        let out_clone2 = out_clone.clone();
        let pending2 = pending.clone();
        move || {
            let v = source.get();
            // already equal to pending/out? still reschedule to debounce
            *pending2.borrow_mut() = Some(v.clone());
            let deadline = Instant::now() + delay_c;
            let out_c = out_clone2.clone();
            let pending_c = pending2.clone();
            let cb: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(val) = pending_c.borrow_mut().take() {
                    // only set if changed to avoid extra frame
                    out_c.set_neq(val);
                }
            });
            schedule(id, deadline, cb);
        }
    });

    // establish initial deps
    reactive::run_observer_now(obs_id);

    // cleanup on unmount (scope drop)
    crate::scoped_effect(move || {
        crate::on_unmount(move || {
            reactive::remove_observer(obs_id);
            cancel(id);
        })
    });

    out
}

/// Keyed variant - stable across conditional branches.
pub fn debounced_signal_with_key<T>(
    key: impl Into<String>,
    source: Signal<T>,
    delay: Duration,
) -> Signal<T>
where
    T: Clone + PartialEq + 'static,
{
    let key = key.into();
    crate::remember_with_key(key, || debounced_signal(source, delay))
        .as_ref()
        .clone()
}
