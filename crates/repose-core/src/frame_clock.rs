use std::cell::Cell;

thread_local! {
    static NEEDS_COMPOSE: Cell<bool> = const { Cell::new(true) };
    static NEEDS_PRESENT: Cell<bool> = const { Cell::new(false) };
    static SIGNAL_FIRED: Cell<bool> = const { Cell::new(false) };
}

/// Request another frame (coalesced). Sets both compose and present flags.
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
