use std::cell::RefCell;
use std::rc::Rc;

use web_time::{Duration, Instant};

use crate::{Signal, reactive, signal, timer};

/// Earliest deadline across the shared timer queue (timers + debounces).
/// Kept for compatibility; `ReposeRuntime::next_wakeup_deadline` reads
/// `timer::next_deadline` directly.
pub fn next_deadline() -> Option<Instant> {
    timer::next_deadline()
}

/// Kept for compatibility; `ReposeRuntime::tick_overlays` already polls the
/// shared queue via `timer::poll`, which also covers debounced entries.
pub fn poll() {
    timer::poll();
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

    // Owned scheduling slot on the shared timer queue. Reassigning drops the
    // previous handle, which cancels it: the reschedule that makes debounce.
    let slot: Rc<RefCell<Option<timer::TimerHandle>>> = Rc::new(RefCell::new(None));

    let obs_id = reactive::new_observer({
        let source = source.clone();
        let slot = slot.clone();
        move || {
            let v = source.get();
            // already equal to pending/out? still reschedule to debounce
            *pending.borrow_mut() = Some(v.clone());
            let out_c = out_clone.clone();
            let pending_c = pending.clone();
            *slot.borrow_mut() = Some(timer::delay(delay_c, move || {
                if let Some(val) = pending_c.borrow_mut().take() {
                    // only set if changed to avoid extra frame
                    out_c.set_neq(val);
                }
            }));
        }
    });

    // establish initial deps
    reactive::run_observer_now(obs_id);

    // cleanup on unmount (scope drop)
    crate::scoped_effect(move || {
        crate::on_unmount(move || {
            reactive::remove_observer(obs_id);
            // Dropping the handle cancels the pending firing.
            *slot.borrow_mut() = None;
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
