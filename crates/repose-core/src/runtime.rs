use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::panic::Location;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use rustc_hash::{FxHashMap, FxHashSet};

use crate::effects::Dispose;
use crate::scope::Scope;
use crate::{CursorIcon, Rect, Scene, View, input::PhysicalKey, semantics::Role};

thread_local! {
    pub static COMPOSER: RefCell<Composer> = RefCell::new(Composer::default());
    static ROOT_SCOPE: RefCell<Option<Scope>> = const { RefCell::new(None) };
    static INITIALIZER_STACK: RefCell<Vec<InitializerKind>> =
        const { RefCell::new(Vec::new()) };
    static PENDING_INITIALIZER_CURSOR: RefCell<Option<usize>> =
        const { RefCell::new(None) };
    static KEYED_DISPOSERS: RefCell<FxHashMap<String, KeyedDisposerEntry>> =
        RefCell::new(FxHashMap::default());
    static KEYED_DISPOSER_ORDER: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static PENDING_KEYED_REGISTRATIONS: RefCell<Vec<(String, Dispose, String)>> =
        const { RefCell::new(Vec::new()) };

    /// Programmatic focus requests queued by `FocusRequester`. A queue (not a
    /// single slot) so multiple requests in one frame are honored in order
    /// instead of last-wins. `CLEAR_FOCUS_MARKER` entries clear focus.
    static FOCUS_REQUESTS: RefCell<std::collections::VecDeque<u64>> =
        const { RefCell::new(std::collections::VecDeque::new()) };
}

pub const CLEAR_FOCUS_MARKER: u64 = u64::MAX;

const ROOT_OWNER: &str = "\0repose:root-owner";

struct KeyedDisposerEntry {
    disposer: Dispose,
    owners: FxHashSet<String>,
}

enum InitializerKind {
    Sequential,
    Keyed,
}

pub(crate) fn scope_owner_token(scope: Option<&str>) -> String {
    match scope {
        Some(scope) if !scope.is_empty() => scope.to_string(),
        _ => ROOT_OWNER.to_string(),
    }
}

fn queue_keyed_registration(key: String, disposer: Dispose, owner: String) {
    let queued = PENDING_KEYED_REGISTRATIONS.try_with(|pending| {
        pending.try_borrow_mut().ok().map(|mut pending| {
            pending.push((key, disposer, owner));
            true
        })
    });
    if !matches!(queued, Ok(Some(true))) {
        crate::request_frame();
    }
}

pub(crate) fn flush_keyed_disposer_registrations() {
    let pending = PENDING_KEYED_REGISTRATIONS.try_with(|pending| {
        pending
            .try_borrow_mut()
            .ok()
            .map(|mut pending| std::mem::take(&mut *pending))
    });
    let Ok(Some(pending)) = pending else {
        crate::request_frame();
        return;
    };
    for (key, disposer, owner) in pending {
        register_keyed_disposer_for_owner(key, disposer, owner);
    }
}

pub(crate) fn register_keyed_disposer(key: String, disposer: Dispose) {
    let owner = scope_owner_token(crate::scope_cache::current_scope_key().as_deref());
    register_keyed_disposer_for_owner(key, disposer, owner);
}

pub(crate) fn register_keyed_disposer_for_owner(key: String, disposer: Dispose, owner: String) {
    let result = KEYED_DISPOSERS
        .try_with(|registry| {
            let mut registry = match registry.try_borrow_mut() {
                Ok(registry) => registry,
                Err(_) => return Err(()),
            };
            if let Some(entry) = registry.get_mut(&key) {
                if entry.owners.contains(&owner) && !entry.disposer.ptr_eq(&disposer) {
                    let previous = std::mem::replace(&mut entry.disposer, disposer.clone());
                    return Ok((false, Some(previous)));
                }
                entry.owners.insert(owner.clone());
                return Ok((false, None));
            }
            registry.insert(
                key.clone(),
                KeyedDisposerEntry {
                    disposer: disposer.clone(),
                    owners: FxHashSet::from_iter([owner.clone()]),
                },
            );
            Ok((true, None))
        })
        .ok();
    match result {
        Some(Ok((true, previous))) => {
            let _ = KEYED_DISPOSER_ORDER.try_with(|order| {
                if let Ok(mut order) = order.try_borrow_mut()
                    && !order.iter().any(|registered| registered == &key)
                {
                    order.push(key.clone());
                }
            });
            drop(previous);
        }
        Some(Ok((false, previous))) => {
            if let Some(previous) = previous {
                run_keyed_disposer(previous);
            }
        }
        Some(Err(())) | None => queue_keyed_registration(key, disposer, owner),
    }
}

pub(crate) fn keyed_disposer_has_owner(key: &str, disposer: &Dispose, owner: &str) -> bool {
    KEYED_DISPOSERS
        .try_with(|registry| {
            registry.try_borrow().ok().and_then(|registry| {
                registry
                    .get(key)
                    .filter(|entry| entry.disposer.ptr_eq(disposer))
                    .map(|entry| entry.owners.contains(owner))
            })
        })
        .ok()
        .flatten()
        .unwrap_or(false)
}

