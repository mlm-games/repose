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
//! short animation, the way the platform does. A committed one keeps easing to
//! `1.0` for a moment, so the screen finishes leaving before the back dispatch
//! pops it: Android commits around a tenth of the drag, and cutting the screen
//! out at that point is what makes a predictive transition look like two
//! animations.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The current predictive-back gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackGesture {
    /// Whether the user is currently dragging the back gesture.
    pub active: bool,
    /// How far the gesture has travelled, `0.0` at the edge. Android commits a
    /// back gesture after a fraction of that travel, so a view should finish its
    /// transition at `1.0` rather than at the release point.
    pub progress: f32,
    /// The screen edge the gesture started from.
    pub edge: SwipeEdge,
}

/// The edge a back gesture started from.
///
/// The platform reports it so a view can move with the finger instead of
/// guessing a direction: a left-edge gesture travels right, a right-edge one
/// travels left.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SwipeEdge {
    #[default]
    Left,
    Right,
}

static ACTIVE: AtomicBool = AtomicBool::new(false);
static EDGE_RIGHT: AtomicBool = AtomicBool::new(false);
static PROGRESS_BITS: AtomicU32 = AtomicU32::new(0);
static SETTLE_BITS: AtomicU32 = AtomicU32::new(0);
static SETTLE_START: Mutex<Option<web_time::Instant>> = Mutex::new(None);
static COMMIT_BITS: AtomicU32 = AtomicU32::new(0);
static COMMIT_START: Mutex<Option<web_time::Instant>> = Mutex::new(None);
static COMMITTED: AtomicBool = AtomicBool::new(false);

/// How long one full predictive transition takes. Compose derives this from the
/// transition's spring; both the cancel and the commit scale it by the fraction
/// they have to cover.
const TRANSITION_MS: f32 = 200.0;

/// The current gesture, with `progress` already settled when it is inactive.
pub fn gesture() -> BackGesture {
    BackGesture {
        active: active(),
        progress: progress(),
        edge: edge(),
    }
}

/// Whether a predictive-back gesture is in progress.
pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

/// The edge the last back gesture started from.
///
/// Kept after the finger lifts so a settling view keeps moving the way the
/// gesture did. Platforms without an edge keep the default.
pub fn edge() -> SwipeEdge {
    if EDGE_RIGHT.load(Ordering::Relaxed) {
        SwipeEdge::Right
    } else {
        SwipeEdge::Left
    }
}

/// Record the edge a gesture started from.
pub fn set_edge(edge: SwipeEdge) {
    let right = edge == SwipeEdge::Right;
    if EDGE_RIGHT.swap(right, Ordering::Relaxed) != right {
        crate::request_frame();
    }
}

/// The gesture's progress, `0.0` at the start and `1.0` fully committed.
///
/// Returns the settling value for a moment after a cancelled gesture, so a view
/// that follows it never jumps back to rest, and keeps easing to `1.0` for a
/// moment after a commit, so the screen finishes leaving before the pop lands.
pub fn progress() -> f32 {
    if active() {
        return f32::from_bits(PROGRESS_BITS.load(Ordering::Relaxed));
    }
    if let Some(progress) = commit_progress() {
        return progress;
    }
    settling_progress()
}

fn commit_progress() -> Option<f32> {
    let phase = commit_phase()?;
    let from = f32::from_bits(COMMIT_BITS.load(Ordering::Relaxed));
    Some(from + (1.0 - from) * phase)
}

/// How far the remaining exit of a committed gesture has run, `0.0` at the
/// commit and `1.0` when the screen has finished leaving.
///
/// The commit covers only the part of the transition the drag did not, so a deep
/// drag lands almost at once, the way a Compose seek finishes.
pub fn commit_phase() -> Option<f32> {
    let start = (*COMMIT_START.lock().unwrap_or_else(|e| e.into_inner()))?;
    let from = f32::from_bits(COMMIT_BITS.load(Ordering::Relaxed));
    let span = TRANSITION_MS * (1.0 - from);
    if span <= 0.0 {
        return Some(1.0);
    }
    let t = (start.elapsed().as_secs_f32() * 1000.0 / span).clamp(0.0, 1.0);
    if t < 1.0 {
        crate::request_frame();
    }
    Some(t)
}

