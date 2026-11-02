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
//! multiply on every platform. A cancelled gesture resets it to `0.0`; a
//! committed gesture becomes inactive and the platform dispatches the actual
//! back request, which unwinds the screen with the normal navigation
//! transition. A screen that springs back from the release point should keep
//! the last active value in its own state.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The current predictive-back gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackGesture {
    /// Whether the user is currently dragging the back gesture.
    pub active: bool,
    /// `0.0` at the edge and `1.0` at the commit threshold.
    pub progress: f32,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static PROGRESS_BITS: AtomicU32 = AtomicU32::new(0);

/// The current gesture, with `progress` already reset when it is inactive.
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
/// Always `0.0` when [`active`] is false.
pub fn progress() -> f32 {
    if active() {
        f32::from_bits(PROGRESS_BITS.load(Ordering::Relaxed))
    } else {
        0.0
    }
}

/// Update the gesture state and request a frame.
///
/// Platform runners call this on the UI thread as gesture events arrive.
pub fn set_gesture(active: bool, progress: f32) {
    let progress = if active && progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let was_active = ACTIVE.swap(active, Ordering::Relaxed);
    let old_progress = PROGRESS_BITS.swap(progress.to_bits(), Ordering::Relaxed);
    if was_active != active || old_progress != progress.to_bits() {
        crate::request_frame();
    }
}
