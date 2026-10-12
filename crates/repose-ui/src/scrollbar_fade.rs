//! M3 scrollbar fade (Compose `Modifier.scrollbar` parity).
//!
//! The bar is fully visible while its content scrolls or while the bar itself
//! is hovered or pressed, then fades out `fade_delay_ms` after activity stops
//! over `fade_duration_ms`. With `fade_enabled == false` it stays fully
//! visible, and the registry below is untouched.
//!
//! Activity is observed at paint time by watching the content offset, so every
//! binding (plain state, lazy lists, custom setters) behaves the same without
//! touching their code. The only wakeup the runtime must provide is the
//! delay-edge deadline: while fading, the animation driver keeps frames
//! coming on its own.

use std::cell::RefCell;
use std::collections::HashMap;

use web_time::{Duration, Instant};

use repose_core::animation::{AnimationSpec, Easing};
use repose_core::scroll::ScrollbarStyle;

/// Axis salts for fade-registry keys (distinct from the hit-region salts).
pub const FADE_KEY_V: u64 = 0x8000_0010;
pub const FADE_KEY_H: u64 = 0x8000_0011;

struct FadeEntry {
    last_offset: f32,
    last_active: Instant,
    delay_ms: u64,
}

thread_local! {
    static ENTRIES: RefCell<HashMap<u64, FadeEntry>> = RefCell::new(HashMap::new());
}

/// Resolve the paint alpha for one scrollbar. `key` combines the container
/// view id with the axis salt, `offset` is the current content offset,
/// `engaged` is true while the thumb or track is hovered or pressed.
pub fn scrollbar_alpha(key: u64, offset: f32, engaged: bool, style: &ScrollbarStyle) -> f32 {
    if !style.fade_enabled {
        return 1.0;
    }
    let now = Instant::now();
    let target = ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        match entries.get_mut(&key) {
            None => {
                // M3 starts hidden when fading is enabled; hover/press still
                // reveals immediately. The baseline starts stale so merely
                // mounting a scrollbar arms no wakeup: only real activity
                // (or engagement) does.
                let active = engaged;
                entries.insert(
                    key,
                    FadeEntry {
                        last_offset: offset,
                        last_active: if active {
                            now
                        } else {
                            now.checked_sub(Duration::from_millis(style.fade_delay_ms))
                                .unwrap_or(now)
                        },
                        delay_ms: style.fade_delay_ms,
                    },
                );
                if active { 1.0 } else { 0.0 }
            }
            Some(entry) => {
                if engaged || entry.last_offset != offset {
                    entry.last_offset = offset;
                    entry.last_active = now;
                    entry.delay_ms = style.fade_delay_ms;
                    1.0
                } else if now.duration_since(entry.last_active)
                    >= Duration::from_millis(entry.delay_ms)
                {
                    0.0
                } else {
                    1.0
                }
            }
        }
    });
    // M3 `tween(fadeDurationMillis)` (FastOutSlowIn, like every M3 tween).
    crate::anim::animate_f32(
        format!("scrollbar_fade:{key}"),
        target,
        AnimationSpec::tween(
            Duration::from_millis(style.fade_duration_ms),
            Easing::FastOutSlowIn,
        ),
    )
}

/// Earliest fade-out edge across tracked scrollbars, for the host wakeup
/// deadline. Stale entries (long-unmounted scrollbars) are pruned here.
pub fn scrollbar_fade_deadline() -> Option<Instant> {
    let now = Instant::now();
    ENTRIES.with(|entries| {
        let mut entries = entries.borrow_mut();
        entries.retain(|_, entry| {
            now.duration_since(entry.last_active)
                < Duration::from_millis(entry.delay_ms) + Duration::from_secs(10)
        });
        entries
            .values()
            .map(|entry| entry.last_active + Duration::from_millis(entry.delay_ms))
            .filter(|deadline| *deadline > now)
            .min()
    })
}

/// Test hook: drop all tracked activity so fade tests start isolated even
/// when a thread previously painted other scrollbars. Pair with
/// `repose_core::shutdown_composition` to also clear animation states.
#[cfg(test)]
pub fn reset_fade_for_tests() {
    ENTRIES.with(|entries| {
        entries.borrow_mut().clear();
    });
}
