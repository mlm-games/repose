use crate::Vec2;
use crate::effects::Dispose;
use crate::input::{Key, Modifiers, PointerKind};
use crate::remember_with_key;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

#[derive(Clone, Debug, PartialEq)]
pub enum Gesture {
    SwipeLeft,
    SwipeRight,
    /// Center-less pinch kept for back-compat with producers that do not
    /// track a centroid (none in-tree emit this, handlers must treat it as
    /// unroutable and return false unless they own the whole surface).
    Pinch {
        delta_scale: f32,
    },
    PinchWithCenter {
        delta_scale: f32,
        center: Vec2,
    },
    /// Two-finger rotation (twist). `delta_rotation` is in radians
    /// (positive = clockwise in screen space, y-down), `center` is the
    /// gesture centroid in physical px.
    Rotate {
        delta_rotation: f32,
        center: Vec2,
    },
    /// 2/3-finger pan (centroid translation). `delta` is in physical px,
    /// positive = content moves right/down (natural scrolling). `center` is
    /// the gesture centroid in physical px (Compose `calculateCentroid`).
    Pan {
        delta: Vec2,
        center: Vec2,
    },
}

/// Low-level drag-and-drop actions dispatched by the platform.
/// The framework handles gesture detection (mouse drag vs touch long press)
/// and the DnD state machine internally.
#[derive(Clone, Debug, PartialEq)]
pub enum DragAction {
    /// Pointer button pressed (mouse down / touch start).
    Press {
        position: Vec2,
        capture_id: u64,
        kind: PointerKind,
        modifiers: Modifiers,
    },
    /// Pointer moved while button is pressed or touch is active.
    Move {
        position: Vec2,
        modifiers: Modifiers,
    },
    /// Pointer released.
    Release {
        position: Vec2,
        modifiers: Modifiers,
    },
    /// Drag cancelled (e.g. Escape key).
    Cancel,
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

    FocusNext,
    FocusPrevious,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,

    Gesture(Gesture),
    Drag(DragAction),
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

#[derive(Clone)]
pub struct ShortcutState {
    pub handler: Option<Handler>,
    pub default_map: ShortcutMap,
    pub scopes: Vec<ShortcutMap>,
    runtime_installs: Rc<RefCell<RuntimeShortcutInstalls>>,
    use_global_fallback: bool,
}

struct RuntimeShortcutMapEntry {
    key: String,
    token: u64,
    map: ShortcutMap,
    cleanup: Dispose,
}

struct RuntimeShortcutHandlerEntry {
    key: String,
    token: u64,
    handler: Handler,
    cleanup: Dispose,
}

#[derive(Default)]
struct RuntimeShortcutInstalls {
    maps: Vec<RuntimeShortcutMapEntry>,
    handlers: Vec<RuntimeShortcutHandlerEntry>,
    next_token: u64,
}

impl ShortcutState {
    pub fn new() -> Self {
        Self {
            handler: None,
            default_map: default_map(),
            scopes: Vec::new(),
            runtime_installs: Rc::new(RefCell::new(RuntimeShortcutInstalls {
                next_token: 1,
                ..RuntimeShortcutInstalls::default()
            })),
            use_global_fallback: true,
        }
    }

    pub fn without_global_fallback(mut self) -> Self {
        self.use_global_fallback = false;
        self
    }

    fn resolve_local_action(&self, chord: &KeyChord) -> Option<Action> {
        if chord.key == Key::Unknown {
            return None;
        }
        if let Some(action) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.action_for(chord))
        {
            return Some(action);
        }
        if let Some(action) = self
            .runtime_installs
            .borrow()
            .maps
            .iter()
            .rev()
            .find_map(|entry| entry.map.action_for(chord))
        {
            return Some(action);
        }
        self.default_map.action_for(chord)
    }

    pub fn resolve_action(&self, chord: &KeyChord) -> Option<Action> {
        self.resolve_local_action(chord).or_else(|| {
            self.use_global_fallback
                .then(|| resolve_global_action(chord.clone()))
                .flatten()
        })
    }

