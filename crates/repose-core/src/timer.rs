//! Wall-clock and frame-based timers without an async runtime.
//!
//! Schedule a callback for an [`Instant`] or a frame count; the platform
//! wakes the event loop exactly then via `ReposeRuntime::next_wakeup_deadline`
//! (`ControlFlow::WaitUntil`). No executor, no threads, no per-frame cost
//! when idle.
//!
//! Compose parallels (approximate):
//!
//! | Compose                           | Here                              |
//! |-----------------------------------|-----------------------------------|
//! | `delay(d)` half of `LaunchedEffect` | [`delay`] (unscoped; hold handle) |
//! | `LaunchedEffect` without keys     | [`scoped_delay`] (unmount only)   |
//! | fixed-rate repetition             | [`interval`] (drift-free)         |
//! | run-after-deadline                | [`timeout`] ([`delay`] alias)      |
//! | flow `debounce` (trailing edge)   | [`Debouncer`] / `debounced_signal`|
//! | sequencing over redraws           | [`delay_frames`]                  |
//!
//! Caveats vs coroutines: handles and flags suppress only *pending* firings.
//! There is no counterpart for cancelling *running* work (`LaunchedEffect`
//! key-change/leave, `withTimeout` aborting in-flight work, `collectLatest`):
//! a firing callback runs to completion, `timeout` is not `withTimeout`, and
//! `delay_frames` counts redraw polls (no vsync timestamp, refresh-rate
//! dependent) rather than subscribing to frames.
//!
//! Rules of thumb: wall-clock waiting goes through [`delay`]/[`interval`];
//! sequencing after animations or redraws through [`delay_frames`]; reactive
//! signal shaping through `debounced_signal`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use web_time::{Duration, Instant};

use crate::{request_frame, unique_component_id};

/// Minimum period for repeating timers. Zero would re-fire every poll and
/// busy-loop the event loop; clamp instead of panicking.
const MIN_PERIOD: Duration = Duration::from_millis(1);

thread_local! {
    static REGISTRY: RefCell<HashMap<u64, Entry>> = RefCell::new(HashMap::new());
    /// Redraw counter, advanced by [`poll`]. Basis for [`delay_frames`].
    static FRAME: RefCell<u64> = const { RefCell::new(0) };
    /// Reentrancy flag: callbacks must not call [`poll`] (nested calls still
    /// work but don't advance the frame counter).
    static IN_POLL: Cell<bool> = const { Cell::new(false) };
}

type Callback = Rc<RefCell<Box<dyn FnMut()>>>;

#[derive(Clone, Copy)]
enum Due {
    /// Fire when wall-clock reaches this instant.
    At(Instant),
    /// Fire once [`FRAME`] reaches this count.
    Frame(u64),
}

#[derive(Clone, Copy)]
enum Repeat {
    Once,
    Every { period: Duration },
    Times { period: Duration, left: u32 },
}

struct Entry {
    due: Due,
    repeat: Repeat,
    callback: Callback,
}

/// Owner of a scheduled timer. Dropping cancels it (no-op if already fired).
#[must_use = "dropping the handle cancels the timer"]
pub struct TimerHandle {
    id: Option<u64>,
}

impl TimerHandle {
    /// Cancel now instead of at drop. Consuming makes double-cancel impossible.
    pub fn cancel(mut self) {
        self.cancel_now();
    }

    /// Detach without cancelling: the timer fires even though no handle owns
    /// it anymore. Prefer holding the handle so timers cancel with their
    /// owner. Never detach a repeating timer unless it is truly global:
    /// nothing will stop it afterwards.
    pub fn detach(self) {
        std::mem::forget(self);
    }

    fn cancel_now(&mut self) {
        if let Some(id) = self.id.take() {
            cancel(id);
        }
    }
}

impl Drop for TimerHandle {
    fn drop(&mut self) {
        self.cancel_now();
    }
}

fn insert(due: Due, repeat: Repeat, callback: Callback) -> TimerHandle {
    let id = unique_component_id();
    REGISTRY.with(|r| {
        r.borrow_mut().insert(id, Entry { due, repeat, callback });
    });
    request_frame();
    TimerHandle { id: Some(id) }
}

