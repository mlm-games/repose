//! Gamepad hardware backends.
//!
//! Platform-agnostic types live in [`repose_core::input`] (`GamepadEvent`,
//! `GamepadButton`, `GamepadAxis`); UI routing lives in
//! [`repose_app::ReposeRuntime::handle_gamepad`]. This module only feeds
//! hardware into events. Backends implement [`GamepadBackend`]:
//! - desktop (Linux/macOS/Windows) and web: gilrs driver (evdev / HID /
//!   XInput-WGI / Web Gamepad API), `gamepad` feature.
//! - android: [`AndroidBackend`] (non-joystick).

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

/// Create the platform backend, or `None` when the `gamepad` feature is off.
///
/// One labeled arm per target (see module docs for the gilrs-Android swap).
pub fn create_backend() -> Option<impl GamepadBackend> {
    // Android: native-keycode backend (buttons via winit key path).
    #[cfg(all(feature = "gamepad", target_os = "android"))]
    {
        AndroidBackend::new()
    }
    // Desktop + web: gilrs driver.
    #[cfg(all(feature = "gamepad", not(target_os = "android")))]
    {
        GilrsBackend::new()
    }
    #[cfg(not(feature = "gamepad"))]
    {
        None::<NoBackend>
    }
}

/// Android backend constructor with a concrete return type (for runners
/// that need [`AndroidBackend::key_button`], which is not on the trait).
#[cfg(all(feature = "gamepad", target_os = "android"))]
pub fn create_android_backend() -> Option<AndroidBackend> {
    AndroidBackend::new()
}

/// Android controller backend: buttons arrive as native keycodes through
/// winit (`Key::Unidentified(NativeKeyCode::Android(code))`  - winit maps
/// `AKEYCODE_BUTTON_*` there deliberately), so there is nothing to poll;
/// [`AndroidBackend::key_button`] translates at the key-event site.
/// Sticks/triggers need a future Paddleboat/JNI driver on this trait.
#[cfg(all(feature = "gamepad", target_os = "android"))]
pub struct AndroidBackend {
    connected: bool,
}

#[cfg(all(feature = "gamepad", target_os = "android"))]
impl AndroidBackend {
    pub fn new() -> Option<Self> {
        Some(Self { connected: false })
    }

    /// Translate a native Android keycode press/release into gamepad events.
    /// Emits a synthetic `Connected` (virtual pad id 0) on first sight.
    /// NOTE: Since Android offers no hotplug event through winit, it returns empty
    /// for non-controller codes so callers can fall through to keyboard.
    pub fn key_button(&mut self, code: u32, pressed: bool) -> Vec<GamepadEvent> {
        let Some(button) = android_code_to_button(code) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(2);
        if !self.connected {
            self.connected = true;
            out.push(GamepadEvent::Connected {
                id: GamepadId(0),
                name: "Android controller".to_string(),
            });
        }
        out.push(GamepadEvent::Button {
            id: GamepadId(0),
            button,
            pressed,
        });
        out
    }
}

#[cfg(all(feature = "gamepad", target_os = "android"))]
impl GamepadBackend for AndroidBackend {
    fn poll(&mut self) -> Vec<GamepadEvent> {
        Vec::new()
    }
}

#[cfg(feature = "gamepad")]
pub fn android_code_to_button(code: u32) -> Option<GamepadButton> {
    Some(match code {
        19 => GamepadButton::DPadUp,
        20 => GamepadButton::DPadDown,
        21 => GamepadButton::DPadLeft,
        22 => GamepadButton::DPadRight,
        23 => GamepadButton::South,          // DPAD_CENTER
        96 => GamepadButton::South,          // BUTTON_A
        97 => GamepadButton::East,           // BUTTON_B
        99 => GamepadButton::West,           // BUTTON_X
        100 => GamepadButton::North,         // BUTTON_Y
        102 => GamepadButton::LeftShoulder,  // BUTTON_L1
        103 => GamepadButton::RightShoulder, // BUTTON_R1
        106 => GamepadButton::LeftStick,     // BUTTON_THUMBL
        107 => GamepadButton::RightStick,    // BUTTON_THUMBR
        108 => GamepadButton::Start,         // BUTTON_START
        109 => GamepadButton::Select,        // BUTTON_SELECT
        _ => return None,
    })
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

    #[cfg(feature = "gamepad")]
    #[test]
    fn android_codes_map_to_standard_layout() {
        assert_eq!(android_code_to_button(96), Some(GamepadButton::South));
        assert_eq!(android_code_to_button(97), Some(GamepadButton::East));
        assert_eq!(android_code_to_button(99), Some(GamepadButton::West));
        assert_eq!(android_code_to_button(100), Some(GamepadButton::North));
        assert_eq!(
            android_code_to_button(102),
            Some(GamepadButton::LeftShoulder)
        );
        assert_eq!(android_code_to_button(108), Some(GamepadButton::Start));
        assert_eq!(android_code_to_button(19), Some(GamepadButton::DPadUp));
        assert_eq!(android_code_to_button(23), Some(GamepadButton::South));
        // Non-controller codes fall through to keyboard.
        assert_eq!(android_code_to_button(29), None); // KEYCODE_A
        assert_eq!(android_code_to_button(98), None); // BUTTON_C
        assert_eq!(android_code_to_button(110), None); // BUTTON_MODE
    }
}