    fn handle_local(&self, action: Action) -> bool {
        let handler = self
            .runtime_installs
            .borrow()
            .handlers
            .iter()
            .rev()
            .map(|entry| entry.handler.clone())
            .next()
            .or_else(|| self.handler.clone());
        handler.is_some_and(|handler| handler(action))
    }

    pub fn handle(&self, action: Action) -> bool {
        if self.handle_local(action.clone()) {
            return true;
        }
        self.use_global_fallback && handle_global(action)
    }
}

impl Default for ShortcutState {
    fn default() -> Self {
        Self::new()
    }
}

struct ScopeEntry {
    token: Option<u64>,
    map: ShortcutMap,
}

struct ShortcutScopeStack(Vec<ScopeEntry>);

impl ShortcutScopeStack {
    #[cfg(test)]
    fn push(&mut self, map: ShortcutMap) {
        self.0.push(ScopeEntry { token: None, map });
    }

    fn push_installed(&mut self, token: u64, map: ShortcutMap) {
        self.0.push(ScopeEntry {
            token: Some(token),
            map,
        });
    }

    #[cfg(test)]
    fn pop(&mut self) -> Option<ShortcutMap> {
        self.0.pop().map(|entry| entry.map)
    }

    fn remove(&mut self, token: u64) -> Option<ShortcutMap> {
        let index = self.0.iter().position(|entry| entry.token == Some(token))?;
        Some(self.0.remove(index).map)
    }

    fn update(&mut self, token: u64, map: ShortcutMap) -> Option<ShortcutMap> {
        let entry = self.0.iter_mut().find(|entry| entry.token == Some(token))?;
        Some(std::mem::replace(&mut entry.map, map))
    }

    fn iter_rev(&self) -> impl Iterator<Item = &ShortcutMap> {
        self.0.iter().rev().map(|entry| &entry.map)
    }
}

struct MapInstallState {
    token: u64,
    active: bool,
    cleanup: Option<Dispose>,
}

struct HandlerInstallState {
    token: u64,
    active: bool,
    cleanup: Option<Dispose>,
}

thread_local! {
    static HANDLER: RefCell<Option<Handler>> = RefCell::new(None);
    static BASE_HANDLER: RefCell<Option<Handler>> = RefCell::new(None);
    static HANDLER_ENTRIES: RefCell<Vec<(u64, Handler)>> = const { RefCell::new(Vec::new()) };
    static DEFAULT_MAP: RefCell<ShortcutMap> = RefCell::new(default_map());
    static SCOPES: RefCell<ShortcutScopeStack> =
        const { RefCell::new(ShortcutScopeStack(Vec::new())) };
    static NEXT_INSTALLER_ID: Cell<u64> = const { Cell::new(1) };
    static ACTIVE_SHORTCUT_STATE: RefCell<Option<ShortcutState>> = const { RefCell::new(None) };
}

fn next_installer_id() -> u64 {
    NEXT_INSTALLER_ID.with(|next| {
        let id = next.get();
        next.set(id.wrapping_add(1));
        id
    })
}

pub fn with_runtime_state<R>(state: &ShortcutState, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<ShortcutState>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ACTIVE_SHORTCUT_STATE.with(|slot| {
                *slot.borrow_mut() = self.0.take();
            });
        }
    }
    let previous = ACTIVE_SHORTCUT_STATE.with(|slot| slot.borrow_mut().replace(state.clone()));
    let _restore = Restore(previous);
    f()
}

fn active_runtime_state() -> Option<ShortcutState> {
    ACTIVE_SHORTCUT_STATE.with(|slot| slot.borrow().clone())
}

fn sync_handler() {
    let handler = HANDLER_ENTRIES
        .try_with(|entries| {
            entries
                .try_borrow()
                .ok()
                .and_then(|entries| entries.last().map(|(_, handler)| handler.clone()))
        })
        .ok()
        .flatten()
        .or_else(|| BASE_HANDLER.with(|base| base.try_borrow().ok().and_then(|base| base.clone())));
    let old = HANDLER.try_with(|current| {
        current
            .try_borrow_mut()
            .ok()
            .map(|mut current| std::mem::replace(&mut *current, handler))
    });
    if let Ok(Some(old)) = old {
        drop(old);
    }
}

