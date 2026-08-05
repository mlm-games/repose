use crate::Vec2;
use crate::color::{Brush, Color};
use crate::geometry::Rect;
use crate::input::{Modifiers, PointerKind};
use crate::runtime::{Frame, HitRegion};
use crate::shortcuts::DragAction;
use crate::text::{FontStyle, FontWeight, TextAlign, TextDecoration};
use crate::view::{Scene, SceneNode};
use std::cell::RefCell;
use std::{any::Any, path::PathBuf, rc::Rc, sync::Arc};
use web_time::Instant;

/// Opaque payload moved during internal drag & drop.
/// Use [`downcast_drag_payload`] on the receiver side to recover a typed value.
pub type DragPayload = Rc<dyn Any>;

/// Wrap a typed value into a [`DragPayload`] for a drag source.
///
/// ```ignore
/// Modifier::new().on_drag_start(|_start| Some(drag_payload(MyItem { id: 1 })))
/// ```
pub fn drag_payload<T: 'static>(value: T) -> DragPayload {
    Rc::new(value)
}

/// Try to downcast a drag payload to a typed reference. Used on the drop side.
///
/// ```ignore
/// if let Some(item) = downcast_drag_payload::<MyItem>(&ev.payload) {
///     // handle item
/// }
/// ```
pub fn downcast_drag_payload<T: 'static>(payload: &DragPayload) -> Option<&T> {
    payload.as_ref().downcast_ref::<T>()
}

/// Block-style convenience for [`Modifier::on_drag_start`] with a typed payload.
///
/// ```ignore
/// use repose_core::{Modifier, drag_and_drop_source};
/// struct MyItem { id: i32 }
/// let m = drag_and_drop_source(Modifier::new(), |_start| Some(MyItem { id: 1 }));
/// ```
///
/// is equivalent to:
///
/// ```ignore
/// Modifier::new().on_drag_start(|_start| Some(drag_payload(MyItem { id: 1 })))
/// ```
pub fn drag_and_drop_source<T, F>(mut modifier: crate::Modifier, on_start: F) -> crate::Modifier
where
    T: 'static,
    F: Fn(DragStart) -> Option<T> + 'static,
{
    modifier = modifier.on_drag_start(move |start| on_start(start).map(drag_payload::<T>));
    modifier
}

/// Typed source + custom drag decoration in one call.
pub fn drag_and_drop_source_with_preview<T, F>(
    mut modifier: crate::Modifier,
    preview: DragPreview,
    on_start: F,
) -> crate::Modifier
where
    T: 'static,
    F: Fn(DragStart) -> Option<T> + 'static,
{
    modifier = modifier
        .draw_drag_decoration_rc(preview)
        .on_drag_start(move |start| on_start(start).map(drag_payload::<T>));
    modifier
}

/// Block-style convenience for [`Modifier::on_drop`] with a typed payload. The
/// drop is accepted when the closure returns `true`; the typed payload is
/// downcast before the closure is invoked.
///
/// ```ignore
/// use repose_core::{Modifier, drag_and_drop_target};
/// struct MyItem { id: i32 }
/// let m = drag_and_drop_target(Modifier::new(), |_ev, item: &MyItem| {
///     println!("got id {}", item.id);
///     true
/// });
/// ```
pub fn drag_and_drop_target<T, F>(mut modifier: crate::Modifier, on_drop: F) -> crate::Modifier
where
    T: 'static,
    F: Fn(&DropEvent, &T) -> bool + 'static,
{
    modifier = modifier.on_drop(move |ev| match downcast_drag_payload::<T>(&ev.payload) {
        Some(v) => on_drop(&ev, v),
        None => false,
    });
    modifier
}

/// Compose-like typed drag/drop modifier helpers.
pub trait DragDropModifierExt: Sized {
    /// Make this node a typed drag source.
    fn drag_source<T>(
        self,
        make_payload: impl Fn(DragStart) -> Option<T> + 'static,
    ) -> crate::Modifier
    where
        T: 'static;

