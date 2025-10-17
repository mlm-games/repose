use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Spring { damping: f32, stiffness: f32 },
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
            Easing::Spring { damping, stiffness } => {
                // Simplified spring physics
                let omega = (stiffness / damping).sqrt();
                let zeta = damping / (2.0 * (stiffness * damping).sqrt());

                if zeta < 1.0 {
                    // Underdamped
                    let omega_d = omega * (1.0 - zeta * zeta).sqrt();
                    let t = t * 2.0; // Adjust time scale
                    1.0 - ((-zeta * omega * t).exp() * (omega_d * t).cos())
                } else {
                    // Overdamped or critically damped - fallback to ease out
                    t * (2.0 - t)
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
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
    pub fn spring() -> Self {
        Self {
            duration: Duration::from_millis(500),
            easing: Easing::Spring {
                damping: 0.8,
                stiffness: 200.0,
            },
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
        if self.start_time.is_none() {
            self.start = self.current.clone();
        }
        self.target = target;
        self.start_time = Some(Instant::now());
    }

    pub fn update(&mut self) -> bool {
        if let Some(start) = self.start_time {
            let elapsed = start.elapsed();

            if elapsed < self.spec.delay {
                return true; // Still waiting for delay
            }

            let animation_time = elapsed - self.spec.delay;

            if animation_time >= self.spec.duration {
                self.current = self.target.clone();
                self.start_time = None;
                return false; // Animation complete
            }

            let t = animation_time.as_secs_f32() / self.spec.duration.as_secs_f32();
            let eased_t = self.spec.easing.interpolate(t);
            self.current = self.start.interpolate(&self.target, eased_t);

            true // Animation ongoing
        } else {
            false // No animation
        }
    }

    pub fn get(&self) -> &T {
        &self.current
    }

    pub fn is_animating(&self) -> bool {
        self.start_time.is_some()
    }
}