fn settling_progress() -> f32 {
    let mut start = SETTLE_START.lock().unwrap_or_else(|e| e.into_inner());
    let Some(at) = *start else {
        return 0.0;
    };
    let from = f32::from_bits(SETTLE_BITS.load(Ordering::Relaxed));
    // Reversing a gesture takes as long as the part of the transition it already
    // played, which is how a Compose cancel is timed.
    let span = TRANSITION_MS * from;
    if span <= 0.0 {
        *start = None;
        SETTLE_BITS.store(0, Ordering::Relaxed);
        return 0.0;
    }
    let t = at.elapsed().as_secs_f32() * 1000.0 / span;
    if t >= 1.0 {
        *start = None;
        SETTLE_BITS.store(0, Ordering::Relaxed);
        return 0.0;
    }
    crate::request_frame();
    from * (1.0 - t)
}

/// Update the gesture state and request a frame.
///
/// Platform runners call this on the UI thread as gesture events arrive. Passing
/// `false` resets at once; use [`cancel`] to let a released gesture settle, or
/// [`commit`] to let it finish leaving.
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
    COMMIT_START
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if was_active != active || old_progress != progress.to_bits() {
        crate::request_frame();
    }
}

/// Abandon the gesture, easing back to rest from where the finger left it.
///
/// The committed case goes through [`commit`] instead: the screen finishes
/// leaving before the back dispatch pops it. A cancel that arrives after the
/// commit is authoritative cannot pull the finished exit back to rest.
pub fn cancel() {
    if committing() {
        return;
    }
    let from = progress();
    ACTIVE.store(false, Ordering::Relaxed);
    PROGRESS_BITS.store(0, Ordering::Relaxed);
    SETTLE_BITS.store(from.to_bits(), Ordering::Relaxed);
    *SETTLE_START.lock().unwrap_or_else(|e| e.into_inner()) = Some(web_time::Instant::now());
    COMMIT_START
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    crate::request_frame();
}

/// Finish a gesture that committed.
///
/// The progress runs to `1.0` over the remaining fraction of the transition
/// while the runner holds the back dispatch: Android commits at roughly a third
/// of the drag, so dispatching at once would cut the screen out mid-transition.
/// [`commit_settled`] reports when the screen has finished leaving.
pub fn commit() {
    let from = progress();
    ACTIVE.store(false, Ordering::Relaxed);
    PROGRESS_BITS.store(0, Ordering::Relaxed);
    SETTLE_START
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    SETTLE_BITS.store(0, Ordering::Relaxed);
    COMMIT_BITS.store(from.to_bits(), Ordering::Relaxed);
    *COMMIT_START.lock().unwrap_or_else(|e| e.into_inner()) = Some(web_time::Instant::now());
    crate::request_frame();
}

/// Whether a committed gesture has finished leaving.
pub fn commit_settled() -> bool {
    commit_phase().is_some_and(|phase| phase >= 1.0)
}

/// Whether a committed gesture is still finishing its exit.
///
/// A view uses this to tell a commit from a cancel while both are inactive:
/// the commit is the one that plays the arriving screen in.
pub fn committing() -> bool {
    commit_phase().is_some_and(|phase| phase < 1.0)
}

/// Note that the gesture that just ended committed.
///
/// A navigation display consumes this with [`take_committed`] while popping the
/// entry the gesture dragged away: the gesture already played the whole
/// transition, so the pop must not animate the destination in a second time.
pub fn mark_committed() {
    COMMITTED.store(true, Ordering::Relaxed);
}

/// Take the [`mark_committed`] flag, leaving it clear.
pub fn take_committed() -> bool {
    COMMITTED.swap(false, Ordering::Relaxed)
}
