//! Gamepad hardware backends.
//!
//! Platform-agnostic types live in [`repose_core::input`] (`GamepadEvent`,
//! `GamepadButton`, `GamepadAxis`); UI routing lives in
//! [`repose_app::ReposeRuntime::handle_gamepad`]. This module only polls
//! hardware into events. Backends implement [`GamepadBackend`]:
//! - desktop (Linux/macOS/Windows) and web: gilrs driver (evdev / HID /
//!   XInput-WGI / Web Gamepad API), `gamepad` feature.
//! - android: stub (no backend in gilrs atm).

use repose_core::input::{GamepadAxis, GamepadButton, GamepadEvent, GamepadId};

/// Hardware poller: drain pending events since the last call.
pub trait GamepadBackend {
    fn poll(&mut self) -> Vec<GamepadEvent>;
}

/// Stick deadzone applied by all backends before emitting axis events.
pub const STICK_DEADZONE: f32 = 0.2;

pub(crate) fn apply_stick_deadzone(v: f32) -> f32 {
    if v.abs() < STICK_DEADZONE {
        0.0
    } else {
        v.signum() * (v.abs() - STICK_DEADZONE) / (1.0 - STICK_DEADZONE)
    }
}

/// Desktop backend driven by gilrs (its mapping database is the reference
/// implementation; repose owns the types and routing above it).
#[cfg(all(feature = "gamepad", not(target_os = "android")))]
pub struct GilrsBackend {
    gilrs: gilrs::Gilrs,
}

#[cfg(all(feature = "gamepad", not(target_os = "android")))]
impl GilrsBackend {
    pub fn new() -> Option<Self> {
        match gilrs::Gilrs::new() {
            Ok(gilrs) => Some(Self { gilrs }),
            Err(e) => {
                log::warn!("gamepad: gilrs init failed ({e}); gamepad input disabled");
                None
            }
        }
    }

    fn map_button(b: gilrs::Button) -> Option<GamepadButton> {
        use gilrs::Button as G;
        Some(match b {
            G::South => GamepadButton::South,
            G::East => GamepadButton::East,
            G::West => GamepadButton::West,
            G::North => GamepadButton::North,
            G::Start => GamepadButton::Start,
            G::Select => GamepadButton::Select,
            G::LeftTrigger => GamepadButton::LeftShoulder,
            G::RightTrigger => GamepadButton::RightShoulder,
            G::LeftThumb => GamepadButton::LeftStick,
            G::RightThumb => GamepadButton::RightStick,
            G::DPadUp => GamepadButton::DPadUp,
            G::DPadDown => GamepadButton::DPadDown,
            G::DPadLeft => GamepadButton::DPadLeft,
            G::DPadRight => GamepadButton::DPadRight,
            _ => return None,
        })
    }

    fn map_axis(a: gilrs::Axis) -> Option<GamepadAxis> {
        use gilrs::Axis as G;
        Some(match a {
            G::LeftStickX => GamepadAxis::LeftStickX,
            G::LeftStickY => GamepadAxis::LeftStickY,
            G::RightStickX => GamepadAxis::RightStickX,
            G::RightStickY => GamepadAxis::RightStickY,
            _ => return None,
        })
    }
}

#[cfg(all(feature = "gamepad", not(target_os = "android")))]
impl GamepadBackend for GilrsBackend {
    fn poll(&mut self) -> Vec<GamepadEvent> {
        use gilrs::EventType as E;
        let mut out = Vec::new();
        while let Some(ev) = self.gilrs.next_event() {
            let id = GamepadId(usize::from(ev.id) as u32);
            match ev.event {
                E::Connected => {
                    let name = self.gilrs.gamepad(ev.id).name().to_string();
                    out.push(GamepadEvent::Connected { id, name });
                }
                E::Disconnected => out.push(GamepadEvent::Disconnected { id }),
                E::ButtonPressed(b, _) | E::ButtonRepeated(b, _) => {
                    if let Some(button) = Self::map_button(b) {
                        out.push(GamepadEvent::Button {
                            id,
                            button,
                            pressed: true,
                        });
                    }
                }
                E::ButtonReleased(b, _) => {
                    if let Some(button) = Self::map_button(b) {
                        out.push(GamepadEvent::Button {
                            id,
                            button,
                            pressed: false,
                        });
                    }
                }
                E::ButtonChanged(b, v, _) => {
                    let axis = match b {
                        gilrs::Button::LeftTrigger2 => Some(GamepadAxis::LeftTrigger),
                        gilrs::Button::RightTrigger2 => Some(GamepadAxis::RightTrigger),
                        _ => None,
                    };
                    if let Some(axis) = axis {
                        out.push(GamepadEvent::Axis {
                            id,
                            axis,
                            value: v.clamp(0.0, 1.0),
                        });
                    }
                }
                E::AxisChanged(a, v, _) => {
                    if let Some(axis) = Self::map_axis(a) {
                        out.push(GamepadEvent::Axis {
                            id,
                            axis,
                            value: apply_stick_deadzone(v),
                        });
                    }
                }
                E::Dropped | E::ForceFeedbackEffectCompleted => {}
                _ => {}
            }
        }
        out
    }
}

/// Create the platform backend, or `None` when the `gamepad` feature is off
/// or no backend exists for this target (android/web land on this trait next).
pub fn create_backend() -> Option<impl GamepadBackend> {
    #[cfg(all(
        feature = "gamepad",
        not(target_os = "android"),
        not(target_arch = "wasm32")
    ))]
    {
        GilrsBackend::new()
    }
    #[cfg(not(all(feature = "gamepad", not(target_os = "android"))))]
    {
        None::<NoBackend>
    }
}

/// Placeholder backend for targets without a driver yet.
pub struct NoBackend;

impl GamepadBackend for NoBackend {
    fn poll(&mut self) -> Vec<GamepadEvent> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_deadzone_snaps_and_rescales() {
        assert_eq!(apply_stick_deadzone(0.0), 0.0);
        assert_eq!(apply_stick_deadzone(0.19), 0.0);
        assert_eq!(apply_stick_deadzone(-0.19), 0.0);
        assert_eq!(apply_stick_deadzone(1.0), 1.0);
        assert_eq!(apply_stick_deadzone(-1.0), -1.0);
        let mid = apply_stick_deadzone(0.6);
        assert!((mid - 0.5).abs() < 1e-6);
    }
}