fn cancel(id: u64) {
    REGISTRY.with(|r| {
        r.borrow_mut().remove(&id);
    });
}

fn wrap_once(cb: impl FnOnce() + 'static) -> Callback {
    let mut cb = Some(cb);
    Rc::new(RefCell::new(Box::new(move || {
        if let Some(f) = cb.take() {
            f();
        }
    }) as Box<dyn FnMut()>))
}

/// Run `cb` once after `duration`. Returns a handle; dropping it cancels.
///
/// The platform sleeps until the deadline (`ControlFlow::WaitUntil`), so an
/// idle timer costs nothing per frame.
pub fn delay(duration: Duration, cb: impl FnOnce() + 'static) -> TimerHandle {
    insert(
        Due::At(saturating_add(Instant::now(), duration)),
        Repeat::Once,
        wrap_once(cb),
    )
}

/// Run `cb` once after `duration`. A [`delay`] alias naming the
/// run-after-deadline intent. This is not `withTimeout`: it cannot abort
/// in-flight work, it only starts `cb` once the duration elapses.
pub fn timeout(duration: Duration, cb: impl FnOnce() + 'static) -> TimerHandle {
    delay(duration, cb)
}

/// Run `cb` once after `frames` redraws have been polled. Counts polls, not
/// vsync frames: no timestamp, refresh-rate dependent. While frame entries
/// are pending [`poll`] keeps requesting frames. A `0` count fires on the
/// next poll.
pub fn delay_frames(frames: u32, cb: impl FnOnce() + 'static) -> TimerHandle {
    let at = FRAME.with(|f| f.borrow().wrapping_add(frames as u64));
    insert(Due::Frame(at), Repeat::Once, wrap_once(cb))
}

/// Run `cb` every `period`, drift-free (next fire = last scheduled fire +
/// `period`, so slow frames skip beats instead of bunching). Dropping the
/// handle stops the timer. Periods below 1ms are clamped to 1ms.
pub fn interval(period: Duration, cb: impl FnMut() + 'static) -> TimerHandle {
    let period = period.max(MIN_PERIOD);
    insert(
        Due::At(saturating_add(Instant::now(), period)),
        Repeat::Every { period },
        Rc::new(RefCell::new(Box::new(cb) as Box<dyn FnMut()>)),
    )
}

/// Like [`interval`], but stops on its own after `times` firings. A `0` count
/// schedules nothing and returns a disarmed handle.
pub fn interval_n(period: Duration, times: u32, cb: impl FnMut() + 'static) -> TimerHandle {
    if times == 0 {
        return TimerHandle { id: None };
    }
    let period = period.max(MIN_PERIOD);
    insert(
        Due::At(saturating_add(Instant::now(), period)),
        Repeat::Times {
            period,
            left: times,
        },
        Rc::new(RefCell::new(Box::new(cb) as Box<dyn FnMut()>)),
    )
}

/// Current redraw count (advanced by [`poll`]). Basis for [`delay_frames`].
pub fn frame_count() -> u64 {
    FRAME.with(|f| *f.borrow())
}

/// Earliest wall-clock deadline, if any. Fed into
/// `ReposeRuntime::next_wakeup_deadline` so the platform sleeps until a timer
/// is due.
pub fn next_deadline() -> Option<Instant> {
    REGISTRY.with(|r| {
        r.borrow()
            .values()
            .filter_map(|e| match e.due {
                Due::At(t) => Some(t),
                Due::Frame(_) => None,
            })
            .min()
    })
}

/// Saturating `Instant + Duration`: absurd durations fire immediately
/// instead of panicking on overflow.
fn saturating_add(t: Instant, d: Duration) -> Instant {
    t.checked_add(d).unwrap_or(t)
}

