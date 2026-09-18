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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

/// Physical key position, layout-independent (W3C `KeyboardEvent.code`)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PhysicalKey {
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,
    IntlBackslash,
    IntlRo,
    IntlYen,
    Escape,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    PrintScreen,
    ScrollLock,
    Pause,
    Insert,
    Home,
    PageUp,
    Delete,
    End,
    PageDown,
    ArrowRight,
    ArrowLeft,
    ArrowDown,
    ArrowUp,
    NumLock,
    NumpadDivide,
    NumpadMultiply,
    NumpadSubtract,
    NumpadAdd,
    NumpadEnter,
    NumpadDecimal,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    Backspace,
    Tab,
    Space,
    CapsLock,
    Enter,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
    ContextMenu,
    Unidentified,
}

impl PhysicalKey {
    /// W3C `code` name (`KeyW`, `Digit1`, `Space`, ...).
    pub fn name(self) -> &'static str {
        match self {
            PhysicalKey::KeyA => "KeyA",
            PhysicalKey::KeyB => "KeyB",
            PhysicalKey::KeyC => "KeyC",
            PhysicalKey::KeyD => "KeyD",
            PhysicalKey::KeyE => "KeyE",
            PhysicalKey::KeyF => "KeyF",
            PhysicalKey::KeyG => "KeyG",
            PhysicalKey::KeyH => "KeyH",
            PhysicalKey::KeyI => "KeyI",
            PhysicalKey::KeyJ => "KeyJ",
            PhysicalKey::KeyK => "KeyK",
            PhysicalKey::KeyL => "KeyL",
            PhysicalKey::KeyM => "KeyM",
            PhysicalKey::KeyN => "KeyN",
            PhysicalKey::KeyO => "KeyO",
            PhysicalKey::KeyP => "KeyP",
            PhysicalKey::KeyQ => "KeyQ",
            PhysicalKey::KeyR => "KeyR",
            PhysicalKey::KeyS => "KeyS",
            PhysicalKey::KeyT => "KeyT",
            PhysicalKey::KeyU => "KeyU",
            PhysicalKey::KeyV => "KeyV",
            PhysicalKey::KeyW => "KeyW",
            PhysicalKey::KeyX => "KeyX",
            PhysicalKey::KeyY => "KeyY",
            PhysicalKey::KeyZ => "KeyZ",
            PhysicalKey::Digit0 => "Digit0",
            PhysicalKey::Digit1 => "Digit1",
            PhysicalKey::Digit2 => "Digit2",
            PhysicalKey::Digit3 => "Digit3",
            PhysicalKey::Digit4 => "Digit4",
            PhysicalKey::Digit5 => "Digit5",
            PhysicalKey::Digit6 => "Digit6",
            PhysicalKey::Digit7 => "Digit7",
            PhysicalKey::Digit8 => "Digit8",
            PhysicalKey::Digit9 => "Digit9",
            PhysicalKey::Minus => "Minus",
            PhysicalKey::Equal => "Equal",
            PhysicalKey::BracketLeft => "BracketLeft",
            PhysicalKey::BracketRight => "BracketRight",
            PhysicalKey::Backslash => "Backslash",
            PhysicalKey::Semicolon => "Semicolon",
            PhysicalKey::Quote => "Quote",
            PhysicalKey::Backquote => "Backquote",
            PhysicalKey::Comma => "Comma",
            PhysicalKey::Period => "Period",
            PhysicalKey::Slash => "Slash",
            PhysicalKey::IntlBackslash => "IntlBackslash",
            PhysicalKey::IntlRo => "IntlRo",
            PhysicalKey::IntlYen => "IntlYen",
            PhysicalKey::Escape => "Escape",
            PhysicalKey::F1 => "F1",
            PhysicalKey::F2 => "F2",
            PhysicalKey::F3 => "F3",
            PhysicalKey::F4 => "F4",
            PhysicalKey::F5 => "F5",
            PhysicalKey::F6 => "F6",
            PhysicalKey::F7 => "F7",
            PhysicalKey::F8 => "F8",
            PhysicalKey::F9 => "F9",
            PhysicalKey::F10 => "F10",
            PhysicalKey::F11 => "F11",
            PhysicalKey::F12 => "F12",
            PhysicalKey::PrintScreen => "PrintScreen",
            PhysicalKey::ScrollLock => "ScrollLock",
            PhysicalKey::Pause => "Pause",
            PhysicalKey::Insert => "Insert",
            PhysicalKey::Home => "Home",
            PhysicalKey::PageUp => "PageUp",
            PhysicalKey::Delete => "Delete",
            PhysicalKey::End => "End",
            PhysicalKey::PageDown => "PageDown",
            PhysicalKey::ArrowRight => "ArrowRight",
            PhysicalKey::ArrowLeft => "ArrowLeft",
            PhysicalKey::ArrowDown => "ArrowDown",
            PhysicalKey::ArrowUp => "ArrowUp",
            PhysicalKey::NumLock => "NumLock",
            PhysicalKey::NumpadDivide => "NumpadDivide",
            PhysicalKey::NumpadMultiply => "NumpadMultiply",
            PhysicalKey::NumpadSubtract => "NumpadSubtract",
            PhysicalKey::NumpadAdd => "NumpadAdd",
            PhysicalKey::NumpadEnter => "NumpadEnter",
            PhysicalKey::NumpadDecimal => "NumpadDecimal",
            PhysicalKey::Numpad0 => "Numpad0",
            PhysicalKey::Numpad1 => "Numpad1",
            PhysicalKey::Numpad2 => "Numpad2",
            PhysicalKey::Numpad3 => "Numpad3",
            PhysicalKey::Numpad4 => "Numpad4",
            PhysicalKey::Numpad5 => "Numpad5",
            PhysicalKey::Numpad6 => "Numpad6",
            PhysicalKey::Numpad7 => "Numpad7",
            PhysicalKey::Numpad8 => "Numpad8",
            PhysicalKey::Numpad9 => "Numpad9",
            PhysicalKey::Backspace => "Backspace",
            PhysicalKey::Tab => "Tab",
            PhysicalKey::Space => "Space",
            PhysicalKey::CapsLock => "CapsLock",
            PhysicalKey::Enter => "Enter",
            PhysicalKey::ShiftLeft => "ShiftLeft",
            PhysicalKey::ShiftRight => "ShiftRight",
            PhysicalKey::ControlLeft => "ControlLeft",
            PhysicalKey::ControlRight => "ControlRight",
            PhysicalKey::AltLeft => "AltLeft",
            PhysicalKey::AltRight => "AltRight",
            PhysicalKey::SuperLeft => "SuperLeft",
            PhysicalKey::SuperRight => "SuperRight",
            PhysicalKey::ContextMenu => "ContextMenu",
            PhysicalKey::Unidentified => "Unidentified",
        }
    }

    /// Parse a W3C `code` name. Unknown names map to `Unidentified`
    /// (never `None`: every physical press has a position, even when
    /// the platform cannot name it).
    pub fn from_name(name: &str) -> Self {
        match name {
            "KeyA" => PhysicalKey::KeyA,
            "KeyB" => PhysicalKey::KeyB,
            "KeyC" => PhysicalKey::KeyC,
            "KeyD" => PhysicalKey::KeyD,
            "KeyE" => PhysicalKey::KeyE,
            "KeyF" => PhysicalKey::KeyF,
            "KeyG" => PhysicalKey::KeyG,
            "KeyH" => PhysicalKey::KeyH,
            "KeyI" => PhysicalKey::KeyI,
            "KeyJ" => PhysicalKey::KeyJ,
            "KeyK" => PhysicalKey::KeyK,
            "KeyL" => PhysicalKey::KeyL,
            "KeyM" => PhysicalKey::KeyM,
            "KeyN" => PhysicalKey::KeyN,
            "KeyO" => PhysicalKey::KeyO,
            "KeyP" => PhysicalKey::KeyP,
            "KeyQ" => PhysicalKey::KeyQ,
            "KeyR" => PhysicalKey::KeyR,
            "KeyS" => PhysicalKey::KeyS,
            "KeyT" => PhysicalKey::KeyT,
            "KeyU" => PhysicalKey::KeyU,
            "KeyV" => PhysicalKey::KeyV,
            "KeyW" => PhysicalKey::KeyW,
            "KeyX" => PhysicalKey::KeyX,
            "KeyY" => PhysicalKey::KeyY,
            "KeyZ" => PhysicalKey::KeyZ,
            "Digit0" => PhysicalKey::Digit0,
            "Digit1" => PhysicalKey::Digit1,
            "Digit2" => PhysicalKey::Digit2,
            "Digit3" => PhysicalKey::Digit3,
            "Digit4" => PhysicalKey::Digit4,
            "Digit5" => PhysicalKey::Digit5,
            "Digit6" => PhysicalKey::Digit6,
            "Digit7" => PhysicalKey::Digit7,
            "Digit8" => PhysicalKey::Digit8,
            "Digit9" => PhysicalKey::Digit9,
            "Minus" => PhysicalKey::Minus,
            "Equal" => PhysicalKey::Equal,
            "BracketLeft" => PhysicalKey::BracketLeft,
            "BracketRight" => PhysicalKey::BracketRight,
            "Backslash" => PhysicalKey::Backslash,
            "Semicolon" => PhysicalKey::Semicolon,
            "Quote" => PhysicalKey::Quote,
            "Backquote" => PhysicalKey::Backquote,
            "Comma" => PhysicalKey::Comma,
            "Period" => PhysicalKey::Period,
            "Slash" => PhysicalKey::Slash,
            "IntlBackslash" => PhysicalKey::IntlBackslash,
            "IntlRo" => PhysicalKey::IntlRo,
            "IntlYen" => PhysicalKey::IntlYen,
            "Escape" => PhysicalKey::Escape,
            "F1" => PhysicalKey::F1,
            "F2" => PhysicalKey::F2,
            "F3" => PhysicalKey::F3,
            "F4" => PhysicalKey::F4,
            "F5" => PhysicalKey::F5,
            "F6" => PhysicalKey::F6,
            "F7" => PhysicalKey::F7,
            "F8" => PhysicalKey::F8,
            "F9" => PhysicalKey::F9,
            "F10" => PhysicalKey::F10,
            "F11" => PhysicalKey::F11,
            "F12" => PhysicalKey::F12,
            "PrintScreen" => PhysicalKey::PrintScreen,
            "ScrollLock" => PhysicalKey::ScrollLock,
            "Pause" => PhysicalKey::Pause,
            "Insert" => PhysicalKey::Insert,
            "Home" => PhysicalKey::Home,
            "PageUp" => PhysicalKey::PageUp,
            "Delete" => PhysicalKey::Delete,
            "End" => PhysicalKey::End,
            "PageDown" => PhysicalKey::PageDown,
            "ArrowRight" => PhysicalKey::ArrowRight,
            "ArrowLeft" => PhysicalKey::ArrowLeft,
            "ArrowDown" => PhysicalKey::ArrowDown,
            "ArrowUp" => PhysicalKey::ArrowUp,
            "NumLock" => PhysicalKey::NumLock,
            "NumpadDivide" => PhysicalKey::NumpadDivide,
            "NumpadMultiply" => PhysicalKey::NumpadMultiply,
            "NumpadSubtract" => PhysicalKey::NumpadSubtract,
            "NumpadAdd" => PhysicalKey::NumpadAdd,
            "NumpadEnter" => PhysicalKey::NumpadEnter,
            "NumpadDecimal" => PhysicalKey::NumpadDecimal,
            "Numpad0" => PhysicalKey::Numpad0,
            "Numpad1" => PhysicalKey::Numpad1,
            "Numpad2" => PhysicalKey::Numpad2,
            "Numpad3" => PhysicalKey::Numpad3,
            "Numpad4" => PhysicalKey::Numpad4,
            "Numpad5" => PhysicalKey::Numpad5,
            "Numpad6" => PhysicalKey::Numpad6,
            "Numpad7" => PhysicalKey::Numpad7,
            "Numpad8" => PhysicalKey::Numpad8,
            "Numpad9" => PhysicalKey::Numpad9,
            "Backspace" => PhysicalKey::Backspace,
            "Tab" => PhysicalKey::Tab,
            "Space" => PhysicalKey::Space,
            "CapsLock" => PhysicalKey::CapsLock,
            "Enter" => PhysicalKey::Enter,
            "ShiftLeft" => PhysicalKey::ShiftLeft,
            "ShiftRight" => PhysicalKey::ShiftRight,
            "ControlLeft" => PhysicalKey::ControlLeft,
            "ControlRight" => PhysicalKey::ControlRight,
            "AltLeft" => PhysicalKey::AltLeft,
            "AltRight" => PhysicalKey::AltRight,
            "SuperLeft" | "MetaLeft" => PhysicalKey::SuperLeft,
            "SuperRight" | "MetaRight" => PhysicalKey::SuperRight,
            "ContextMenu" => PhysicalKey::ContextMenu,
            _ => PhysicalKey::Unidentified,
        }
    }
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
    ShiftLeft,
    ShiftRight,
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
    /// Physical key position, layout-independent. `None` for synthetic events.
    pub physical: Option<PhysicalKey>,
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
