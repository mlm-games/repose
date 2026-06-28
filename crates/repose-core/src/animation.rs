use parking_lot::RwLock;
use std::sync::OnceLock;
use web_time::{Duration, Instant};

use crate::request_frame;

pub(crate) fn now() -> Instant {
    let lock = CLOCK.get_or_init(|| RwLock::new(Box::new(SystemClock) as Box<dyn Clock>));
    lock.read().now()
}

/// Physical spring parameters. Duration is emergent (determined by physics), not specified.
#[derive(Clone, Copy, Debug)]
pub struct SpringSpec {
    /// Damping ratio ζ: 0 = undamped, <1 = underdamped (overshoot), 1 = critically damped,
    /// >1 = overdamped.
    pub damping_ratio: f32,
    /// Stiffness k: higher = faster, snappier response.
    pub stiffness: f32,
    /// Progress threshold for settling: when `|progress - 1.0| < this`, the spring is
    /// considered visually close enough to the target and stops. Default: 0.005 (0.5%).
    pub settle_progress: f32,
    /// Velocity threshold for settling (in progress-units/second). Default: 0.1.
    pub settle_velocity: f32,
}

impl SpringSpec {
    pub const fn new(damping_ratio: f32, stiffness: f32) -> Self {
        Self {
            damping_ratio,
            stiffness,
            settle_progress: 0.005,
            settle_velocity: 0.1,
        }
    }
    /// Gentle preset: low overshoot, moderate speed.
    pub const fn gentle() -> Self {
        Self::new(0.5, 200.0)
    }
    /// Bouncier preset: more overshoot, faster.
    pub const fn bouncy() -> Self {
        Self::new(0.2, 300.0)
    }
    /// Critically damped: no overshoot, fast settle.
    pub const fn crit() -> Self {
        Self::new(1.0, 200.0)
    }
    /// Snappy preset: high damping, high stiffness.
    pub const fn stiff() -> Self {
        Self::new(0.8, 600.0)
    }

    /// Set the settling threshold in progress units. Lower values = more precise settling.
    /// For example, 0.001 means the spring stops when within 0.1% of the target.
    pub const fn with_settle_progress(mut self, threshold: f32) -> Self {
        self.settle_progress = threshold;
        self
    }

    /// Set the velocity threshold for settling (progress-units/second).
    pub const fn with_settle_velocity(mut self, threshold: f32) -> Self {
        self.settle_velocity = threshold;
        self
    }
}

/// A cubic bezier curve with control points (p1x, p1y), (p2x, p2y).
/// P0 = (0, 0) and P3 = (1, 1) are fixed.
#[derive(Clone, Copy, Debug)]
pub struct CubicBezier {
    pub p1x: f32,
    pub p1y: f32,
    pub p2x: f32,
    pub p2y: f32,
}

impl CubicBezier {
    pub const fn new(p1x: f32, p1y: f32, p2x: f32, p2y: f32) -> Self {
        Self { p1x, p1y, p2x, p2y }
    }
}

/// Compose Material3 EmphasizedDecelerate: cubic-bezier(0.05, 0.7, 0.1, 1.0).
pub const EASING_EMPHASIZED_DECELERATE: CubicBezier = CubicBezier::new(0.05, 0.7, 0.1, 1.0);
/// Compose Material3 StandardDecelerate: cubic-bezier(0.2, 0.0, 0.0, 1.0).
pub const EASING_STANDARD_DECELERATE: CubicBezier = CubicBezier::new(0.2, 0.0, 0.0, 1.0);

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
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
    /// Android FastOutSlowIn: cubic-bezier(0.4, 0.0, 0.2, 1.0).
    /// Starts fast, decelerates through the middle, ends slow.
    FastOutSlowIn,
    /// Custom cubic-bezier easing with control points (p1x, p1y), (p2x, p2y).
    Custom(CubicBezier),
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
            Easing::FastOutSlowIn => eval_cubic_bezier(0.4, 0.0, 0.2, 1.0, t),
            Easing::Custom(cb) => eval_cubic_bezier(cb.p1x, cb.p1y, cb.p2x, cb.p2y, t),
        }
    }
}