    /// Typed drag-enter.
    fn on_drag_enter_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static;

    /// Typed drag-over.
    fn on_drag_over_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static;

    /// Typed drag-leave.
    fn on_drag_leave_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static;

    /// Typed drop target.
    fn on_drop_typed<T>(
        self,
        f: impl Fn(&DropEvent, &T) -> bool + 'static,
    ) -> crate::Modifier
    where
        T: 'static;

    /// Common case: typed drag-over + typed drop.
    fn drop_target<T>(
        self,
        on_drop: impl Fn(&DropEvent, &T) -> bool + 'static,
    ) -> crate::Modifier
    where
        T: 'static;
}

impl DragDropModifierExt for crate::Modifier {
    fn drag_source<T>(
        self,
        make_payload: impl Fn(DragStart) -> Option<T> + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drag_start(move |start| make_payload(start).map(drag_payload::<T>))
    }

    fn on_drag_enter_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drag_enter(move |ev| {
            if let Some(payload) = downcast_drag_payload::<T>(&ev.payload) {
                f(&ev, payload);
            }
        })
    }

    fn on_drag_over_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drag_over(move |ev| {
            if let Some(payload) = downcast_drag_payload::<T>(&ev.payload) {
                f(&ev, payload);
            }
        })
    }

    fn on_drag_leave_typed<T>(
        self,
        f: impl Fn(&DragOver, &T) + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drag_leave(move |ev| {
            if let Some(payload) = downcast_drag_payload::<T>(&ev.payload) {
                f(&ev, payload);
            }
        })
    }

    fn on_drop_typed<T>(
        self,
        f: impl Fn(&DropEvent, &T) -> bool + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drop(move |ev| {
            let Some(payload) = downcast_drag_payload::<T>(&ev.payload) else {
                return false;
            };
            f(&ev, payload)
        })
    }

    fn drop_target<T>(
        self,
        on_drop: impl Fn(&DropEvent, &T) -> bool + 'static,
    ) -> crate::Modifier
    where
        T: 'static,
    {
        self.on_drop_typed(on_drop)
    }
}

#[derive(Clone, Debug)]
pub struct DragStart {
    pub source_id: u64,
    pub position: Vec2,
    pub modifiers: Modifiers,
}

#[derive(Clone, Debug)]
pub struct DragOver {
    pub source_id: u64,
    pub target_id: u64,
    pub position: Vec2,
    pub modifiers: Modifiers,
    pub payload: DragPayload,
}

#[derive(Clone, Debug)]
pub struct DropEvent {
    pub source_id: u64,
    pub target_id: u64,
    pub position: Vec2,
    pub modifiers: Modifiers,
    pub payload: DragPayload,
}

/// Sent to the drag source when the drag ends (drop or cancel).
#[derive(Clone, Copy, Debug)]
pub struct DragEnd {
    pub accepted: bool,
}

/// Context passed to a drag preview painter each frame while dragging.
#[derive(Clone)]
pub struct DragPreviewCtx {
    /// Current pointer position (px).
    pub pointer: Vec2,
    /// Pointer position when the drag started (px).
    pub start_pointer: Vec2,
    /// Source hit-region rect at drag start (px).
    pub source_rect: Rect,
    /// `pointer_at_start - source_rect.origin` so the ghost sticks under the finger/cursor.
    pub grab_offset: Vec2,
    /// Active payload (clone of session payload).
    pub payload: DragPayload,
}

/// Painter for the floating drag decoration.
/// Coordinates are in **screen px**. Draw relative to `ctx.pointer` / `ctx.grab_offset`.
pub type DragPreview = Rc<dyn Fn(&mut Scene, &DragPreviewCtx)>;

/// Optional one-shot preview set from inside `on_drag_start` (overrides modifier).
thread_local! {
    static PENDING_PREVIEW: RefCell<Option<DragPreview>> = const { RefCell::new(None) };
}