/// Advance the frame counter and fire due timers. Called once per redraw
/// (from `ReposeRuntime::tick_overlays`). Do not call from timer callbacks;
/// nested calls are tolerated but skip the frame increment.
pub fn poll() {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            IN_POLL.with(|f| f.set(false));
        }
    }
    let nested = IN_POLL.with(|f| f.replace(true));
    let _guard = Guard;
    let frame = FRAME.with(|f| {
        let mut f = f.borrow_mut();
        // One count per outer redraw; nested polls reuse the current frame.
        if !nested {
            *f = f.wrapping_add(1);
        }
        *f
    });
    let now = Instant::now();
    // Ids are never reused, so rechecking membership before firing lets a
    // same-batch cancel actually suppress the timer.
    let mut due: Vec<(u64, Callback)> = Vec::new();
    let mut remove: Vec<u64> = Vec::new();
    let mut need_frames = false;
    REGISTRY.with(|r| {
        let mut reg = r.borrow_mut();
        for (id, entry) in reg.iter_mut() {
            let is_due = match entry.due {
                Due::At(t) => t <= now,
                Due::Frame(f) => {
                    if frame >= f {
                        true
                    } else {
                        need_frames = true;
                        false
                    }
                }
            };
            if !is_due {
                continue;
            }
            due.push((*id, entry.callback.clone()));
            // Re-base on the old schedule, not now: keeps fixed-rate phase.
            let base = match entry.due {
                Due::At(t) => t,
                Due::Frame(_) => now,
            };
            match entry.repeat {
                Repeat::Once => remove.push(*id),
                Repeat::Every { period } => {
                    entry.due = Due::At(skip_ahead(base, period, now));
                }
                Repeat::Times { period, left } => {
                    if left <= 1 {
                        remove.push(*id);
                    } else {
                        entry.repeat = Repeat::Times {
                            period,
                            left: left - 1,
                        };
                        entry.due = Due::At(skip_ahead(base, period, now));
                    }
                }
            }
        }
    });
    for (id, cb) in due.iter() {
        let live = REGISTRY.with(|r| r.borrow().contains_key(id));
        if live {
            cb.borrow_mut()();
        }
    }
    if !remove.is_empty() {
        REGISTRY.with(|r| {
            let mut reg = r.borrow_mut();
            for id in remove {
                reg.remove(&id);
            }
            need_frames = need_frames || reg.values().any(|e| matches!(e.due, Due::Frame(_)));
        });
    }
    if need_frames {
        request_frame();
    }
}

/// Skip missed beats so sleepers resume without bursting.
fn skip_ahead(mut next: Instant, period: Duration, now: Instant) -> Instant {
    let mut guard = 0u32;
    while next <= now && guard < 1024 {
        next = saturating_add(next, period);
        guard += 1;
    }
    if next <= now {
        saturating_add(now, period)
    } else {
        next
    }
}

/// Trailing-edge debouncer: each call reschedules the single pending firing.
/// Cloneable (shared slot); dropping all clones cancels it.
#[derive(Clone)]
pub struct Debouncer {
    delay: Duration,
    pending: Rc<RefCell<Option<TimerHandle>>>,
}

impl Default for Debouncer {
    /// 300 ms trailing debounce, the conventional UI default.
    fn default() -> Self {
        Self::new(Duration::from_millis(300))
    }
}

impl Debouncer {
    /// Debounce firings by `delay` of inactivity.
    pub fn new(delay: Duration) -> Self {
        Self {
            delay: delay.max(MIN_PERIOD),
            pending: Rc::new(RefCell::new(None)),
        }
    }

    /// Schedule `cb` after a quiet `delay`, replacing any pending firing.
    pub fn call(&self, cb: impl FnOnce() + 'static) {
        *self.pending.borrow_mut() = Some(delay(self.delay, cb));
    }

    /// Drop the pending firing, if any.
    pub fn cancel_pending(&self) {
        *self.pending.borrow_mut() = None;
    }
}

/// Leading-edge throttler with one coalesced trailing firing per period.
#[derive(Clone)]
pub struct Throttler {
    period: Duration,
    last_fire: Rc<RefCell<Option<Instant>>>,
    pending: Rc<RefCell<Option<TimerHandle>>>,
}

impl Throttler {
    /// Throttle firings to at most one leading plus one trailing per `period`.
    pub fn new(period: Duration) -> Self {
        Self {
            period: period.max(MIN_PERIOD),
            last_fire: Rc::new(RefCell::new(None)),
            pending: Rc::new(RefCell::new(None)),
        }
    }

