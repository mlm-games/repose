#[cfg(test)]
mod tests {
    use crate::COMPOSER;
    use crate::Color;
    use crate::Rect;
    use crate::Vec2;
    use crate::animation::*;
    use crate::remember_with_key;
    use crate::scope::*;
    use crate::signal::*;
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
        COMPOSER.with(|c| c.borrow_mut().keyed_slots.clear());

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