/// Evaluate a cubic bezier with control points P1=(p1x,p1y), P2=(p2x,p2y)
/// (P0=(0,0) and P3=(1,1) are fixed). Uses Newton's method (5 iterations)
/// to find `u` such that x(u) = t, then returns y(u).
fn eval_cubic_bezier(p1x: f32, p1y: f32, p2x: f32, p2y: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let mut u = t;
    for _ in 0..6 {
        let omu = 1.0 - u;
        let x = 3.0 * omu * omu * u * p1x + 3.0 * omu * u * u * p2x + u * u * u;
        let dx = 3.0 * omu * omu * p1x + 6.0 * omu * u * (p2x - p1x) + 3.0 * u * u * (1.0 - p2x);
        if dx.abs() < 1e-10 {
            break;
        }
        u -= (x - t) / dx;
        u = u.clamp(0.0, 1.0);
    }
    let omu = 1.0 - u;
    3.0 * omu * omu * u * p1y + 3.0 * omu * u * u * p2y + u * u * u
}

/// Cubic Hermite spline interpolation. Given interval width `h`, normalized
/// position `x` in [0,1], endpoint values `y1,y2` and their tangents `t1,t2`,
/// returns the interpolated value.
///
/// Factored to reduce operation count (adapted from Compose's MonoSpline).
fn hermite_interpolate(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    let x3 = x2 * x;
    h * t1 * (x - 2.0 * x2 + x3) + h * t2 * (x3 - x2) + y1 - (3.0 * x2 - 2.0 * x3) * (y1 - y2)
}

/// Derivative of cubic Hermite spline at normalized position `x`.
#[allow(dead_code)]
fn hermite_differential(h: f32, x: f32, y1: f32, y2: f32, t1: f32, t2: f32) -> f32 {
    let x2 = x * x;
    h * (t1 - 2.0 * x * (2.0 * t1 + t2) + 3.0 * (t1 + t2) * x2) - 6.0 * (x - x2) * (y1 - y2)
}

/// A monotone cubic Hermite spline for C1-continuous interpolation of `f32` values.
///
/// Uses the Fritsch–Carlson method to compute tangents that preserve monotonicity
/// and prevent overshoot. Based on Android Compose's `MonoSpline`.
#[derive(Clone, Debug)]
pub struct MonoSpline {
    times: Vec<f32>,
    values: Vec<f32>,
    tangents: Vec<f32>,
}

impl MonoSpline {
    /// Build a spline from keyframe times and values.
    /// Times must be sorted ascending and have at least 2 entries.
    /// Values must have the same length as times.
    pub fn new(times: Vec<f32>, values: Vec<f32>) -> Self {
        assert!(times.len() >= 2, "MonoSpline requires at least 2 keyframes");
        assert_eq!(times.len(), values.len());
        let n = times.len();
        let mut tangents = vec![0.0; n];

        // Compute slopes for each segment
        let mut slopes = vec![0.0; n.saturating_sub(1)];
        for i in 0..n - 1 {
            let dt = times[i + 1] - times[i];
            slopes[i] = (values[i + 1] - values[i]) / dt;
        }

        // Tangents at interior knots: average of adjacent slopes
        tangents[0] = slopes[0];
        for i in 1..n - 1 {
            tangents[i] = (slopes[i - 1] + slopes[i]) * 0.5;
        }
        tangents[n - 1] = slopes[n - 2];

        // Fritsch–Carlson monotonicity preservation
        for i in 0..n - 1 {
            if slopes[i] == 0.0 {
                tangents[i] = 0.0;
                tangents[i + 1] = 0.0;
            } else {
                let a = tangents[i] / slopes[i];
                let b = tangents[i + 1] / slopes[i];
                let h = (a * a + b * b).sqrt();
                if h > 9.0 {
                    let t = 3.0 / h;
                    tangents[i] = t * a * slopes[i];
                    tangents[i + 1] = t * b * slopes[i];
                }
            }
        }

        Self {
            times,
            values,
            tangents,
        }
    }