pub fn set(handler: Option<Handler>) {
    let old = BASE_HANDLER.try_with(|base| {
        base.try_borrow_mut()
            .ok()
            .map(|mut base| std::mem::replace(&mut *base, handler))
    });
    if let Ok(Some(old)) = old {
        drop(old);
    }
    sync_handler();
}

fn handle_global(action: Action) -> bool {
    let handler = HANDLER
        .try_with(|current| {
            current
                .try_borrow()
                .ok()
                .and_then(|current| current.clone())
        })
        .ok()
        .flatten();
    handler.map(|handler| handler(action)).unwrap_or(false)
}

pub fn handle(action: Action) -> bool {
    if let Some(state) = active_runtime_state() {
        return state.handle(action);
    }
    handle_global(action)
}

fn resolve_global_action(chord: KeyChord) -> Option<Action> {
    if chord.key == Key::Unknown {
        return None;
    }
    let scoped = SCOPES
        .try_with(|scopes| {
            scopes
                .try_borrow()
                .ok()
                .and_then(|scopes| scopes.iter_rev().find_map(|scope| scope.action_for(&chord)))
        })
        .ok()
        .flatten();
    scoped.or_else(|| {
        DEFAULT_MAP
            .try_with(|map| map.try_borrow().ok().and_then(|map| map.action_for(&chord)))
            .ok()
            .flatten()
    })
}

pub fn resolve_action(chord: KeyChord) -> Option<Action> {
    if let Some(state) = active_runtime_state() {
        return state.resolve_action(&chord);
    }
    resolve_global_action(chord)
}

pub fn set_default_map(map: ShortcutMap) {
    let old = DEFAULT_MAP.try_with(|current| {
        current
            .try_borrow_mut()
            .ok()
            .map(|mut current| std::mem::replace(&mut *current, map))
    });
    if let Ok(Some(old)) = old {
        drop(old);
    }
    crate::request_frame();
}

fn map_cleanup_key(key: &str) -> String {
    format!("shortcut-map-cleanup:{key}")
}

fn handler_cleanup_key(key: &str) -> String {
    format!("shortcut-handler-cleanup:{key}")
}

fn owner_disposer(key: String, cleanup: Dispose, owner: String) -> Dispose {
    Dispose::new(move || {
        crate::runtime::remove_keyed_disposer_for_owner(&key, &cleanup, &owner);
    })
}

fn runtime_owner_disposer(key: String, cleanup: Dispose, owner: String) -> Dispose {
    Dispose::new(move || {
        cleanup.run();
        crate::runtime::remove_keyed_disposer_for_owner(&key, &cleanup, &owner);
    })
}

fn register_runtime_cleanup(state: &ShortcutState, key: &str, cleanup: Dispose) -> Dispose {
    let cleanup_key = format!(
        "runtime-shortcut-cleanup:{}:{key}",
        Rc::as_ptr(&state.runtime_installs) as usize
    );
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    if !crate::runtime::keyed_disposer_has_owner(&cleanup_key, &cleanup, &owner) {
        crate::runtime::register_keyed_disposer_for_owner(
            cleanup_key.clone(),
            cleanup.clone(),
            owner.clone(),
        );
        if let Some(scope) = crate::scope::current_scope() {
            let registered = cleanup.clone();
            let scope_key = cleanup_key.clone();
            let scope_owner = owner.clone();
            scope.add_disposer(move || {
                crate::runtime::remove_keyed_disposer_for_owner(
                    &scope_key,
                    &registered,
                    &scope_owner,
                );
            });
        }
    }
    runtime_owner_disposer(cleanup_key, cleanup, owner)
}

fn install_runtime_map(state: &ShortcutState, key: String, map: ShortcutMap) -> Dispose {
    let mut installs = state.runtime_installs.borrow_mut();
    if let Some(entry) = installs.maps.iter_mut().find(|entry| entry.key == key) {
        entry.map = map;
        return entry.cleanup.clone();
    }
    let token = installs.next_token;
    installs.next_token = installs.next_token.wrapping_add(1);
    let weak: Weak<RefCell<RuntimeShortcutInstalls>> = Rc::downgrade(&state.runtime_installs);
    let cleanup_key = key.clone();
    let cleanup = Dispose::new(move || {
        let Some(installs) = weak.upgrade() else {
            return;
        };
        let mut installs = installs.borrow_mut();
        if let Some(index) = installs
            .maps
            .iter()
            .position(|entry| entry.key == cleanup_key && entry.token == token)
        {
            installs.maps.remove(index);
        }
    });
    let disposer = register_runtime_cleanup(state, &key, cleanup);
    installs.maps.push(RuntimeShortcutMapEntry {
        key,
        token,
        map,
        cleanup: disposer.clone(),
    });
    disposer
}