pub(crate) fn remove_keyed_disposer_for_owner(key: &str, disposer: &Dispose, owner: &str) -> bool {
    let mut to_run = None;
    let mut owner_removed = false;
    let mut registry_removed = false;
    let result = KEYED_DISPOSERS.try_with(|registry| {
        let mut registry = match registry.try_borrow_mut() {
            Ok(registry) => registry,
            Err(_) => return false,
        };
        let Some(entry) = registry.get_mut(key) else {
            return true;
        };
        if !entry.disposer.ptr_eq(disposer) {
            return true;
        }
        owner_removed = entry.owners.remove(owner);
        if entry.owners.is_empty() {
            if let Some(entry) = registry.remove(key) {
                to_run = Some(entry.disposer);
                registry_removed = true;
            }
        }
        true
    });
    if !matches!(result, Ok(true)) {
        queue_keyed_registration(key.to_string(), disposer.clone(), owner.to_string());
        return false;
    }
    if registry_removed {
        let _ = KEYED_DISPOSER_ORDER.try_with(|order| {
            if let Ok(mut order) = order.try_borrow_mut() {
                order.retain(|registered| registered != key);
            }
        });
    }
    if let Some(disposer) = to_run {
        run_keyed_disposer(disposer);
        true
    } else {
        owner_removed
    }
}

pub(crate) fn take_keyed_disposer(key: &str) -> Option<Dispose> {
    let disposer = KEYED_DISPOSERS
        .try_with(|registry| {
            registry
                .try_borrow_mut()
                .ok()?
                .remove(key)
                .map(|entry| entry.disposer)
        })
        .ok()
        .flatten();
    if disposer.is_some() {
        let _ = KEYED_DISPOSER_ORDER.try_with(|order| {
            if let Ok(mut order) = order.try_borrow_mut() {
                order.retain(|registered| registered != key);
            }
        });
    }
    disposer
}

pub(crate) fn take_all_keyed_disposers() -> Vec<Dispose> {
    flush_keyed_disposer_registrations();
    let order = KEYED_DISPOSER_ORDER
        .try_with(|order| {
            order
                .try_borrow_mut()
                .ok()
                .map(|mut order| std::mem::take(&mut *order))
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    let disposers = KEYED_DISPOSERS
        .try_with(|registry| {
            registry.try_borrow_mut().ok().map(|mut registry| {
                let mut result = Vec::with_capacity(registry.len());
                for key in order.into_iter().rev() {
                    if let Some(entry) = registry.remove(&key) {
                        result.push(entry.disposer);
                    }
                }
                result.extend(registry.drain().map(|(_, entry)| entry.disposer));
                result
            })
        })
        .ok()
        .flatten()
        .unwrap_or_default();
    disposers
}

pub(crate) fn run_keyed_disposer(disposer: Dispose) {
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| disposer.run()));
}

/// Process-wide unique id for namespacing `remember_with_key` slots
/// (dialogs, menus, sheets, tooltips, ...).
static COMPONENT_ID: AtomicU64 = AtomicU64::new(1);

