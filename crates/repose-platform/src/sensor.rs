//! Motion-sensor hardware backends (gyroscope / accelerometer).
//!
//! Separate from [`crate::gamepad`] on purpose: a sensor driver reads devices
//! only for their IMU, leaving buttons, axes and rumble to the gamepad
//! backend. Keeping the traits apart means no driver has to claim both.
//!
//! Linux reads `/dev/input/event*` directly (`evdev`, `sensors` feature),
//! because motion never reaches gilrs. The input protocol gives it a precise
//! shape: a node flagged `INPUT_PROP_ACCELEROMETER` reports accelerometer data
//! on its directional axes (`ABS_X/Y/Z`) and gyroscope data on its rotational
//! axes (`ABS_RX/RY/RZ`), and must not mix them with regular stick axes. Each
//! axis `AbsInfo` resolution is counts per unit, so `value / resolution` is g
//! and degrees per second.
//!
//! Every mainline driver (hid-steam, hid-playstation, hid-nintendo,
//! hid-sony) puts motion on its own node named after the pad, so readings
//! carry the pad's name for the runner to resolve to a
//! [`GamepadId`](repose_core::input::GamepadId).

use repose_core::input::SensorSample;

#[cfg(all(feature = "sensors", target_os = "linux"))]
mod evdev;
#[cfg(all(feature = "sensors", target_os = "linux"))]
pub use evdev::{EvdevSensorBackend, MotionScale};

/// One reading tagged with the hardware device that produced it. The runner
/// resolves `device` against the connected pads' names, so backends never
/// assign pad ids themselves.
#[derive(Clone, Debug, PartialEq)]
pub struct SensorReading {
    pub device: String,
    pub sample: SensorSample,
}

/// Hardware poller for motion sensors. Default: no sensors.
pub trait SensorBackend {
    fn poll_sensors(&mut self) -> Vec<SensorReading> {
        Vec::new()
    }
}

/// Placeholder backend for targets without a motion driver yet.
pub struct NoSensorBackend;

impl SensorBackend for NoSensorBackend {}

/// Create the platform sensor backend, or `None` when `sensors` is off or the
/// target has no motion driver yet.
pub fn create_backend() -> Option<impl SensorBackend> {
    #[cfg(all(feature = "sensors", target_os = "linux"))]
    {
        Some(EvdevSensorBackend::new())
    }
    #[cfg(any(not(feature = "sensors"), not(target_os = "linux")))]
    {
        None::<NoSensorBackend>
    }
}
