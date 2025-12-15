use parking_lot::RwLock;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub(crate) fn now() -> Instant {
    let lock = CLOCK.get_or_init(|| RwLock::new(Box::new(SystemClock) as Box<dyn Clock>));
    lock.read().now()
}

#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Monotonic, critically-damped, y(t)=1-(1+ω t)e^{-ω t}, t∈[0,1].
    SpringCrit {
        omega: f32,
    },
    /// Underdamped, low-overshoot preset (ζ≈0.5, ω≈8)
    SpringGentle,
    /// Underdamped, bouncier preset (ζ≈0.2, ω≈12)
    SpringBouncy,
}

impl Easing {
    pub fn interpolate(&self, t: f32) -> f32 {
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => t * (2.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    -1.0 + (4.0 - 2.0 * t) * t
                }
            }
            Easing::SpringCrit { omega } => {
                let w = (*omega).max(0.0);
                let tt = t.max(0.0);
                // y = 1 - (1 + w t) e^{-w t}
                1.0 - (1.0 + w * tt) * (-(w * tt)).exp()
            }
            Easing::SpringGentle => spring_underdamped_normalized(t, 0.5, 8.0),
            Easing::SpringBouncy => spring_underdamped_normalized(t, 0.2, 12.0),
        }
    }
}

fn spring_underdamped_normalized(t: f32, zeta: f32, omega: f32) -> f32 {
    let tt = t.max(0.0);
    let z = zeta.clamp(0.0, 0.999);
    let w = omega.max(0.0);
    let wd = w * (1.0 - z * z).sqrt();
    let exp_term = (-z * w * tt).exp();
    let cos_term = (wd * tt).cos();
    let sin_term = (wd * tt).sin();
    // Standard second-order underdamped unit-step response
    let c = z / (1.0 - z * z).sqrt();
    let y = 1.0 - exp_term * (cos_term + c * sin_term);
    y.clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug)]
pub struct AnimationSpec {
    pub duration: Duration,
    pub easing: Easing,
    pub delay: Duration,
}

impl Default for AnimationSpec {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(300),
            easing: Easing::EaseInOut,
            delay: Duration::ZERO,
        }
    }
}

impl AnimationSpec {
    pub fn tween(duration: Duration, easing: Easing) -> Self {
        Self {
            duration,
            easing,
            delay: Duration::ZERO,
        }
    }
    /// Critically-damped monotonic spring (no overshoot).
    pub fn spring_crit(omega: f32, duration: Duration) -> Self {
        Self {
            duration,
            easing: Easing::SpringCrit { omega },
            delay: Duration::ZERO,
        }
    }
    /// Gentle underdamped preset (small overshoot).
    pub fn spring_gentle() -> Self {
        Self {
            duration: Duration::from_millis(450),
            easing: Easing::SpringGentle,
            delay: Duration::ZERO,
        }
    }
    /// Bouncier underdamped preset.
    pub fn spring_bouncy() -> Self {
        Self {
            duration: Duration::from_millis(700),
            easing: Easing::SpringBouncy,
            delay: Duration::ZERO,
        }
    }

    pub fn fast() -> Self {
        Self {
            duration: Duration::from_millis(150),
            easing: Easing::EaseOut,
            delay: Duration::ZERO,
        }
    }

    pub fn slow() -> Self {
        Self {
            duration: Duration::from_millis(600),
            easing: Easing::EaseInOut,
            delay: Duration::ZERO,
        }
    }
}

pub trait Interpolate {
    fn interpolate(&self, other: &Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Interpolate for crate::Color {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        crate::Color(
            (self.0 as f32 + (other.0 as f32 - self.0 as f32) * t) as u8,
            (self.1 as f32 + (other.1 as f32 - self.1 as f32) * t) as u8,
            (self.2 as f32 + (other.2 as f32 - self.2 as f32) * t) as u8,
            (self.3 as f32 + (other.3 as f32 - self.3 as f32) * t) as u8,
        )
    }
}

// Animation clock
pub trait Clock: Send + Sync + 'static {
    fn now(&self) -> Instant;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

static CLOCK: OnceLock<RwLock<Box<dyn Clock>>> = OnceLock::new();

/// Install a global animation clock. Platform sets this to SystemClock; tests can set TestClock.
pub fn set_clock(clock: Box<dyn Clock>) {
    let lock = CLOCK.get_or_init(|| RwLock::new(Box::new(SystemClock) as Box<dyn Clock>));
    *lock.write() = clock;
}
/// Install default system clock if none present (idempotent).
pub fn ensure_system_clock() {
    let _ = CLOCK.get_or_init(|| RwLock::new(Box::new(SystemClock) as Box<dyn Clock>));
}

/// A test clock you can drive deterministically.
#[derive(Clone)]
pub struct TestClock {
    pub t: Instant,
}
impl Clock for TestClock {
    fn now(&self) -> Instant {
        self.t
    }
}

/// Animated value that transitions smoothly
pub struct AnimatedValue<T: Interpolate + Clone> {
    current: T,
    target: T,
    start: T,
    spec: AnimationSpec,
    start_time: Option<Instant>,
}

impl<T: Interpolate + Clone> AnimatedValue<T> {
    pub fn new(initial: T, spec: AnimationSpec) -> Self {
        Self {
            current: initial.clone(),
            target: initial.clone(),
            start: initial,
            spec,
            start_time: None,
        }
    }

    pub fn set_target(&mut self, target: T) {
        if self.start_time.is_some() {
            self.update();
            self.start = self.current.clone();
        } else {
            self.start = self.current.clone();
        }

        self.target = target;
        self.start_time = Some(now());
    }

    pub fn update(&mut self) -> bool {
        if let Some(start) = self.start_time {
            let elapsed = now().saturating_duration_since(start);

            if elapsed < self.spec.delay {
                return true; // Still in delay phase
            }

            let animation_time = elapsed - self.spec.delay;

            if animation_time >= self.spec.duration {
                // Animation complete
                self.current = self.target.clone();
                self.start_time = None;
                return false;
            }

            let t =
                (animation_time.as_secs_f32() / self.spec.duration.as_secs_f32()).clamp(0.0, 1.0);
            let eased_t = self.spec.easing.interpolate(t);

            let eased_t = eased_t.clamp(0.0, 1.0);

            self.current = self.start.interpolate(&self.target, eased_t);
            true
        } else {
            false
        }
    }

    pub fn get(&self) -> &T {
        &self.current
    }

    pub fn is_animating(&self) -> bool {
        self.start_time.is_some()
    }
}
