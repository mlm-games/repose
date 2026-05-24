#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::Color;
    use crate::Rect;
    use crate::Vec2;
    use crate::animation::*;
    use crate::remember_with_key;
    use crate::scope::*;
    use crate::signal::*;
    use crate::{
        clear_composer, new_observer, produce_state, remove_observer, run_observer_now,
        signal_changed,
    };
    use web_time::{Duration, Instant};

    #[test]
    fn test_signal_basic() {
        let sig = signal(42);
        assert_eq!(sig.get(), 42);

        sig.set(100);
        assert_eq!(sig.get(), 100);

        sig.update(|v| *v += 1);
        assert_eq!(sig.get(), 101);
    }

    #[test]
    fn test_signal_subscription() {
        let sig = signal(0);
        let called = std::rc::Rc::new(std::cell::RefCell::new(false));

        let called_clone = called.clone();
        sig.subscribe(move |_| {
            *called_clone.borrow_mut() = true;
        });

        sig.set(42);
        assert!(*called.borrow());
    }

    #[test]
    fn test_observer_tracks_signal_read() {
        let sig = signal(0);
        let sig2 = sig.clone();
        let observed = Rc::new(RefCell::new(Vec::new()));

        let obs = {
            let observed = observed.clone();
            new_observer(move || {
                let v = sig.get();
                observed.borrow_mut().push(v);
            })
        };
        run_observer_now(obs);
        assert_eq!(*observed.borrow(), vec![0]);

        sig2.set(42);
        assert_eq!(*observed.borrow(), vec![0, 42]);

        sig2.set(99);
        assert_eq!(*observed.borrow(), vec![0, 42, 99]);

        remove_observer(obs);
    }

    #[test]
    fn test_observer_tracks_multiple_signals() {
        let a = signal(1);
        let a2 = a.clone();
        let b = signal(2);
        let b2 = b.clone();
        let observed = Rc::new(RefCell::new(Vec::new()));

        let obs = {
            let observed = observed.clone();
            new_observer(move || {
                let sum = a.get() + b.get();
                observed.borrow_mut().push(sum);
            })
        };
        run_observer_now(obs);
        assert_eq!(*observed.borrow(), vec![3]);

        a2.set(10);
        assert_eq!(*observed.borrow(), vec![3, 12]);

        b2.set(20);
        assert_eq!(*observed.borrow(), vec![3, 12, 30]);

        remove_observer(obs);
    }

    #[test]
    fn test_remove_observer_stops_notifications() {
        let sig = signal(0);
        let sig2 = sig.clone();
        let count = Rc::new(RefCell::new(0));

        let obs = {
            let count = count.clone();
            new_observer(move || {
                sig.get();
                *count.borrow_mut() += 1;
            })
        };
        run_observer_now(obs);
        assert_eq!(*count.borrow(), 1);

        sig2.set(1);
        assert_eq!(*count.borrow(), 2);

        remove_observer(obs);

        sig2.set(2);
        assert_eq!(*count.borrow(), 2);
    }

    #[test]
    fn test_reentrant_signal_write_no_panic() {
        let a = signal(0);
        let a2 = a.clone();
        let b = signal(0);
        let b2 = b.clone();
        let observed = Rc::new(RefCell::new(Vec::new()));

        let obs = {
            let a = a.clone();
            let observed = observed.clone();
            new_observer(move || {
                let bv = b.get();
                a.set(bv);
                observed.borrow_mut().push(bv);
            })
        };
        run_observer_now(obs);
        assert_eq!(*observed.borrow(), vec![0]);
        assert_eq!(a2.get(), 0);

        b2.set(42);
        assert_eq!(*observed.borrow(), vec![0, 42]);
        assert_eq!(a2.get(), 42);

        a2.set(100);
        assert_eq!(*observed.borrow(), vec![0, 42]);

        remove_observer(obs);
    }

    #[test]
    fn test_signal_changed_directly() {
        let sig = signal(10);
        let sig_id = sig.id();
        let count = Rc::new(RefCell::new(0));

        let obs = {
            let count = count.clone();
            new_observer(move || {
                sig.get();
                *count.borrow_mut() += 1;
            })
        };
        run_observer_now(obs);
        assert_eq!(*count.borrow(), 1);

        signal_changed(sig_id);
        assert_eq!(*count.borrow(), 2);

        remove_observer(obs);
    }

    #[test]
    fn test_observer_dead_observer_after_remove() {
        let sig = signal(0);
        let sig2 = sig.clone();
        let obs = new_observer(move || {
            sig.get();
        });
        run_observer_now(obs);
        remove_observer(obs);

        sig2.set(1);
    }

    #[test]
    fn test_produce_state_tracks_dependencies() {
        let a = signal(1);
        let b = signal(2);

        let sum = produce_state("test_sum", {
            let a = a.clone();
            let b = b.clone();
            move || a.get() + b.get()
        });

        // Initial computed value
        assert_eq!(sum.get(), 3);

        a.set(10);
        assert_eq!(sum.get(), 12);

        b.set(20);
        assert_eq!(sum.get(), 30);
    }

    #[test]
    fn test_produce_state_chained() {
        // Chains: a -> b -> c, where b is produce_state from a, c from b
        let a = signal(1);

        let b = produce_state("chain_b", {
            let a = a.clone();
            move || a.get() * 2
        });
        assert_eq!(b.get(), 2);

        let c = produce_state("chain_c", {
            let b = b.clone();
            move || b.get() + 10
        });
        assert_eq!(c.get(), 12);

        a.set(5);
        assert_eq!(b.get(), 10);
        assert_eq!(c.get(), 20);
    }

    #[test]
    fn test_scope_cleanup() {
        let cleaned_up = std::rc::Rc::new(std::cell::RefCell::new(false));

        {
            let scope = Scope::new();
            let cleaned_up_clone = cleaned_up.clone();
            scope.add_disposer(move || {
                *cleaned_up_clone.borrow_mut() = true;
            });

            assert!(!*cleaned_up.borrow());
        } // Scope drops here

        // Cleanup should not run yet (need explicit dispose)
        // This test shows we need to explicitly call dispose
    }

    #[test]
    fn test_scope_explicit_dispose() {
        let cleaned_up = std::rc::Rc::new(std::cell::RefCell::new(false));

        let scope = Scope::new();
        let cleaned_up_clone = cleaned_up.clone();
        scope.add_disposer(move || {
            *cleaned_up_clone.borrow_mut() = true;
        });

        assert!(!*cleaned_up.borrow());
        scope.dispose();
        assert!(*cleaned_up.borrow());
    }

    #[test]
    fn test_key_based_remember() {
        clear_composer();

        let val1 = remember_with_key("test", || 42);
        let val2 = remember_with_key("test", || 100);

        // Should return the same instance
        assert_eq!(*val1, 42);
        assert_eq!(*val2, 42); // Not 100, because key exists
    }

    #[test]
    fn test_color_from_hex() {
        let c = Color::from_hex("#FF5733");
        assert_eq!(c, Color(255, 87, 51, 255));

        let c_alpha = Color::from_hex("#FF5733AA");
        assert_eq!(c_alpha, Color(255, 87, 51, 170));
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect {
            x: 10.0,
            y: 10.0,
            w: 100.0,
            h: 50.0,
        };

        assert!(rect.contains(Vec2 { x: 50.0, y: 30.0 }));
        assert!(!rect.contains(Vec2 { x: 5.0, y: 30.0 }));
        assert!(!rect.contains(Vec2 { x: 50.0, y: 70.0 }));
    }

    #[test]
    fn test_animation_deterministic() {
        let t0 = Instant::now();
        set_clock(Box::new(TestClock { t: t0 }));

        let mut a = AnimatedValue::new(
            0.0f32,
            AnimationSpec::tween(Duration::from_millis(1000), Easing::Linear),
        );
        a.set_target(10.0);
        // advance 250ms
        set_clock(Box::new(TestClock {
            t: t0 + Duration::from_millis(250),
        }));
        assert!(a.update());
        assert!((*a.get() - 2.5).abs() < 0.01);

        set_clock(Box::new(TestClock {
            t: t0 + Duration::from_millis(1000),
        }));
        let cont = a.update();
        assert!(!cont);
        assert!((*a.get() - 10.0).abs() < 0.001);
    }
}
