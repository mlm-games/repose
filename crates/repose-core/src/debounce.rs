use std::cell::RefCell;
use std::rc::Rc;

use web_time::{Duration, Instant};

use crate::{Signal, reactive, signal, timer};

/// Earliest deadline across the shared timer queue (timers + debounces).
#[deprecated(
    note = "Debounced entries live on the shared timer queue, which `ReposeRuntime::tick_overlays` already polls via `timer::poll`. Polling here too would advance frame timers twice per redraw. Use `timer::next_deadline` instead."
)]
pub fn next_deadline() -> Option<Instant> {
    timer::next_deadline()
}

/// Fires due entries on the shared timer queue.
#[deprecated(
    note = "`ReposeRuntime::tick_overlays` already polls the shared queue via `timer::poll`. Calling this too would advance frame timers twice per redraw. Use `timer::poll` instead."
)]
pub fn poll() {
    timer::poll();
}

/// Debounce `source` by `delay`. Returns a new `Signal<T>` that follows `source`
/// after `delay` of inactivity. Resets on every `source` change.
///
/// Must be called inside composition. Callsite-keyed: repeated calls from one
/// call site share one observer/timer; use `debounced_signal_with_key` when
/// several debounces share a call site.
#[track_caller]
pub fn debounced_signal<T>(source: Signal<T>, delay: Duration) -> Signal<T>
where
    T: Clone + PartialEq + 'static,
{
    let loc = std::panic::Location::caller();
    debounced_signal_with_key(
        format!("debounce:{}:{}:{}", loc.file(), loc.line(), loc.column()),
        source,
        delay,
    )
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
    crate::remember_with_key(format!("debounce:{key}"), || {
        debounced_signal_uncached(source.clone(), delay)
    })
    .as_ref()
    .clone()
}

fn debounced_signal_uncached<T>(source: Signal<T>, delay: Duration) -> Signal<T>
where
    T: Clone + PartialEq + 'static,
{
    let out = signal(source.get());
    let out_clone = out.clone();
    let pending: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let delay_c = delay;

    let slot: Rc<RefCell<Option<timer::TimerHandle>>> = Rc::new(RefCell::new(None));

    let obs_id = reactive::new_observer({
        let source = source.clone();
        let slot = slot.clone();
        move || {
            let v = source.get();
            *pending.borrow_mut() = Some(v.clone());
            let out_c = out_clone.clone();
            let pending_c = pending.clone();
            *slot.borrow_mut() = Some(timer::delay(delay_c, move || {
                if let Some(val) = pending_c.borrow_mut().take() {
                    out_c.set_neq(val);
                }
            }));
        }
    });

    reactive::run_observer_now(obs_id);

    crate::scoped_effect(move || {
        crate::on_unmount(move || {
            reactive::remove_observer(obs_id);
            *slot.borrow_mut() = None;
        })
    });

    out
}