/// Returns a fresh unique component id. See [`COMPONENT_ID`].
pub fn unique_component_id() -> u64 {
    COMPONENT_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn take_focus_request() -> Option<u64> {
    FOCUS_REQUESTS.with(|r| r.borrow_mut().pop_front())
}

/// Drain all queued focus requests in FIFO order.
pub fn drain_focus_requests() -> Vec<u64> {
    FOCUS_REQUESTS.with(|r| r.borrow_mut().drain(..).collect())
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
    /// Queued; unlike the old single-slot behavior, multiple requests in one
    /// frame are all honored in order. If the target is not laid out yet the
    /// request is dropped (target assigned during layout/paint).
    pub fn request_focus(&self) {
        if let Some(id) = *self.target.borrow() {
            FOCUS_REQUESTS.with(|r| r.borrow_mut().push_back(id));
        }
    }

    /// Free/clear focus from the associated widget on the next frame.
    /// If the associated widget currently has focus, focus is cleared entirely.
    /// Corresponds to Compose's `freeFocus()`.
    pub fn free_focus(&self) {
        FOCUS_REQUESTS.with(|r| r.borrow_mut().push_back(CLEAR_FOCUS_MARKER));
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
    /// The `force` parameter is accepted for API compatibility. In repose
    /// focus is always cleared immediately (no keep-focus mechanism).
    pub fn clear_focus(&self, _force: bool) {
        FOCUS_REQUESTS.with(|r| r.borrow_mut().push_back(CLEAR_FOCUS_MARKER));
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
        let next = if let Some(sub_chain) =
            focus_group_chain(&self.chain, &self.hit_regions, self.focused)
        {
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
            } else if reverse {
                sub_chain[sub_chain.len() - 1]
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
            } else if reverse {
                self.chain[self.chain.len() - 1]
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

/// Sub-chain of ids sharing the focused element's focus group.
/// Returns `None` when focus is outside any group (full chain applies).
/// Tab and spatial navigation both scope to this so modals trap focus.
pub fn focus_group_chain(
    chain: &[u64],
    hit_regions: &[HitRegion],
    current: Option<u64>,
) -> Option<Vec<u64>> {
    let cur = current?;
    let group_id = hit_regions.iter().find(|h| h.id == cur)?.focus_group_id?;
    Some(
        chain
            .iter()
            .copied()
            .filter(|&id| {
                id == group_id
                    || hit_regions
                        .iter()
                        .any(|h| h.id == id && h.focus_group_id == Some(group_id))
            })
            .collect(),
    )
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
        None => {
            // For games: Tab/Shift-Tab should establish initial focus, not arrows.
            return None;
        }
    };

    let mut best: Option<(u64, f32)> = None;

    // Modal trap: arrows stay inside the focused element's focus group.
    let scoped: Vec<u64>;
    let chain: &[u64] = match focus_group_chain(chain, hit_regions, current) {
        Some(sub) => {
            scoped = sub;
            &scoped
        }
        None => chain,
    };

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
    pub slot_callers: Vec<&'static Location<'static>>,
    pub cursor: usize,
    pub keyed_slots: FxHashMap<String, Box<dyn Any>>,
    pub keyed_owner: FxHashMap<String, String>,
    pub keyed_owners: FxHashMap<String, rustc_hash::FxHashSet<String>>,
    pub live_keyed_owners: rustc_hash::FxHashSet<String>,
    pub scope_caches: FxHashMap<String, crate::scope_cache::ScopeCache>,
    pub live_scope_keys: rustc_hash::FxHashSet<String>,
}

pub struct ComposeGuard {
    scope: Scope,
}

pub(crate) fn keyed_disposer_order(key: &str) -> usize {
    KEYED_DISPOSER_ORDER
        .try_with(|order| {
            order
                .try_borrow()
                .ok()
                .and_then(|order| order.iter().position(|registered| registered == key))
        })
        .ok()
        .flatten()
        .unwrap_or(usize::MAX)
}

pub(crate) fn release_keyed_owner(key: &str, owner: &str) -> (bool, Option<Box<dyn Any>>) {
    let result = COMPOSER
        .try_with(|composer| {
            let mut composer = match composer.try_borrow_mut() {
                Ok(composer) => composer,
                Err(_) => return (false, None, None),
            };
            let owners = match composer.keyed_owners.get_mut(key) {
                Some(owners) => owners,
                None => return (false, None, None),
            };
            owners.remove(owner);
            if !owners.is_empty() {
                return (true, None, None);
            }
            composer.keyed_owners.remove(key);
            let old_owner = composer.keyed_owner.remove(key);
            (true, composer.keyed_slots.remove(key), old_owner)
        })
        .unwrap_or((false, None, None));
    drop(result.2);
    (result.0, result.1)
}

pub(crate) fn take_dead_keyed_slots(composer: &mut Composer) -> Vec<(String, Box<dyn Any>)> {
    let dead: Vec<String> = composer
        .keyed_slots
        .keys()
        .filter(|key| {
            let live = composer
                .keyed_owners
                .get(*key)
                .map(|owners| {
                    owners
                        .iter()
                        .any(|owner| composer.live_keyed_owners.contains(owner))
                })
                .or_else(|| {
                    composer
                        .keyed_owner
                        .get(*key)
                        .map(|owner| composer.live_keyed_owners.contains(owner))
                })
                .unwrap_or(false);
            !live
        })
        .cloned()
        .collect();
    let mut removed = Vec::with_capacity(dead.len());
    for key in dead {
        if let Some(value) = composer.keyed_slots.remove(&key) {
            removed.push((key.clone(), value));
        }
        composer.keyed_owner.remove(&key);
        composer.keyed_owners.remove(&key);
    }
    removed
}

pub(crate) fn current_scope_key_for_remember() -> Option<String> {
    crate::scope_cache::current_scope_key()
}

fn flush_initializer_cursor() {
    let pending = PENDING_INITIALIZER_CURSOR
        .try_with(|pending| {
            pending
                .try_borrow_mut()
                .ok()
                .and_then(|mut pending| pending.take())
        })
        .ok()
        .flatten();
    let Some(cursor) = pending else {
        return;
    };
    let restored = COMPOSER
        .try_with(|composer| {
            composer
                .try_borrow_mut()
                .ok()
                .map(|mut composer| composer.cursor = cursor)
        })
        .ok()
        .flatten();
    if restored.is_none() {
        let _ = PENDING_INITIALIZER_CURSOR.try_with(|pending| {
            if let Ok(mut pending) = pending.try_borrow_mut() {
                *pending = Some(cursor);
            }
        });
    }
}

impl ComposeGuard {
    pub fn begin() -> Self {
        flush_initializer_cursor();
        COMPOSER.with(|c| {
            let mut c = c.borrow_mut();
            c.cursor = 0;
            c.live_scope_keys.clear();
            c.live_keyed_owners.clear();
            c.live_scope_keys.insert(String::new());
            c.live_keyed_owners.insert(scope_owner_token(None));
        });

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

    pub fn live_scope_keys(&self) -> rustc_hash::FxHashSet<String> {
        COMPOSER.with(|c| c.borrow().live_scope_keys.clone())
    }
}

impl Drop for ComposeGuard {
    fn drop(&mut self) {
        let removed = COMPOSER.with(|c| {
            let mut c = c.borrow_mut();
            let n = c.cursor;
            let mut removed = Vec::new();
            if c.slots.len() > n {
                removed.extend(c.slots.drain(n..));
            }
            if c.slot_callers.len() > n {
                c.slot_callers.truncate(n);
            }
            removed
        });
        drop(removed);
        crate::scope_cache::gc_dead_scopes();
    }
}

/// Dispose the root composition scope and clear all composer caches.
///
/// Call once on process exit (desktop `exiting`, tests). After this the next
/// `ComposeGuard::begin` starts from a fresh root scope.
pub fn shutdown_composition() {
    let scope = ROOT_SCOPE.with(|rs| rs.borrow_mut().take());
    if let Some(scope) = scope {
        scope.dispose();
    }
    let removed = COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let slots = std::mem::take(&mut c.slots);
        let callers = std::mem::take(&mut c.slot_callers);
        let keyed = std::mem::take(&mut c.keyed_slots);
        let caches = std::mem::take(&mut c.scope_caches);
        c.keyed_owner.clear();
        c.keyed_owners.clear();
        c.live_keyed_owners.clear();
        c.live_scope_keys.clear();
        c.cursor = 0;
        (slots, callers, keyed, caches)
    });
    drop(removed);
    for disposer in take_all_keyed_disposers() {
        run_keyed_disposer(disposer);
    }
    crate::animation_driver::shutdown();
    crate::scope_cache::clear_all_scope_deps();
    INITIALIZER_STACK.with(|stack| stack.borrow_mut().clear());
    PENDING_INITIALIZER_CURSOR.with(|pending| {
        let _ = pending.borrow_mut().take();
    });
}

struct InitializerGuard {
    active: bool,
    cursor: Option<usize>,
}

impl InitializerGuard {
    fn assert_sequential_allowed() {
        assert!(
            INITIALIZER_STACK.with(|stack| stack
                .try_borrow()
                .map(|stack| stack.is_empty())
                .unwrap_or(false)),
            "remember initializer re-entered composition; use an explicit keyed helper"
        );
    }

    fn enter_sequential() -> (Self, usize) {
        Self::assert_sequential_allowed();
        let cursor = COMPOSER.with(|composer| {
            let mut composer = composer.borrow_mut();
            let cursor = composer.cursor;
            composer.cursor = composer.cursor.wrapping_add(1);
            cursor
        });
        let pushed = INITIALIZER_STACK
            .try_with(|stack| {
                stack.try_borrow_mut().ok().map(|mut stack| {
                    stack.push(InitializerKind::Sequential);
                    true
                })
            })
            .ok()
            .flatten();
        if pushed != Some(true) {
            let _ = COMPOSER.try_with(|composer| {
                if let Ok(mut composer) = composer.try_borrow_mut() {
                    composer.cursor = cursor;
                }
            });
            panic!("initializer stack busy");
        }
        (
            Self {
                active: true,
                cursor: Some(cursor),
            },
            cursor,
        )
    }

    fn assert_keyed_allowed() {
        assert!(
            INITIALIZER_STACK.with(|stack| stack
                .try_borrow()
                .map(|stack| stack.is_empty())
                .unwrap_or(false)),
            "remember initializer re-entered composition; use an explicit keyed helper"
        );
    }

    fn enter_keyed() -> Self {
        Self::assert_keyed_allowed();
        INITIALIZER_STACK.with(|stack| {
            stack
                .try_borrow_mut()
                .expect("initializer stack busy")
                .push(InitializerKind::Keyed)
        });
        Self {
            active: true,
            cursor: None,
        }
    }

    fn commit(&mut self) {
        if !self.active {
            return;
        }
        let _ = INITIALIZER_STACK.try_with(|stack| {
            if let Ok(mut stack) = stack.try_borrow_mut() {
                stack.pop();
            }
        });
        self.active = false;
    }
}

impl Drop for InitializerGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(cursor) = self.cursor {
            let restored = COMPOSER
                .try_with(|composer| {
                    composer
                        .try_borrow_mut()
                        .ok()
                        .map(|mut composer| composer.cursor = cursor)
                })
                .ok()
                .flatten();
            if restored.is_none() {
                let _ = PENDING_INITIALIZER_CURSOR.try_with(|pending| {
                    if let Ok(mut pending) = pending.try_borrow_mut() {
                        *pending = Some(cursor);
                    }
                });
            }
        }
        let _ = INITIALIZER_STACK.try_with(|stack| {
            if let Ok(mut stack) = stack.try_borrow_mut() {
                stack.pop();
            }
        });
    }
}

/// Slot-based remember (sequential composition only).
/// This prevents state aliasing when the composition tree structure changes between frames
#[track_caller]
pub fn remember<T: 'static>(init: impl FnOnce() -> T) -> Rc<T> {
    flush_initializer_cursor();
    let caller = Location::caller();
    let (mut initializer, cursor) = InitializerGuard::enter_sequential();

    let existing = COMPOSER.with(|c| {
        let c = c.borrow();
        if cursor >= c.slots.len() || c.slot_callers.get(cursor).copied() != Some(caller) {
            return None;
        }
        c.slots[cursor].downcast_ref::<Rc<T>>().cloned()
    });
    if let Some(rc) = existing {
        initializer.commit();
        return rc;
    }

    if COMPOSER.with(|c| {
        let c = c.borrow();
        cursor < c.slots.len() && c.slot_callers.get(cursor).copied() == Some(caller)
    }) {
        log::warn!(
            "remember: slot {} type changed {}. \
             Use remember_with_key(key, || ...) for conditional branches.",
            cursor,
            std::any::type_name::<T>(),
        );
    }

    let rc = Rc::new(init());
    let old = COMPOSER.with(|c| {
        let mut c = c.borrow_mut();
        let value: Box<dyn Any> = Box::new(rc.clone());
        let old = if cursor < c.slots.len() {
            Some(std::mem::replace(&mut c.slots[cursor], value))
        } else {
            c.slots.push(value);
            None
        };
        if cursor < c.slot_callers.len() {
            c.slot_callers[cursor] = caller;
        } else {
            c.slot_callers.push(caller);
        }
        old
    });
    initializer.commit();
    drop(old);
    rc
}