    /// Evaluate the spline at time `t`.
    /// Clamps `t` to the spline's time range. Extrapolates using the endpoint tangent.
    pub fn evaluate(&self, t: f32) -> f32 {
        let n = self.times.len();
        let first = self.times[0];
        let last = self.times[n - 1];

        if t <= first {
            return self.values[0] + (t - first) * self.tangents[0];
        }
        if t >= last {
            return self.values[n - 1] + (t - last) * self.tangents[n - 1];
        }

        for i in 0..n - 1 {
            if t >= self.times[i] && t <= self.times[i + 1] {
                let h = self.times[i + 1] - self.times[i];
                let x = (t - self.times[i]) / h;
                return hermite_interpolate(
                    h,
                    x,
                    self.values[i],
                    self.values[i + 1],
                    self.tangents[i],
                    self.tangents[i + 1],
                );
            }
        }

        self.values[n - 1] // fallback
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
    /// If set, use true physical spring simulation (duration is ignored, emergent from physics).
    pub spring: Option<SpringSpec>,
    /// If set, wrap the animation in repeat behavior (n iterations, optional ping-pong).
    pub repeat: Option<RepeatableSpec>,
}

impl Default for AnimationSpec {
    fn default() -> Self {
        Self {
            duration: Duration::from_millis(300),
            easing: Easing::EaseInOut,
            delay: Duration::ZERO,
            spring: None,
            repeat: None,
        }
    }
}

impl AnimationSpec {
    pub fn tween(duration: Duration, easing: Easing) -> Self {
        Self {
            duration,
            easing,
            delay: Duration::ZERO,
            spring: None,
            repeat: None,
        }
    }
    /// True physical spring simulation - duration is emergent, no fixed duration needed.
    pub fn spring(spring: SpringSpec) -> Self {
        Self {
            duration: Duration::ZERO,
            easing: Easing::Linear,
            delay: Duration::ZERO,
            spring: Some(spring),
            repeat: None,
        }
    }
    /// Gentle underdamped preset (small overshoot). Uses true spring physics.
    pub fn spring_gentle() -> Self {
        Self::spring(SpringSpec::gentle())
    }
    /// Bouncier underdamped preset. Uses true spring physics.
    pub fn spring_bouncy() -> Self {
        Self::spring(SpringSpec::bouncy())
    }
    /// Critically damped spring with given omega (angular frequency). Uses true spring physics.
    pub fn spring_crit(omega: f32) -> Self {
        Self::spring(SpringSpec::new(1.0, omega * omega))
    }

    pub fn fast() -> Self {
        Self {
            duration: Duration::from_millis(150),
            easing: Easing::EaseOut,
            delay: Duration::ZERO,
            spring: None,
            repeat: None,
        }
    }

    pub fn slow() -> Self {
        Self {
            duration: Duration::from_millis(600),
            easing: Easing::EaseInOut,
            delay: Duration::ZERO,
            spring: None,
            repeat: None,
        }
    }

    /// Wrap this spec in a repeatable animation.
    /// Pass `RepeatableSpec::infinite()` for infinite repeats.
    pub fn repeated(mut self, repeat: RepeatableSpec) -> Self {
        self.repeat = Some(repeat);
        self
    }
}

/// A keyframe animation specification.
///
/// Defines a sequence of keyframes at specific timestamps (0.0 to 1.0),
/// with target values and optional easing between each pair.
#[derive(Clone, Debug)]
pub struct KeyframesSpec<T: Clone> {
    /// Keyframes as (timestamp 0.0-1.0, value, optional easing between previous and this).
    /// The first keyframe should be at t=0.0 and uses no easing.
    pub keyframes: Vec<(f32, T, Option<Easing>)>,
}

impl<T: Clone + Interpolate> KeyframesSpec<T> {
    pub fn new(keyframes: Vec<(f32, T)>) -> Self {
        let with_easing = keyframes.into_iter().map(|(t, v)| (t, v, None)).collect();
        Self {
            keyframes: with_easing,
        }
    }

