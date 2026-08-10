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
    /// Position in window/surface physical pixels (device px).
    pub position: Vec2,
    /// Top-left of the hit region this event is being delivered to.
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

    /// Position in window/surface physical pixels.
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
}
