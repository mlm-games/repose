//! Global driver that advances all active animations before composition.
//!
//! Mirrors Compose's `BroadcastFrameClock.sendFrame()`
use std::cell::RefCell;
use std::rc::Rc;

use crate::request_frame;

type TickFn = Rc<RefCell<dyn FnMut() -> bool>>;

thread_local! {
    static REGISTRY: RefCell<Vec<(String, TickFn)>> = RefCell::new(Vec::new());
}

/// Register an animation tick callback keyed by `key`.
/// The callback should return `true` if the animation is still running,
/// `false` if it has settled (will be auto-removed).
pub fn register(key: String, tick: TickFn) {
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
/// Settled animations are kept in the registry so `set_target` can
/// reactivate them without re-registering.
pub fn tick() -> bool {
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
        request_frame();
    }
    any_still
}

/// Returns `true` if any animation is currently registered.
pub fn is_active() -> bool {
    REGISTRY.with(|reg| !reg.borrow().is_empty())
}