    /// Add easing between the previous keyframe and this one.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        if let Some(last) = self.keyframes.last_mut() {
            last.2 = Some(easing);
        }
        self
    }

    pub fn evaluate(&self, t: f32) -> T {
        let t = t.clamp(0.0, 1.0);
        let kf = &self.keyframes;
        if kf.is_empty() {
            panic!("KeyframesSpec must have at least one keyframe");
        }

        // Linear interpolation (with per-segment easing)
        for i in 0..kf.len() - 1 {
            let (t0, _, _) = kf[i];
            let (t1, ref v1, easing) = kf[i + 1];
            if t >= t0 && t <= t1 {
                let segment_t = if (t1 - t0).abs() < f32::EPSILON {
                    1.0
                } else {
                    (t - t0) / (t1 - t0)
                };
                let eased_t = match easing {
                    Some(e) => e.interpolate(segment_t),
                    None => segment_t,
                };
                return kf[i].1.interpolate(v1, eased_t);
            }
        }
        kf.last().unwrap().1.clone()
    }
}

/// A keyframe animation specification with smooth cubic Hermite spline interpolation.
///
/// Provides C1 continuity (smooth derivatives at keyframe boundaries),
/// unlike `KeyframesSpec` which uses C0 linear interpolation.
///
/// Uses the Fritsch–Carlson monotonicity-preserving Hermite spline
#[derive(Clone, Debug)]
pub struct SplineKeyframes {
    spline: MonoSpline,
}

impl SplineKeyframes {
    /// Build a spline keyframe from time/value pairs.
    ///
    /// Times should be in [0.0, 1.0] and sorted ascending.
    /// At least 2 keyframes are required.
    pub fn new(keyframes: Vec<(f32, f32)>) -> Self {
        assert!(
            keyframes.len() >= 2,
            "SplineKeyframes requires at least 2 keyframes"
        );
        let times: Vec<f32> = keyframes.iter().map(|(t, _)| *t).collect();
        let values: Vec<f32> = keyframes.iter().map(|(_, v)| *v).collect();
        Self {
            spline: MonoSpline::new(times, values),
        }
    }

    /// Evaluate the spline at normalized time `t` (0.0 to 1.0).
    pub fn evaluate(&self, t: f32) -> f32 {
        self.spline.evaluate(t.clamp(0.0, 1.0))
    }
}

/// A repeatable animation specification.
///
/// Wraps another animation spec and causes it to repeat.
/// Default: infinite repeat with no reverse.
#[derive(Clone, Copy, Debug)]
pub struct RepeatableSpec {
    /// Number of repetitions. `None` means infinite.
    pub iterations: Option<u32>,
    /// If true, alternate direction each iteration (forward, backward, forward...).
    pub reverse: bool,
    /// Delay between each iteration.
    pub delay_between: Duration,
}

impl Default for RepeatableSpec {
    fn default() -> Self {
        Self {
            iterations: None,
            reverse: false,
            delay_between: Duration::ZERO,
        }
    }
}

impl RepeatableSpec {
    pub fn new(iterations: u32) -> Self {
        Self {
            iterations: Some(iterations),
            reverse: false,
            delay_between: Duration::ZERO,
        }
    }

    pub fn infinite() -> Self {
        Self {
            iterations: None,
            reverse: false,
            delay_between: Duration::ZERO,
        }
    }

    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    pub fn delay_between(mut self, d: Duration) -> Self {
        self.delay_between = d;
        self
    }
}

/// Decay animation configuration.
///
/// Models a damped decay (e.g., for fling-to-stop animations).
#[derive(Clone, Copy, Debug)]
pub struct DecayAnimationSpec {
    /// How quickly the animation decelerates. Lower = faster stop.
    pub friction: f32,
    /// Minimum velocity threshold to stop.
    pub stop_threshold: f32,
}

impl Default for DecayAnimationSpec {
    fn default() -> Self {
        Self {
            friction: 0.8,
            stop_threshold: 1.0,
        }
    }
}

impl DecayAnimationSpec {
    pub fn new(friction: f32) -> Self {
        Self {
            friction: friction.clamp(0.01, 1.0),
            stop_threshold: 1.0,
        }
    }
}

