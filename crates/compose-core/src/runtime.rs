use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::scope::Scope;
use crate::{semantics::Role, semantics::Semantics, Color, Rect, Scene, View};

thread_local! {
    static COMPOSER: RefCell<Composer> = RefCell::new(Composer::default());
    static ROOT_SCOPE: RefCell<Option<Scope>> = RefCell::new(None);
}

#[derive(Default)]
struct Composer {
    slots: Vec<Box<dyn Any>>,
    cursor: usize,
    keyed_slots: HashMap<String, Box<dyn Any>>,
}

pub struct ComposeGuard {
    scope: Scope,
}

impl ComposeGuard {
    pub fn begin() -> Self {
        let scope = Scope::new();

        COMPOSER.with(|c| {
            let mut c = c.borrow_mut();
            c.cursor = 0;
        });

        ROOT_SCOPE.with(|rs| {
            *rs.borrow_mut() = Some(scope.clone());
        });

        ComposeGuard { scope }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        ROOT_SCOPE.with(|rs| {
            *rs.borrow_mut() = None;
        });
    }
}

/// Slot-based remember (sequential composition only)
pub fn remember<T: 'static>(init: impl FnOnce() -> T) -> Rc<T> {
    COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        if c.cursor >= c.slots.len() {
            c.slots.push(Box::new(Rc::new(init())));
        }
        let cursor = c.cursor;
        c.cursor += 1;
        let boxed = &c.slots[cursor];
        boxed.downcast_ref::<Rc<T>>().unwrap().clone()
    })
}

/// Key-based remember (safe with conditionals!)
pub fn remember_with_key<T: 'static>(key: impl Into<String>, init: impl FnOnce() -> T) -> Rc<T> {
    COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let key = key.into();

        if !c.keyed_slots.contains_key(&key) {
            c.keyed_slots.insert(key.clone(), Box::new(Rc::new(init())));
        }

        c.keyed_slots
            .get(&key)
            .unwrap()
            .downcast_ref::<Rc<T>>()
            .unwrap()
            .clone()
    })
}

pub fn remember_state<T: 'static>(init: impl FnOnce() -> T) -> Rc<RefCell<T>> {
    remember(|| RefCell::new(init()))
}

pub fn remember_state_with_key<T: 'static>(
    key: impl Into<String>,
    init: impl FnOnce() -> T,
) -> Rc<RefCell<T>> {
    remember_with_key(key, || RefCell::new(init()))
}

/// Frame — output of composition for a tick: scene + input/semantics.
pub struct Frame {
    pub scene: Scene,
    pub hit_regions: Vec<HitRegion>,
    pub semantics_nodes: Vec<SemNode>,
    pub focus_chain: Vec<u64>,
}

#[derive(Clone)]
pub struct HitRegion {
    pub id: u64,
    pub rect: Rect,
    pub on_click: Option<Rc<dyn Fn()>>,
    pub on_scroll: Option<Rc<dyn Fn(f32) -> f32>>,
    pub focusable: bool,
    pub on_pointer_down: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub z_index: f32,
}

#[derive(Clone)]
pub struct SemNode {
    pub id: u64,
    pub role: Role,
    pub label: Option<String>,
    pub rect: Rect,
    pub focused: bool,
}

pub struct Scheduler {
    next_id: u64,
    pub focused: Option<u64>,
    pub size: (u32, u32),
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            focused: None,
            size: (1280, 800),
        }
    }

    pub fn id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn compose<F>(
        &mut self,
        mut build_root: F,
        layout_paint: impl Fn(&View, (u32, u32)) -> (Scene, Vec<HitRegion>, Vec<SemNode>),
    ) -> Frame
    where
        F: FnMut(&mut Scheduler) -> View,
    {
        let guard = ComposeGuard::begin();
        let root = guard.scope.run(|| build_root(self));
        let (scene, hits, sem) = layout_paint(&root, self.size);

        let focus_chain: Vec<u64> = hits.iter().filter(|h| h.focusable).map(|h| h.id).collect();

        Frame {
            scene,
            hit_regions: hits,
            semantics_nodes: sem,
            focus_chain,
        }
    }
}