/// Key-based remember.
pub fn remember_with_key<T: 'static>(key: impl Into<String>, init: impl FnOnce() -> T) -> Rc<T> {
    flush_initializer_cursor();
    InitializerGuard::assert_keyed_allowed();
    let key = key.into();
    let scope = current_scope_key_for_remember();
    let owner = scope_owner_token(scope.as_deref());
    let existing = COMPOSER.with(|composer| {
        let composer = composer.borrow();
        composer
            .keyed_slots
            .get(&key)
            .and_then(|value| value.downcast_ref::<Rc<T>>().cloned())
    });
    if let Some(existing) = existing {
        let old_owner = COMPOSER.with(|composer| {
            let mut composer = composer.borrow_mut();
            let old_owner = composer.keyed_owner.insert(key.clone(), owner.clone());
            composer
                .keyed_owners
                .entry(key.clone())
                .or_default()
                .insert(owner.clone());
            composer.live_keyed_owners.insert(owner);
            old_owner
        });
        drop(old_owner);
        return existing;
    }

    let has_wrong_type = COMPOSER.with(|composer| {
        let composer = composer.borrow();
        if composer.keyed_slots.contains_key(&key) {
            log::warn!(
                "remember_with_key: key '{}' reused with a different type; replacing.",
                key
            );
        }
        cfg!(debug_assertions) && composer.keyed_slots.len() > 10_000
    });
    if has_wrong_type {
        log::warn!(
            "remember_with_key: more than 10k keys stored; are you generating unbounded dynamic keys?"
        );
    }

    let mut initializer = InitializerGuard::enter_keyed();
    let value = Rc::new(init());
    let (result, old) = COMPOSER.with(|composer| {
        let mut composer = composer.borrow_mut();
        if let Some(existing) = composer.keyed_slots.get(&key)
            && let Some(existing) = existing.downcast_ref::<Rc<T>>()
        {
            let existing = existing.clone();
            composer.keyed_owner.insert(key.clone(), owner.clone());
            composer
                .keyed_owners
                .entry(key.clone())
                .or_default()
                .insert(owner.clone());
            composer.live_keyed_owners.insert(owner);
            return (existing, None);
        }
        let old = composer
            .keyed_slots
            .insert(key.clone(), Box::new(value.clone()));
        composer.keyed_owner.insert(key.clone(), owner.clone());
        composer
            .keyed_owners
            .entry(key)
            .or_default()
            .insert(owner.clone());
        (value, old)
    });
    initializer.commit();
    drop(old);
    result
}

