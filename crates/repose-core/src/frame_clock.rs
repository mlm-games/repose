use std::cell::Cell;

thread_local! {
    static NEEDS_FRAME: Cell<bool> = const { Cell::new(true) };
    static SIGNAL_FIRED: Cell<bool> = const { Cell::new(false) };
}

/// Request another frame (coalesced).
#[inline]
pub fn request_frame() {
    NEEDS_FRAME.set(true);
}

/// Returns true if a frame was requested since last check, and clears the flag.
#[inline]
pub fn take_frame_request() -> bool {
    NEEDS_FRAME.replace(false)
}

/// Non-consuming check (rarely needed).
#[inline]
pub fn peek_frame_request() -> bool {
    NEEDS_FRAME.get()
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
