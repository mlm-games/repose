//! Shared runner helpers extracted from duplicated desktop/web/android `App` impls.

use repose_app::{ReposeRuntime, TouchGestureState};
use repose_core::Vec2;
use repose_core::input::PointerButton;
use repose_core::shortcuts::{Action, Gesture};
use winit::event::{ElementState, MouseScrollDelta, Touch};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::common::{map_key, winit_key_to_repose};

/// Update `Modifiers` from winit state - shared.
pub fn on_modifiers_changed(rt: &mut ReposeRuntime, state: &winit::keyboard::ModifiersState) {
    crate::common::update_modifiers(&mut rt.modifiers, state);
}

/// CursorMoved helper - returns cursor + whether inspector hover updated.
pub fn on_cursor_moved(
    rt: &mut ReposeRuntime,
    pos: Vec2,
    inspector: &mut Option<repose_devtools::Inspector>,
) -> Option<repose_core::CursorIcon> {
    let result = rt.handle_pointer_move(pos);
    if let (Some(inspector), Some(f)) = (inspector, &rt.frame_cache)
        && inspector.hud.inspector_enabled {
            let hit = f.hit_regions.iter().find(|h| h.rect.contains(pos));
            let hover_rect = hit.map(|h| h.rect);
            let hover_info = hit.and_then(|h| {
                f.semantics_nodes.iter().find(|s| s.id == h.id).map(|s| {
                    repose_devtools::HoveredInfo {
                        id: s.id,
                        role: format!("{:?}", s.role),
                        label: s.label.clone(),
                    }
                })
            });
            inspector.hud.set_hovered(hover_rect, hover_info);
        }
    result.cursor
}

/// MouseWheel helper - converts delta to px and dispatches.
pub fn on_mouse_wheel(rt: &mut ReposeRuntime, delta: MouseScrollDelta, scale: f32) -> bool {
    let (dx_px, dy_px) = match delta {
        MouseScrollDelta::LineDelta(x, y) => {
            let unit_px = repose_core::locals::dp_to_px(60.0) * scale;
            (-(x * unit_px), -(y * unit_px))
        }
        MouseScrollDelta::PixelDelta(p) => (-(p.x as f32), -(p.y as f32)),
    };
    rt.handle_scroll(Vec2 { x: dx_px, y: dy_px })
}

/// Map winit MouseButton -> PointerButton.
pub fn map_mouse_button(btn: winit::event::MouseButton) -> Option<PointerButton> {
    match btn {
        winit::event::MouseButton::Left => Some(PointerButton::Primary),
        winit::event::MouseButton::Right => Some(PointerButton::Secondary),
        winit::event::MouseButton::Middle => Some(PointerButton::Tertiary),
        _ => None,
    }
}

/// Raw touch result without dispatch - caller handles `dispatch_action` to avoid double-borrow.
pub struct TouchResult {
    pub dirty: bool,
    pub pinch: Option<(f32, Vec2)>,
    pub pan: Option<Vec2>,
    pub swipe_right: Option<bool>,
}

pub fn handle_touch_raw(
    rt: &mut ReposeRuntime,
    touch_gestures: &mut TouchGestureState,
    t: &Touch,
    scale: f32,
) -> TouchResult {
    let pos_px = (t.location.x as f32, t.location.y as f32);
    let tid = t.id;
    match t.phase {
        winit::event::TouchPhase::Started => {
            touch_gestures.touch_started(rt, tid, pos_px);
            TouchResult {
                dirty: true,
                pinch: None,
                pan: None,
                swipe_right: None,
            }
        }
        winit::event::TouchPhase::Moved => {
            let (dirty, pinch, pan) = touch_gestures.touch_moved(rt, tid, pos_px, scale);
            TouchResult {
                dirty,
                pinch,
                pan,
                swipe_right: None,
            }
        }
        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
            let cancelled = t.phase == winit::event::TouchPhase::Cancelled;
            let swipe_right = touch_gestures.touch_ended(rt, tid, pos_px, cancelled);
            TouchResult {
                dirty: false,
                pinch: None,
                pan: None,
                swipe_right,
            }
        }
    }
}

