//! Browser gamepad polling: the Web Gamepad API, read once per frame.
//!
//! The API has no event stream the way the desktop backends have, so the pads are
//! sampled and the difference against the last sample becomes the event stream.

use std::collections::HashMap;

use repose_core::input::{GamepadAxis, GamepadButton, GamepadEvent, GamepadId};
use wasm_bindgen::JsCast;
use web_sys::{Gamepad, GamepadButton as WebGamepadButton};

const BUTTON_COUNT: usize = 16;
const AXIS_COUNT: usize = 6;

/// The standard mapping's button indices. The two trigger buttons (6 and 7) are
/// left out: the triggers report their level on the axes, and a digital reading of
/// them would fire the shoulders a second time.
const BUTTONS: [(usize, GamepadButton); 14] = [
    (0, GamepadButton::South),
    (1, GamepadButton::East),
    (2, GamepadButton::West),
    (3, GamepadButton::North),
    (4, GamepadButton::LeftShoulder),
    (5, GamepadButton::RightShoulder),
    (8, GamepadButton::Select),
    (9, GamepadButton::Start),
    (10, GamepadButton::LeftStick),
    (11, GamepadButton::RightStick),
    (12, GamepadButton::DPadUp),
    (13, GamepadButton::DPadDown),
    (14, GamepadButton::DPadLeft),
    (15, GamepadButton::DPadRight),
];

/// The standard mapping's axis indices. The two triggers report 0.0..=1.0.
const AXES: [(usize, GamepadAxis); 6] = [
    (0, GamepadAxis::LeftStickX),
    (1, GamepadAxis::LeftStickY),
    (2, GamepadAxis::RightStickX),
    (3, GamepadAxis::RightStickY),
    (4, GamepadAxis::LeftTrigger),
    (5, GamepadAxis::RightTrigger),
];

#[derive(Clone)]
struct PadState {
    browser_id: String,
    buttons: [bool; BUTTON_COUNT],
    axes: [f32; AXIS_COUNT],
}

impl Default for PadState {
    fn default() -> Self {
        Self {
            browser_id: String::new(),
            buttons: [false; BUTTON_COUNT],
            axes: [0.0; AXIS_COUNT],
        }
    }
}

/// The browser's pads, sampled per frame. The browser's own gamepad index is the
/// backend id, so a pad keeps its id for as long as it stays plugged in.
#[derive(Default)]
pub struct WebGamepads {
    pads: HashMap<u32, PadState>,
}

impl WebGamepads {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sample the pads and return what changed since the last sample.
    pub fn poll(&mut self) -> Vec<GamepadEvent> {
        let mut events = Vec::new();
        let Some(navigator) = web_sys::window().map(|w| w.navigator()) else {
            return events;
        };
        let Ok(pads) = navigator.get_gamepads() else {
            return events;
        };
        let mut live = Vec::new();

        for index in 0..pads.length() {
            let value = pads.get(index);
            let Ok(pad) = value.dyn_into::<Gamepad>() else {
                continue;
            };
            if !pad.connected() {
                continue;
            }
            let id = index as u32;
            live.push(id);
            let mut state = read_pad(&pad);
            state.browser_id = pad.id();

            // A different device can appear at the same browser index. Treat
            // that as a disconnect/connect pair instead of inheriting the old
            // pad's button and axis state.
            if self
                .pads
                .get(&id)
                .is_some_and(|previous| previous.browser_id != state.browser_id)
            {
                self.pads.remove(&id);
                events.push(GamepadEvent::Disconnected { id: GamepadId(id) });
            }

            let Some(previous) = self.pads.get(&id).cloned() else {
                self.pads.insert(id, state.clone());
                events.push(GamepadEvent::Connected {
                    id: GamepadId(id),
                    name: pad.id(),
                    vendor_id: 0,
                    product_id: 0,
                });
                for mapping in BUTTONS.iter() {
                    if state.buttons[mapping.0] {
                        events.push(GamepadEvent::Button {
                            id: GamepadId(id),
                            button: mapping.1,
                            pressed: true,
                        });
                    }
                }
                for mapping in AXES.iter() {
                    if state.axes[mapping.0] != 0.0 {
                        events.push(GamepadEvent::Axis {
                            id: GamepadId(id),
                            axis: mapping.1,
                            value: state.axes[mapping.0],
                        });
                    }
                }
                continue;
            };

            for mapping in BUTTONS.iter() {
                if state.buttons[mapping.0] != previous.buttons[mapping.0] {
                    events.push(GamepadEvent::Button {
                        id: GamepadId(id),
                        button: mapping.1,
                        pressed: state.buttons[mapping.0],
                    });
                }
            }
            for mapping in AXES.iter() {
                if (state.axes[mapping.0] - previous.axes[mapping.0]).abs() > 1e-3 {
                    events.push(GamepadEvent::Axis {
                        id: GamepadId(id),
                        axis: mapping.1,
                        value: state.axes[mapping.0],
                    });
                }
            }
            self.pads.insert(id, state);
        }

        let gone: Vec<u32> = self
            .pads
            .keys()
            .copied()
            .filter(|id| !live.contains(id))
            .collect();
        for id in gone {
            self.pads.remove(&id);
            events.push(GamepadEvent::Disconnected { id: GamepadId(id) });
        }

        events
    }
}

fn read_pad(pad: &Gamepad) -> PadState {
    let mut state = PadState::default();
    let buttons = pad.buttons();
    for slot in 0..BUTTON_COUNT.min(buttons.length() as usize) {
        let value = buttons.get(slot as u32);
        let Ok(button) = value.dyn_into::<WebGamepadButton>() else {
            continue;
        };
        state.buttons[slot] = button.pressed();
    }
    let axes = pad.axes();
    for slot in 0..AXIS_COUNT.min(axes.length() as usize) {
        let value = axes.get(slot as u32);
        state.axes[slot] = value.as_f64().unwrap_or(0.0) as f32;
    }
    state
}