/// Call from `on_drag_start` to supply a session-specific preview.
///
/// ```ignore
/// .on_drag_start(move |_s| {
///     provide_drag_preview(drag_preview_label(title.clone(), Color::from_hex("#44AAFF")));
///     Some(drag_payload(MyItem { .. }))
/// })
/// ```
pub fn provide_drag_preview(preview: DragPreview) {
    PENDING_PREVIEW.with(|p| *p.borrow_mut() = Some(preview));
}

fn take_pending_preview() -> Option<DragPreview> {
    PENDING_PREVIEW.with(|p| p.borrow_mut().take())
}

/// Simple floating label chip (good default for tabs / list rows).
pub fn drag_preview_label(label: impl Into<String>, accent: Color) -> DragPreview {
    let label = label.into();
    Rc::new(move |scene: &mut Scene, ctx: &DragPreviewCtx| {
        draw_label_chip(scene, ctx.pointer, &label, accent, false);
    })
}

/// Label chip with a slightly larger "elevated" look (shadow-ish fill).
pub fn drag_preview_chip(label: impl Into<String>, accent: Color) -> DragPreview {
    let label = label.into();
    Rc::new(move |scene: &mut Scene, ctx: &DragPreviewCtx| {
        draw_label_chip(scene, ctx.pointer, &label, accent, true);
    })
}

fn draw_label_chip(
    scene: &mut Scene,
    pointer: Vec2,
    label: &str,
    accent: Color,
    elevated: bool,
) {
    let ts = crate::locals::text_scale().0;
    let pad_x = crate::locals::dp_to_px(10.0);
    let pad_y = crate::locals::dp_to_px(6.0);
    let font_px = crate::locals::dp_to_px(13.0) * ts;
    // Approximate width: ~0.55em per char (good enough without measuring).
    let text_w = (label.chars().count() as f32 * font_px * 0.55)
        .max(crate::locals::dp_to_px(24.0));
    let w = text_w + pad_x * 2.0;
    let h = font_px + pad_y * 2.0;
    let r = crate::locals::dp_to_px(8.0);

    let origin = Vec2 {
        x: pointer.x + crate::locals::dp_to_px(14.0),
        y: pointer.y + crate::locals::dp_to_px(14.0),
    };
    let rect = Rect {
        x: origin.x,
        y: origin.y,
        w,
        h,
    };

    if elevated {
        // Soft "shadow"
        scene.nodes.push(SceneNode::Rect {
            rect: Rect {
                x: rect.x + crate::locals::dp_to_px(2.0),
                y: rect.y + crate::locals::dp_to_px(3.0),
                w: rect.w,
                h: rect.h,
            },
            brush: Brush::Solid(Color::from_rgba(0, 0, 0, 50)),
            radius: [r; 4],
        });
    }

    let bg = accent.with_alpha(0xDD);
    scene.nodes.push(SceneNode::Rect {
        rect,
        brush: Brush::Solid(bg),
        radius: [r; 4],
    });
    scene.nodes.push(SceneNode::Border {
        rect,
        color: accent.with_alpha(0xFF),
        width: crate::locals::dp_to_px(1.0),
        radius: [r; 4],
    });
    scene.nodes.push(SceneNode::Text {
        rect: Rect {
            x: rect.x + pad_x,
            y: rect.y + pad_y,
            w: text_w,
            h: font_px,
        },
        text: Arc::<str>::from(label),
        color: Color::WHITE,
        size: font_px,
        font_family: None,
        text_align: TextAlign::Unspecified,
        font_weight: FontWeight::MEDIUM,
        font_style: FontStyle::Normal,
        text_decoration: TextDecoration::default(),
        letter_spacing: 0.0,
        line_height: 0.0,
        extra_style: Default::default(),
        url: None,
        font_variation_settings: None,
    });
}

