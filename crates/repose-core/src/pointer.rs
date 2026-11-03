//! Gesture handlers driven by pointer events.
//!
//! [`TapGestures`] and [`DragGestures`] are state machines over
//! [`PointerEvent`]s, attached with [`Modifier::pointer_input`]. Their state
//! lives in a modifier node, so a drag keeps accumulating while the view
//! recomposes.
//!
//! [`Modifier::pointer_input`]: crate::Modifier::pointer_input

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::time::Duration;

use crate::animation;
use crate::animation_driver;
use crate::input::{PointerEvent, PointerEventKind, PointerId};
use crate::modifier_node::{ModifierNode, ModifierNodeElement};
use crate::{Vec2, request_frame};

/// Receives the pointer events of one hit region.
///
/// `on_event` returns `true` when it handled the event; that consumes it so
/// ancestors in the main pass do not also react.
pub trait PointerInputHandler: Any {
    /// Build a fresh handler with the same configuration and no runtime state.
    fn clone_handler(&self) -> Box<dyn PointerInputHandler>;
    /// Copy configuration from a freshly built handler into this one, leaving
    /// runtime state (press position, accumulated drag) alone.
    fn sync(&mut self, next: &dyn PointerInputHandler) {
        let _ = next;
    }
    fn on_event(&mut self, event: &PointerEvent) -> bool;
    fn on_attach(&mut self) {}
    fn on_detach(&mut self) {}
    #[doc(hidden)]
    fn as_any(&self) -> &dyn Any;
}

struct PointerInputNode {
    handler: RefCell<Box<dyn PointerInputHandler>>,
}

