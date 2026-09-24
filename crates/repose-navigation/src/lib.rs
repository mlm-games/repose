#![allow(non_snake_case)]
pub mod deeplink;

use std::{
    any::{Any, TypeId},
    cell::RefCell,
    fmt::Debug,
    rc::Rc,
};

use repose_core::*;
use repose_ui::{Box as VBox, Column, ViewExt, anim::animate_f32_from};
use serde::{Deserialize, Serialize};

pub trait NavKey: Clone + Debug + 'static + Serialize + for<'de> Deserialize<'de> {}
impl<T> NavKey for T where T: Clone + Debug + 'static + Serialize + for<'de> Deserialize<'de> {}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransitionDir {
    None,
    Push,
    Pop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SavedStateTypeError {
    key: &'static str,
    expected: TypeId,
    actual: TypeId,
}

impl SavedStateTypeError {
    pub fn key(&self) -> &'static str {
        self.key
    }

    pub fn expected(&self) -> TypeId {
        self.expected
    }

    pub fn actual(&self) -> TypeId {
        self.actual
    }
}

impl std::fmt::Display for SavedStateTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "saved result {:?} has the wrong type", self.key)
    }
}

impl std::error::Error for SavedStateTypeError {}

#[derive(Default)]
pub struct SavedState {
    map: RefCell<std::collections::HashMap<&'static str, Box<dyn Any>>>,
    results: RefCell<std::collections::HashMap<&'static str, Box<dyn Any>>>,
}

impl SavedState {
    pub fn remember<T: 'static + Clone>(
        &self,
        key: &'static str,
        init: impl FnOnce() -> T,
    ) -> Rc<RefCell<T>> {
        if let Some(b) = self.map.borrow().get(key)
            && let Some(rc) = b.downcast_ref::<Rc<RefCell<T>>>()
        {
            return rc.clone();
        }
        let rc = Rc::new(RefCell::new(init()));
        self.map.borrow_mut().insert(key, Box::new(rc.clone()));
        rc
    }

    /// Stores a one-shot, in-memory result. A later set for the same slot
    /// replaces the previous value.
    pub fn set_result<T: 'static>(&self, key: &'static str, val: T) {
        self.results.borrow_mut().insert(key, Box::new(val));
    }

    /// Takes a result only when its type matches. A mismatch reports both
    /// type IDs and leaves the stored value available for the correct type.
    pub fn try_take_result<T: 'static>(
        &self,
        key: &'static str,
    ) -> Result<Option<T>, SavedStateTypeError> {
        let mut results = self.results.borrow_mut();
        let Some(value) = results.remove(key) else {
            return Ok(None);
        };
        let actual = (*value).type_id();
        match value.downcast::<T>() {
            Ok(value) => Ok(Some(*value)),
            Err(value) => {
                results.insert(key, value);
                Err(SavedStateTypeError {
                    key,
                    expected: TypeId::of::<T>(),
                    actual,
                })
            }
        }
    }

    pub fn take_result<T: 'static>(&self, key: &'static str) -> Option<T> {
        self.try_take_result(key).ok().flatten()
    }
}

struct Entry<K: NavKey> {
    id: u64,
    key: K,
    saved: Rc<SavedState>,
    /// Scope owned by this navigation entry.
    /// Disposed when the entry is popped, so `scoped_effect` cleanups run on unmount.
    scope: Scope,
}

struct BackState<K: NavKey> {
    entries: Vec<Entry<K>>,
    next_id: u64,
    last_dir: TransitionDir,
    transition_id: u64,
}

