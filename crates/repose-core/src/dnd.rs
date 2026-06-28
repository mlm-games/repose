use crate::Vec2;
use crate::color::{Brush, Color};
use crate::geometry::Rect;
use crate::text::{FontStyle, FontWeight, TextAlign, TextDecoration};
use crate::input::{Modifiers, PointerKind};
use crate::runtime::{Frame, HitRegion};
use crate::shortcuts::DragAction;
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
#[derive(Clone, Debug)]
pub struct DragSession {
    pub source_id: u64,
    pub payload: DragPayload,
    pub start_px: (f32, f32),
    pub over_id: Option<u64>,
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

    let payload = cb(DragStart {
        source_id: capture_id,
        position: current_pos,
        modifiers,
    });
    let Some(payload) = payload else {
        return false;
    };

    DND_SESSION.with(|s| {
        *s.borrow_mut() = Some(DragSession {
            source_id: capture_id,
            payload,
            start_px: (start_pos.x, start_pos.y),
            over_id: None,
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
                    // Widget doesn't support drag — try mouse down again next time
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
                            // Widget doesn't support drag — cancel long press
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
                return true; // consumed: touch is active, don't fall through
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

    let color = if external_file_drag {
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
            color,
            width: crate::locals::dp_to_px(2.0),
            radius: [r; 4],
        });
    }

    // Cursor badge
    let badge = Rect {
        x: pos.x + crate::locals::dp_to_px(12.0),
        y: pos.y + crate::locals::dp_to_px(12.0),
        w: crate::locals::dp_to_px(110.0),
        h: crate::locals::dp_to_px(24.0),
    };

    let bg = if external_file_drag {
        Color::from_hex("#FFAA0077")
    } else {
        Color::from_hex("#44AAFF77")
    };

    let r = crate::locals::dp_to_px(8.0);
    scene.nodes.push(SceneNode::Rect {
        rect: badge,
        brush: Brush::Solid(bg),
        radius: [r; 4],
    });
    scene.nodes.push(SceneNode::Text {
        rect: Rect {
            x: badge.x + crate::locals::dp_to_px(8.0),
            y: badge.y + crate::locals::dp_to_px(6.0),
            w: 0.0,
            h: crate::locals::dp_to_px(14.0),
        },
        text: Arc::<str>::from(" "),
        color: Color::WHITE,
        size: crate::locals::dp_to_px(12.0),
        font_family: None,
        text_align: TextAlign::Unspecified,
        font_weight: FontWeight::NORMAL,
        font_style: FontStyle::Normal,
        text_decoration: TextDecoration::default(),
        letter_spacing: 0.0,
        line_height: 0.0,
    });
}