impl ModifierNode for PointerInputNode {
    fn on_attach(&self) {
        self.handler.borrow_mut().on_attach();
    }
    fn on_detach(&self) {
        self.handler.borrow_mut().on_detach();
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct PointerInputElement {
    handler: Box<dyn PointerInputHandler>,
    handler_type: TypeId,
}

impl ModifierNodeElement for PointerInputElement {
    fn create(&self) -> Rc<dyn ModifierNode> {
        Rc::new(PointerInputNode {
            handler: RefCell::new(self.handler.clone_handler()),
        })
    }

    fn update(&self, node: &Rc<dyn ModifierNode>) {
        if let Some(node) = node.as_any().downcast_ref::<PointerInputNode>() {
            node.handler.borrow_mut().sync(&*self.handler);
        }
    }

    fn key(&self) -> Option<u64> {
        let mut hasher = rustc_hash::FxHasher::default();
        self.handler_type.hash(&mut hasher);
        Some(hasher.finish())
    }
}

/// Dispatches pointer events into a handler stored on a modifier node.
#[derive(Clone)]
#[doc(hidden)]
pub struct PointerInputDispatch {
    node: Rc<dyn ModifierNode>,
}

impl PointerInputDispatch {
    pub(crate) fn new(node: Rc<dyn ModifierNode>) -> Self {
        Self { node }
    }

    #[doc(hidden)]
    pub fn on_event(&self, event: &PointerEvent) -> bool {
        let Some(node) = self.node.as_any().downcast_ref::<PointerInputNode>() else {
            return false;
        };
        node.handler.borrow_mut().on_event(event)
    }
}

pub(crate) fn element(handler: Box<dyn PointerInputHandler>) -> Rc<dyn ModifierNodeElement> {
    let handler_type = (&*handler as &dyn Any).type_id();
    Rc::new(PointerInputElement {
        handler,
        handler_type,
    })
}

/// Runs `action` once, `delay` from now, on the animation clock. The tick
/// re-arms itself until the deadline so a paused clock does not fire early.
fn schedule_once(key: String, delay: Duration, action: impl Fn() + 'static) {
    let start = animation::now();
    let action = Rc::new(action);
    animation_driver::register(
        key,
        Rc::new(RefCell::new(move || {
            if animation::now().duration_since(start) >= delay {
                action();
                request_frame();
                false
            } else {
                request_frame();
                true
            }
        })),
    );
    request_frame();
}

fn cancel(key: &str) {
    animation_driver::unregister(key);
}

/// A press that ended without moving past the touch slop.
#[derive(Clone, Copy, Debug)]
pub struct TapEvent {
    pub id: PointerId,
    pub position: Vec2,
}

enum TapPhase {
    Idle,
    Pressed {
        id: PointerId,
        start: Vec2,
        long_fired: bool,
        /// First tap still waiting for its second tap when this press began.
        pending_first: Option<TapEvent>,
    },
    WaitingSecond {
        first: TapEvent,
    },
}

struct TapConfig {
    touch_slop_px: f32,
    double_tap_ms: f32,
    long_press_ms: f32,
    on_tap: Option<Rc<dyn Fn(TapEvent)>>,
    on_double_tap: Option<Rc<dyn Fn(TapEvent)>>,
    on_long_press: Option<Rc<dyn Fn(TapEvent)>>,
}

impl Clone for TapConfig {
    fn clone(&self) -> Self {
        Self {
            touch_slop_px: self.touch_slop_px,
            double_tap_ms: self.double_tap_ms,
            long_press_ms: self.long_press_ms,
            on_tap: self.on_tap.clone(),
            on_double_tap: self.on_double_tap.clone(),
            on_long_press: self.on_long_press.clone(),
        }
    }
}

/// Tap, double tap, and long press.
///
/// With `on_double_tap` set, a first tap waits one `double_tap_ms` window
/// before it is reported as a tap. A long press fires while the finger is still
/// down; releasing afterwards reports nothing.
#[derive(Clone)]
pub struct TapGestures {
    config: Rc<TapConfig>,
    phase: Rc<RefCell<TapPhase>>,
    tick_key: String,
}

impl TapGestures {
    pub fn new() -> Self {
        Self {
            config: Rc::new(TapConfig {
                touch_slop_px: 12.0,
                double_tap_ms: 300.0,
                long_press_ms: 500.0,
                on_tap: None,
                on_double_tap: None,
                on_long_press: None,
            }),
            phase: Rc::new(RefCell::new(TapPhase::Idle)),
            tick_key: format!("tap:{}", next_id()),
        }
    }

    pub fn on_tap(mut self, f: impl Fn(TapEvent) + 'static) -> Self {
        Rc::make_mut(&mut self.config).on_tap = Some(Rc::new(f));
        self
    }

    pub fn on_double_tap(mut self, f: impl Fn(TapEvent) + 'static) -> Self {
        Rc::make_mut(&mut self.config).on_double_tap = Some(Rc::new(f));
        self
    }

    pub fn on_long_press(mut self, f: impl Fn(TapEvent) + 'static) -> Self {
        Rc::make_mut(&mut self.config).on_long_press = Some(Rc::new(f));
        self
    }

    pub fn touch_slop(mut self, px: f32) -> Self {
        Rc::make_mut(&mut self.config).touch_slop_px = px;
        self
    }

    pub fn double_tap_window(mut self, ms: f32) -> Self {
        Rc::make_mut(&mut self.config).double_tap_ms = ms;
        self
    }

    pub fn long_press_window(mut self, ms: f32) -> Self {
        Rc::make_mut(&mut self.config).long_press_ms = ms;
        self
    }

    fn arm_long_press(&self, id: PointerId) {
        let Some(on_long_press) = self.config.on_long_press.clone() else {
            return;
        };
        let phase = self.phase.clone();
        let delay = Duration::from_secs_f32(self.config.long_press_ms.max(0.0) / 1000.0);
        schedule_once(self.tick_key.clone(), delay, move || {
            let mut phase = phase.borrow_mut();
            let TapPhase::Pressed {
                id: pressed,
                start: at,
                long_fired,
                ..
            } = &mut *phase
            else {
                return;
            };
            if *pressed != id {
                return;
            }
            *long_fired = true;
            let position = *at;
            drop(phase);
            on_long_press(TapEvent { id, position });
        });
    }

    fn report_abandoned_first(&self, first: Option<TapEvent>) {
        if let (Some(first), Some(on_tap)) = (first, &self.config.on_tap) {
            on_tap(first);
        }
    }

    fn schedule_single_tap(&self, first: TapEvent) {
        let on_tap = self.config.on_tap.clone();
        let phase = self.phase.clone();
        let delay = Duration::from_secs_f32(self.config.double_tap_ms.max(0.0) / 1000.0);
        // The window always closes, even with no `on_tap`: a pending first tap
        // that never expires would turn any later tap into a double tap.
        schedule_once(self.tick_key.clone(), delay, move || {
            let mut phase = phase.borrow_mut();
            if !matches!(&*phase, TapPhase::WaitingSecond { first: pending } if pending.id == first.id)
            {
                return;
            }
            *phase = TapPhase::Idle;
            drop(phase);
            if let Some(on_tap) = &on_tap {
                on_tap(first);
            }
        });
    }
}

impl Default for TapGestures {
    fn default() -> Self {
        Self::new()
    }
}

impl PointerInputHandler for TapGestures {
    fn clone_handler(&self) -> Box<dyn PointerInputHandler> {
        Box::new(Self {
            config: self.config.clone(),
            phase: Rc::new(RefCell::new(TapPhase::Idle)),
            tick_key: format!("tap:{}", next_id()),
        })
    }

    fn sync(&mut self, next: &dyn PointerInputHandler) {
        if let Some(next) = (next as &dyn Any).downcast_ref::<TapGestures>() {
            self.config = next.config.clone();
        }
    }

    fn on_event(&mut self, event: &PointerEvent) -> bool {
        let mut phase = self.phase.borrow_mut();
        match event.event {
            PointerEventKind::Down(_) => {
                cancel(&self.tick_key);
                let pending_first = match &*phase {
                    TapPhase::WaitingSecond { first } => Some(*first),
                    _ => None,
                };
                *phase = TapPhase::Pressed {
                    id: event.id,
                    start: event.position,
                    long_fired: false,
                    pending_first,
                };
                drop(phase);
                self.arm_long_press(event.id);
                false
            }
            PointerEventKind::Move => {
                let TapPhase::Pressed { start, pending_first, .. } = &*phase else {
                    return false;
                };
                let dx = event.position.x - start.x;
                let dy = event.position.y - start.y;
                if dx.hypot(dy) > self.config.touch_slop_px {
                    let pending_first = *pending_first;
                    *phase = TapPhase::Idle;
                    drop(phase);
                    cancel(&self.tick_key);
                    self.report_abandoned_first(pending_first);
                    return false;
                }
                false
            }
            PointerEventKind::Up(_) => {
                let matches_press = match &*phase {
                    TapPhase::Pressed { id, .. } => *id == event.id,
                    TapPhase::WaitingSecond { .. } => true,
                    TapPhase::Idle => false,
                };
                if !matches_press {
                    return false;
                }
                let previous = std::mem::replace(&mut *phase, TapPhase::Idle);
                drop(phase);
                match previous {
                    TapPhase::Pressed {
                        id,
                        long_fired,
                        pending_first,
                        ..
                    } => {
                        if long_fired {
                            return true;
                        }
                        let tap = TapEvent {
                            id,
                            position: event.position,
                        };
                        cancel(&self.tick_key);
                        if let Some(first) = pending_first {
                            if let Some(on_double_tap) = &self.config.on_double_tap {
                                on_double_tap(TapEvent {
                                    id: first.id,
                                    position: event.position,
                                });
                            }
                            return true;
                        }
                        if self.config.on_double_tap.is_some() {
                            *self.phase.borrow_mut() = TapPhase::WaitingSecond { first: tap };
                            self.schedule_single_tap(tap);
                        } else if let Some(on_tap) = &self.config.on_tap {
                            on_tap(tap);
                        }
                        true
                    }
                    TapPhase::WaitingSecond { first } => {
                        cancel(&self.tick_key);
                        if let Some(on_tap) = &self.config.on_tap {
                            on_tap(first);
                        }
                        true
                    }
                    TapPhase::Idle => false,
                }
            }
            PointerEventKind::Cancel => {
                let pending_first = match &*phase {
                    TapPhase::Pressed {
                        pending_first, ..
                    } => *pending_first,
                    _ => None,
                };
                let had = !matches!(&*phase, TapPhase::Idle);
                *phase = TapPhase::Idle;
                drop(phase);
                cancel(&self.tick_key);
                self.report_abandoned_first(pending_first);
                had
            }
            PointerEventKind::Enter | PointerEventKind::Leave => false,
        }
    }

    fn on_detach(&mut self) {
        cancel(&self.tick_key);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DragStart {
    pub id: PointerId,
    pub position: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct DragUpdate {
    pub id: PointerId,
    pub position: Vec2,
    pub delta: Vec2,
    pub total: Vec2,
}

#[derive(Clone, Copy, Debug)]
pub struct DragEnd {
    pub id: PointerId,
    pub position: Vec2,
    pub total: Vec2,
}

struct DragConfig {
    touch_slop_px: f32,
    on_start: Option<Rc<dyn Fn(DragStart)>>,
    on_drag: Option<Rc<dyn Fn(DragUpdate)>>,
    on_end: Option<Rc<dyn Fn(DragEnd)>>,
    on_cancel: Option<Rc<dyn Fn()>>,
}

impl Clone for DragConfig {
    fn clone(&self) -> Self {
        Self {
            touch_slop_px: self.touch_slop_px,
            on_start: self.on_start.clone(),
            on_drag: self.on_drag.clone(),
            on_end: self.on_end.clone(),
            on_cancel: self.on_cancel.clone(),
        }
    }
}

#[derive(Clone, Copy)]
enum DragPhase {
    Idle,
    Pressed {
        id: PointerId,
        start: Vec2,
    },
    Dragging {
        id: PointerId,
        start: Vec2,
        last: Vec2,
    },
}

/// Drag detection with a touch slop before the gesture starts.
#[derive(Clone)]
pub struct DragGestures {
    config: DragConfig,
    phase: DragPhase,
}

impl DragGestures {
    pub fn new() -> Self {
        Self {
            config: DragConfig {
                touch_slop_px: 8.0,
                on_start: None,
                on_drag: None,
                on_end: None,
                on_cancel: None,
            },
            phase: DragPhase::Idle,
        }
    }

    pub fn on_start(mut self, f: impl Fn(DragStart) + 'static) -> Self {
        self.config.on_start = Some(Rc::new(f));
        self
    }

    pub fn on_drag(mut self, f: impl Fn(DragUpdate) + 'static) -> Self {
        self.config.on_drag = Some(Rc::new(f));
        self
    }

    pub fn on_end(mut self, f: impl Fn(DragEnd) + 'static) -> Self {
        self.config.on_end = Some(Rc::new(f));
        self
    }

    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.config.on_cancel = Some(Rc::new(f));
        self
    }

    pub fn touch_slop(mut self, px: f32) -> Self {
        self.config.touch_slop_px = px;
        self
    }
}

impl Default for DragGestures {
    fn default() -> Self {
        Self::new()
    }
}

fn delta(from: Vec2, to: Vec2) -> Vec2 {
    Vec2 {
        x: to.x - from.x,
        y: to.y - from.y,
    }
}

impl PointerInputHandler for DragGestures {
    fn clone_handler(&self) -> Box<dyn PointerInputHandler> {
        Box::new(Self {
            config: self.config.clone(),
            phase: DragPhase::Idle,
        })
    }

    fn sync(&mut self, next: &dyn PointerInputHandler) {
        if let Some(next) = (next as &dyn Any).downcast_ref::<DragGestures>() {
            self.config = next.config.clone();
        }
    }

    fn on_event(&mut self, event: &PointerEvent) -> bool {
        match event.event {
            PointerEventKind::Down(_) => {
                // A second pointer must not steal an in-flight drag; its own
                // release would leave the first drag without an end.
                if matches!(self.phase, DragPhase::Idle) {
                    self.phase = DragPhase::Pressed {
                        id: event.id,
                        start: event.position,
                    };
                }
                false
            }
            PointerEventKind::Move => match self.phase {
                DragPhase::Pressed { id, start, .. } if id == event.id => {
                    let total = delta(start, event.position);
                    if total.x.hypot(total.y) <= self.config.touch_slop_px {
                        return false;
                    }
                    self.phase = DragPhase::Dragging {
                        id,
                        start,
                        last: event.position,
                    };
                    if let Some(on_start) = &self.config.on_start {
                        on_start(DragStart {
                            id,
                            position: event.position,
                        });
                    }
                    if let Some(on_drag) = &self.config.on_drag {
                        on_drag(DragUpdate {
                            id,
                            position: event.position,
                            delta: total,
                            total,
                        });
                    }
                    true
                }
                DragPhase::Dragging { id, start, last } if id == event.id => {
                    let step = delta(last, event.position);
                    self.phase = DragPhase::Dragging {
                        id,
                        start,
                        last: event.position,
                    };
                    if let Some(on_drag) = &self.config.on_drag {
                        on_drag(DragUpdate {
                            id,
                            position: event.position,
                            delta: step,
                            total: delta(start, event.position),
                        });
                    }
                    true
                }
                _ => false,
            },
            PointerEventKind::Up(_) => {
                let ours = matches!(self.phase, DragPhase::Dragging { id, .. } if id == event.id);
                if !ours {
                    return false;
                }
                match std::mem::replace(&mut self.phase, DragPhase::Idle) {
                    DragPhase::Dragging { id, start, .. } => {
                        if let Some(on_end) = &self.config.on_end {
                            on_end(DragEnd {
                                id,
                                position: event.position,
                                total: delta(start, event.position),
                            });
                        }
                        true
                    }
                    _ => false,
                }
            }
            PointerEventKind::Cancel => {
                let was_dragging = matches!(self.phase, DragPhase::Dragging { .. });
                self.phase = DragPhase::Idle;
                if was_dragging {
                    if let Some(on_cancel) = &self.config.on_cancel {
                        on_cancel();
                    }
                    true
                } else {
                    false
                }
            }
            PointerEventKind::Enter | PointerEventKind::Leave => false,
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

fn next_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// The gesture dispatcher for a modifier chain, if it carries one.
pub(crate) fn dispatch_from(
    nodes: &[crate::modifier_node::ModifierNodeHandle],
) -> Option<PointerInputDispatch> {
    if nodes.is_empty() {
        return None;
    }
    nodes.iter().find_map(|handle| {
        let node = handle.node();
        let is_input = node.as_any().downcast_ref::<PointerInputNode>().is_some();
        is_input.then(|| PointerInputDispatch::new(node))
    })
}
