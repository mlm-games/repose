use std::any::Any;
use std::cell::{Cell, RefCell};
use std::panic::Location;
use std::rc::Rc;

use rustc_hash::FxHashMap;

use crate::scope::Scope;
use crate::{Rect, Scene, View, semantics::Role};

thread_local! {
    pub static COMPOSER: RefCell<Composer> = RefCell::new(Composer::default());
    static ROOT_SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };

    /// A programmatic focus request, set by `FocusRequester::request_focus()` /
    /// `free_focus()`. Stores the view ID that should receive focus on the next frame,
    /// or `Some(CLEAR_FOCUS_MARKER)` to clear focus.
    static FOCUS_REQUEST: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Sentinel value meaning "clear focus entirely".
pub const CLEAR_FOCUS_MARKER: u64 = u64::MAX;

pub fn take_focus_request() -> Option<u64> {
    FOCUS_REQUEST.with(|r| r.replace(None))
}

/// A handle that can programmatically request focus for a widget.
///
/// Similar to Compose's `FocusRequester`. Create one via `remember(FocusRequester::new)`,
/// attach it via `.focus_requester(...)` on a modifier, and call `request_focus()` to
/// move keyboard focus to the associated widget on the next frame.
#[derive(Clone)]
pub struct FocusRequester {
    /// Target view ID, set during layout/paint by the modifier system.
    pub target: Rc<RefCell<Option<u64>>>,
}

impl FocusRequester {
    pub fn new() -> Self {
        Self {
            target: Rc::new(RefCell::new(None)),
        }
    }

    /// Request focus for the associated widget on the next frame.
    pub fn request_focus(&self) {
        if let Some(id) = *self.target.borrow() {
            FOCUS_REQUEST.with(|r| r.set(Some(id)));
        }
    }

    /// Free/clear focus from the associated widget on the next frame.
    /// If the associated widget currently has focus, focus is cleared entirely.
    /// Corresponds to Compose's `freeFocus()`.
    pub fn free_focus(&self) {
        FOCUS_REQUEST.with(|r| r.set(Some(CLEAR_FOCUS_MARKER)));
    }

    /// Request focus for the associated widget on the next frame,
    /// bypassing some focusability checks. Corresponds to Compose's
    /// `captureFocus()`, which is typically used internally by the focus system.
    /// In repose this is an alias for `request_focus()`.
    pub fn capture_focus(&self) {
        self.request_focus();
    }
}

impl Default for FocusRequester {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction for focus movement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Next,
    Previous,
    Left,
    Right,
    Up,
    Down,
}

/// A manager for programmatic focus navigation.
///
/// Wraps a `&Scheduler` and provides methods to move focus.
/// Can also be used standalone with a focus chain and focused element.
#[derive(Clone)]
pub struct FocusManager {
    /// The ordered list of focusable element IDs.
    pub chain: Vec<u64>,
    /// The currently focused element (if any).
    pub focused: Option<u64>,
    /// Hit regions for focus group lookups.
    pub hit_regions: Vec<HitRegion>,
}

impl FocusManager {
    pub fn new(chain: Vec<u64>, focused: Option<u64>) -> Self {
        Self {
            chain,
            focused,
            hit_regions: Vec::new(),
        }
    }

    /// Move focus in the given direction.
    /// Returns the new focused element ID, or `None` if no movement is possible.
    pub fn move_focus(&mut self, dir: FocusDirection) -> Option<u64> {
        match dir {
            FocusDirection::Next | FocusDirection::Previous => {
                self.move_tab(dir == FocusDirection::Previous)
            }
            _ => None, // use move_focus_spatial when hit regions are available
        }
    }

    /// Clears focus entirely on the next frame.
    /// Corresponds to Compose's `FocusManager.clearFocus()`.
    /// The `force` parameter is accepted for API compatibility; in repose
    /// focus is always cleared immediately (no keep-focus mechanism).
    pub fn clear_focus(&self, _force: bool) {
        FOCUS_REQUEST.with(|r| r.set(Some(CLEAR_FOCUS_MARKER)));
    }