/// Default ghost: translucent clone of the source bounds, locked to grab offset.
fn draw_default_source_ghost(scene: &mut Scene, ctx: &DragPreviewCtx, accent: Color) {
    let w = ctx.source_rect.w.max(crate::locals::dp_to_px(24.0));
    let h = ctx.source_rect.h.max(crate::locals::dp_to_px(16.0));
    let rect = Rect {
        x: ctx.pointer.x - ctx.grab_offset.x,
        y: ctx.pointer.y - ctx.grab_offset.y,
        w,
        h,
    };
    let r = crate::locals::dp_to_px(6.0);
    scene.nodes.push(SceneNode::Rect {
        rect,
        brush: Brush::Solid(accent.with_alpha(0x55)),
        radius: [r; 4],
    });
    scene.nodes.push(SceneNode::Border {
        rect,
        color: accent.with_alpha(0xCC),
        width: crate::locals::dp_to_px(1.5),
        radius: [r; 4],
    });
}

/// A single dropped file descriptor.
/// - On desktop: `path` is `Some(PathBuf)`.
/// - On web: `path` is usually `None` (browser doesn't expose local paths).
#[derive(Clone, Debug)]
pub struct DroppedFile {
    pub name: String,
    pub path: Option<PathBuf>,
}

/// Payload type for file drag/drop coming from the OS/browser.
#[derive(Clone, Debug)]
pub struct DroppedFiles {
    pub files: Vec<DroppedFile>,
}

/// Tracks an active drag session (internal widget-to-widget DnD).
#[derive(Clone)]
pub struct DragSession {
    pub source_id: u64,
    pub payload: DragPayload,
    pub start_px: (f32, f32),
    pub over_id: Option<u64>,
    /// Source hit rect at drag start (px).
    pub source_rect: Rect,
    /// Pointer - source origin at drag start.
    pub grab_offset: Vec2,
    /// Optional custom decoration (Compose `drawDragDecoration`).
    pub preview: Option<DragPreview>,
}

// Manual Debug (Rc<dyn Fn> is not Debug)
impl std::fmt::Debug for DragSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DragSession")
            .field("source_id", &self.source_id)
            .field("start_px", &self.start_px)
            .field("over_id", &self.over_id)
            .field("source_rect", &self.source_rect)
            .field("grab_offset", &self.grab_offset)
            .field("preview", &self.preview.as_ref().map(|_| "…"))
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct MouseDownState {
    position: Vec2,
    capture_id: u64,
}

#[derive(Clone)]
struct TouchDownState {
    time: Instant,
    position: Vec2,
    capture_id: u64,
    long_press_pending: bool,
}

const LONG_PRESS_MS: u128 = 400;

thread_local! {
    static DND_FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
    static DND_SCALE: RefCell<f32> = const { RefCell::new(1.0) };
    static DND_SESSION: RefCell<Option<DragSession>> = const { RefCell::new(None) };
    static DND_MOUSE_DOWN: RefCell<Option<MouseDownState>> = const { RefCell::new(None) };
    static DND_TOUCH_DOWN: RefCell<Option<TouchDownState>> = const { RefCell::new(None) };
}

/// Set the current frame for DnD hit-testing. Called by platform after each render.
pub fn set_dnd_frame(frame: Option<Frame>) {
    DND_FRAME.with(|f| *f.borrow_mut() = frame);
}

/// Set the display scale for DnD slop calculation.
pub fn set_dnd_scale(scale: f32) {
    DND_SCALE.with(|s| *s.borrow_mut() = scale);
}

/// Check if a drag session is currently active.
pub fn is_dragging() -> bool {
    DND_SESSION.with(|s| s.borrow().is_some())
}

/// Current drag session snapshot (if any).
pub fn current_drag_session() -> Option<DragSession> {
    DND_SESSION.with(|s| s.borrow().clone())
}