/// Raw slot state (`Rc<RefCell<T>>`). Writes via `borrow_mut()` don't request a
/// frame - prefer [`remember_mutable`] if a write must always recompose.
#[track_caller]
pub fn remember_state<T: 'static>(init: impl FnOnce() -> T) -> Rc<RefCell<T>> {
    remember(|| RefCell::new(init()))
}

/// Key-based variant of [`remember_state`]. Same no-frame-on-write caveat.
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

/// Hit-test region in physical pixels (`rect` carries px magnitudes,
/// like Compose `Rect`).
#[derive(Clone, Default)]
pub struct HitRegion {
    pub id: u64,
    pub rect: Rect,
    /// Tree depth: 0 = root, higher = deeper child. Used for three-pass
    /// pointer dispatch to determine ancestor/descendant ordering.
    pub depth: u32,
    pub parent: Option<u64>,
    pub on_click: Option<Rc<dyn Fn()>>,
    pub on_double_click: Option<Rc<dyn Fn()>>,
    pub on_long_click: Option<Rc<dyn Fn()>>,
    pub on_scroll: Option<Rc<dyn Fn(crate::Vec2) -> crate::Vec2>>,
    pub focusable: bool,
    pub on_pointer_down: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_cancel: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(crate::input::PointerEvent)>>,
    pub z_index: f32,
    pub disabled: bool,
    pub on_text_change: Option<Rc<dyn Fn(String)>>,
    pub on_text_submit: Option<Rc<dyn Fn(String)>>,
    /// If this hit region belongs to a TextField, this persistent key is used
    /// for looking up platform-managed TextFieldState. Falls back to `id` if None.
    pub tf_state_key: Option<u64>,

