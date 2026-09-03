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
    if let (Some(inspector), Some(f)) = (inspector, &rt.frame_cache) {
        if inspector.hud.inspector_enabled {
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

/// `dispatch` is called for pinch/pan/swipe gestures (usually `self.dispatch_action`).
pub fn on_touch(
    rt: &mut ReposeRuntime,
    touch_gestures: &mut TouchGestureState,
    t: &Touch,
    scale: f32,
    mut dispatch: impl FnMut(Action) -> bool,
) -> bool {
    let pos_px = (t.location.x as f32, t.location.y as f32);
    let tid = t.id;
    match t.phase {
        winit::event::TouchPhase::Started => {
            touch_gestures.touch_started(rt, tid, pos_px);
            true
        }
        winit::event::TouchPhase::Moved => {
            let (mut dirty, pinch, pan) = touch_gestures.touch_moved(rt, tid, pos_px, scale);
            if let Some((delta_scale, center)) = pinch {
                if dispatch(Action::Gesture(Gesture::PinchWithCenter {
                    delta_scale,
                    center,
                })) {
                    dirty = true;
                }
            }
            if let Some(delta) = pan {
                if dispatch(Action::Gesture(Gesture::Pan { delta })) {
                    dirty = true;
                }
            }
            dirty
        }
        winit::event::TouchPhase::Ended | winit::event::TouchPhase::Cancelled => {
            let cancelled = t.phase == winit::event::TouchPhase::Cancelled;
            let swipe_right = touch_gestures.touch_ended(rt, tid, pos_px, cancelled);
            let mut dirty = false;
            if let Some(right) = swipe_right {
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
    }
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
    {
        if let Some(inspector) = inspector {
            inspector.hud.toggle_inspector();
            return true;
        }
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
            cursor: cursor.map(|(a, b)| (a as usize, b as usize)),
        },
        Ime::Commit(text) => repose_core::input::ImeEvent::Commit(text.clone()),
        Ime::Disabled => repose_core::input::ImeEvent::Cancel,
    };
    rt.handle_ime(&ev);
}
