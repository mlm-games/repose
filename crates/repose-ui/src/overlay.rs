use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::{Rc, Weak};

use repose_core::{Modifier, View, ViewKind, request_frame};
use web_time::{Duration, Instant};

thread_local! {
    static SNACKBAR_TICKS: RefCell<Vec<Rc<dyn Fn(u32)>>> = RefCell::new(Vec::new());
    static SNACKBAR_DISMISSING: Cell<bool> = const { Cell::new(false) };
    static SNACKBAR_REGISTRY: RefCell<Vec<Weak<RefCell<SnackbarState>>>> =
        RefCell::new(Vec::new());
}

/// Set whether the current frame's snackbar is in the exit-animation phase.
/// Called by the overlay builder before rendering the snackbar view.
pub fn snackbar_set_dismissing(v: bool) {
    SNACKBAR_DISMISSING.with(|c| c.set(v));
}

/// Read by the Snackbar component to decide its exit animation target.
pub fn snackbar_is_dismissing() -> bool {
    SNACKBAR_DISMISSING.with(|c| c.get())
}

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

#[derive(Clone)]
pub struct SnackbarController {
    inner: Rc<RefCell<SnackbarState>>,
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
    pub builder: Rc<dyn Fn() -> View>,
}

struct SnackbarState {
    queue: VecDeque<SnackbarRequest>,
    active: Option<ActiveSnackbar>,
}

struct ActiveSnackbar {
    id: u64,
    message: String,
    action: Option<SnackbarAction>,
    deadline: Instant,
    dismiss_started: Rc<Cell<bool>>,
    dismiss_deadline: Option<Instant>,
}

impl SnackbarController {
    pub fn new(overlay: OverlayHandle) -> Self {
        let controller = Self {
            inner: Rc::new(RefCell::new(SnackbarState {
                queue: VecDeque::new(),
                active: None,
            })),
            overlay,
        };

        let tick = {
            let controller = controller.clone();
            Rc::new(move |elapsed_ms| controller.tick(elapsed_ms))
        };
        SNACKBAR_TICKS.with(|slot| slot.borrow_mut().push(tick));
        SNACKBAR_REGISTRY.with(|reg| reg.borrow_mut().push(Rc::downgrade(&controller.inner)));
        controller
    }

    pub fn tick_for_frame(elapsed_ms: u32) {
        SNACKBAR_TICKS.with(|ticks| {
            for cb in ticks.borrow().iter() {
                cb(elapsed_ms);
            }
        });
    }

    /// Earliest `Instant` when any snackbar needs to wake (dismiss start or
    /// finish). `None` if no snackbar is active/queued.
    pub fn next_deadline() -> Option<Instant> {
        SNACKBAR_REGISTRY.with(|reg| {
            let mut reg = reg.borrow_mut();
            // prune dead ones
            reg.retain(|w| w.upgrade().is_some());
            let mut earliest: Option<Instant> = None;
            for weak in reg.iter() {
                if let Some(rc) = weak.upgrade() {
                    let state = rc.borrow();
                    if let Some(active) = &state.active {
                        let deadline = if active.dismiss_started.get() {
                            active.dismiss_deadline.unwrap_or_else(Instant::now)
                        } else {
                            active.deadline
                        };
                        earliest = Some(match earliest {
                            Some(e) => e.min(deadline),
                            None => deadline,
                        });
                    } else if let Some(req) = state.queue.front() {
                        // queued item will activate immediately after current dismisses;
                        // its deadline is not yet scheduled, so ignore until active.
                        let _ = req;
                    }
                }
            }
            earliest
        })
    }

    pub fn show(&self, request: SnackbarRequest) {
        let mut inner = self.inner.borrow_mut();
        if let Some(_) = inner.active {
            inner.queue.push_back(request);
        } else {
            drop(inner);
            self.activate_next(request);
        }
    }

    pub fn tick(&self, elapsed_ms: u32) {
        // Keep elapsed_ms path for compat, will be removed eventually.
        let now = Instant::now();
        let mut inner = self.inner.borrow_mut();
        if let Some(active) = inner.active.as_mut() {
            if active.dismiss_started.get() {
                if let Some(dd) = active.dismiss_deadline {
                    if now >= dd {
                        self.overlay.dismiss(active.id);
                        inner.active = None;
                    }
                } else {
                    // if deadline missing (should not happen though)
                    active.dismiss_deadline = Some(now + Duration::from_millis(200));
                    request_frame();
                }
            } else if now >= active.deadline || elapsed_ms >= 60_000 {
                active.dismiss_started.set(true);
                active.dismiss_deadline = Some(now + Duration::from_millis(200));
                request_frame();
            } else {
                // keep remaining as deadline.
                let _ = elapsed_ms;
            }
        }
        drop(inner);
        self.activate_next_if_needed();
    }

    pub fn dismiss(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(active) = inner.active.as_mut()
            && !active.dismiss_started.get()
        {
            active.dismiss_started.set(true);
            active.dismiss_deadline = Some(Instant::now() + Duration::from_millis(200));
            request_frame();
        }
    }

    pub fn current(&self) -> Option<(String, Option<SnackbarAction>)> {
        let inner = self.inner.borrow();
        inner
            .active
            .as_ref()
            .map(|active| (active.message.clone(), active.action.clone()))
    }

    fn activate_next_if_needed(&self) {
        let mut inner = self.inner.borrow_mut();
        if inner.active.is_some() {
            return;
        }
        let Some(req) = inner.queue.pop_front() else {
            return;
        };
        drop(inner);
        self.activate_next(req);
    }

    fn activate_next(&self, req: SnackbarRequest) {
        let mut inner = self.inner.borrow_mut();
        if inner.active.is_some() {
            return;
        }
        let dismiss_flag = Rc::new(Cell::new(false));
        let flag_for_builder = dismiss_flag.clone();
        let original_builder = req.builder.clone();
        let wrapped_builder: Rc<dyn Fn() -> View> = Rc::new(move || {
            snackbar_set_dismissing(flag_for_builder.get());
            (original_builder)()
        });
        // action's on_click also dismisses the snackbar
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
        let id = self.overlay.show_entry(wrapped_builder, 900.0, true);
        let deadline = Instant::now() + Duration::from_millis(u64::from(req.duration_ms.max(1)));
        inner.active = Some(ActiveSnackbar {
            id,
            message: req.message,
            action,
            deadline,
            dismiss_started: dismiss_flag,
            dismiss_deadline: None,
        });
    }
}