    /// True if this hit region corresponds to a multiline text input (TextArea).
    pub tf_multiline: bool,

    /// Unclipped top-left of the TextField *content* box (padding-inset).
    /// Used for pointer->grapheme mapping so parent scroll clipping of `rect`
    /// does not shift selection into the top of the content.
    /// `None` for non-textfields.
    pub tf_content_origin: Option<(f32, f32)>,

    /// When false, the field rejects edits and is not focusable
    pub tf_enabled: bool,

    /// When true, selection/focus/copy are allowed but mutations are rejected
    pub tf_read_only: bool,

    /// Controlled text snapshot for this field (last compose).
    pub tf_value: String,

    /// Font size for this text field in [`Sp`](crate::units::Sp)
    /// (for hit-test / caret mapping). `Sp::ZERO` means use `TF_FONT_SP` default.
    pub tf_font_size: crate::units::Sp,

    // internal
    pub on_drag_start: Option<Rc<dyn Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload>>>,
    pub on_drag_end: Option<Rc<dyn Fn(crate::dnd::DragEnd)>>,
    pub on_drag_enter: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_over: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_leave: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drop: Option<Rc<dyn Fn(crate::dnd::DropEvent) -> bool>>,
    /// Copied onto the drag session when a drag starts from this region.
    pub drag_preview: Option<crate::dnd::DragPreview>,

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

    /// IME keyboard hints, populated for text-field hit regions so the
    /// platform runner can configure the OS keyboard / IME on focus.
    pub keyboard_type: crate::text::KeyboardType,
    pub capitalization: crate::text::KeyboardCapitalization,
    pub ime_action: crate::text::ImeAction,
    /// Whether auto-correct is enabled. `None` = follow platform default;
    /// password keyboards always resolve to `false` in the layout engine.
    pub auto_correct: Option<bool>,

    /// Shared interaction source auto-wired by the layout engine
    /// (press/hover/focus/drag). Used by keyboard activation and focus
    /// transitions so they stay in parity with pointer input.
    /// `None` when the component does not need one (no indication/state colors).
    pub interaction_source: Option<crate::modifier::InteractionSource>,
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
            on_double_click: m.on_double_click.clone(),
            on_long_click: m.on_long_click.clone(),
            on_pointer_down: m.on_pointer_down.clone(),
            on_pointer_move: m.on_pointer_move.clone(),
            on_pointer_up: m.on_pointer_up.clone(),
            on_pointer_cancel: m.on_pointer_cancel.clone(),
            on_pointer_enter: m.on_pointer_enter.clone(),
            on_pointer_leave: m.on_pointer_leave.clone(),
            on_action: m.on_action.clone(),
            on_key_event: m.on_key_event.clone(),
            on_preview_key_event: m.on_preview_key_event.clone(),
            cursor: m.cursor.clone(),
            on_drag_start: m.on_drag_start.clone(),
            on_drag_end: m.on_drag_end.clone(),
            on_drag_enter: m.on_drag_enter.clone(),
            on_drag_over: m.on_drag_over.clone(),
            on_drag_leave: m.on_drag_leave.clone(),
            on_scroll: m.on_scroll.clone(),
            on_drop: m.on_drop.clone(),
            drag_preview: m.drag_preview.clone(),
            disabled: m.disabled,
            tf_enabled: true,
            tf_read_only: false,
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
    pub checked: Option<bool>,
    pub selected: Option<bool>,
    pub value: Option<String>,
}

impl Default for SemNode {
    fn default() -> Self {
        Self {
            id: 0,
            parent: None,
            role: Role::default(),
            label: None,
            rect: Rect::default(),
            focused: false,
            enabled: true,
            selectable_group: false,
            checked: None,
            selected: None,
            value: None,
        }
    }
}

