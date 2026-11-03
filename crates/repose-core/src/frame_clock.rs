use std::cell::{Cell, RefCell};
use std::sync::OnceLock;
use web_time::{Duration, Instant};

thread_local! {
    static NEEDS_COMPOSE: Cell<bool> = const { Cell::new(true) };
    static NEEDS_PRESENT: Cell<bool> = const { Cell::new(false) };
    static SIGNAL_FIRED: Cell<bool> = const { Cell::new(false) };
    static FRAME_CALLBACKS: RefCell<Vec<Box<dyn FnOnce(Duration)>>> =
        const { RefCell::new(Vec::new()) };
    static RUNNING_FRAME_CALLBACKS: Cell<bool> = const { Cell::new(false) };
}

static FRAME_EPOCH: OnceLock<Instant> = OnceLock::new();

/// Run `f` on the next frame with the time elapsed since the first frame.
///
/// The callback runs once, before composition, so writes it makes are visible
/// in that same frame. Calling it again from inside `f` schedules the next one.
pub fn with_frame_nanos(f: impl FnOnce(Duration) + 'static) {
    FRAME_CALLBACKS.with(|callbacks| callbacks.borrow_mut().push(Box::new(f)));
    request_frame();
}

/// Drain and run the callbacks queued with [`with_frame_nanos`]. Called by the
/// animation driver at the start of each frame.
pub(crate) fn run_frame_callbacks() {
    if RUNNING_FRAME_CALLBACKS.with(Cell::get) {
        return;
    }
    RUNNING_FRAME_CALLBACKS.with(|running| running.set(true));
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            RUNNING_FRAME_CALLBACKS.with(|running| running.set(false));
        }
    }
    let _guard = Guard;
    let callbacks = FRAME_CALLBACKS.with(|callbacks| std::mem::take(&mut *callbacks.borrow_mut()));
    if callbacks.is_empty() {
        return;
    }
    let now = FRAME_EPOCH.get_or_init(Instant::now).elapsed();
    for callback in callbacks {
        callback(now);
    }
}

/// Request another frame (coalesced). Sets both compose and present flags.
///
/// Repose only composes/presents on demand: a static tree paints once and
/// then idles. Anything that animates or polls external state (playback
/// ticking, sim stepping, viewport mounts) must call this every frame it
/// wants a follow-up.
#[inline]
pub fn request_frame() {
    NEEDS_COMPOSE.set(true);
    NEEDS_PRESENT.set(true);
}

/// Request a present-only (no full recompose), e.g. for texture uploads.
#[inline]
pub fn request_present() {
    NEEDS_PRESENT.set(true);
}

/// Returns true if a compose was requested since last check, and clears the flag.
#[inline]
pub fn take_frame_request() -> bool {
    NEEDS_COMPOSE.replace(false)
}

/// Returns true if a present was requested since last check, and clears the flag.
#[inline]
pub fn take_present_request() -> bool {
    NEEDS_PRESENT.replace(false)
}

#[inline]
pub fn peek_present_request() -> bool {
    NEEDS_PRESENT.get()
}

/// Non-consuming check (rarely needed).
#[inline]
pub fn peek_frame_request() -> bool {
    NEEDS_COMPOSE.get()
}

/// Mark that a signal just fired (real data change).
#[inline]
pub fn signal_fired() {
    SIGNAL_FIRED.set(true);
}

/// Returns true if `signal_fired()` was called since last check, and clears it.
#[inline]
pub fn take_signal_fired() -> bool {
    SIGNAL_FIRED.replace(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::rc::Rc;

    #[test]
    fn frame_callbacks_run_once() {
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        with_frame_nanos(move |_| counter.set(counter.get() + 1));
        run_frame_callbacks();
        assert_eq!(hits.get(), 1);
        run_frame_callbacks();
        assert_eq!(hits.get(), 1);
    }
}