fn touch_slop_px(scale: f32) -> f32 {
    6.0 * scale
}

fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
}

fn is_dnd_target(hit: &HitRegion) -> bool {
    hit.on_drop.is_some()
        || hit.on_drag_enter.is_some()
        || hit.on_drag_over.is_some()
        || hit.on_drag_leave.is_some()
}

pub fn dnd_target_id_at(frame: &Frame, pos: Vec2) -> Option<u64> {
    frame
        .hit_regions
        .iter()
        .rev()
        .filter(|h| h.rect.contains(pos))
        .find(|h| is_dnd_target(h))
        .map(|h| h.id)
}

fn dnd_update_over(frame: &Frame, session: &mut DragSession, modifiers: Modifiers, pos: Vec2) {
    let new_over = dnd_target_id_at(frame, pos);

    if new_over != session.over_id {
        if let Some(prev) = session.over_id {
            if let Some(i) = hit_index_by_id(frame, prev) {
                if let Some(cb) = &frame.hit_regions[i].on_drag_leave {
                    cb(DragOver {
                        source_id: session.source_id,
                        target_id: prev,
                        position: pos,
                        modifiers,
                        payload: session.payload.clone(),
                    });
                }
            }
        }

        if let Some(now) = new_over {
            if let Some(i) = hit_index_by_id(frame, now) {
                if let Some(cb) = &frame.hit_regions[i].on_drag_enter {
                    cb(DragOver {
                        source_id: session.source_id,
                        target_id: now,
                        position: pos,
                        modifiers,
                        payload: session.payload.clone(),
                    });
                }
            }
        }

        session.over_id = new_over;
    }

    if let Some(over) = session.over_id {
        if let Some(i) = hit_index_by_id(frame, over) {
            if let Some(cb) = &frame.hit_regions[i].on_drag_over {
                cb(DragOver {
                    source_id: session.source_id,
                    target_id: over,
                    position: pos,
                    modifiers,
                    payload: session.payload.clone(),
                });
            }
        }
    }
}

/// Finish a drag-and-drop session.
fn dnd_finish(
    frame: &Frame,
    session: DragSession,
    modifiers: Modifiers,
    pos: Vec2,
    accept_if_possible: bool,
) -> bool {
    let mut accepted = false;
    if accept_if_possible {
        let drop_target = dnd_target_id_at(frame, pos);
        if let Some(tid) = drop_target {
            if let Some(i) = hit_index_by_id(frame, tid) {
                if let Some(cb) = &frame.hit_regions[i].on_drop {
                    accepted = cb(DropEvent {
                        source_id: session.source_id,
                        target_id: tid,
                        position: pos,
                        modifiers,
                        payload: session.payload.clone(),
                    });
                }
            }
        }
    }

    if let Some(i) = hit_index_by_id(frame, session.source_id) {
        if let Some(cb) = &frame.hit_regions[i].on_drag_end {
            cb(DragEnd { accepted });
        }
    }

    accepted
}

fn initiate_drag(
    frame: &Frame,
    capture_id: u64,
    start_pos: Vec2,
    current_pos: Vec2,
    modifiers: Modifiers,
) -> bool {
    let Some(i) = hit_index_by_id(frame, capture_id) else {
        return false;
    };
    let Some(cb) = &frame.hit_regions[i].on_drag_start else {
        return false;
    };

    // Clear any stale pending preview from a previous aborted start.
    let _ = take_pending_preview();

    let payload = cb(DragStart {
        source_id: capture_id,
        position: current_pos,
        modifiers,
    });
    let Some(payload) = payload else {
        let _ = take_pending_preview();
        return false;
    };

    // Prefer provide_drag_preview(...) from on_drag_start; else modifier decoration.
    let preview = take_pending_preview().or_else(|| frame.hit_regions[i].drag_preview.clone());

    let source_rect = frame.hit_regions[i].rect;
    let grab_offset = Vec2 {
        x: start_pos.x - source_rect.x,
        y: start_pos.y - source_rect.y,
    };

    DND_SESSION.with(|s| {
        *s.borrow_mut() = Some(DragSession {
            source_id: capture_id,
            payload,
            start_px: (start_pos.x, start_pos.y),
            over_id: None,
            source_rect,
            grab_offset,
            preview,
        });
    });
    true
}