pub struct Scheduler {
    next_id: u64,
    /// Per-scope unique IDs, assigned lazily when a scope first executes.
    /// Keyed by the scope key string from `scope!`.
    scope_key_to_id: FxHashMap<String, u32>,
    next_scope_id: u32,
    /// Stack of active scope keys. `id()` allocates from the innermost scope
    /// so nested `scope!` bodies get stable packed IDs and the outer scope
    /// resumes correctly after the inner exits (previously `exit_scope` reset
    /// to `None`, leaking outer IDs into the global sequence).
    current_scope: Vec<String>,
    /// Per-scope local ID counters. Reset to 0 when a scope re-executes.
    scope_local_counters: FxHashMap<String, u32>,
    pub focused: Option<u64>,
    pub size: (u32, u32),
    /// Last known mouse pointer position in physical px, updated on
    /// mouse move/press/release without passing through focus dispatch.
    pub pointer_pos_px: Option<(f32, f32)>,
    /// Polled physical keys currently down, layout-independent
    /// positions (see [`PhysicalKey`]). The platform runner maintains
    /// this from raw `KeyboardInput` events without passing through
    /// focus dispatch; games reconcile their event-staged held sets
    /// against it every frame (GML `keyboard_check` parity). Cleared
    /// on window focus loss (no key-ups arrive across an alt-tab).
    pub held_keys: HashSet<PhysicalKey>,
    /// Window focus as of the last platform event. `false` drops every
    /// held key on the game side.
    pub window_focused: bool,
    /// Live touch contacts in physical px: `(touch id, x, y)` per
    /// finger currently down. Written by the platform runners from
    /// raw `WindowEvent::Touch` (Started/Moved/Ended-Cancelled), read
    /// by games through `feed_polled`-style staging snapshots
    /// (GML `device_mouse_x_to_gui(i)` parity: stable per-finger slot
    /// ids, positions sampled every tick while held).
    pub touch_points: Vec<(u64, f32, f32)>,
    /// Polled mouse-button levels, same source as `held_keys`
    /// (GML `mouse_check_button` parity; repairs a missed button-up
    /// when the release lands outside the window).
    pub mouse_primary: bool,
    pub mouse_secondary: bool,
    pub mouse_middle: bool,
    /// App-requested cursor override. When `Some`, the platform
    /// applies it instead of the hover-derived icon: games hide the
    /// OS pointer here (`CursorIcon::Hidden`) while drawing their own
    /// crosshair, GML `window_set_cursor(cr_none)` parity. `None`
    /// restores hover behavior. Set during composition (any view can
    /// write it; last write per frame wins), consumed by the runner
    /// after `frame()`.
    pub cursor_override: Option<CursorIcon>,
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
            current_scope: Vec::new(),
            scope_local_counters: FxHashMap::default(),
            focused: None,
            size: (1280, 800),
            pointer_pos_px: None,
            held_keys: HashSet::new(),
            window_focused: true,
            touch_points: Vec::new(),
            mouse_primary: false,
            mouse_secondary: false,
            mouse_middle: false,
            cursor_override: None,
        }
    }

    /// Enter a named scope. Subsequent `id()` calls within this scope
    /// will allocate from the scope's local counter, producing packed
    /// `(scope_id << 32) | local_id` values that are stable across sibling
    /// recompositions. Nested scopes push; `exit_scope` pops.
    pub fn enter_scope(&mut self, key: &str) {
        if !self.current_scope.iter().any(|k| k == key) {
            self.scope_local_counters.insert(key.to_string(), 0);
        }
        self.current_scope.push(key.to_string());
        self.get_or_create_scope_id(key);
    }

    /// Exit the innermost scope. No-op if the stack is empty or the top does
    /// not match (defensive: never corrupt an outer scope).
    pub fn exit_scope(&mut self) {
        self.current_scope.pop();
    }

    /// RAII scope entry: pops on drop, so a panicking scope body cannot leave
    /// the scheduler stuck in the wrong scope (which previously leaked outer
    /// IDs into the global sequence).
    pub fn scope_guard<'a>(&'a mut self, key: &str) -> SchedulerScopeGuard<'a> {
        self.enter_scope(key);
        SchedulerScopeGuard { sched: self }
    }

    /// Panic-safe scope entry that does NOT hold a borrow: the guarded body
    /// (including nested `scope!`) can keep using the `Scheduler`. Used by
    /// the `scope!` macro.
    pub fn scope_guard_raw(&mut self, key: &str) -> SchedulerScopeGuardRaw {
        let ptr = self as *mut Scheduler;
        unsafe { SchedulerScopeGuardRaw::enter(ptr, key) }
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
        if let Some(key) = self.current_scope.last().cloned() {
            let scope_id = self.scope_key_to_id.get(&key).copied().unwrap_or(0);
            let local = self.scope_local_counters.get_mut(&key).unwrap();
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

    /// True while the named physical key is in the polled snapshot.
    pub fn is_held(&self, key: PhysicalKey) -> bool {
        self.held_keys.contains(&key)
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

    pub fn gc_scope_state(&mut self, live_keys: &rustc_hash::FxHashSet<String>) {
        self.scope_key_to_id
            .retain(|key, _| live_keys.contains(key));
        self.scope_local_counters
            .retain(|key, _| live_keys.contains(key));
    }
}

/// RAII guard from [`Scheduler::scope_guard`]. Pops the scope on drop.
pub struct SchedulerScopeGuard<'a> {
    sched: &'a mut Scheduler,
}

impl Drop for SchedulerScopeGuard<'_> {
    fn drop(&mut self) {
        self.sched.exit_scope();
    }
}