fn install_runtime_handler(state: &ShortcutState, key: String, handler: Handler) -> Dispose {
    let mut installs = state.runtime_installs.borrow_mut();
    if let Some(entry) = installs.handlers.iter_mut().find(|entry| entry.key == key) {
        entry.handler = handler;
        return entry.cleanup.clone();
    }
    let token = installs.next_token;
    installs.next_token = installs.next_token.wrapping_add(1);
    let weak: Weak<RefCell<RuntimeShortcutInstalls>> = Rc::downgrade(&state.runtime_installs);
    let cleanup_key = key.clone();
    let cleanup = Dispose::new(move || {
        let Some(installs) = weak.upgrade() else {
            return;
        };
        let mut installs = installs.borrow_mut();
        if let Some(index) = installs
            .handlers
            .iter()
            .position(|entry| entry.key == cleanup_key && entry.token == token)
        {
            installs.handlers.remove(index);
        }
    });
    let disposer = register_runtime_cleanup(state, &key, cleanup);
    installs.handlers.push(RuntimeShortcutHandlerEntry {
        key,
        token,
        handler,
        cleanup: disposer.clone(),
    });
    disposer
}

fn install_shortcut_map_state(key: impl Into<String>, map: ShortcutMap) -> Dispose {
    let key = key.into();
    if let Some(state) = active_runtime_state() {
        return install_runtime_map(&state, key, map);
    }
    let state_key = format!("shortcut-map-state:{key}");
    let state: Rc<RefCell<MapInstallState>> = remember_with_key(state_key, || {
        RefCell::new(MapInstallState {
            token: 0,
            active: false,
            cleanup: None,
        })
    });
    let (token, cleanup, installed) = {
        let weak: Weak<RefCell<MapInstallState>> = Rc::downgrade(&state);
        let mut state = state.borrow_mut();
        if state.active {
            (
                state.token,
                state.cleanup.clone().expect("active map state"),
                false,
            )
        } else {
            let token = next_installer_id();
            let cleanup = Dispose::new(move || {
                let removed = SCOPES
                    .try_with(|scopes| {
                        scopes
                            .try_borrow_mut()
                            .ok()
                            .and_then(|mut scopes| scopes.remove(token))
                    })
                    .ok()
                    .flatten();
                drop(removed);
                if let Some(state) = weak.upgrade() {
                    let old = {
                        let mut state = state.borrow_mut();
                        if state.token == token {
                            state.active = false;
                            state.cleanup.take()
                        } else {
                            None
                        }
                    };
                    drop(old);
                }
            });
            state.token = token;
            state.active = true;
            state.cleanup = Some(cleanup.clone());
            (token, cleanup, true)
        }
    };
    if installed {
        SCOPES.with(|scopes| scopes.borrow_mut().push_installed(token, map));
    } else {
        let old = SCOPES
            .try_with(|scopes| {
                scopes
                    .try_borrow_mut()
                    .ok()
                    .and_then(|mut scopes| scopes.update(token, map))
            })
            .ok()
            .flatten();
        drop(old);
    }
    let cleanup_key = map_cleanup_key(&key);
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    if !crate::runtime::keyed_disposer_has_owner(&cleanup_key, &cleanup, &owner) {
        crate::runtime::register_keyed_disposer_for_owner(
            cleanup_key.clone(),
            cleanup.clone(),
            owner.clone(),
        );
        if let Some(scope) = crate::scope::current_scope() {
            let registered = cleanup.clone();
            let scope_key = cleanup_key.clone();
            let scope_owner = owner.clone();
            scope.add_disposer(move || {
                crate::runtime::remove_keyed_disposer_for_owner(
                    &scope_key,
                    &registered,
                    &scope_owner,
                );
            });
        }
    }
    owner_disposer(cleanup_key, cleanup, owner)
}

