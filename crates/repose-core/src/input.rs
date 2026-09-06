use crate::Vec2;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
}

#[derive(Clone, Copy, Debug)]
pub enum PointerButton {
    Primary,   // Left mouse, touch
    Secondary, // Right mouse
    Tertiary,  // Middle mouse
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEventPass {
    /// Top-down pass: ancestor -> descendant. Allows ancestors to preview or
    /// intercept events before descendants see them.
    Initial,
    /// Bottom-up pass: descendant -> ancestor. The primary pass where gesture
    /// handlers react to and consume events. A child that consumes its event
    /// prevents the parent from reacting (Compose's requireUnconsumed).
    Main,
    /// Top-down pass: ancestor -> descendant. Allows descendants to learn
    /// about events consumed by ancestors during the Main pass.
    Final,
}

#[derive(Clone, Copy, Debug)]
pub enum PointerEventKind {
    Down(PointerButton),
    Up(PointerButton),
    Move,
    Cancel,
    Enter,
    Leave,
}

#[derive(Clone, Debug)]
pub struct PointerEvent {
    pub id: PointerId,
    pub kind: PointerKind,
    pub event: PointerEventKind,
    /// Position relative to `origin` (hit-region top-left), in physical px.
    pub position: Vec2,
    /// Top-left of the hit region this event is being delivered to (physical px).
    pub origin: Vec2,
    pub pressure: f32,
    pub modifiers: Modifiers,
    /// Shared consumed state -> every clone of this event points to the same
    /// Cell. Calling `consume()` on any clone marks it consumed for all clones.
    pub consumed: Rc<Cell<bool>>,
}

impl PointerEvent {
    pub fn new(
        id: PointerId,
        kind: PointerKind,
        event: PointerEventKind,
        position: Vec2,
        pressure: f32,
        modifiers: Modifiers,
    ) -> Self {
        Self {
            id,
            kind,
            event,
            position,
            origin: Vec2::ZERO,
            pressure,
            modifiers,
            consumed: Rc::new(Cell::new(false)),
        }
    }

    /// Absolute position in window/surface physical pixels.
    pub fn position_in_window(&self) -> Vec2 {
        self.position + self.origin
    }

    /// Mark this event as consumed. Once consumed, subsequent handlers in the
    /// same pass should skip processing it (equivalent to Compose's
    /// `PointerInputChange.consume()`).
    pub fn consume(&self) {
        self.consumed.set(true);
    }

