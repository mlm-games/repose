use crate::Vec2;

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
    pub position: Vec2,
    pub pressure: f32,
    pub modifiers: Modifiers,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Cmd on Mac, Win key on Windows
}

#[derive(Clone, Debug)]
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
}

#[derive(Clone, Debug)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
    pub is_repeat: bool,
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