impl AnimatedValue<f32> {
    /// Tick the decay animation. Returns `true` if still animating.
    pub fn update_decay(&mut self, friction: f32, stop_threshold: f32) -> bool {
        let _start = match self.start_time {
            Some(s) => s,
            None => return false,
        };

        let now = now();
        let dt = match self.last_update {
            Some(last) => now.saturating_duration_since(last).as_secs_f32().min(0.05),
            None => 0.0,
        };
        self.last_update = Some(now);

        if dt <= 0.0 {
            return true;
        }

        if self.velocity.abs() < stop_threshold {
            self.velocity = 0.0;
            self.start_time = None;
            return false;
        }

        self.velocity *= friction.powf(dt * 60.0);
        let delta = self.velocity * dt;
        // We store the "current value" as a single f32 offset
        // that accumulates. But AnimatedValue<f32> stores explicit
        // start/target. For decay we just accumulate the current.
        // Because of the AnimatedValue structure, we use progress as
        // the accumulated value relative to start.
        let new_progress = self.progress + delta;
        self.progress = new_progress;
        // current = start + (target - start) * progress but target = ???.
        // For decay, progress IS the value (starting from 0).
        // We repurpose: current = start + progress (progress is offset from start).
        // Since T = f32, we can just set current directly.
        if self.progress.abs() < 0.001 && self.velocity.abs() < stop_threshold {
            self.progress = 0.0;
            self.velocity = 0.0;
            self.start_time = None;
            return false;
        }

        self.current = self.start.interpolate(&self.target, self.progress);
        true
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
        let lerp = |a: u8, b: u8| {
            (a as f32 + (b as f32 - a as f32) * t)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        crate::Color(
            lerp(self.0, other.0),
            lerp(self.1, other.1),
            lerp(self.2, other.2),
            lerp(self.3, other.3),
        )
    }
}

impl Interpolate for crate::Vec2 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        crate::Vec2 {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
        }
    }
}

impl Interpolate for crate::Size {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        crate::Size {
            width: self.width.interpolate(&other.width, t),
            height: self.height.interpolate(&other.height, t),
        }
    }
}

impl Interpolate for crate::Rect {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        crate::Rect {
            x: self.x.interpolate(&other.x, t),
            y: self.y.interpolate(&other.y, t),
            w: self.w.interpolate(&other.w, t),
            h: self.h.interpolate(&other.h, t),
        }
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

/// Animated value that transitions smoothly.
///
/// Supports two modes:
/// - **Tween** (when `spec.spring` is `None`): interpolates between `start` and `target`
///   over a fixed duration using an easing curve.
/// - **Spring** (when `spec.spring` is `Some`): numerically integrates a physical spring ODE
///   (`x'' = -k·(x - target) - d·x'`) with emergent duration. When the target changes
///   mid-animation, the current value and velocity carry forward seamlessly.
pub struct AnimatedValue<T: Interpolate + Clone> {
    current: T,
    target: T,
    start: T,
    spec: AnimationSpec,
    keyframes: Option<KeyframesSpec<T>>,
    iteration: u32,
    start_time: Option<Instant>,
    // Spring simulation state (progress-based, works for any T: Interpolate)
    progress: f32,
    velocity: f32,
    last_update: Option<Instant>,
}

impl<T: Interpolate + Clone> AnimatedValue<T> {
    pub fn new(initial: T, spec: AnimationSpec) -> Self {
        Self {
            current: initial.clone(),
            target: initial.clone(),
            start: initial,
            spec,
            keyframes: None,
            iteration: 0,
            start_time: None,
            progress: 1.0,
            velocity: 0.0,
            last_update: None,
        }
    }

    pub fn set_spec(&mut self, spec: AnimationSpec) {
        self.spec = spec;
    }

    /// Set a keyframes spec for multi-stage animation.
    /// When set, `set_target` is ignored and the value is driven by the keyframe sequence.
    pub fn set_keyframes(&mut self, keyframes: KeyframesSpec<T>) {
        self.keyframes = Some(keyframes);
        self.start_time = Some(now());
        self.last_update = None;
        self.iteration = 0;
    }

