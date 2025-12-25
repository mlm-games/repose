#![allow(non_snake_case)]
use std::{any::Any, cell::RefCell, fmt::Debug, rc::Rc};

use repose_core::*;
use repose_ui::{
    Box as VBox, Stack, ViewExt,
    anim::{animate_f32, animate_f32_from},
};
use serde::{Deserialize, Serialize};

pub trait NavKey: Clone + Debug + 'static + Serialize + for<'de> Deserialize<'de> {}
impl<T> NavKey for T where T: Clone + Debug + 'static + Serialize + for<'de> Deserialize<'de> {}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TransitionDir {
    None,
    Push,
    Pop,
}

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
    pub fn set_result<T: 'static>(&self, key: &'static str, val: T) {
        self.results.borrow_mut().insert(key, Box::new(val));
    }
    pub fn take_result<T: 'static>(&self, key: &'static str) -> Option<T> {
        self.results
            .borrow_mut()
            .remove(key)?
            .downcast::<T>()
            .ok()
            .map(|b| *b)
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
}

#[derive(Clone)]
pub struct NavBackStack<K: NavKey> {
    inner: Rc<RefCell<BackState<K>>>,
    version: Rc<Signal<u64>>,
}
impl<K: NavKey> NavBackStack<K> {
    pub fn top(&self) -> Option<(u64, K, Rc<SavedState>, Scope)> {
        let s = self.inner.borrow();
        s.entries
            .last()
            .map(|e| (e.id, e.key.clone(), e.saved.clone(), e.scope.clone()))
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

    fn push_inner(&self, key: K) {
        let mut s = self.inner.borrow_mut();
        let id = s.next_id;
        s.next_id += 1;
        s.entries.push(Entry {
            id,
            key,
            saved: Rc::new(SavedState::default()),
            scope: Scope::new(),
        });
        s.last_dir = TransitionDir::Push;
    }

    /// Pop the top entry (if any) and dispose its scope.
    fn pop_inner(&self) -> bool {
        let entry = {
            let mut s = self.inner.borrow_mut();
            s.last_dir = TransitionDir::Pop;
            s.entries.pop()
        };

        if let Some(e) = entry {
            e.scope.dispose();
            true
        } else {
            false
        }
    }

    fn replace_inner(&self, key: K) {
        let mut s = self.inner.borrow_mut();
        if let Some(last) = s.entries.last_mut() {
            last.key = key;
        } else {
            let id = s.next_id;
            s.next_id += 1;
            s.entries.push(Entry {
                id,
                key,
                saved: Rc::new(SavedState::default()),
                scope: Scope::new(),
            });
        }
        s.last_dir = TransitionDir::Push;
    }

    pub fn to_json(&self) -> String
    where
        K: Serialize,
    {
        let s = self.inner.borrow();
        let keys: Vec<&K> = s.entries.iter().map(|e| &e.key).collect();
        serde_json::to_string(&keys).unwrap_or("[]".into())
    }

    pub fn from_json(&self, json: &str)
    where
        K: for<'de> Deserialize<'de>,
    {
        if let Ok(keys) = serde_json::from_str::<Vec<K>>(json) {
            // Dispose all existing scopes before clearing.
            let old_entries = {
                let mut s = self.inner.borrow_mut();
                std::mem::take(&mut s.entries)
            };
            for e in old_entries {
                e.scope.dispose();
            }

            let mut s = self.inner.borrow_mut();
            s.entries = Vec::new();
            for k in keys {
                let id = s.next_id;
                s.next_id += 1;
                s.entries.push(Entry {
                    id,
                    key: k,
                    saved: Rc::new(SavedState::default()),
                    scope: Scope::new(),
                });
            }
            s.last_dir = TransitionDir::None;
            drop(s);
            self.bump();
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
        self.stack.bump();
    }
    pub fn pop(&self) -> bool {
        // Don't pop if only one entry is present
        if self.stack.size() <= 1 {
            return false;
        }
        let ok = self.stack.pop_inner();
        if ok {
            self.stack.bump();
        }
        ok
    }
    pub fn clear_and_push(&self, k: K) {
        while self.stack.pop_inner() {}
        self.stack.push_inner(k);
        self.stack.bump();
    }
    pub fn pop_to<F: Fn(&K) -> bool>(&self, pred: F, inclusive: bool) {
        let count = {
            let s = self.stack.inner.borrow();
            if let Some(idx) = s.entries.iter().rposition(|e| pred(&e.key)) {
                s.entries.len() - idx - (if inclusive { 0 } else { 1 })
            } else {
                0
            }
        };
        for _ in 0..count {
            let _ = self.stack.pop_inner();
        }
        if count > 0 {
            self.stack.bump();
        }
    }
}

pub fn remember_back_stack<K: NavKey>(start: K) -> std::rc::Rc<NavBackStack<K>> {
    remember_with_key("nav3:stack", || NavBackStack {
        inner: std::rc::Rc::new(std::cell::RefCell::new(BackState {
            entries: vec![Entry {
                id: 1,
                key: start,
                saved: std::rc::Rc::new(SavedState::default()),
                scope: Scope::new(),
            }],
            next_id: 2,
            last_dir: TransitionDir::None,
        })),
        version: std::rc::Rc::new(signal(0)),
    })
}

pub struct EntryScope<K: NavKey> {
    id: u64,
    key: K,
    saved: Rc<SavedState>,
    nav: Navigator<K>,
}
impl<K: NavKey> EntryScope<K> {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn key(&self) -> &K {
        &self.key
    }
    pub fn navigator(&self) -> Navigator<K> {
        self.nav.clone()
    }
    pub fn remember_saveable<T: 'static + Clone>(
        &self,
        slot: &'static str,
        init: impl FnOnce() -> T,
    ) -> Rc<RefCell<T>> {
        self.saved.remember(slot, init)
    }
    pub fn set_result<T: 'static>(&self, slot: &'static str, v: T) {
        self.saved.set_result(slot, v)
    }
    pub fn take_result<T: 'static>(&self, slot: &'static str) -> Option<T> {
        self.saved.take_result(slot)
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
    let _v = stack.version.get(); // join reactive graph
    let (id, key, saved, entry_scope) = match stack.top() {
        Some(t) => t,
        None => return VBox(Modifier::new()),
    };
    let scope = EntryScope {
        id,
        key,
        saved,
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
    let t = animate_f32_from(format!("nav3:{id}"), initial, target, transition.spec);

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
    let framed = Stack(Modifier::new().fill_max_size())
        .child(VBox(Modifier::new().translate(dx, 0.0).alpha(alpha)).child(v));
    maybe_intercept_back(framed, on_back)
}

fn maybe_intercept_back(v: View, _on_back: Option<Rc<dyn Fn()>>) -> View {
    // placeholder: platform loop will call the back handler; we expose setter below.
    v
}

/// Back-dispatcher
///
/// platform calls handle_back(); app sets handler during composition.
pub mod back {
    use std::{cell::RefCell, rc::Rc};

    type Handler = Rc<dyn Fn() -> bool>;

    thread_local! {
        static H: RefCell<Option<Handler>> = RefCell::new(None);
    }

    pub fn set(handler: Option<Handler>) {
        H.with(|h| *h.borrow_mut() = handler);
    }

    pub fn handle() -> bool {
        H.with(|h| {
            if let Some(handler) = h.borrow().as_ref() {
                handler()
            } else {
                false
            }
        })
    }
}

/// Install/uninstall the global back handler for the displayed stack.
pub fn InstallBackHandler<K: NavKey>(stack: NavBackStack<K>) -> Dispose {
    let nav = Navigator {
        stack: stack.clone(),
    };
    back::set(Some(Rc::new(move || nav.pop())));
    on_unmount(|| back::set(None))
}