/// Panic-safe scope guard that does NOT hold a borrow across the body.
///
/// The `scope!` macro uses this (not [`SchedulerScopeGuard`]) so the guarded
/// body — including nested `scope!` invocations — can keep using the
/// `Scheduler` normally. At drop time (normal or unwind) no other borrows of
/// the scheduler are live, since the body has ended.
pub struct SchedulerScopeGuardRaw {
    sched: *mut Scheduler,
}

impl SchedulerScopeGuardRaw {
    /// # Safety
    /// `sched` must point to a valid `Scheduler` for the guard's lifetime,
    /// and the scheduler must not be used while the guard is being dropped
    /// (guaranteed when the guard outlives the body it protects).
    pub unsafe fn enter(sched: *mut Scheduler, key: &str) -> Self {
        unsafe {
            (*sched).enter_scope(key);
        }
        Self { sched }
    }
}

impl Drop for SchedulerScopeGuardRaw {
    fn drop(&mut self) {
        unsafe {
            (*self.sched).exit_scope();
        }
    }
}

impl Scheduler {
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

        let live_scope_keys = guard.live_scope_keys();
        let frame = Frame {
            scene,
            hit_regions: hits,
            semantics_nodes: sem,
            focus_chain,
        };
        drop(guard);
        self.gc_scope_state(&live_scope_keys);
        frame
    }
}

/// Test helper: full composer reset. Production shutdown should call
/// `shutdown_composition`.
#[cfg(test)]
pub fn clear_composer() {
    shutdown_composition();
}

#[cfg(test)]
mod focus_trap_tests {
    use super::*;

    fn region(id: u64, x: f32, group: Option<u64>) -> HitRegion {
        HitRegion {
            id,
            rect: Rect {
                x,
                y: 0.0,
                w: 10.0,
                h: 10.0,
            },
            focus_group_id: group,
            ..Default::default()
        }
    }

    #[test]
    fn arrows_stay_inside_group() {
        // Dialog buttons 2,3 in group 9; background button 4 outside;
        // outsider 1 sits left of button 2 and would win unconstrained.
        let chain = vec![1, 2, 3, 4];
        let regions = vec![
            region(1, 0.0, None),
            region(2, 20.0, Some(9)),
            region(3, 40.0, Some(9)),
            region(4, 60.0, None),
        ];
        assert_eq!(
            spatial_focus_next(&chain, &regions, Some(2), FocusDirection::Left),
            None,
            "outsider 1 is left of 2 but outside the group: trapped"
        );
        assert_eq!(
            spatial_focus_next(&chain, &regions, Some(2), FocusDirection::Right),
            Some(3)
        );
        assert_eq!(
            spatial_focus_next(&chain, &regions, Some(3), FocusDirection::Left),
            Some(2)
        );
        assert_eq!(
            spatial_focus_next(&chain, &regions, Some(1), FocusDirection::Right),
            Some(2),
            "ungrouped focus still sees the full chain"
        );
    }

    #[test]
    fn tab_cycles_inside_group() {
        let chain = vec![1, 2, 3, 4];
        let regions = vec![
            region(1, 0.0, None),
            region(2, 20.0, Some(9)),
            region(3, 40.0, Some(9)),
            region(4, 60.0, None),
        ];
        let mut fm = FocusManager::new(chain, Some(2));
        fm.hit_regions = regions;
        assert_eq!(fm.move_tab(false), Some(3));
        assert_eq!(fm.move_tab(false), Some(2));
        assert_eq!(fm.move_tab(true), Some(3));
    }

    #[test]
    fn tab_from_outside_can_enter_group() {
        let chain = vec![1, 2, 3, 4];
        let regions = vec![
            region(1, 0.0, None),
            region(2, 20.0, Some(9)),
            region(3, 40.0, Some(9)),
            region(4, 60.0, None),
        ];
        let mut fm = FocusManager::new(chain, Some(1));
        fm.hit_regions = regions;
        assert_eq!(fm.move_tab(false), Some(2));
        let mut fm = FocusManager::new(vec![1, 2, 3, 4], Some(1));
        fm.hit_regions = vec![
            region(1, 0.0, None),
            region(2, 20.0, Some(9)),
            region(3, 40.0, Some(9)),
            region(4, 60.0, None),
        ];
        assert_eq!(fm.move_tab(true), Some(4));
    }

    #[test]
    fn empty_group_never_moves() {
        let chain = vec![1, 4];
        let regions = vec![region(1, 0.0, None), region(4, 60.0, None)];
        let mut fm = FocusManager::new(chain, Some(1));
        fm.hit_regions = regions.clone();
        assert_eq!(fm.move_tab(false), Some(4));
        let chain = vec![1, 2, 4];
        let regions = vec![
            region(1, 0.0, None),
            region(2, 20.0, Some(77)),
            region(4, 60.0, None),
        ];
        let mut fm = FocusManager::new(chain, Some(2));
        fm.hit_regions = regions;
        assert_eq!(fm.move_tab(false), Some(2));
        assert_eq!(
            spatial_focus_next(&fm.chain, &fm.hit_regions, Some(2), FocusDirection::Right),
            None
        );
    }
}