    /// Returns `true` if `consume()` was called on this event or any clone of it.
    pub fn is_consumed(&self) -> bool {
        self.consumed.get()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,    // Cmd on Mac, Win key on Windows
    pub command: bool, // egui like (Cmd on macOS, Ctrl elsewhere)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Character(char),
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    F(u8), // F1-F12
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyEventType {
    /// Key pressed down.
    Down,
    /// Key released.
    Up,
    /// Unknown or unsupported event type.
    Unknown,
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub is_repeat: bool,
    /// Whether this is a key-down or key-up event.
    pub event_type: KeyEventType,
    /// UTF-16 code point for character keys, or 0 for non-characters.
    /// Matches Compose's `utf16CodePoint`.
    pub utf16_code_point: u16,
}

#[derive(Clone, Debug)]
pub struct TextInputEvent {
    pub text: String,
}

#[derive(Clone, Debug)]
pub enum ImeEvent {
    /// IME composition started
    Start,
    /// Composition text updated
    Update {
        text: String,
        cursor: Option<(usize, usize)>, // (start, end) of composition range
    },
    /// Composition committed (finalized)
    Commit(String),
    /// Composition cancelled
    Cancel,
}

#[derive(Clone, Debug)]
pub enum InputEvent {
    Pointer(PointerEvent),
    Key(KeyEvent),
    Text(TextInputEvent),
    Ime(ImeEvent),
    Gamepad(GamepadEvent),
}

/// Opaque gamepad handle. Backend-local index, stable for the connection
/// lifetime. Survives across frames; invalid after [`GamepadEvent::Disconnected`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GamepadId(pub u32);

/// Standard-layout buttons (SDL gamecontroller mapping positions).
/// Backends translate hardware codes to these; unknown buttons are dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    /// Bottom face button (A / Cross). UI default: activate.
    South,
    /// Right face button (B / Circle). UI default: back.
    East,
    /// Left face button (X / Square).
    West,
    /// Top face button (Y / Triangle).
    North,
    Start,
    Select,
    LeftShoulder,
    RightShoulder,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

/// Analog axes, normalized to -1.0..=1.0. Triggers report 0.0..=1.0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    RightStickX,
    RightStickY,
    LeftTrigger,
    RightTrigger,
}

#[derive(Clone, Debug)]
pub enum GamepadEvent {
    Connected {
        id: GamepadId,
        name: String,
    },
    Disconnected {
        id: GamepadId,
    },
    Button {
        id: GamepadId,
        button: GamepadButton,
        pressed: bool,
    },
    Axis {
        id: GamepadId,
        axis: GamepadAxis,
        /// -1.0..=1.0 (sticks) or 0.0..=1.0 (triggers). Backends deadzone.
        value: f32,
    },
}

impl GamepadEvent {
    pub fn id(&self) -> GamepadId {
        match *self {
            GamepadEvent::Connected { id, .. } => id,
            GamepadEvent::Disconnected { id } => id,
            GamepadEvent::Button { id, .. } => id,
            GamepadEvent::Axis { id, .. } => id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum InputMode {
    #[default]
    Touch,
    /// Keyboard, Tab, arrow/D-pad, or other non-pointer navigation.
    Keyboard,
}

thread_local! {
    static INPUT_MODE: Cell<InputMode> = const { Cell::new(InputMode::Touch) };
}

/// Current input mode (Compose `InputModeManager.inputMode`).
///
/// Composition-local override ([`crate::locals::with_input_mode`]) wins over
/// the thread default.
#[inline]
pub fn input_mode() -> InputMode {
    crate::locals::local_input_mode().unwrap_or_else(|| INPUT_MODE.get())
}

/// Force the global default input mode (no frame request). Prefer
/// [`request_input_mode`] from event handlers.
#[inline]
pub fn set_input_mode_default(mode: InputMode) {
    INPUT_MODE.set(mode);
}

/// Request a new input mode. Returns `true` if the mode changed.
///
/// On change, requests a frame so focus chrome can appear/disappear.
pub fn request_input_mode(mode: InputMode) -> bool {
    let prev = INPUT_MODE.get();
    if prev == mode {
        return false;
    }
    INPUT_MODE.set(mode);
    crate::frame_clock::request_frame();
    true
}

/// `true` when focus indication should paint (focused **and** keyboard mode).
#[inline]
pub fn is_focus_visible(focused: bool) -> bool {
    focused && input_mode() == InputMode::Keyboard
}

#[cfg(test)]
mod input_mode_tests {
    use super::*;
    use crate::frame_clock::take_frame_request;
    use crate::modifier::{Interaction, MutableInteractionSource};

    #[test]
    fn request_input_mode_changes_and_requests_frame() {
        set_input_mode_default(InputMode::Touch);
        let _ = take_frame_request();

        assert!(!request_input_mode(InputMode::Touch));
        assert!(!take_frame_request());

        assert!(request_input_mode(InputMode::Keyboard));
        assert_eq!(input_mode(), InputMode::Keyboard);
        assert!(take_frame_request());

        assert!(request_input_mode(InputMode::Touch));
        assert_eq!(input_mode(), InputMode::Touch);
        set_input_mode_default(InputMode::Touch);
    }

    #[test]
    fn focus_visible_requires_keyboard_mode() {
        set_input_mode_default(InputMode::Touch);
        let src = MutableInteractionSource::new();
        src.emit(Interaction::Focus);
        assert!(src.source().collect_is_focused());
        assert!(!src.source().collect_is_focus_visible());

        set_input_mode_default(InputMode::Keyboard);
        assert!(src.source().collect_is_focus_visible());

        set_input_mode_default(InputMode::Touch);
        src.emit(Interaction::Unfocus);
    }

    #[test]
    fn with_input_mode_overrides_global() {
        set_input_mode_default(InputMode::Touch);
        crate::locals::with_input_mode(InputMode::Keyboard, || {
            assert_eq!(input_mode(), InputMode::Keyboard);
        });
        assert_eq!(input_mode(), InputMode::Touch);
    }
}