#[derive(Clone)]
pub struct NavBackStack<K: NavKey> {
    inner: Rc<RefCell<BackState<K>>>,
    version: Rc<Signal<u64>>,
    stack_id: u64,
}
impl<K: NavKey> NavBackStack<K> {
    pub fn top(&self) -> Option<(u64, K, Rc<SavedState>, Scope)> {
        let s = self.inner.borrow();
        s.entries
            .last()
            .map(|e| (e.id, e.key.clone(), e.saved.clone(), e.scope.clone()))
    }
    pub fn current(&self) -> Option<(u64, K, Rc<SavedState>, Scope)> {
        self.top()
    }
    pub fn current_id(&self) -> Option<u64> {
        self.inner.borrow().entries.last().map(|entry| entry.id)
    }
    pub fn is_current(&self, entry_id: u64) -> bool {
        self.current_id() == Some(entry_id)
    }
    fn transition_id(&self) -> u64 {
        self.inner.borrow().transition_id
    }
    pub fn size(&self) -> usize {
        self.inner.borrow().entries.len()
    }
    pub fn last_dir(&self) -> TransitionDir {
        self.inner.borrow().last_dir
    }
    fn bump(&self) {
        let v = self.version.get();
        self.version.set(v.wrapping_add(1));
    }

    fn fresh_entry(&self, s: &mut BackState<K>, key: K) {
        let id = s.next_id;
        s.next_id += 1;
        s.entries.push(Entry {
            id,
            key,
            saved: Rc::new(SavedState::default()),
            scope: Scope::new(),
        });
    }

    fn push_inner(&self, key: K) {
        let mut s = self.inner.borrow_mut();
        self.fresh_entry(&mut s, key);
        s.last_dir = TransitionDir::Push;
        s.transition_id = s.transition_id.wrapping_add(1);
    }

    fn pop_entries(&self, count: usize) -> bool {
        if count == 0 {
            return false;
        }
        let entries = {
            let mut s = self.inner.borrow_mut();
            let count = count.min(s.entries.len().saturating_sub(1));
            if count == 0 {
                return false;
            }
            let split_at = s.entries.len() - count;
            let entries = s.entries.split_off(split_at);
            s.last_dir = TransitionDir::Pop;
            s.transition_id = s.transition_id.wrapping_add(1);
            entries
        };
        self.bump();
        for entry in entries {
            entry.scope.dispose();
        }
        true
    }

    fn pop_inner(&self) -> bool {
        self.pop_entries(1)
    }

    fn pop_with_result_for_entry<T: 'static>(
        &self,
        entry_id: u64,
        slot: &'static str,
        value: T,
    ) -> bool {
        let parent_saved = {
            let s = self.inner.borrow();
            if s.entries.len() <= 1 || s.entries.last().map(|entry| entry.id) != Some(entry_id) {
                return false;
            }
            s.entries[s.entries.len() - 2].saved.clone()
        };
        parent_saved.set_result(slot, value);
        self.pop_inner()
    }

    pub fn pop_with_result<T: 'static>(&self, slot: &'static str, value: T) -> bool {
        let entry_id = self.inner.borrow().entries.last().map(|entry| entry.id);
        entry_id.is_some_and(|entry_id| self.pop_with_result_for_entry(entry_id, slot, value))
    }

    pub fn set_result_for<T: 'static, F: Fn(&K) -> bool>(
        &self,
        predicate: F,
        slot: &'static str,
        value: T,
    ) -> bool {
        let s = self.inner.borrow();
        let Some(entry) = s.entries.iter().rev().find(|entry| predicate(&entry.key)) else {
            return false;
        };
        entry.saved.set_result(slot, value);
        drop(s);
        self.bump();
        true
    }

    /// Replace the top entry with a fresh destination.
    /// The replacement always gets a fresh `SavedState`: carrying the popped
    /// entry's state across would leak scroll/`remember_saveable` values
    /// between unrelated destinations.
    fn replace_inner(&self, key: K) {
        let old = {
            let mut s = self.inner.borrow_mut();
            let old = s.entries.pop();
            self.fresh_entry(&mut s, key);
            s.last_dir = TransitionDir::Push;
            s.transition_id = s.transition_id.wrapping_add(1);
            old
        };
        self.bump();
        if let Some(e) = old {
            e.scope.dispose();
        }
    }

    /// Serializes route keys only. `remember_saveable` values and one-shot
    /// results remain in memory and are intentionally not serialized.
    pub fn to_json(&self) -> String
    where
        K: Serialize,
    {
        let s = self.inner.borrow();
        let keys: Vec<&K> = s.entries.iter().map(|e| &e.key).collect();
        serde_json::to_string(&keys).unwrap_or_else(|e| {
            log::error!("NavBackStack::to_json serialization failed: {e}; keeping stack");
            serde_json::to_string(&keys).unwrap_or("[]".into())
        })
    }

    /// Restore from `to_json` output. Rejects empty/malformed payloads and
    /// always keeps at least one entry, so a failed restore can never leave
    /// the navigator rendering an empty screen with a dead back handler.
    pub fn from_json(&self, json: &str)
    where
        K: for<'de> Deserialize<'de>,
    {
        let Ok(keys) = serde_json::from_str::<Vec<K>>(json) else {
            log::error!("NavBackStack::from_json: malformed payload; keeping current stack");
            return;
        };
        if keys.is_empty() {
            log::error!("NavBackStack::from_json: empty stack rejected; keeping current stack");
            return;
        }
        let old_entries = {
            let mut s = self.inner.borrow_mut();
            let old = std::mem::take(&mut s.entries);
            for k in keys {
                self.fresh_entry(&mut s, k);
            }
            s.last_dir = TransitionDir::None;
            s.transition_id = s.transition_id.wrapping_add(1);
            old
        };
        self.bump();
        for e in old_entries {
            e.scope.dispose();
        }
    }
}