/// Handle a DragAction from the platform. Returns true if the action was consumed.
pub fn handle_drag_action(action: &DragAction) -> bool {
    let scale = DND_SCALE.with(|s| *s.borrow());
    let slop = touch_slop_px(scale);

    match *action {
        DragAction::Press {
            position,
            capture_id,
            kind,
            ..
        } => {
            match kind {
                PointerKind::Mouse => {
                    DND_MOUSE_DOWN.with(|m| {
                        *m.borrow_mut() = Some(MouseDownState {
                            position,
                            capture_id,
                        });
                    });
                }
                _ => {
                    // Touch (or pen/unknown): start long-press timer
                    DND_TOUCH_DOWN.with(|t| {
                        *t.borrow_mut() = Some(TouchDownState {
                            time: web_time::Instant::now(),
                            position,
                            capture_id,
                            long_press_pending: true,
                        });
                    });
                }
            }
            false
        }

        DragAction::Move {
            position,
            modifiers,
        } => {
            // If already dragging, update
            if DND_SESSION.with(|s| s.borrow().is_some()) {
                if let Some(frame) = DND_FRAME.with(|f| f.borrow().clone()) {
                    DND_SESSION.with(|s| {
                        if let Some(ref mut session) = *s.borrow_mut() {
                            dnd_update_over(&frame, session, modifiers, position);
                        }
                    });
                }
                return true;
            }

            // Mouse: try drag initiation (drag past slop)
            if let Some(down) = DND_MOUSE_DOWN.with(|m| m.borrow().clone()) {
                let dx = position.x - down.position.x;
                let dy = position.y - down.position.y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist >= slop {
                    if let Some(frame) = DND_FRAME.with(|f| f.borrow().clone()) {
                        if initiate_drag(
                            &frame,
                            down.capture_id,
                            down.position,
                            position,
                            modifiers,
                        ) {
                            // Update over immediately
                            DND_SESSION.with(|s| {
                                if let Some(ref mut session) = *s.borrow_mut() {
                                    dnd_update_over(&frame, session, modifiers, position);
                                }
                            });
                            DND_MOUSE_DOWN.with(|m| *m.borrow_mut() = None);
                            return true;
                        }
                    }
                    // Widget doesn't support drag - try mouse down again next time
                    // (actually, clear it so we don't retry on every move)
                    DND_MOUSE_DOWN.with(|m| *m.borrow_mut() = None);
                }
                return true; // consumed: mouse is pressed, don't fall through to scroll
            }

            // Touch: try long-press initiation
            if let Some(touch) = DND_TOUCH_DOWN.with(|t| t.borrow().clone()) {
                if touch.long_press_pending {
                    let elapsed_ms = (Instant::now() - touch.time).as_millis() as u128;
                    let dx = position.x - touch.position.x;
                    let dy = position.y - touch.position.y;
                    let dist = (dx * dx + dy * dy).sqrt();

                    if elapsed_ms >= LONG_PRESS_MS && dist <= slop {
                        if let Some(frame) = DND_FRAME.with(|f| f.borrow().clone()) {
                            if initiate_drag(
                                &frame,
                                touch.capture_id,
                                touch.position,
                                position,
                                modifiers,
                            ) {
                                DND_SESSION.with(|s| {
                                    if let Some(ref mut session) = *s.borrow_mut() {
                                        dnd_update_over(&frame, session, modifiers, position);
                                    }
                                });
                                DND_TOUCH_DOWN.with(|t| *t.borrow_mut() = None);
                                return true;
                            }
                            // Widget doesn't support drag - cancel long press
                            DND_TOUCH_DOWN.with(|t| {
                                if let Some(ref mut td) = *t.borrow_mut() {
                                    td.long_press_pending = false;
                                }
                            });
                        }
                    }
                    if dist > slop {
                        DND_TOUCH_DOWN.with(|t| {
                            if let Some(ref mut td) = *t.borrow_mut() {
                                td.long_press_pending = false;
                            }
                        });
                    }
                }
                // Only consume if still waiting for long-press (within slop, timer not yet expired).
                // If long-press was cancelled (moved past slop), let scroll handle the event.
                let still_pending = DND_TOUCH_DOWN.with(|t| {
                    t.borrow().as_ref().map(|td| td.long_press_pending).unwrap_or(false)
                });
                if still_pending {
                    return true;
                }
            }

            false
        }

        DragAction::Release {
            position,
            modifiers,
        } => {
            let mut consumed = false;

            if let Some(session) = DND_SESSION.with(|s| s.borrow_mut().take()) {
                if let Some(frame) = DND_FRAME.with(|f| f.borrow().clone()) {
                    dnd_finish(&frame, session, modifiers, position, true);
                }
                consumed = true;
            }

            DND_MOUSE_DOWN.with(|m| *m.borrow_mut() = None);
            DND_TOUCH_DOWN.with(|t| *t.borrow_mut() = None);

            consumed
        }

        DragAction::Cancel => {
            let mut consumed = false;
            if let Some(session) = DND_SESSION.with(|s| s.borrow_mut().take()) {
                if let Some(frame) = DND_FRAME.with(|f| f.borrow().clone()) {
                    dnd_finish(
                        &frame,
                        session,
                        Modifiers::default(),
                        Vec2::default(),
                        false,
                    );
                }
                consumed = true;
            }
            DND_MOUSE_DOWN.with(|m| *m.borrow_mut() = None);
            DND_TOUCH_DOWN.with(|t| *t.borrow_mut() = None);
            consumed
        }
    }
}

