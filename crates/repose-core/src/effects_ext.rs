use crate::{Dispose, on_unmount, remember, scoped_effect};
use std::cell::RefCell;

/// cleanup on key change or unmount
pub fn disposable_effect<K: PartialEq + Clone + 'static>(
    key: K,
    effect: impl FnOnce() -> Dispose + 'static,
) {
    // Slot-based (like Compose). For branch-stability use `remember_with_key` variants later.
    let last_key = remember(|| RefCell::new(None::<K>));
    let cleanup_slot = remember(|| RefCell::new(None::<Dispose>));
    let installed = remember(|| RefCell::new(false));

    // Install a single unmount disposer for this callsite.
    if !*installed.borrow() {
        *installed.borrow_mut() = true;
        let cleanup_slot = cleanup_slot.clone();
        scoped_effect(move || {
            on_unmount(move || {
                if let Some(d) = cleanup_slot.borrow_mut().take() {
                    d.run();
                }
            })
        });
    }

    // Key change: cleanup previous + run new effect
    let changed = last_key.borrow().as_ref() != Some(&key);
    if changed {
        *last_key.borrow_mut() = Some(key);

        if let Some(d) = cleanup_slot.borrow_mut().take() {
            d.run();
        }

        let d = effect();
        *cleanup_slot.borrow_mut() = Some(d);
    }
}

/// runs on every recomposition
pub fn side_effect(effect: impl Fn()) {
    effect();
}

/// Internal implementation: keyed by a per-callsite id string.
pub fn launched_effect_internal<K: PartialEq + Clone + 'static>(
    callsite: &'static str,
    key: K,
    effect: impl FnOnce() + 'static,
) {
    // One slot per call-site, with K baked into its type.
    let last_key =
        crate::remember_with_key(format!("launched:{callsite}"), || RefCell::new(None::<K>));

    let mut last = last_key.borrow_mut();
    if last.as_ref() != Some(&key) {
        *last = Some(key);
        // doesn't cancel on unmount
        effect();
    }
}

#[macro_export] // Should probably move this to macros (might want to move the above part too?)
macro_rules! launched_effect {
    ($key:expr, $effect:expr) => {
        $crate::effects_ext::launched_effect_internal(
            concat!(module_path!(), ":", line!(), ":", column!()),
            $key,
            $effect,
        )
    };
}