#[derive(Clone)]
pub struct Navigator<K: NavKey> {
    pub stack: NavBackStack<K>,
}
impl<K: NavKey> Navigator<K> {
    pub fn push(&self, k: K) {
        self.stack.push_inner(k);
        self.stack.bump();
    }
    pub fn replace(&self, k: K) {
        self.stack.replace_inner(k);
    }
    pub fn pop(&self) -> bool {
        if self.stack.size() <= 1 {
            return false;
        }
        self.stack.pop_inner()
    }
    pub fn pop_with_result<T: 'static>(&self, slot: &'static str, value: T) -> bool {
        self.stack.pop_with_result(slot, value)
    }
    pub fn set_result_for<T: 'static, F: Fn(&K) -> bool>(
        &self,
        predicate: F,
        slot: &'static str,
        value: T,
    ) -> bool {
        self.stack.set_result_for(predicate, slot, value)
    }
    pub fn clear_and_push(&self, k: K) {
        let old_entries = {
            let mut s = self.stack.inner.borrow_mut();
            let old = std::mem::take(&mut s.entries);
            self.stack.fresh_entry(&mut s, k);
            s.last_dir = TransitionDir::Push;
            s.transition_id = s.transition_id.wrapping_add(1);
            old
        };
        self.stack.bump();
        for e in old_entries {
            e.scope.dispose();
        }
    }
    pub fn pop_to<F: Fn(&K) -> bool>(&self, pred: F, inclusive: bool) {
        let count = {
            let s = self.stack.inner.borrow();
            if s.entries.is_empty() {
                0
            } else if let Some(idx) = s.entries.iter().rposition(|e| pred(&e.key)) {
                let raw = s.entries.len() - idx - (if inclusive { 0 } else { 1 });
                raw.min(s.entries.len().saturating_sub(1))
            } else {
                0
            }
        };
        self.stack.pop_entries(count);
    }
}

#[track_caller]
pub fn remember_back_stack<K: NavKey>(start: K) -> std::rc::Rc<NavBackStack<K>> {
    let caller = std::panic::Location::caller();
    remember_back_stack_with_key(
        format!(
            "nav3:stack:{}:{}:{}",
            file!(),
            caller.line(),
            caller.column()
        ),
        start,
    )
}

/// Explicit-key variant for dynamic hosts (loops, tab hosts) where several
/// stacks share one call site. Keys must be unique per stack.
pub fn remember_back_stack_with_key<K: NavKey>(
    key: impl Into<String>,
    start: K,
) -> std::rc::Rc<NavBackStack<K>> {
    let key = key.into();
    remember_with_key(key, || NavBackStack {
        inner: std::rc::Rc::new(std::cell::RefCell::new(BackState {
            entries: vec![Entry {
                id: 1,
                key: start,
                saved: std::rc::Rc::new(SavedState::default()),
                scope: Scope::new(),
            }],
            next_id: 2,
            last_dir: TransitionDir::None,
            transition_id: 0,
        })),
        version: std::rc::Rc::new(signal(0)),
        stack_id: unique_component_id(),
    })
}