    /// Spatial focus navigation: find the closest focusable element in a given
    /// direction using bounding rect geometry.
    pub fn move_focus_spatial(
        &mut self,
        dir: FocusDirection,
        hit_regions: &[HitRegion],
    ) -> Option<u64> {
        let next = spatial_focus_next(&self.chain, hit_regions, self.focused, dir)?;
        self.focused = Some(next);
        Some(next)
    }

    /// Tab forward or backward in the focus chain.
    /// When the current focus belongs to a focus group, navigation is restricted
    /// to elements within that group.
    pub fn move_tab(&mut self, reverse: bool) -> Option<u64> {
        if self.chain.is_empty() {
            return None;
        }
        let current_group = self.focused.and_then(|cur| {
            self.hit_regions
                .iter()
                .find(|h| h.id == cur)
                .and_then(|h| h.focus_group_id)
        });
        let next = if let Some(group_id) = current_group {
            // Build sub-chain of elements in the same focus group
            let sub_chain: Vec<u64> = self
                .chain
                .iter()
                .copied()
                .filter(|&id| {
                    id == group_id
                        || self
                            .hit_regions
                            .iter()
                            .any(|h| h.id == id && h.focus_group_id == Some(group_id))
                })
                .collect();
            if sub_chain.is_empty() {
                return None;
            }
            if let Some(cur) = self.focused {
                if let Some(idx) = sub_chain.iter().position(|&id| id == cur) {
                    if reverse {
                        if idx == 0 {
                            sub_chain[sub_chain.len() - 1]
                        } else {
                            sub_chain[idx - 1]
                        }
                    } else {
                        sub_chain[(idx + 1) % sub_chain.len()]
                    }
                } else {
                    sub_chain[0]
                }
            } else {
                sub_chain[0]
            }
        } else {
            if let Some(cur) = self.focused {
                if let Some(idx) = self.chain.iter().position(|&id| id == cur) {
                    if reverse {
                        if idx == 0 {
                            self.chain[self.chain.len() - 1]
                        } else {
                            self.chain[idx - 1]
                        }
                    } else {
                        self.chain[(idx + 1) % self.chain.len()]
                    }
                } else {
                    self.chain[0]
                }
            } else {
                self.chain[0]
            }
        };
        self.focused = Some(next);
        Some(next)
    }

    /// Set target ID on a FocusRequester (called during layout).
    pub fn set_requester_target(requester: &FocusRequester, id: u64) {
        *requester.target.borrow_mut() = Some(id);
    }
}

/// Find the next focusable element in a given spatial direction.
///
/// Uses the bounding rects from `hit_regions` to determine which element is
/// "next" in the given direction from the currently focused element.
pub fn spatial_focus_next(
    chain: &[u64],
    hit_regions: &[HitRegion],
    current: Option<u64>,
    dir: FocusDirection,
) -> Option<u64> {
    if chain.is_empty() {
        return None;
    }

    let current_rect =
        current.and_then(|id| hit_regions.iter().find(|h| h.id == id).map(|h| h.rect));

    // For Next/Previous, use tab-order navigation
    match dir {
        FocusDirection::Next | FocusDirection::Previous => {
            let mut fm = FocusManager {
                chain: chain.to_vec(),
                focused: current,
                hit_regions: hit_regions.to_vec(),
            };
            return fm.move_tab(dir == FocusDirection::Previous);
        }
        _ => {}
    }

    let (cx, cy) = match current_rect {
        Some(r) => (r.x + r.w / 2.0, r.y + r.h / 2.0),
        None => return chain.first().copied(),
    };

    let mut best: Option<(u64, f32)> = None;

    for &id in chain {
        if Some(id) == current {
            continue;
        }
        let Some(hr) = hit_regions.iter().find(|h| h.id == id) else {
            continue;
        };
        let r = hr.rect;
        let other_cx = r.x + r.w / 2.0;
        let other_cy = r.y + r.h / 2.0;
        let dx = other_cx - cx;
        let dy = other_cy - cy;

        let in_direction = match dir {
            FocusDirection::Left => dx < 0.0 && dy.abs() <= r.h.max(1.0),
            FocusDirection::Right => dx > 0.0 && dy.abs() <= r.h.max(1.0),
            FocusDirection::Up => dy < 0.0 && dx.abs() <= r.w.max(1.0),
            FocusDirection::Down => dy > 0.0 && dx.abs() <= r.w.max(1.0),
            _ => false,
        };

        if !in_direction {
            continue;
        }

        let dist = dx * dx + dy * dy;
        let weight = dist / (r.w * r.h + 1.0).max(1.0);

        match best {
            Some((_, best_weight)) if weight >= best_weight => {}
            _ => best = Some((id, weight)),
        }
    }

    best.map(|(id, _)| id)
}

