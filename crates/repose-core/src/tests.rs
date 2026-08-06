#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::Color;
    use crate::Rect;
    use crate::Vec2;
    use crate::animation::*;
    use crate::error::{ErrorBoundary, throw_boundary};
    use crate::remember_with_key;
    use crate::runtime::ComposeGuard;
    use crate::scope::*;
    use crate::signal::*;
    use crate::state::remember_mutable;
    use crate::state::remember_mutable_with_key;
    use crate::{View, ViewKind};
    use crate::{
        clear_composer, new_observer, produce_state, produce_state_eq, remove_observer,
        run_observer_now, signal_changed,
    };
    use web_time::{Duration, Instant};

    const FALLBACK_ID: u64 = 0xF00;
    const CONTENT_ID: u64 = 0xC0;

    fn id_view(id: u64) -> View {
        View::new(id, ViewKind::Box)
    }

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
    fn signal_with_reads_without_clone() {
        let sig = signal(String::from("hello"));
        let len = sig.with(|s| s.len());
        assert_eq!(len, 5);
        let _ = sig.get(); // still Clone-able for get
    }

    #[test]
    fn signal_set_neq_skips_notify() {
        let sig = signal(0);
        let count = Rc::new(RefCell::new(0usize));
        let count_clone = count.clone();
        sig.subscribe(move |_| *count_clone.borrow_mut() += 1);

        sig.set_neq(0); // unchanged -> no notify
        assert_eq!(*count.borrow(), 0);

        sig.set_neq(1); // changed -> notify
        assert_eq!(*count.borrow(), 1);

        sig.set_neq(1); // unchanged again
        assert_eq!(*count.borrow(), 1);
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
    fn test_produce_state_eq_skips_writes() {
        let a = signal(1);

        // Subscriber count proves no write happens when the derived value
        // is unchanged after a dependency change.
        let writes = Rc::new(RefCell::new(0usize));
        let eq = produce_state_eq("eq_skip", {
            let a = a.clone();
            move || a.get().min(10)
        });
        let writes_clone = writes.clone();
        eq.subscribe(move |_| *writes_clone.borrow_mut() += 1);

        assert_eq!(eq.get(), 1);

        a.set(5); // min still 5 -> changed
        assert_eq!(eq.get(), 5);
        assert_eq!(*writes.borrow(), 1);

        a.set(8); // min still 8 -> changed
        assert_eq!(eq.get(), 8);
        assert_eq!(*writes.borrow(), 2);

        a.set(2); // min is 2, but current value 8 -> changed
        assert_eq!(eq.get(), 2);
        assert_eq!(*writes.borrow(), 3);

        a.set(99); // min clamped to 10, current 2 -> changed
        assert_eq!(eq.get(), 10);
        assert_eq!(*writes.borrow(), 4);

        a.set(120); // min still 10, no write
        assert_eq!(eq.get(), 10);
        assert_eq!(*writes.borrow(), 4);
    }

    #[test]
    fn test_scope_cleanup_on_drop() {
        let cleaned_up = std::rc::Rc::new(std::cell::RefCell::new(false));

        {
            let scope = Scope::new();
            let cleaned_up_clone = cleaned_up.clone();
            scope.add_disposer(move || {
                *cleaned_up_clone.borrow_mut() = true;
            });

            assert!(!*cleaned_up.borrow());
        } // ScopeInner::drop calls disposers

        assert!(*cleaned_up.borrow());
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

    #[test]
    fn mutable_requests_frame() {
        clear_composer();
        let m = remember_mutable(|| 0);
        crate::take_frame_request(); // clear any pending request
        assert_eq!(*m.get(), 0);
        assert!(!crate::take_frame_request());

        m.set(1);
        assert!(crate::take_frame_request(), "set must request a frame");
        assert_eq!(*m.get(), 1);

        m.update(|v| *v += 1);
        assert!(crate::take_frame_request(), "update must request a frame");
        assert_eq!(*m.get(), 2);
    }

    #[test]
    fn mutable_set_neq_skips_frame_when_unchanged() {
        clear_composer();
        let m = remember_mutable(|| 5);
        crate::take_frame_request(); // clear any pending request

        m.set_neq(5);
        assert!(
            !crate::take_frame_request(),
            "set_neq with equal value must not request a frame"
        );
        assert_eq!(*m.get(), 5);

        m.set_neq(7);
        assert!(
            crate::take_frame_request(),
            "set_neq with different value must request a frame"
        );
        assert_eq!(*m.get(), 7);
    }

    #[test]
    fn mutable_update_neq_skips_frame_when_unchanged() {
        clear_composer();
        let m = remember_mutable(|| 10);
        crate::take_frame_request(); // clear any pending request

        m.update_neq(|v| *v = 10);
        assert!(
            !crate::take_frame_request(),
            "update_neq that leaves the value equal must not request a frame"
        );

        m.update_neq(|v| *v += 1);
        assert!(
            crate::take_frame_request(),
            "update_neq that changes the value must request a frame"
        );
        assert_eq!(*m.get(), 11);
    }

    #[test]
    fn mutable_with_reads_without_clone() {
        clear_composer();
        let m = remember_mutable(|| 3u64);
        let doubled = m.with(|v| v * 2);
        assert_eq!(doubled, 6);
        assert_eq!(*m.get(), 3);
    }

    #[test]
    fn mutable_keyed_is_stable_across_branches() {
        clear_composer();
        // Same key must return the same instance across calls (conditional
        // branch stability): the second init closure must be ignored.
        let a = remember_mutable_with_key("mv_keyed", || 42);
        let b = remember_mutable_with_key("mv_keyed", || 0);
        a.set(99);
        assert_eq!(*b.get(), 99, "keyed Mutable must be stable across branches");
    }

    fn build_boundary(
        boom: Rc<RefCell<bool>>,
        content_runs: Rc<RefCell<usize>>,
        fallback_calls: Rc<RefCell<usize>>,
        last_reset: Rc<RefCell<Option<Rc<dyn Fn()>>>>,
        use_throw: bool,
    ) -> View {
        ErrorBoundary(
            {
                let fallback_calls = fallback_calls.clone();
                move |_info, reset| {
                    *fallback_calls.borrow_mut() += 1;
                    *last_reset.borrow_mut() = Some(reset);
                    id_view(FALLBACK_ID)
                }
            },
            {
                let content_runs = content_runs.clone();
                move || {
                    *content_runs.borrow_mut() += 1;
                    if *boom.borrow() {
                        if use_throw {
                            throw_boundary("wasm-safe trip");
                        } else {
                            panic!("boom");
                        }
                    }
                    id_view(CONTENT_ID)
                }
            },
        )
    }

    #[test]
    fn error_boundary_sticky_and_reset() {
        clear_composer();
        let boom = Rc::new(RefCell::new(true));
        let content_runs = Rc::new(RefCell::new(0usize));
        let fallback_calls = Rc::new(RefCell::new(0usize));
        let last_reset = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));

        // Frame 1: content panics -> fallback.
        {
            let _g = ComposeGuard::begin();
            let v = build_boundary(
                boom.clone(),
                content_runs.clone(),
                fallback_calls.clone(),
                last_reset.clone(),
                false,
            );
            assert_eq!(v.id, FALLBACK_ID);
            assert_eq!(*fallback_calls.borrow(), 1);
            assert_eq!(*content_runs.borrow(), 1);
        }

        // Frame 2: sticky fallback, content is NOT re-run (no repeat panic).
        {
            let _g = ComposeGuard::begin();
            let v = build_boundary(
                boom.clone(),
                content_runs.clone(),
                fallback_calls.clone(),
                last_reset.clone(),
                false,
            );
            assert_eq!(v.id, FALLBACK_ID);
            assert_eq!(*fallback_calls.borrow(), 2);
            assert_eq!(*content_runs.borrow(), 1);
        }

        // Recover and fire reset: content is re-entered.
        *boom.borrow_mut() = false;
        last_reset.borrow().as_ref().expect("reset captured")();

        {
            let _g = ComposeGuard::begin();
            let v = build_boundary(
                boom.clone(),
                content_runs.clone(),
                fallback_calls.clone(),
                last_reset.clone(),
                false,
            );
            assert_eq!(v.id, CONTENT_ID);
            assert_eq!(*content_runs.borrow(), 2);
        }
    }

    #[test]
    fn error_boundary_throw_boundary_wasm_safe() {
        clear_composer();
        let boom = Rc::new(RefCell::new(true));
        let content_runs = Rc::new(RefCell::new(0usize));
        let fallback_calls = Rc::new(RefCell::new(0usize));
        let last_reset = Rc::new(RefCell::new(None::<Rc<dyn Fn()>>));

        {
            let _g = ComposeGuard::begin();
            let v = build_boundary(
                boom.clone(),
                content_runs.clone(),
                fallback_calls.clone(),
                last_reset.clone(),
                true,
            );
            assert_eq!(v.id, FALLBACK_ID);
            assert_eq!(*content_runs.borrow(), 1);
        }

        // Reset path works for throw_boundary too.
        *boom.borrow_mut() = false;
        last_reset.borrow().as_ref().expect("reset captured")();

        {
            let _g = ComposeGuard::begin();
            let v = build_boundary(
                boom.clone(),
                content_runs.clone(),
                fallback_calls.clone(),
                last_reset.clone(),
                true,
            );
            assert_eq!(v.id, CONTENT_ID);
            assert_eq!(*content_runs.borrow(), 2);
        }
    }
}