pub struct EntryScope<K: NavKey> {
    id: u64,
    key: K,
    saved: Rc<SavedState>,
    scope: Scope,
    nav: Navigator<K>,
}
impl<K: NavKey> EntryScope<K> {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn is_current(&self) -> bool {
        self.nav.stack.is_current(self.id)
    }
    pub fn navigator(&self) -> Navigator<K> {
        self.nav.clone()
    }
    pub fn run<R>(&self, f: impl FnOnce() -> R) -> R {
        self.scope.run(f)
    }
    pub fn remember_saveable<T: 'static + Clone>(
        &self,
        slot: &'static str,
        init: impl FnOnce() -> T,
    ) -> Rc<RefCell<T>> {
        self.saved.remember(slot, init)
    }
    pub fn set_result<T: 'static>(&self, slot: &'static str, v: T) -> bool {
        if !self.is_current() {
            return false;
        }
        self.saved.set_result(slot, v);
        self.nav.stack.bump();
        true
    }
    pub fn try_take_result<T: 'static>(
        &self,
        slot: &'static str,
    ) -> Result<Option<T>, SavedStateTypeError> {
        if !self.is_current() {
            return Ok(None);
        }
        let result = self.saved.try_take_result(slot);
        if matches!(&result, Ok(Some(_))) {
            self.nav.stack.bump();
        }
        result
    }
    pub fn take_result<T: 'static>(&self, slot: &'static str) -> Option<T> {
        self.try_take_result(slot).ok().flatten()
    }
    pub fn pop_with_result<T: 'static>(&self, slot: &'static str, value: T) -> bool {
        self.nav
            .stack
            .pop_with_result_for_entry(self.id, slot, value)
    }
    pub fn set_result_for<T: 'static, F: Fn(&K) -> bool>(
        &self,
        predicate: F,
        slot: &'static str,
        value: T,
    ) -> bool {
        self.is_current() && self.nav.set_result_for(predicate, slot, value)
    }
}

pub type EntryRenderer<K> = Rc<dyn Fn(&EntryScope<K>) -> View>;
pub fn renderer<K: NavKey>(f: impl Fn(&EntryScope<K>) -> View + 'static) -> EntryRenderer<K> {
    Rc::new(f)
}

#[derive(Clone, Copy)]
pub struct NavTransition {
    pub slide_px: f32,
    pub fade: bool,
    pub spec: AnimationSpec,
}
impl Default for NavTransition {
    fn default() -> Self {
        Self {
            slide_px: 60.0,
            fade: true,
            spec: AnimationSpec::fast(),
        }
    }
}

pub fn NavDisplay<K: NavKey>(
    stack: Rc<NavBackStack<K>>,
    make_view: EntryRenderer<K>,
    on_back: Option<Rc<dyn Fn()>>,
    transition: NavTransition,
) -> View {
    let _version = stack.version.get();
    let transition_id = stack.transition_id();
    let (id, key, saved, entry_scope) = match stack.current() {
        Some(t) => t,
        None => return VBox(Modifier::new()),
    };
    let scope = EntryScope {
        id,
        key,
        saved,
        scope: entry_scope.clone(),
        nav: Navigator {
            stack: (*stack).clone(),
        },
    };

    let dir = stack.last_dir();
    if dir == TransitionDir::None {
        let v = entry_scope.run(|| (make_view)(&scope));
        return maybe_intercept_back(v, on_back);
    }

    let (initial, target) = if dir == TransitionDir::Push {
        (0.0, 1.0)
    } else {
        (1.0, 0.0)
    };
    let t = animate_f32_from(
        format!("nav3:{}:{}:{}", stack.stack_id, transition_id, id),
        initial,
        target,
        transition.spec,
    );

    let slide = if dir == TransitionDir::Push {
        1.0 - t
    } else {
        t
    };
    let dx = slide
        * transition.slide_px
        * if dir == TransitionDir::Push {
            1.0
        } else {
            -1.0
        };
    let alpha = if transition.fade {
        0.75 + 0.25 * (1.0 - slide)
    } else {
        1.0
    };

    let v = entry_scope.run(|| (make_view)(&scope));
    let framed = Column(Modifier::new().fill_max_size()).child(
        VBox(
            Modifier::new()
                .fill_max_size()
                .translate(dx, 0.0)
                .alpha(alpha),
        )
        .child(v),
    );
    maybe_intercept_back(framed, on_back)
}