    pub fn set_target(&mut self, target: T) {
        if self.start_time.is_some() {
            self.update();
        }
        self.keyframes = None;
        self.start = self.current.clone();
        self.target = target;
        self.start_time = Some(now());
        self.last_update = None;
        self.iteration = 0;
        if self.spec.spring.is_some() {
            // Spring mode: start progress at 0 (the current value), carry velocity forward
            self.progress = 0.0;
        }
    }

    /// Snap immediately to a value without animating.
    pub fn snap_to(&mut self, value: T) {
        self.current = value.clone();
        self.target = value.clone();
        self.start = value;
        self.keyframes = None;
        self.start_time = None;
        self.progress = 1.0;
        self.velocity = 0.0;
        self.last_update = None;
    }

    pub fn update(&mut self) -> bool {
        let spring_spec = self.spec.spring;
        let mut still = if let Some(spring) = spring_spec {
            self.update_spring(&spring)
        } else if self.keyframes.is_some() {
            self.update_keyframes()
        } else {
            self.update_tween()
        };

        if !still {
            // Check if we should repeat
            if let Some(repeat) = &self.spec.repeat {
                let maxed = repeat
                    .iterations
                    .is_some_and(|max| self.iteration + 1 >= max);
                if !maxed {
                    self.iteration += 1;
                    if repeat.reverse {
                        std::mem::swap(&mut self.start, &mut self.target);
                    }
                    self.progress = 0.0;
                    self.velocity = 0.0;
                    self.start_time = Some(now());
                    self.last_update = None;
                    still = true;
                }
            }
        }

        if still {
            request_frame();
            crate::signal_fired();
        }
        still
    }

    fn update_keyframes(&mut self) -> bool {
        let start = match self.start_time {
            Some(s) => s,
            None => return false,
        };
        let elapsed = now().saturating_duration_since(start);
        if elapsed < self.spec.delay {
            return true;
        }
        let animation_time = elapsed - self.spec.delay;
        if animation_time >= self.spec.duration {
            if let Some(ref kf) = self.keyframes {
                self.current = kf.evaluate(1.0);
            }
            self.start_time = None;
            return false;
        }
        let t = (animation_time.as_secs_f32() / self.spec.duration.as_secs_f32()).clamp(0.0, 1.0);
        let eased_t = self.spec.easing.interpolate(t).clamp(0.0, 1.0);
        if let Some(ref kf) = self.keyframes {
            self.current = kf.evaluate(eased_t);
        }
        true
    }

    fn update_spring(&mut self, spring: &SpringSpec) -> bool {
        let start = match self.start_time {
            Some(s) => s,
            None => return false,
        };

        let now = now();
        let dt = match self.last_update {
            Some(last) => now.saturating_duration_since(last).as_secs_f32().min(0.05),
            None => 0.0,
        };
        self.last_update = Some(now);

        // Still in delay phase
        let elapsed = now.saturating_duration_since(start);
        if elapsed < self.spec.delay {
            return true;
        }

        if dt <= 0.0 {
            return true;
        }

        // Spring ODE (progress-based, target progress = 1.0)
        let k = spring.stiffness;
        let d = 2.0 * spring.damping_ratio * k.sqrt();
        let displacement = self.progress - 1.0;

        if displacement.abs() < spring.settle_progress
            && self.velocity.abs() < spring.settle_velocity
        {
            // Settled
            self.progress = 1.0;
            self.velocity = 0.0;
            self.current = self.target.clone();
            self.start_time = None;
            self.last_update = None;
            return false;
        }

        // Semi-implicit Euler (symplectic integrator, more stable than explicit)
        let acceleration = -k * displacement - d * self.velocity;
        self.velocity += acceleration * dt;
        self.progress += self.velocity * dt;

        // Clamp progress to prevent extreme overshoot
        self.progress = self.progress.clamp(-0.1, 2.0);

        self.current = self.start.interpolate(&self.target, self.progress);
        true
    }

    fn update_tween(&mut self) -> bool {
        if let Some(start) = self.start_time {
            let elapsed = now().saturating_duration_since(start);

            if elapsed < self.spec.delay {
                return true;
            }

            let animation_time = elapsed - self.spec.delay;

            if animation_time >= self.spec.duration {
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

    pub fn has_keyframes(&self) -> bool {
        self.keyframes.is_some()
    }
}
