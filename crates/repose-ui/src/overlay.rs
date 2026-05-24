use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use repose_core::{request_frame, Modifier, View, ViewKind};

thread_local! {
    static SNACKBAR_TICKS: RefCell<Vec<Rc<dyn Fn(u32)>>> = RefCell::new(Vec::new());
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
    remaining_ms: u32,
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
        controller
    }

    pub fn tick_for_frame(elapsed_ms: u32) {
        SNACKBAR_TICKS.with(|ticks| {
            for cb in ticks.borrow().iter() {
                cb(elapsed_ms);
            }
        });
    }

    pub fn show(&self, request: SnackbarRequest) {
        let mut inner = self.inner.borrow_mut();
        inner.queue.push_back(request.clone());
        if inner.active.is_none() {
            drop(inner);
            self.activate_next((request.builder)(), request);
        }
    }

    pub fn tick(&self, elapsed_ms: u32) {
        let mut inner = self.inner.borrow_mut();
        if let Some(active) = inner.active.as_mut() {
            if elapsed_ms >= active.remaining_ms {
                self.overlay.dismiss(active.id);
                inner.active = None;
            } else {
                active.remaining_ms -= elapsed_ms;
            }
        }
        drop(inner);
        self.activate_next_if_needed();
    }

    pub fn dismiss(&self) {
        let mut inner = self.inner.borrow_mut();
        if let Some(active) = inner.active.take() {
            self.overlay.dismiss(active.id);
        }
        drop(inner);
        self.activate_next_if_needed();
    }

    pub fn current(&self) -> Option<(String, Option<SnackbarAction>)> {
        let inner = self.inner.borrow();
        inner
            .active
            .as_ref()
            .map(|active| (active.message.clone(), active.action.clone()))
    }

    fn activate_next_if_needed(&self) {
        let (view, req) = {
            let mut inner = self.inner.borrow_mut();
            if inner.active.is_some() {
                return;
            }
            let Some(req) = inner.queue.pop_front() else {
                return;
            };
            let view = (req.builder)();
            (view, req)
        };
        self.activate_next(view, req);
    }

    fn activate_next(&self, view: View, req: SnackbarRequest) {
        let mut inner = self.inner.borrow_mut();
        if inner.active.is_some() {
            return;
        }
        let id = self.overlay.show_with(view, 900.0, true);
        inner.active = Some(ActiveSnackbar {
            id,
            message: req.message,
            action: req.action,
            remaining_ms: req.duration_ms.max(1),
        });
    }
}
