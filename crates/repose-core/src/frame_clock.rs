use std::sync::atomic::{AtomicBool, Ordering};

/// Global "please render another frame" flag.
///
/// This is intentionally simple:
/// - Any UI-relevant state change calls `request_frame()`.
/// - Platform runners call `take_frame_request()` in `about_to_wait`
///   to decide whether to `request_redraw()`.
static NEEDS_FRAME: AtomicBool = AtomicBool::new(true);

/// Distinguishes signal-driven frame requests from event-handler-driven ones.
static SIGNAL_FIRED: AtomicBool = AtomicBool::new(false);

/// Request another frame (coalesced).
#[inline]
pub fn request_frame() {
    NEEDS_FRAME.store(true, Ordering::Release);
}

/// Returns true if a frame was requested since last check, and clears the flag.
#[inline]
pub fn take_frame_request() -> bool {
    NEEDS_FRAME.swap(false, Ordering::AcqRel)
}

/// Non-consuming check (rarely needed).
#[inline]
pub fn peek_frame_request() -> bool {
    NEEDS_FRAME.load(Ordering::Acquire)
}

/// Mark that a signal just fired (real data change).
#[inline]
pub fn signal_fired() {
    SIGNAL_FIRED.store(true, Ordering::Release);
}

/// Returns true if `signal_fired()` was called since last check, and clears it.
#[inline]
pub fn take_signal_fired() -> bool {
    SIGNAL_FIRED.swap(false, Ordering::AcqRel)
}