pub fn install_shortcut_map_with_key(key: impl Into<String>, map: ShortcutMap) -> Dispose {
    install_shortcut_map_state(key, map)
}

#[allow(non_snake_case)]
pub fn InstallShortcutMapWithKey(key: impl Into<String>, map: ShortcutMap) -> Dispose {
    install_shortcut_map_state(key, map)
}

#[track_caller]
#[allow(non_snake_case)]
pub fn InstallShortcutMap(map: ShortcutMap) -> Dispose {
    let location = std::panic::Location::caller();
    install_shortcut_map_state(
        format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ),
        map,
    )
}

pub fn install_shortcut_handler_state(key: impl Into<String>, handler: Handler) -> Dispose {
    let key = key.into();
    if let Some(state) = active_runtime_state() {
        return install_runtime_handler(&state, key, handler);
    }
    let state_key = format!("shortcut-handler-state:{key}");
    let state: Rc<RefCell<HandlerInstallState>> = remember_with_key(state_key, || {
        RefCell::new(HandlerInstallState {
            token: 0,
            active: false,
            cleanup: None,
        })
    });
    let (token, cleanup, installed) = {
        let weak: Weak<RefCell<HandlerInstallState>> = Rc::downgrade(&state);
        let mut state = state.borrow_mut();
        if state.active {
            (
                state.token,
                state.cleanup.clone().expect("active handler state"),
                false,
            )
        } else {
            let token = next_installer_id();
            let cleanup = Dispose::new(move || {
                let removed = HANDLER_ENTRIES
                    .try_with(|entries| {
                        entries.try_borrow_mut().ok().and_then(|mut entries| {
                            let index = entries.iter().position(|(id, _)| *id == token)?;
                            Some(entries.remove(index).1)
                        })
                    })
                    .ok()
                    .flatten();
                drop(removed);
                if let Some(state) = weak.upgrade() {
                    let old = {
                        let mut state = state.borrow_mut();
                        if state.token == token {
                            state.active = false;
                            state.cleanup.take()
                        } else {
                            None
                        }
                    };
                    drop(old);
                }
                sync_handler();
            });
            state.token = token;
            state.active = true;
            state.cleanup = Some(cleanup.clone());
            (token, cleanup, true)
        }
    };
    if installed {
        let old = HANDLER_ENTRIES
            .try_with(|entries| {
                entries.try_borrow_mut().ok().map(|mut entries| {
                    entries.push((token, handler));
                    None::<Handler>
                })
            })
            .ok()
            .flatten();
        drop(old);
        sync_handler();
    } else {
        let old = HANDLER_ENTRIES
            .try_with(|entries| {
                entries.try_borrow_mut().ok().and_then(|mut entries| {
                    let entry = entries.iter_mut().find(|(id, _)| *id == token)?;
                    let old = entry.1.clone();
                    entry.1 = handler;
                    Some(old)
                })
            })
            .ok()
            .flatten();
        drop(old);
        sync_handler();
    }
    let cleanup_key = handler_cleanup_key(&key);
    let owner =
        crate::runtime::scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    if !crate::runtime::keyed_disposer_has_owner(&cleanup_key, &cleanup, &owner) {
        crate::runtime::register_keyed_disposer_for_owner(
            cleanup_key.clone(),
            cleanup.clone(),
            owner.clone(),
        );
        if let Some(scope) = crate::scope::current_scope() {
            let registered = cleanup.clone();
            let scope_key = cleanup_key.clone();
            let scope_owner = owner.clone();
            scope.add_disposer(move || {
                crate::runtime::remove_keyed_disposer_for_owner(
                    &scope_key,
                    &registered,
                    &scope_owner,
                );
            });
        }
    }
    owner_disposer(cleanup_key, cleanup, owner)
}

pub fn install_shortcut_handler_with_key(key: impl Into<String>, handler: Handler) -> Dispose {
    install_shortcut_handler_state(key, handler)
}

