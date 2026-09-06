use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use repose_core::{Modifier, View, ViewKind, request_frame};
use web_time::{Duration, Instant};

// Registry of live snackbar controllers, held weakly so a dropped
// controller stops ticking without explicit teardown. Dead entries are
// pruned on every read (`tick_all` / `next_deadline`).
thread_local! {
    static SNACKBAR_REGISTRY: RefCell<Vec<Weak<SnackbarControllerInner>>> =
        const { RefCell::new(Vec::new()) };
}

/// Duration of the snackbar exit animation before the overlay entry is removed.
const SNACKBAR_EXIT_ANIM_MS: u64 = 200;

#[derive(Clone)]
pub struct OverlayHandle {
    inner: Rc<RefCell<OverlayState>>,
}

#[derive(Default)]
struct OverlayState {
    next_id: u64,
    entries: Vec<OverlayEntry>,
}

#[derive(Clone)]
pub struct OverlayEntry {
    pub id: u64,
    pub builder: Rc<dyn Fn() -> View>,
    pub z_index: f32,
    pub pass_through: bool,
}

impl Default for OverlayHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl OverlayHandle {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(OverlayState {
                next_id: 1,
                entries: Vec::new(),
            })),
        }
    }

    pub fn show(&self, view: View) -> u64 {
        self.show_with(view, 0.0, false)
    }

    pub fn show_with(&self, view: View, z_index: f32, pass_through: bool) -> u64 {
        let builder = Rc::new(move || view.clone());
        self.show_entry(builder, z_index, pass_through)
    }

    pub fn show_builder(&self, builder: Rc<dyn Fn() -> View>) -> u64 {
        self.show_entry(builder, 0.0, false)
    }

    pub fn show_entry(
        &self,
        builder: Rc<dyn Fn() -> View>,
        z_index: f32,
        pass_through: bool,
    ) -> u64 {
        let mut inner = self.inner.borrow_mut();
        let id = inner.next_id;
        inner.next_id += 1;
        inner.entries.push(OverlayEntry {
            id,
            builder,
            z_index,
            pass_through,
        });
        request_frame();
        id
    }

    /// Show `builder` and return a guard owning the entry. Dropping the
    /// guard dismisses the entry; see [`OverlayGuard`].
    pub fn show_guard(
        &self,
        builder: Rc<dyn Fn() -> View>,
        z_index: f32,
        pass_through: bool,
    ) -> OverlayGuard {
        OverlayGuard::show(self, builder, z_index, pass_through)
    }

    pub fn dismiss(&self, id: u64) -> bool {
        let mut inner = self.inner.borrow_mut();
        let before = inner.entries.len();
        inner.entries.retain(|entry| entry.id != id);
        let removed = inner.entries.len() != before;
        if removed {
            request_frame();
        }
        removed
    }

    pub fn clear(&self) {
        let mut inner = self.inner.borrow_mut();
        if !inner.entries.is_empty() {
            inner.entries.clear();
            request_frame();
        }
    }

    pub fn host(&self, modifier: Modifier, content: View) -> View {
        let mut root = View::new(0, ViewKind::OverlayHost).modifier(modifier);
        root.children.push(content);
        let mut overlays = self.inner.borrow().entries.clone();
        // Sort ascending by z_index so higher z_index overlays are painted last (on top)
        overlays.sort_by(|a, b| {
            a.z_index
                .partial_cmp(&b.z_index)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for entry in overlays {
            let view = (entry.builder)();
            let mut modifier = view
                .modifier
                .clone()
                .z_index(entry.z_index)
                .render_z_index(entry.z_index + 1000.0);
            if entry.pass_through {
                modifier = modifier.hit_passthrough();
            }
            root.children.push(view.modifier(modifier));
        }
        root
    }
}

/// RAII owner of a shown overlay entry.
///
/// Dropping the guard dismisses the entry, so callers can't leak entries by
/// forgetting `dismiss` or mismatching a `0`-sentinel id. The typical shape
/// is a `remember`-ed `RefCell<Option<OverlayGuard>>`: set it to `Some` when
/// the overlay should show, back to `None` to hide.
///
/// The guard holds only the handle + id (no entry content), so it never
/// participates in reference cycles with the overlay state.
#[must_use = "dropping the guard dismisses the overlay entry"]
pub struct OverlayGuard {
    handle: OverlayHandle,
    id: Option<u64>,
}

impl OverlayGuard {
    /// Show `builder` on `handle` and own the resulting entry.
    pub fn show(
        handle: &OverlayHandle,
        builder: Rc<dyn Fn() -> View>,
        z_index: f32,
        pass_through: bool,
    ) -> Self {
        let id = handle.show_entry(builder, z_index, pass_through);
        Self {
            handle: handle.clone(),
            id: Some(id),
        }
    }

    /// Entry id, or `None` after [`dismiss`](Self::dismiss).
    pub fn id(&self) -> Option<u64> {
        self.id
    }

    /// Dismiss now instead of at drop. Consuming the guard makes a
    /// double-dismiss impossible.
    pub fn dismiss(mut self) {
        self.dismiss_now();
    }

    fn dismiss_now(&mut self) {
        if let Some(id) = self.id.take() {
            self.handle.dismiss(id);
        }
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        self.dismiss_now();
    }
}

#[derive(Clone)]
pub struct SnackbarController {
    inner: Rc<SnackbarControllerInner>,
}

struct SnackbarControllerInner {
    state: RefCell<SnackbarState>,
    overlay: OverlayHandle,
}

#[derive(Clone)]
pub struct SnackbarAction {
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

#[derive(Clone)]
pub struct SnackbarRequest {
    pub message: String,
    pub action: Option<SnackbarAction>,
    pub duration_ms: u32,
    /// Called on every frame with whether the snackbar is in its
    /// exit-animation phase, so the view can animate out.
    pub builder: Rc<dyn Fn(bool) -> View>,
}

struct SnackbarState {
    queue: VecDeque<SnackbarRequest>,
    active: Option<ActiveSnackbar>,
}

/// Lifecycle phase of the visible snackbar. Both phases carry the `Instant`
/// at which the snackbar leaves the phase (dismiss start, then removal).
enum SnackbarPhase {
    Showing { deadline: Instant },
    Dismissing { deadline: Instant },
}

struct ActiveSnackbar {
    /// Owns the overlay entry; clearing `active` dismisses the entry via drop.
    /// Never read by name — its `Drop` impl is the whole point.
    #[allow(dead_code)]
    guard: OverlayGuard,
    message: String,
    action: Option<SnackbarAction>,
    phase: SnackbarPhase,
}

impl ActiveSnackbar {
    fn deadline(&self) -> Instant {
        match self.phase {
            SnackbarPhase::Showing { deadline } | SnackbarPhase::Dismissing { deadline } => {
                deadline
            }
        }
    }

    fn is_dismissing(&self) -> bool {
        matches!(self.phase, SnackbarPhase::Dismissing { .. })
    }

    /// Idempotent transition into the exit animation.
    fn start_dismiss(&mut self, now: Instant) {
        if !self.is_dismissing() {
            self.phase = SnackbarPhase::Dismissing {
                deadline: now + Duration::from_millis(SNACKBAR_EXIT_ANIM_MS),
            };
            request_frame();
        }
    }
}

impl SnackbarController {
    pub fn new(overlay: OverlayHandle) -> Self {
        let controller = Self {
            inner: Rc::new(SnackbarControllerInner {
                state: RefCell::new(SnackbarState {
                    queue: VecDeque::new(),
                    active: None,
                }),
                overlay,
            }),
        };

        SNACKBAR_REGISTRY.with(|reg| reg.borrow_mut().push(Rc::downgrade(&controller.inner)));
        controller
    }

    fn live_controllers() -> Vec<Rc<SnackbarControllerInner>> {
        SNACKBAR_REGISTRY.with(|reg| {
            let mut reg = reg.borrow_mut();
            reg.retain(|w| w.upgrade().is_some());
            reg.iter().filter_map(Weak::upgrade).collect()
        })
    }

    /// Tick all live controllers once. Call once per redraw.
    pub fn tick_all() {
        for inner in Self::live_controllers() {
            Self { inner }.tick();
        }
    }

    /// Earliest `Instant` when any snackbar needs to wake (dismiss start or
    /// finish). `None` if no snackbar is active.
    pub fn next_deadline() -> Option<Instant> {
        Self::live_controllers()
            .iter()
            .filter_map(|inner| {
                inner
                    .state
                    .borrow()
                    .active
                    .as_ref()
                    .map(ActiveSnackbar::deadline)
            })
            .min()
    }

    pub fn show(&self, request: SnackbarRequest) {
        if self.inner.state.borrow().active.is_some() {
            self.inner.state.borrow_mut().queue.push_back(request);
        } else {
            self.activate_next(request);
        }
    }

    pub fn tick(&self) {
        let now = Instant::now();
        let mut finished: Option<ActiveSnackbar> = None;
        {
            let mut state = self.inner.state.borrow_mut();
            if let Some(active) = state.active.as_mut() {
                match active.phase {
                    SnackbarPhase::Showing { deadline } if now >= deadline => {
                        active.start_dismiss(now);
                    }
                    SnackbarPhase::Dismissing { deadline } if now >= deadline => {
                        finished = state.active.take();
                    }
                    _ => {}
                }
            }
        }
        drop(finished);
        self.activate_next_if_needed();
    }

    pub fn dismiss(&self) {
        let now = Instant::now();
        if let Some(active) = self.inner.state.borrow_mut().active.as_mut() {
            active.start_dismiss(now);
        }
    }

    pub fn current(&self) -> Option<(String, Option<SnackbarAction>)> {
        let inner = self.inner.state.borrow();
        inner
            .active
            .as_ref()
            .map(|active| (active.message.clone(), active.action.clone()))
    }

    fn activate_next_if_needed(&self) {
        if self.inner.state.borrow().active.is_some() {
            return;
        }
        let req = self.inner.state.borrow_mut().queue.pop_front();
        if let Some(req) = req {
            self.activate_next(req);
        }
    }

    fn activate_next(&self, req: SnackbarRequest) {
        if self.inner.state.borrow().active.is_some() {
            return;
        }
        let weak = Rc::downgrade(&self.inner);
        let original_builder = req.builder.clone();
        let wrapped_builder: Rc<dyn Fn() -> View> = Rc::new(move || {
            let dismissing = weak
                .upgrade()
                .and_then(|inner| {
                    inner
                        .state
                        .borrow()
                        .active
                        .as_ref()
                        .map(ActiveSnackbar::is_dismissing)
                })
                .unwrap_or(false);
            (original_builder)(dismissing)
        });
        let action = req.action.map(|a| SnackbarAction {
            on_click: {
                let original = a.on_click;
                let controller = self.clone();
                Rc::new(move || {
                    (original)();
                    controller.dismiss();
                })
            },
            label: a.label,
        });
        let guard = OverlayGuard::show(&self.inner.overlay, wrapped_builder, 900.0, true);
        let deadline = Instant::now() + Duration::from_millis(u64::from(req.duration_ms.max(1)));
        self.inner.state.borrow_mut().active = Some(ActiveSnackbar {
            guard,
            message: req.message,
            action,
            phase: SnackbarPhase::Showing { deadline },
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_request() -> SnackbarRequest {
        SnackbarRequest {
            message: "hi".into(),
            action: None,
            duration_ms: 60_000,
            builder: Rc::new(|_| View::new(0, ViewKind::OverlayHost)),
        }
    }

    #[test]
    fn dropped_controller_is_pruned_from_registry() {
        {
            let overlay = OverlayHandle::new();
            let controller = SnackbarController::new(overlay);
            controller.show(test_request());
            SnackbarController::tick_all();
            assert!(SnackbarController::next_deadline().is_some());
        }
        SnackbarController::tick_all();
        assert!(SnackbarController::next_deadline().is_none());
    }

    #[test]
    fn dismiss_schedules_exit_deadline() {
        let overlay = OverlayHandle::new();
        let controller = SnackbarController::new(overlay);
        controller.show(test_request());
        let before = Instant::now();
        controller.dismiss();
        let deadline = SnackbarController::next_deadline().expect("deadline while dismissing");
        assert!(deadline >= before);
        assert!(deadline.saturating_duration_since(before) <= Duration::from_millis(500));
        controller.dismiss();
        let again = SnackbarController::next_deadline().expect("deadline while dismissing");
        assert_eq!(deadline, again);
        drop(controller);
        SnackbarController::tick_all();
    }

    #[test]
    fn guard_drop_dismisses_entry() {
        let overlay = OverlayHandle::new();
        let builder: Rc<dyn Fn() -> View> = Rc::new(|| View::new(0, ViewKind::OverlayHost));
        {
            let guard = overlay.show_guard(builder.clone(), 5.0, false);
            assert!(guard.id().is_some());
            assert_eq!(overlay.inner.borrow().entries.len(), 1);
            guard.dismiss();
            assert!(overlay.inner.borrow().entries.is_empty());
        }
        let guard = overlay.show_guard(builder, 5.0, false);
        assert_eq!(overlay.inner.borrow().entries.len(), 1);
        drop(guard);
        assert!(overlay.inner.borrow().entries.is_empty());
    }
}