fn maybe_intercept_back(v: View, on_back: Option<Rc<dyn Fn()>>) -> View {
    let Some(on_back) = on_back else { return v };
    // Per-screen handler runs on Escape key-down (desktop) and takes
    // precedence over the global `InstallBackHandler`..
    VBox(
        Modifier::new()
            .semantics(Semantics::new(Role::Container))
            .on_preview_key_event(move |ke: KeyEvent| {
                if ke.event_type != KeyEventType::Down || ke.is_repeat {
                    return false;
                }
                let action = repose_core::shortcuts::resolve_action(
                    repose_core::shortcuts::KeyChord::new(ke.key.clone(), ke.modifiers),
                );
                if ke.key == Key::Escape
                    || matches!(action, Some(repose_core::shortcuts::Action::Back))
                {
                    on_back();
                    true
                } else {
                    false
                }
            }),
    )
    .child(v)
}

/// Back-dispatcher
///
/// platform calls handle_back(); app sets handler during composition.
pub mod back {
    use std::{cell::RefCell, rc::Rc};

    type Handler = Rc<dyn Fn() -> bool>;

    struct Installed {
        handler: Handler,
        owner: String,
    }

    #[derive(Default)]
    struct Registry {
        current: Option<Handler>,
        installed: Vec<Installed>,
    }

    thread_local! {
        static REGISTRY: RefCell<Registry> = RefCell::new(Registry::default());
    }

    pub fn set(handler: Option<Handler>) {
        let _ = REGISTRY.try_with(|registry| registry.borrow_mut().current = handler);
    }

    pub(crate) fn current() -> Option<Handler> {
        REGISTRY
            .try_with(|registry| registry.borrow().current.clone())
            .unwrap_or(None)
    }

    fn same_handler(a: &Handler, b: &Handler) -> bool {
        Rc::ptr_eq(a, b)
    }

    pub(crate) fn install(handler: Handler, owner: String) -> Option<Handler> {
        REGISTRY
            .try_with(|slot| {
                let mut registry = slot.borrow_mut();
                let previous = registry.current.clone();
                registry.installed.retain(|entry| entry.owner != owner);
                registry.installed.push(Installed {
                    handler: handler.clone(),
                    owner,
                });
                registry.current = Some(handler);
                previous
            })
            .ok()
            .flatten()
    }

    pub(crate) fn restore(handler: &Handler) {
        let _ = REGISTRY.try_with(|slot| {
            let mut registry = slot.borrow_mut();
            let Some(index) = registry
                .installed
                .iter()
                .position(|entry| same_handler(&entry.handler, handler))
            else {
                return;
            };
            registry.installed.remove(index);
            if registry
                .current
                .as_ref()
                .is_some_and(|item| same_handler(item, handler))
            {
                registry.current = registry.installed.last().map(|entry| entry.handler.clone());
            }
        });
    }

    pub fn handle() -> bool {
        let handler = current();
        handler.is_some_and(|handler| handler())
    }

    #[cfg(test)]
    pub(crate) fn reset_for_test() {
        let _ = REGISTRY.try_with(|registry| *registry.borrow_mut() = Registry::default());
    }
}

/// Install/uninstall the global back handler for the displayed stack entry.
#[track_caller]
pub fn InstallBackHandler<K: NavKey>(stack: NavBackStack<K>) -> Dispose {
    let nav = Navigator {
        stack: stack.clone(),
    };
    let handler: Rc<dyn Fn() -> bool> = Rc::new(move || nav.pop());
    let handler_for_effect = handler.clone();
    let owner = format!("navigation-back:{}", stack.stack_id);
    let key = format!(
        "navigation-back-effect:{}:{}:{}:{}",
        file!(),
        line!(),
        column!(),
        owner
    );
    let install = move || {
        back::install(handler_for_effect.clone(), owner.clone());
        on_unmount(move || back::restore(&handler_for_effect))
    };
    if current_scope().is_some() {
        effect_once_with_key(key, install)
    } else {
        install()
    }
}

