use std::cell::RefCell;
use std::collections::VecDeque;

thread_local! {
    static RUMBLE: RefCell<VecDeque<(f32, f32, u32)>> =
        const { RefCell::new(VecDeque::new()) };
}

/// Queue a dual-motor rumble for every connected pad (SDL-style:
/// `low_freq` = strong motor, `high_freq` = weak motor, both `0.0..=1.0`).
/// The runtime merges entries into the per-pad request queue once per
/// frame; a `duration_ms` of 0 stops.
pub fn push(low_freq: f32, high_freq: f32, duration_ms: u32) {
    RUMBLE.with(|r| {
        r.borrow_mut().push_back((
            low_freq.clamp(0.0, 1.0),
            high_freq.clamp(0.0, 1.0),
            duration_ms,
        ));
    });
}

/// Drain queued rumbles in FIFO order.
pub fn take() -> Vec<(f32, f32, u32)> {
    RUMBLE.with(|r| r.borrow_mut().drain(..).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_clamps_and_take_drains_fifo() {
        push(2.0, -1.0, 150);
        push(0.0, 0.0, 0);
        assert_eq!(take(), vec![(1.0, 0.0, 150), (0.0, 0.0, 0)]);
        assert!(take().is_empty());
    }
}