    /// Run `cb` now if the period elapsed since the last firing, else
    /// coalesce it into the single trailing firing at the period edge.
    /// Trailing windows slide from the actual fire time, not the edge.
    pub fn call(&self, cb: impl FnOnce() + 'static) {
        let now = Instant::now();
        let edge = self
            .last_fire
            .borrow()
            .map(|t| saturating_add(t, self.period))
            .unwrap_or(now);
        if now >= edge {
            *self.last_fire.borrow_mut() = Some(now);
            cb();
        } else {
            let last_fire = self.last_fire.clone();
            *self.pending.borrow_mut() = Some(delay(edge - now, move || {
                *last_fire.borrow_mut() = Some(Instant::now());
                cb();
            }));
        }
    }
}

/// [`delay`] tied to the current composition scope: if the scope disposes
/// before the deadline, the callback is suppressed. Schedules once per mount;
/// use [`scoped_delay_with_key`] to restart on change.
pub fn scoped_delay(duration: Duration, cb: impl FnOnce() + 'static) {
    scoped_delay_with_key((), duration, cb);
}

struct ScopedSlot<K> {
    key: Option<K>,
    alive: Rc<RefCell<bool>>,
    installed: bool,
}

/// Keyed [`scoped_delay`]: reschedules when `key` changes (cancelling the
/// previous generation via its flag) and suppresses on unmount. Must be
/// called inside composition; outside a scope it degrades to [`delay`].
pub fn scoped_delay_with_key<K: PartialEq + Clone + 'static>(
    key: K,
    duration: Duration,
    cb: impl FnOnce() + 'static,
) {
    let cell: Rc<RefCell<ScopedSlot<K>>> = crate::remember(|| {
        RefCell::new(ScopedSlot {
            key: None,
            alive: Rc::new(RefCell::new(true)),
            installed: false,
        })
    });
    let mut slot = cell.borrow_mut();
    if !slot.installed {
        slot.installed = true;
        // Read the flag at dispose time: key changes replace it.
        let cell_c = cell.clone();
        crate::scoped_effect(move || {
            crate::on_unmount(move || {
                *cell_c.borrow().alive.borrow_mut() = false;
            })
        });
    }
    if slot.key.as_ref() != Some(&key) {
        *slot.alive.borrow_mut() = false;
        let alive = Rc::new(RefCell::new(true));
        slot.alive = alive.clone();
        slot.key = Some(key);
        delay(duration, move || {
            if *alive.borrow() {
                cb();
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    fn sleep_ms(ms: u64) {
        std::thread::sleep(StdDuration::from_millis(ms));
    }

    /// Clear the thread-local registry so reused test threads don't leak state.
    fn reset() {
        REGISTRY.with(|r| r.borrow_mut().clear());
    }

    #[test]
    fn delay_fires_after_duration() {
        reset();
        let fired = Rc::new(RefCell::new(false));
        let fired_c = fired.clone();
        let _h = delay(Duration::from_millis(5), move || {
            *fired_c.borrow_mut() = true;
        });
        poll();
        assert!(!*fired.borrow(), "must not fire before the deadline");
        sleep_ms(30);
        poll();
        assert!(*fired.borrow(), "must fire once the deadline passes");
        sleep_ms(30);
        poll();
        assert!(next_deadline().is_none(), "one-shot must not reschedule");
    }

    #[test]
    fn drop_cancels_delay() {
        reset();
        let fired = Rc::new(RefCell::new(false));
        let fired_c = fired.clone();
        let h = delay(Duration::from_millis(5), move || {
            *fired_c.borrow_mut() = true;
        });
        drop(h);
        sleep_ms(30);
        poll();
        assert!(!*fired.borrow(), "cancelled timer must not fire");
    }

    #[test]
    fn interval_repeats_and_drop_stops() {
        reset();
        let count = Rc::new(RefCell::new(0u32));
        let count_c = count.clone();
        let h = interval(Duration::from_millis(5), move || {
            *count_c.borrow_mut() += 1;
        });
        sleep_ms(30);
        poll();
        assert!(
            *count.borrow() >= 1,
            "must fire at least once, got {}",
            *count.borrow()
        );
        let after_first = *count.borrow();
        drop(h);
        sleep_ms(30);
        poll();
        assert_eq!(*count.borrow(), after_first, "dropped interval must stop");
    }

    #[test]
    fn interval_n_fires_exactly_n_times() {
        reset();
        let count = Rc::new(RefCell::new(0u32));
        let count_c = count.clone();
        let _h = interval_n(Duration::from_millis(5), 3, move || {
            *count_c.borrow_mut() += 1;
        });
        for _ in 0..10 {
            sleep_ms(15);
            poll();
        }
        assert_eq!(*count.borrow(), 3);
        assert!(next_deadline().is_none());
    }

    #[test]
    fn delay_frames_counts_polls() {
        reset();
        let fired = Rc::new(RefCell::new(false));
        let fired_c = fired.clone();
        let start = frame_count();
        let _h = delay_frames(3, move || {
            *fired_c.borrow_mut() = true;
        });
        poll();
        poll();
        assert!(!*fired.borrow(), "must not fire before 3 polls");
        poll();
        assert!(*fired.borrow(), "must fire on the 3rd poll");
        assert_eq!(frame_count(), start + 3);
    }

    #[test]
    fn debouncer_coalesces_rapid_calls() {
        reset();
        let count = Rc::new(RefCell::new(0u32));
        let deb = Debouncer::new(Duration::from_millis(10));
        for _ in 0..5 {
            let count_c = count.clone();
            deb.call(move || {
                *count_c.borrow_mut() += 1;
            });
        }
        sleep_ms(40);
        poll();
        assert_eq!(
            *count.borrow(),
            1,
            "rapid calls must coalesce into one firing"
        );
    }

    #[test]
    fn throttler_leads_and_trails_once() {
        reset();
        let count = Rc::new(RefCell::new(0u32));
        let thro = Throttler::new(Duration::from_millis(50));
        for _ in 0..5 {
            let count_c = count.clone();
            thro.call(move || {
                *count_c.borrow_mut() += 1;
            });
        }
        assert_eq!(*count.borrow(), 1, "first call fires immediately");
        sleep_ms(80);
        poll();
        assert_eq!(
            *count.borrow(),
            2,
            "the rest collapse into one trailing firing"
        );
    }

    #[test]
    fn same_batch_cancel_suppresses() {
        // Due-buffer order follows HashMap iteration (nondeterministic), so
        // retry until the canceller runs first; only then can suppression be
        // observed. 32 misses in a row is effectively impossible.
        for _ in 0..32 {
            reset();
            let events: Rc<RefCell<Vec<&'static str>>> = Rc::new(RefCell::new(Vec::new()));
            let slot: Rc<RefCell<Option<TimerHandle>>> = Rc::new(RefCell::new(None));
            let ev_c = events.clone();
            let slot_c = slot.clone();
            let _first = delay(Duration::from_millis(1), move || {
                ev_c.borrow_mut().push("cancel");
                *slot_c.borrow_mut() = None;
            });
            let ev_c = events.clone();
            *slot.borrow_mut() = Some(delay(Duration::from_millis(1), move || {
                ev_c.borrow_mut().push("second");
            }));
            sleep_ms(20);
            poll();
            let ev = events.borrow();
            if *ev == ["cancel"] {
                return;
            }
            assert_eq!(
                *ev,
                ["second", "cancel"],
                "unexpected event sequence: {ev:?}"
            );
        }
        panic!("canceller never ran first in 32 trials");
    }

    #[test]
    fn interval_holds_phase_when_poll_late() {
        reset();
        let stamps = Rc::new(RefCell::new(Vec::new()));
        let stamps_c = stamps.clone();
        let period = Duration::from_millis(20);
        let _h = interval(period, move || {
            stamps_c.borrow_mut().push(Instant::now());
        });
        // Sleep through ~3 periods, then poll once: one firing at most, and
        // the next deadline re-bases on the old schedule instead of drifting.
        sleep_ms(70);
        poll();
        assert_eq!(stamps.borrow().len(), 1, "one poll fires once at most");
        let first = stamps.borrow()[0];
        let next = next_deadline().expect("interval must reschedule");
        let gap = next.saturating_duration_since(first);
        assert!(
            gap < period + Duration::from_millis(15),
            "reschedule must not drift by the full lateness, gap was {gap:?}"
        );
    }
}