#[derive(Default)]
pub struct Composer {
    pub slots: Vec<Box<dyn Any>>,
    /// Caller identity for each slot, used to detect stale slots
    /// when the composition tree changes between frames.
    pub slot_callers: Vec<&'static Location<'static>>,
    pub cursor: usize,
    pub keyed_slots: FxHashMap<String, Box<dyn Any>>,
    /// Per-scope cached state for the `scope!` macro.
    /// Keyed by the scope key string.
    pub scope_caches: FxHashMap<String, crate::scope_cache::ScopeCache>,
}

pub struct ComposeGuard {
    scope: Scope,
}

impl ComposeGuard {
    pub fn begin() -> Self {
        COMPOSER.with(|c| c.borrow_mut().cursor = 0);

        let scope = ROOT_SCOPE.with(|rs| {
            if let Some(existing) = rs.borrow().clone() {
                existing
            } else {
                let s = Scope::new();
                *rs.borrow_mut() = Some(s.clone());
                s
            }
        });

        ComposeGuard { scope }
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        // ROOT_SCOPE.with(|rs| { Do not clear every frame
        //     *rs.borrow_mut() = None;
        // });
    }
}

/// Slot-based remember (sequential composition only).
/// This prevents state aliasing when the composition tree structure changes between frames
#[track_caller]
pub fn remember<T: 'static>(init: impl FnOnce() -> T) -> Rc<T> {
    // Capture BEFORE any closure — Location::caller() returns the correct
    // track_caller location only at the function's top level, not inside closures.
    let caller = Location::caller();
    COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let cursor = c.cursor;
        c.cursor += 1;

        if cursor >= c.slots.len() {
            let rc: Rc<T> = Rc::new(init());
            c.slots.push(Box::new(rc.clone()));
            c.slot_callers.push(caller);
            return rc;
        }

        let stored_caller = c.slot_callers.get(cursor).copied();
        if stored_caller != Some(caller) {
            let rc: Rc<T> = Rc::new(init());
            c.slots[cursor] = Box::new(rc.clone());
            if cursor < c.slot_callers.len() {
                c.slot_callers[cursor] = caller;
            } else {
                c.slot_callers.push(caller);
            }
            return rc;
        }

        if let Some(rc) = c.slots[cursor].downcast_ref::<Rc<T>>() {
            rc.clone()
        } else {
            log::warn!(
                "remember: slot {} type changed {}. \
                 Use remember_with_key(key, || ...) for conditional branches.",
                cursor,
                std::any::type_name::<T>(),
            );
            let rc: Rc<T> = Rc::new(init());
            c.slots[cursor] = Box::new(rc.clone());
            rc
        }
    })
}

/// Key-based remember
pub fn remember_with_key<T: 'static>(key: impl Into<String>, init: impl FnOnce() -> T) -> Rc<T> {
    COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let key = key.into();

        if let Some(existing) = c.keyed_slots.get(&key) {
            if let Some(rc) = existing.downcast_ref::<Rc<T>>() {
                return rc.clone();
            } else {
                log::warn!(
                    "remember_with_key: key '{}' reused with a different type; replacing.",
                    key
                );
            }
        }

        if cfg!(debug_assertions) && c.keyed_slots.len() > 10_000 {
            log::warn!(
                "remember_with_key: more than 10k keys stored; \
                are you generating unbounded dynamic keys (e.g., using timestamps)?"
            );
        }

        let rc: Rc<T> = Rc::new(init());
        c.keyed_slots.insert(key, Box::new(rc.clone()));
        rc
    })
}

#[track_caller]
pub fn remember_state<T: 'static>(init: impl FnOnce() -> T) -> Rc<RefCell<T>> {
    remember(|| RefCell::new(init()))
}

pub fn remember_state_with_key<T: 'static>(
    key: impl Into<String>,
    init: impl FnOnce() -> T,
) -> Rc<RefCell<T>> {
    remember_with_key(key, || RefCell::new(init()))
}

