use crate::effects::{Dispose, on_unmount};
use crate::input::{Key, Modifiers};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    SwipeLeft,
    SwipeRight,
    /// delta_scale > 1 => zoom in; < 1 => zoom out
    Pinch {
        delta_scale: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Copy,
    Cut,
    Paste,
    SelectAll,
    Undo,
    Redo,

    Back,
    Find,
    Save,

    Gesture(Gesture),
    Custom(Rc<str>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl KeyChord {
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

#[derive(Clone, Debug)]
pub struct ShortcutBinding {
    pub chord: KeyChord,
    pub action: Action,
}

#[derive(Clone, Debug, Default)]
pub struct ShortcutMap {
    pub bindings: Vec<ShortcutBinding>,
}

impl ShortcutMap {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }

    pub fn bind(mut self, key: Key, modifiers: Modifiers, action: Action) -> Self {
        self.bindings.push(ShortcutBinding {
            chord: KeyChord::new(key, modifiers),
            action,
        });
        self
    }

    pub fn bind_action(mut self, action: Action) -> Self {
        if let Some(chord) = default_chord_for(&action) {
            self.bindings.push(ShortcutBinding { chord, action });
        }
        self
    }

    pub fn merge(mut self, other: ShortcutMap) -> Self {
        self.bindings.extend(other.bindings);
        self
    }

    pub fn insert(&mut self, key: Key, modifiers: Modifiers, action: Action) {
        self.bindings.push(ShortcutBinding {
            chord: KeyChord::new(key, modifiers),
            action,
        });
    }

    pub fn action_for(&self, chord: &KeyChord) -> Option<Action> {
        self.bindings
            .iter()
            .rev()
            .find(|binding| &binding.chord == chord)
            .map(|binding| binding.action.clone())
    }
}

pub type Handler = Rc<dyn Fn(Action) -> bool>;

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = RefCell::new(None);
    static DEFAULT_MAP: RefCell<ShortcutMap> = RefCell::new(default_map());
    static SCOPES: RefCell<Vec<ShortcutMap>> = const { RefCell::new(Vec::new()) };
}

/// Set/clear the global handler (prefer InstallShortcutHandler + scoped_effect).
pub fn set(handler: Option<Handler>) {
    HANDLER.with(|h| *h.borrow_mut() = handler);
}

/// Dispatch an action to the global handler. Returns true if consumed.
pub fn handle(action: Action) -> bool {
    HANDLER.with(|h| h.borrow().as_ref().map(|f| f(action)).unwrap_or(false))
}

/// Resolve a key chord to an action using scoped + default maps.
pub fn resolve_action(chord: KeyChord) -> Option<Action> {
    if chord.key == Key::Unknown {
        return None;
    }

    if let Some(action) = SCOPES.with(|scopes| {
        scopes
            .borrow()
            .iter()
            .rev()
            .find_map(|scope| scope.action_for(&chord))
    }) {
        return Some(action);
    }

    DEFAULT_MAP.with(|m| m.borrow().action_for(&chord))
}

/// Replace the default shortcut map used by resolve_action.
pub fn set_default_map(map: ShortcutMap) {
    DEFAULT_MAP.with(|m| *m.borrow_mut() = map);
}

/// Push a shortcut map for the current scope, popped on unmount.
#[allow(non_snake_case)]
pub fn InstallShortcutMap(map: ShortcutMap) -> Dispose {
    SCOPES.with(|scopes| scopes.borrow_mut().push(map));
    on_unmount(|| {
        SCOPES.with(|scopes| {
            scopes.borrow_mut().pop();
        });
    })
}

/// Install/uninstall a global shortcut handler for the current scope.
/// Restores the previous handler on unmount (supports nesting).
#[allow(non_snake_case)]
pub fn InstallShortcutHandler(handler: Handler) -> Dispose {
    let prev = HANDLER.with(|h| h.borrow_mut().replace(handler));
    on_unmount(move || {
        HANDLER.with(|h| *h.borrow_mut() = prev);
    })
}

pub fn default_chord_for(action: &Action) -> Option<KeyChord> {
    // On non-macOS, sets ctrl true
    let cmd = Modifiers {
        command: true,
        ctrl: !cfg!(target_os = "macos"),
        ..Modifiers::default()
    };
    match action {
        Action::Copy => Some(KeyChord::new(Key::Character('c'), cmd)),
        Action::Cut => Some(KeyChord::new(Key::Character('x'), cmd)),
        Action::Paste => Some(KeyChord::new(Key::Character('v'), cmd)),
        Action::SelectAll => Some(KeyChord::new(Key::Character('a'), cmd)),
        Action::Undo => Some(KeyChord::new(Key::Character('z'), cmd)),
        Action::Redo => Some(KeyChord::new(
            Key::Character('z'),
            Modifiers {
                command: true,
                shift: true,
                ctrl: !cfg!(target_os = "macos"),
                ..Modifiers::default()
            },
        )),
        Action::Find => Some(KeyChord::new(Key::Character('f'), cmd)),
        Action::Save => Some(KeyChord::new(Key::Character('s'), cmd)),
        _ => None,
    }
}

pub fn default_map() -> ShortcutMap {
    let mut map = ShortcutMap::new();
    let actions = [
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::SelectAll,
        Action::Undo,
        Action::Redo,
        Action::Find,
        Action::Save,
    ];
    for action in actions {
        if let Some(chord) = default_chord_for(&action) {
            map.insert(chord.key, chord.modifiers, action);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_action_prefers_scopes() {
        let mut map = ShortcutMap::new();
        map.insert(
            Key::Character('k'),
            Modifiers::default(),
            Action::Custom("one".into()),
        );
        set_default_map(map);

        let mut scope = ShortcutMap::new();
        scope.insert(
            Key::Character('k'),
            Modifiers::default(),
            Action::Custom("two".into()),
        );

        SCOPES.with(|scopes| scopes.borrow_mut().push(scope));

        let chord = KeyChord::new(Key::Character('k'), Modifiers::default());
        assert_eq!(
            resolve_action(chord.clone()),
            Some(Action::Custom("two".into()))
        );

        SCOPES.with(|scopes| scopes.borrow_mut().pop());
        assert_eq!(resolve_action(chord), Some(Action::Custom("one".into())));
    }
}