/// HACK: Legacy wrapper that dispatches gestures inline (use `handle_touch_raw` when caller
/// needs to avoid double-borrow of `self` containing `rt`).
pub fn on_touch(
    rt: &mut ReposeRuntime,
    touch_gestures: &mut TouchGestureState,
    t: &Touch,
    scale: f32,
    mut dispatch: impl FnMut(Action) -> bool,
) -> bool {
    let r = handle_touch_raw(rt, touch_gestures, t, scale);
    let mut dirty = r.dirty;
    if let Some((delta_scale, center)) = r.pinch
        && dispatch(Action::Gesture(Gesture::PinchWithCenter {
            delta_scale,
            center,
        })) {
            dirty = true;
        }
    if let Some(delta) = r.pan
        && dispatch(Action::Gesture(Gesture::Pan { delta })) {
            dirty = true;
        }
    if let Some(right) = r.swipe_right {
        let g = if right {
            Gesture::SwipeRight
        } else {
            Gesture::SwipeLeft
        };
        if dispatch(Action::Gesture(g)) {
            dirty = true;
        }
    }
    dirty
}

/// Touch handler that also syncs IME for focused textfields (web/android).
/// Returns whether a redraw is needed. Probably shared for desktop once winit unifies touch.
pub fn on_touch_with_ime(
    rt: &mut ReposeRuntime,
    touch_gestures: &mut TouchGestureState,
    t: &Touch,
    scale: f32,
    window: &winit::window::Window,
    dispatch: impl FnMut(Action) -> bool,
) -> bool {
    let pos_px = (t.location.x as f32, t.location.y as f32);
    let tid = t.id;
    if t.phase == winit::event::TouchPhase::Started {
        let focused = touch_gestures.touch_started(rt, tid, pos_px);
        if let Some(fid) = focused {
            if rt.is_textfield(fid) {
                let (purpose, ac, cap) = rt.focused_keyboard_hints();
                crate::common::set_ime_for_textfield_ex(window, true, purpose, ac, cap);
            } else {
                crate::common::set_ime_for_textfield(window, false);
            }
        } else {
            crate::common::set_ime_for_textfield(window, false);
        }
        return true;
    }
    on_touch(rt, touch_gestures, t, scale, dispatch)
}

/// Shared inspector toggle + runtime dispatch.
/// Returns true if event consumed / needs redraw.
pub fn on_keyboard_input(
    rt: &mut ReposeRuntime,
    key_event: &winit::event::KeyEvent,
    inspector: &mut Option<repose_devtools::Inspector>,
) -> bool {
    if key_event.state == ElementState::Pressed
        && !key_event.repeat
        && rt.modifiers.ctrl
        && rt.modifiers.shift
        && key_event.physical_key == PhysicalKey::Code(KeyCode::KeyI)
        && let Some(inspector) = inspector {
            inspector.hud.toggle_inspector();
            return true;
        }
    let mapped = map_key(key_event.physical_key, &rt.modifiers);
    let ke = winit_key_to_repose(key_event, &mapped, &rt.modifiers);
    rt.handle_key_with_text(&ke, key_event.text.as_deref())
}

/// Ime dispatch helper.
pub fn on_ime(rt: &mut ReposeRuntime, ime: &winit::event::Ime) {
    use winit::event::Ime;
    let ev = match ime {
        Ime::Enabled => repose_core::input::ImeEvent::Start,
        Ime::Preedit(text, cursor) => repose_core::input::ImeEvent::Update {
            text: text.clone(),
            cursor: cursor.map(|(a, b)| (a, b)),
        },
        Ime::Commit(text) => repose_core::input::ImeEvent::Commit(text.clone()),
        Ime::Disabled => repose_core::input::ImeEvent::Cancel,
    };
    rt.handle_ime(&ev);
}