#[allow(non_snake_case)]
pub fn InstallShortcutHandlerWithKey(key: impl Into<String>, handler: Handler) -> Dispose {
    install_shortcut_handler_state(key, handler)
}

#[track_caller]
#[allow(non_snake_case)]
pub fn InstallShortcutHandler(handler: Handler) -> Dispose {
    let location = std::panic::Location::caller();
    install_shortcut_handler_state(
        format!(
            "{}:{}:{}",
            location.file(),
            location.line(),
            location.column()
        ),
        handler,
    )
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
        Action::FocusNext => Some(KeyChord::new(Key::Tab, Modifiers::default())),
        Action::FocusPrevious => Some(KeyChord::new(
            Key::Tab,
            Modifiers {
                shift: true,
                ..Modifiers::default()
            },
        )),
        Action::FocusLeft => Some(KeyChord::new(Key::ArrowLeft, Modifiers::default())),
        Action::FocusRight => Some(KeyChord::new(Key::ArrowRight, Modifiers::default())),
        Action::FocusUp => Some(KeyChord::new(Key::ArrowUp, Modifiers::default())),
        Action::FocusDown => Some(KeyChord::new(Key::ArrowDown, Modifiers::default())),
        _ => None,
    }
}

pub fn default_map() -> ShortcutMap {
    let mut map = ShortcutMap::new();
    let actions = vec![
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::SelectAll,
        Action::Undo,
        Action::Redo,
        Action::Find,
        Action::Save,
        Action::FocusNext,
        Action::FocusPrevious,
        Action::FocusLeft,
        Action::FocusRight,
        Action::FocusUp,
        Action::FocusDown,
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

    #[test]
    fn runtime_installers_are_isolated() {
        let a = ShortcutState::new();
        let b = ShortcutState::new();
        let chord = KeyChord::new(Key::Character('j'), Modifiers::default());
        let mut map_a = ShortcutMap::new();
        map_a.insert(
            chord.key.clone(),
            chord.modifiers,
            Action::Custom("a".into()),
        );
        let mut map_b = ShortcutMap::new();
        map_b.insert(
            chord.key.clone(),
            chord.modifiers,
            Action::Custom("b".into()),
        );
        let dispose_a = with_runtime_state(&a, || InstallShortcutMapWithKey("same-install", map_a));
        let dispose_b = with_runtime_state(&b, || InstallShortcutMapWithKey("same-install", map_b));
        assert_eq!(a.resolve_action(&chord), Some(Action::Custom("a".into())));
        assert_eq!(b.resolve_action(&chord), Some(Action::Custom("b".into())));
        dispose_a.run();
        assert!(a.resolve_action(&chord).is_none());
        assert_eq!(b.resolve_action(&chord), Some(Action::Custom("b".into())));
        dispose_b.run();
    }

    #[test]
    fn keyed_map_updates_and_cleans_up() {
        set_default_map(ShortcutMap::new());
        let chord = KeyChord::new(Key::Character('m'), Modifiers::default());
        let mut first = ShortcutMap::new();
        first.insert(
            chord.key.clone(),
            chord.modifiers,
            Action::Custom("first".into()),
        );
        let mut second = ShortcutMap::new();
        second.insert(
            chord.key.clone(),
            chord.modifiers,
            Action::Custom("second".into()),
        );
        let key = "shortcut-test-map-update";
        let disposer = install_shortcut_map_with_key(key, first);
        assert_eq!(
            resolve_action(chord.clone()),
            Some(Action::Custom("first".into()))
        );
        let _ = install_shortcut_map_with_key(key, second);
        assert_eq!(
            resolve_action(chord.clone()),
            Some(Action::Custom("second".into()))
        );
        disposer.run();
        assert_eq!(resolve_action(chord), None);
    }

    #[test]
    fn handler_restores_base_after_keyed_cleanup() {
        let base = Rc::new(|_action: Action| false);
        let installed = Rc::new(|_action: Action| true);
        set(Some(base));
        let key = "shortcut-test-handler-restore";
        let disposer = install_shortcut_handler_with_key(key, installed);
        assert!(handle(Action::Copy));
        disposer.run();
        assert!(!handle(Action::Copy));
        set(None);
    }
}