#[cfg(test)]
mod nav_state_tests {
    use super::*;

    #[test]
    fn from_json_rejects_empty_and_malformed() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        nav.push("details".to_string());
        assert_eq!(nav.stack.size(), 2);

        nav.stack.from_json("[]");
        assert_eq!(nav.stack.size(), 2, "empty restore must keep the stack");

        nav.stack.from_json("not json");
        assert_eq!(nav.stack.size(), 2, "malformed restore must keep the stack");

        nav.stack.from_json("[\"only\"]");
        assert_eq!(nav.stack.size(), 1);
    }

    #[test]
    fn pop_with_result_reaches_parent() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        nav.push("child".to_string());
        assert!(nav.pop_with_result("result", 42u32));
        let (_, _, saved, _) = nav.stack.top().expect("parent");
        assert!(saved.try_take_result::<String>("result").is_err());
        assert_eq!(saved.take_result::<u32>("result"), Some(42));
    }

    #[test]
    fn stale_entry_scope_cannot_pop_a_new_top() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        nav.push("child".to_string());
        let (child_id, child_key, child_saved, child_scope) = nav.stack.top().expect("child");
        let stale = EntryScope {
            id: child_id,
            key: child_key,
            saved: child_saved,
            scope: child_scope,
            nav: nav.clone(),
        };
        nav.push("new-top".to_string());
        assert!(!stale.set_result("stale", 2u32));
        assert!(!stale.pop_with_result("result", 1u32));
        assert_eq!(nav.stack.size(), 3);
    }

    #[test]
    fn result_delivery_invalidates_navigation_observers() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        let before = nav.stack.version.get();
        assert!(nav.set_result_for(|key| key == "home", "result", 7u32));
        assert_ne!(nav.stack.version.get(), before);
        let (_, _, saved, _) = nav.stack.top().expect("home");
        assert_eq!(saved.take_result::<u32>("result"), Some(7));
    }

    #[test]
    fn back_handler_restores_after_out_of_order_cleanup() {
        back::reset_for_test();
        let first: Rc<dyn Fn() -> bool> = Rc::new(|| false);
        let second: Rc<dyn Fn() -> bool> = Rc::new(|| false);
        let third_called = Rc::new(std::cell::Cell::new(false));
        let third: Rc<dyn Fn() -> bool> = {
            let third_called = third_called.clone();
            Rc::new(move || {
                third_called.set(true);
                true
            })
        };
        back::install(first.clone(), "first".into());
        back::install(second.clone(), "second".into());
        back::install(third.clone(), "third".into());
        back::restore(&second);
        assert!(back::handle());
        assert!(third_called.get());
        back::restore(&third);
        assert!(!back::handle());
        back::restore(&first);
        assert!(!back::handle());
        back::reset_for_test();
    }

    #[test]
    fn serialization_restores_routes_without_results() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        nav.push("details".to_string());
        assert!(nav.set_result_for(|key| key == "home", "result", 9u32));
        let json = nav.stack.to_json();
        nav.stack.from_json(&json);
        let home_saved = nav.stack.inner.borrow().entries[0].saved.clone();
        assert!(home_saved.take_result::<u32>("result").is_none());
    }

    #[test]
    fn replace_does_not_carry_saved_state() {
        repose_core::runtime::ComposeGuard::begin();
        let stack = remember_back_stack("home".to_string());
        let _hold = repose_core::runtime::ComposeGuard::begin();
        let nav = Navigator {
            stack: (*stack).clone(),
        };
        {
            let (_, _, saved, _) = nav.stack.top().expect("top");
            saved.set_result("slot", 42u32);
        }
        nav.replace("other".to_string());
        let (_, _, saved, _) = nav.stack.top().expect("top");
        assert!(
            saved.take_result::<u32>("slot").is_none(),
            "replace must start with fresh SavedState"
        );
    }
}