/// Frame - output of composition for a tick: scene + input/semantics.
#[derive(Clone)]
pub struct Frame {
    pub scene: Scene,
    pub hit_regions: Vec<HitRegion>,
    pub semantics_nodes: Vec<SemNode>,
    pub focus_chain: Vec<u64>,
}

#[derive(Clone, Default)]
pub struct HitRegion {
    pub id: u64,
    pub rect: Rect,
    /// Tree depth: 0 = root, higher = deeper child. Used for three-pass
    /// pointer dispatch to determine ancestor/descendant ordering.
    pub depth: u32,
    pub on_click: Option<Rc<dyn Fn()>>,
    pub on_scroll: Option<Rc<dyn Fn(crate::Vec2) -> crate::Vec2>>,
    pub focusable: bool,
    pub on_pointer_down: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_cancel: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub z_index: f32,
    pub on_text_change: Option<Rc<dyn Fn(String)>>,
    pub on_text_submit: Option<Rc<dyn Fn(String)>>,
    /// If this hit region belongs to a TextField, this persistent key is used
    /// for looking up platform-managed TextFieldState. Falls back to `id` if None.
    pub tf_state_key: Option<u64>,

    /// True if this hit region corresponds to a multiline text input (TextArea).
    pub tf_multiline: bool,

    // internal
    pub on_drag_start: Option<Rc<dyn Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload>>>,
    pub on_drag_end: Option<Rc<dyn Fn(crate::dnd::DragEnd)>>,
    pub on_drag_enter: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_over: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_leave: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drop: Option<Rc<dyn Fn(crate::dnd::DropEvent) -> bool>>,

    pub on_action: Option<Rc<dyn Fn(crate::shortcuts::Action) -> bool>>,

    /// Called when a key event is received while this element is focused.
    /// Return `true` to consume the event.
    pub on_key_event: Option<Rc<dyn Fn(crate::input::KeyEvent) -> bool>>,
    /// Called before `on_key_event`. Return `true` to consume before normal dispatch.
    pub on_preview_key_event: Option<Rc<dyn Fn(crate::input::KeyEvent) -> bool>>,

    /// Cursor hint for desktop/web.
    pub cursor: Option<crate::CursorIcon>,

    /// If `Some(group_id)`, this hit region belongs to a focus group with the
    /// given id. Tab navigation will cycle within the group instead of moving
    /// to elements outside it. Set automatically by the layout engine when the
    /// element is a descendant of a node with `focus_group: true`.
    pub focus_group_id: Option<u64>,
}

impl HitRegion {
    /// Seed a HitRegion with all the modifier's event handlers + dnd + cursor.
    /// Call‑sites should only override the fields that differ (on_click, focusable, etc.)
    /// via struct‑update syntax: `HitRegion { focusable: true, ..from_modifier(..) }`.
    pub fn from_modifier(id: u64, rect: Rect, m: &crate::modifier::Modifier) -> Self {
        Self {
            id,
            rect,
            z_index: m.z_index,
            on_click: m.on_click.clone(),
            on_pointer_down: m.on_pointer_down.clone(),
            on_pointer_move: m.on_pointer_move.clone(),
            on_pointer_up: m.on_pointer_up.clone(),
            on_pointer_cancel: m.on_pointer_cancel.clone(),
            on_pointer_enter: m.on_pointer_enter.clone(),
            on_pointer_leave: m.on_pointer_leave.clone(),
            on_action: m.on_action.clone(),
            on_key_event: m.on_key_event.clone(),
            on_preview_key_event: m.on_preview_key_event.clone(),
            cursor: m.cursor,
            on_drag_start: m.on_drag_start.clone(),
            on_drag_end: m.on_drag_end.clone(),
            on_drag_enter: m.on_drag_enter.clone(),
            on_drag_over: m.on_drag_over.clone(),
            on_drag_leave: m.on_drag_leave.clone(),
            on_scroll: m.on_scroll.clone(),
            on_drop: m.on_drop.clone(),
            ..Default::default()
        }
    }
}

