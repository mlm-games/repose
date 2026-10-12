//! Track-press auto-repeat for scrollbars.
//!
//! Compose parity (`TrackPressScroller` in
//! `compose/foundation/.../Scrollbar.skiko.kt`): pressing the scrollbar track
//! outside the thumb pages once immediately, then keeps paging toward the held
//! point after a 300ms delay, every 100ms, until release.
//!
//! The repeat itself cannot live in the scrollbar hit closures: those run on
//! input dispatch, but repeats must fire on time even when the pointer sits
//! still and no input arrives. So the track-press closure registers a page
//! callback here, and the app runtime polls it once per tick next to
//! `poll_long_press`, with the next fire time feeding `next_wakeup_deadline`
//! so a sleeping host still wakes up to page.

use std::cell::RefCell;
use std::rc::Rc;

use web_time::{Duration, Instant};

use repose_core::input::{PointerId, PointerKind};

/// First repeat fires this long after the track press; Compose
/// `DelayBeforeSecondScrollOnTrackPress`.
const FIRST_DELAY_MS: u64 = 300;
/// Subsequent pages fire at this cadence; Compose
/// `DelayBetweenScrollsOnTrackPress`.
const REPEAT_MS: u64 = 100;

/// Pointer key for non-touch pointers. Mouse and pen share one active repeat:
/// there is only ever one system pointer of that kind driving a repeat.
const MOUSE_POINTER: u64 = u64::MAX;

/// Stable key for the pointer behind an event. Touch fingers keep their id so
/// two fingers can hold two tracks; mouse and pen collapse to one key.
pub fn pointer_key(kind: PointerKind, id: PointerId) -> u64 {
    match kind {
        PointerKind::Touch => id.0,
        _ => MOUSE_POINTER,
    }
}

/// Same key from the runtime side, which addresses pointers by touch id.
pub fn key_for_touch(touch: Option<u64>) -> u64 {
    touch.unwrap_or(MOUSE_POINTER)
}

struct ActiveRepeat {
    pointer: u64,
    next_fire: Instant,
    page: Rc<dyn Fn()>,
}

thread_local! {
    static ACTIVE: RefCell<Option<ActiveRepeat>> = const { RefCell::new(None) };
}

/// Start (or restart) paging for `pointer`. The caller pages once
/// immediately; the first repeat fires [`FIRST_DELAY_MS`] later.
pub fn begin_track_repeat(pointer: u64, page: Rc<dyn Fn()>) {
    ACTIVE.with(|active| {
        *active.borrow_mut() = Some(ActiveRepeat {
            pointer,
            next_fire: Instant::now() + Duration::from_millis(FIRST_DELAY_MS),
            page,
        });
    });
}

/// Stop paging for `pointer`. No-op when another pointer holds the repeat.
pub fn cancel_track_repeat(pointer: u64) {
    ACTIVE.with(|active| {
        let mut slot = active.borrow_mut();
        if slot.as_ref().is_some_and(|repeat| repeat.pointer == pointer) {
            *slot = None;
        }
    });
}

/// Stop every repeat. Used on global pointer cancel, where per-pointer
/// bookkeeping may already be gone.
pub fn cancel_all_track_repeats() {
    ACTIVE.with(|active| {
        *active.borrow_mut() = None;
    });
}

/// Convenience for runtime press/release/cancel paths.
pub fn cancel_track_repeat_for_touch(touch: Option<u64>) {
    cancel_track_repeat(key_for_touch(touch));
}

/// Fire due repeats. Call once per tick next to the other gesture polls.
/// Returns true when a page ran (and a frame was requested).
pub fn poll_track_repeat() -> bool {
    let now = Instant::now();
    let page = ACTIVE.with(|active| {
        let mut slot = active.borrow_mut();
        match slot.as_mut() {
            Some(repeat) if now >= repeat.next_fire => {
                repeat.next_fire = now + Duration::from_millis(REPEAT_MS);
                Some(repeat.page.clone())
            }
            _ => None,
        }
    });
    match page {
        Some(page) => {
            page();
            repose_core::request_frame();
            true
        }
        None => false,
    }
}

/// Next repeat fire time, for the host wakeup deadline.
pub fn track_repeat_deadline() -> Option<Instant> {
    ACTIVE.with(|active| active.borrow().as_ref().map(|repeat| repeat.next_fire))
}
