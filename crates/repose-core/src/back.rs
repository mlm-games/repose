//! Predictive-back gesture state.
//!
//! Android 14 and later report the back gesture before it commits. The Android
//! runner stores that progress here so a view can move with the finger:
//!
//! ```ignore
//! let offset = repose_core::back::progress() * 120.0;
//! ```
//!
//! [`progress`] is `0.0` unless a gesture is in progress, so it is safe to
//! multiply on every platform. A cancelled gesture settles back to `0.0` over a
//! short animation, the way the platform does; a committed one resets at once
//! and the runner dispatches the back request, which unwinds the screen with the
//! navigation transition.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The current predictive-back gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackGesture {
    /// Whether the user is currently dragging the back gesture.
    pub active: bool,
    /// `0.0` at the edge and `1.0` at the commit threshold. Keeps returning the
    /// settled value for a moment after a cancel.
    pub progress: f32,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PROGRESS_BITS: AtomicU32 = AtomicU32::new(0);
static SETTLE_BITS: AtomicU32 = AtomicU32::new(0);
static SETTLE_START: Mutex<Option<web_time::Instant>> = Mutex::new(None);
static COMMITTED_AT: Mutex<Option<web_time::Instant>> = Mutex::new(None);

/// How long a cancelled gesture takes to return to rest.
const SETTLE_MS: f32 = 180.0;

/// The current gesture, with `progress` already settled when it is inactive.
pub fn gesture() -> BackGesture {
    BackGesture {
        active: active(),
        progress: progress(),
    }
}

/// Whether a predictive-back gesture is in progress.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// The gesture's progress, `0.0` at the start and `1.0` fully committed.
///
/// Returns the settling value for a moment after a cancelled gesture, so a view
/// that follows it never jumps back to rest.
pub fn progress() -> f32 {
    if active() {
        return f32::from_bits(PROGRESS_BITS.load(Ordering::Relaxed));
    }
    settling_progress()
}

fn settling_progress() -> f32 {
    let mut start = SETTLE_START.lock().unwrap_or_else(|e| e.into_inner());
    let Some(at) = *start else {
        return 0.0;
    };
    let t = at.elapsed().as_secs_f32() * 1000.0 / SETTLE_MS;
    if t >= 1.0 {
        *start = None;
        SETTLE_BITS.store(0, Ordering::Relaxed);
        return 0.0;
    }
    crate::request_frame();
    let from = f32::from_bits(SETTLE_BITS.load(Ordering::Relaxed));
    from * (1.0 - t) * (1.0 - t)
}

/// Update the gesture state and request a frame.
///
/// Platform runners call this on the UI thread as gesture events arrive. Passing
/// `false` resets at once; use [`cancel`] to let a released gesture settle.
pub fn set_gesture(active: bool, progress: f32) {
    let progress = if active && progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let was_active = ACTIVE.swap(active, Ordering::Relaxed);
    let old_progress = PROGRESS_BITS.swap(progress.to_bits(), Ordering::Relaxed);
    SETTLE_START
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    SETTLE_BITS.store(0, Ordering::Relaxed);
    if was_active != active || old_progress != progress.to_bits() {
        crate::request_frame();
    }
}

/// Abandon the gesture, easing back to rest from where the finger left it.
///
/// The committed case resets through [`commit`] instead: the navigation
/// transition takes over from the release point.
pub fn cancel() {
    let from = progress();
    ACTIVE.store(false, Ordering::Relaxed);
    PROGRESS_BITS.store(0, Ordering::Relaxed);
    SETTLE_BITS.store(from.to_bits(), Ordering::Relaxed);
    *SETTLE_START.lock().unwrap_or_else(|e| e.into_inner()) = Some(web_time::Instant::now());
    crate::request_frame();
}

/// Finish a gesture that committed.
///
/// Resets at once and remembers the moment, so a navigation display can land
/// where the gesture already was instead of animating the destination in again.
pub fn commit() {
    set_gesture(false, 0.0);
    *COMMITTED_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(web_time::Instant::now());
}

/// Whether a predictive gesture committed within the last few hundred
/// milliseconds.
pub fn committed_recently() -> bool {
    let mut at = COMMITTED_AT.lock().unwrap_or_else(|e| e.into_inner());
    let Some(started) = *at else {
        return false;
    };
    if started.elapsed().as_millis() > 400 {
        *at = None;
        return false;
    }
    true
}
