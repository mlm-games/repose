//! Global driver that advances all active animations before composition.
//!
//! Mirrors Compose's `BroadcastFrameClock.sendFrame()`
//!
//! Stale entries (from pages that were navigated away from) are swept
//! at the beginning of each `tick()` call: only keys that were `touch`ed
//! during the previous composition pass survive.
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::request_frame;

type TickFn = Rc<RefCell<dyn FnMut() -> bool>>;

thread_local! {
    static REGISTRY: RefCell<Vec<(String, TickFn)>> = RefCell::new(Vec::new());
    static TOUCHED: RefCell<Vec<String>> = RefCell::new(Vec::new());
    static LIVE_EPOCH: Cell<u64> = Cell::new(0);
}

/// Mark `key` as still active this frame. Called from `animate_f32_from` etc.
/// Entries whose key was not touched before the next sweep are removed.
pub fn touch(key: &str) {
    TOUCHED.with(|t| t.borrow_mut().push(key.to_string()));
}

/// Register an animation tick callback keyed by `key`.
/// The callback should return `true` if the animation is still running,
/// `false` if it has settled (will be kept in the registry so
/// `set_target` can reactivate it without re-registering).
/// Re-registering with an existing key replaces the old callback.
pub fn register(key: String, tick: TickFn) {
    LIVE_EPOCH.with(|e| e.set(e.get() + 1));
    REGISTRY.with(|reg| {
        let mut list = reg.borrow_mut();
        list.retain(|(k, _)| k != &key);
        list.push((key, tick));
    });
}

/// Unregister an animation by key.
pub fn unregister(key: &str) {
    REGISTRY.with(|reg| {
        reg.borrow_mut().retain(|(k, _)| k != key);
    });
}

/// Advance all registered animations. Returns `true` if any is still running.
/// If any animation is still running, calls `request_frame()`.
pub fn tick() -> bool {
    TOUCHED.with(|touched| {
        let touched = std::mem::take(&mut *touched.borrow_mut());
        REGISTRY.with(|reg| {
            let mut list = reg.borrow_mut();
            list.retain(|(k, _)| touched.iter().any(|tk| tk == k));
        });
    });

    let mut any_still = false;
    REGISTRY.with(|reg| {
        let mut list = reg.borrow_mut();
        for (_key, tick_fn) in list.iter_mut() {
            if tick_fn.borrow_mut()() {
                any_still = true;
            }
        }
    });
    if any_still {
        LIVE_EPOCH.with(|e| e.set(e.get() + 1));
        request_frame();
    }
    any_still
}

/// Returns `true` if any animation is currently registered.
pub fn is_active() -> bool {
    REGISTRY.with(|reg| !reg.borrow().is_empty())
}

/// When a repaint-boundary stamp hashes this, the boundary is
/// repainted every frame for as long as any tween is live, then cache hits
/// resume once animations settle.
pub fn live_epoch() -> u64 {
    LIVE_EPOCH.with(|e| e.get())
}