/// Flattened semantics node produced by `layout_and_paint`.
///
/// This is the source of truth for accessibility backends: it contains the
/// resolved screen rect, role, label, and focus/enabled state.
///
/// The platform runner should convert this into OS‑specific accessibility trees (when implemented)
/// (AT‑SPI on Linux, TalkBack on Android, etc.).
#[derive(Clone)]
pub struct SemNode {
    /// Stable id, shared with the associated `HitRegion` / `ViewId`.
    pub id: u64,

    /// `None` means direct child of the window root.
    pub parent: Option<u64>,

    pub role: Role,
    pub label: Option<String>,
    pub rect: Rect,
    pub focused: bool,
    pub enabled: bool,
    /// Marks this node as a collection of selectable children (e.g., Tabs).
    pub selectable_group: bool,
}

pub struct Scheduler {
    next_id: u64,
    /// Per-scope unique IDs, assigned lazily when a scope first executes.
    /// Keyed by the scope key string from `scope!`.
    scope_key_to_id: FxHashMap<String, u32>,
    next_scope_id: u32,
    /// When set, `id()` allocates from this scope's local counter instead of the global counter.
    /// The returned ID is `(scope_id << 32) | local_id`, which is stable even when
    /// prior sibling scopes change their view count.
    current_scope: Option<String>,
    /// Per-scope local ID counters. Reset to 0 when a scope re-executes.
    scope_local_counters: FxHashMap<String, u32>,
    pub focused: Option<u64>,
    pub size: (u32, u32),
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            scope_key_to_id: FxHashMap::default(),
            next_scope_id: 1,
            current_scope: None,
            scope_local_counters: FxHashMap::default(),
            focused: None,
            size: (1280, 800),
        }
    }

    /// Enter a named scope. Subsequent `id()` calls within this scope
    /// will allocate from the scope's local counter, producing packed
    /// `(scope_id << 32) | local_id` values that are stable across sibling
    /// recompositions.
    pub fn enter_scope(&mut self, key: &str) {
        self.current_scope = Some(key.to_string());
        // Reset local counter — the body will re-assign IDs fresh
        self.scope_local_counters.insert(key.to_string(), 0);
        // Ensure a scope_id exists (lazy allocation)
        self.get_or_create_scope_id(key);
    }

    /// Exit the current scope. Subsequent `id()` calls return global IDs again.
    pub fn exit_scope(&mut self) {
        self.current_scope = None;
    }

    fn get_or_create_scope_id(&mut self, key: &str) -> u32 {
        if let Some(&id) = self.scope_key_to_id.get(key) {
            id
        } else {
            let id = self.next_scope_id;
            self.next_scope_id += 1;
            self.scope_key_to_id.insert(key.to_string(), id);
            id
        }
    }

    pub fn id(&mut self) -> u64 {
        if let Some(key) = &self.current_scope {
            // Scope-local ID: packed (scope_id << 32) | local_id
            let scope_id = self.scope_key_to_id.get(key).copied().unwrap_or(0);
            let local = self.scope_local_counters.get_mut(key).unwrap();
            let id = *local;
            *local += 1;
            (scope_id as u64) << 32 | id as u64
        } else {
            // Global sequential ID (for non-scoped views)
            let id = self.next_id;
            self.next_id += 1;
            id
        }
    }

    pub fn id_count(&self) -> u64 {
        self.next_id - 1
    }

    /// Snapshot the current ID counter (before executing a scope body) so the
    /// delta can be computed after the body returns.
    pub fn snapshot_id(&self) -> u64 {
        self.next_id
    }

    /// Advance the ID counter by `count` without assigning IDs.
    /// Used by the scope! macro to reserve IDs for a cached scope subtree.
    pub fn advance_id(&mut self, count: u32) {
        self.next_id += count as u64;
    }

    /// Number of IDs assigned since `prev_id` (the value returned by
    /// `snapshot_id()` before executing a scope body).
    pub fn ids_used_since(&self, prev_id: u64) -> u32 {
        (self.next_id - prev_id) as u32
    }

    pub fn repose<F>(
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

/// Avoids cross-test pollution
#[cfg(test)]
pub fn clear_composer() {
    COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        c.slots.clear();
        c.slot_callers.clear();
        c.keyed_slots.clear();
        c.scope_caches.clear();
        c.cursor = 0;
    });
    ROOT_SCOPE.with(|rs| {
        *rs.borrow_mut() = None;
    });
}