/// Draw drag overlay indicator on the scene.
/// `external_file_drag` enables orange styling for OS/browser file-drop overlays.
pub fn overlay_drag_indicator(
    scene: &mut Scene,
    mouse_pos_px: (f32, f32),
    external_file_drag: bool,
) {
    if !is_dragging() && !external_file_drag {
        return;
    }

    let pos = Vec2 {
        x: mouse_pos_px.0,
        y: mouse_pos_px.1,
    };

    let frame = DND_FRAME.with(|f| f.borrow().clone());
    let Some(ref f) = frame else {
        return;
    };

    let accent = if external_file_drag {
        Color::from_hex("#FFAA00")
    } else {
        Color::from_hex("#44AAFF")
    };

    // Highlight best drop target under cursor
    if let Some(tid) = dnd_target_id_at(f, pos)
        && let Some(hit) = f.hit_regions.iter().find(|h| h.id == tid)
    {
        let r = crate::locals::dp_to_px(8.0);
        scene.nodes.push(SceneNode::Border {
            rect: hit.rect,
            color: accent,
            width: crate::locals::dp_to_px(2.0),
            radius: [r; 4],
        });
    }

    if external_file_drag {
        draw_label_chip(scene, pos, "Drop files", accent, true);
        return;
    }

    DND_SESSION.with(|s| {
        let session = s.borrow();
        let Some(ref session) = *session else {
            return;
        };

        let ctx = DragPreviewCtx {
            pointer: pos,
            start_pointer: Vec2 {
                x: session.start_px.0,
                y: session.start_px.1,
            },
            source_rect: session.source_rect,
            grab_offset: session.grab_offset,
            payload: session.payload.clone(),
        };

        if let Some(ref preview) = session.preview {
            preview(scene, &ctx);
        } else {
            draw_default_source_ghost(scene, &ctx, accent);
        }
    });
}
