use crate::*;
use repose_core::Modifiers;
use repose_core::Vec2;
use repose_core::input::{PointerButton, PointerEvent, PointerEventKind, PointerId, PointerKind};
use repose_core::locals::dp_to_px;
use repose_core::runtime::Frame;
use repose_ui::TextFieldState;
use repose_ui::textfield::{
    TF_FONT_DP, TF_PADDING_X_DP, caret_xy_for_byte, index_for_x_bytes, index_for_xy_bytes,
    measure_text,
};

pub(crate) fn tick_snackbar(last_redraw: web_time::Instant) {
    let now = web_time::Instant::now();
    let elapsed = now.saturating_duration_since(last_redraw);
    let ms = elapsed.as_millis().min(u32::MAX as u128) as u32;
    if ms > 0 {
        repose_ui::overlay::SnackbarController::tick_for_frame(ms);
    }
}

pub(crate) fn request_redraw(window: &Option<std::sync::Arc<winit::window::Window>>) {
    if let Some(w) = window {
        w.request_redraw();
    }
}

pub(crate) fn tf_key_of_in_frame(frame_cache: &Option<Frame>, visual_id: u64) -> u64 {
    if let Some(f) = frame_cache {
        return tf_key_of(f, visual_id);
    }
    visual_id
}

pub(crate) fn is_textfield_in_frame(frame_cache: &Option<Frame>, id: u64) -> bool {
    if let Some(f) = frame_cache {
        f.semantics_nodes
            .iter()
            .any(|n| n.id == id && n.role == Role::TextField)
    } else {
        false
    }
}

pub(crate) fn update_modifiers(modifiers: &mut Modifiers, state: &winit::keyboard::ModifiersState) {
    modifiers.shift = state.shift_key();
    modifiers.ctrl = state.control_key();
    modifiers.alt = state.alt_key();
    modifiers.meta = state.super_key();
    modifiers.command = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
}

/// Like `index_for_x_bytes` but applies visual transformation if active on the state.
/// The returned offset is in the original text's byte space.
pub(crate) fn index_for_x_bytes_vt(state: &TextFieldState, font_px: f32, x_px: f32) -> usize {
    if let Some(vt) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let tfmd = vt.filter(&annotated);
        let display_idx = index_for_x_bytes(tfmd.text.as_str(), font_px, x_px, 400, 0);
        tfmd.offset_mapping.transformed_to_original(display_idx)
    } else {
        index_for_x_bytes(&state.text, font_px, x_px, 400, 0)
    }
}

/// Like `index_for_xy_bytes` but applies visual transformation if active on the state.
pub(crate) fn index_for_xy_bytes_vt(
    state: &TextFieldState,
    font_px: f32,
    wrap_w: f32,
    x_px: f32,
    y_px: f32,
) -> usize {
    if let Some(vt) = &state.visual_transformation {
        let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
        let tfmd = vt.filter(&annotated);
        let display_idx = index_for_xy_bytes(tfmd.text.as_str(), font_px, wrap_w, x_px, y_px);
        tfmd.offset_mapping.transformed_to_original(display_idx)
    } else {
        index_for_xy_bytes(&state.text, font_px, wrap_w, x_px, y_px)
    }
}

/// Find the top-most hit region index under `pos` (reverse iteration).
pub(crate) fn top_hit_index(frame: &Frame, pos: Vec2) -> Option<usize> {
    frame
        .hit_regions
        .iter()
        .enumerate()
        .rev()
        .find(|(_, h)| h.rect.contains(pos))
        .map(|(i, _)| i)
}

pub(crate) fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    frame.hit_regions.iter().position(|h| h.id == id)
}

pub(crate) fn tf_key_of(frame: &Frame, visual_id: u64) -> u64 {
    if let Some(i) = hit_index_by_id(frame, visual_id) {
        let hr = &frame.hit_regions[i];
        return hr.tf_state_key.unwrap_or(hr.id);
    }
    visual_id
}

pub(crate) fn pe_mouse(event: PointerEventKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent::new(
        PointerId(0),
        PointerKind::Mouse,
        event,
        pos,
        1.0,
        mods,
    )
}

pub(crate) fn pe_touch(event: PointerEventKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent::new(
        PointerId(0),
        PointerKind::Touch,
        event,
        pos,
        1.0,
        mods,
    )
}

pub(crate) fn pe_down_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent::new(
        PointerId(0),
        kind,
        PointerEventKind::Down(PointerButton::Primary),
        pos,
        1.0,
        mods,
    )
}

pub(crate) fn pe_up_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent::new(
        PointerId(0),
        kind,
        PointerEventKind::Up(PointerButton::Primary),
        pos,
        1.0,
        mods,
    )
}

pub(crate) fn map_key(key: winit::keyboard::PhysicalKey) -> repose_core::input::Key {
    use repose_core::input::Key;
    use winit::keyboard::{KeyCode, PhysicalKey};

    match key {
        PhysicalKey::Code(KeyCode::Enter) => Key::Enter,
        PhysicalKey::Code(KeyCode::Tab) => Key::Tab,
        PhysicalKey::Code(KeyCode::Backspace) => Key::Backspace,
        PhysicalKey::Code(KeyCode::Delete) => Key::Delete,
        PhysicalKey::Code(KeyCode::Escape) => Key::Escape,
        PhysicalKey::Code(KeyCode::ArrowLeft) => Key::ArrowLeft,
        PhysicalKey::Code(KeyCode::ArrowRight) => Key::ArrowRight,
        PhysicalKey::Code(KeyCode::ArrowUp) => Key::ArrowUp,
        PhysicalKey::Code(KeyCode::ArrowDown) => Key::ArrowDown,
        PhysicalKey::Code(KeyCode::Home) => Key::Home,
        PhysicalKey::Code(KeyCode::End) => Key::End,
        PhysicalKey::Code(KeyCode::PageUp) => Key::PageUp,
        PhysicalKey::Code(KeyCode::PageDown) => Key::PageDown,
        PhysicalKey::Code(KeyCode::Space) => Key::Space,
        PhysicalKey::Code(KeyCode::KeyA) => Key::Character('a'),
        PhysicalKey::Code(KeyCode::KeyB) => Key::Character('b'),
        PhysicalKey::Code(KeyCode::KeyC) => Key::Character('c'),
        PhysicalKey::Code(KeyCode::KeyD) => Key::Character('d'),
        PhysicalKey::Code(KeyCode::KeyE) => Key::Character('e'),
        PhysicalKey::Code(KeyCode::KeyF) => Key::Character('f'),
        PhysicalKey::Code(KeyCode::KeyG) => Key::Character('g'),
        PhysicalKey::Code(KeyCode::KeyH) => Key::Character('h'),
        PhysicalKey::Code(KeyCode::KeyI) => Key::Character('i'),
        PhysicalKey::Code(KeyCode::KeyJ) => Key::Character('j'),
        PhysicalKey::Code(KeyCode::KeyK) => Key::Character('k'),
        PhysicalKey::Code(KeyCode::KeyL) => Key::Character('l'),
        PhysicalKey::Code(KeyCode::KeyM) => Key::Character('m'),
        PhysicalKey::Code(KeyCode::KeyN) => Key::Character('n'),
        PhysicalKey::Code(KeyCode::KeyO) => Key::Character('o'),
        PhysicalKey::Code(KeyCode::KeyP) => Key::Character('p'),
        PhysicalKey::Code(KeyCode::KeyQ) => Key::Character('q'),
        PhysicalKey::Code(KeyCode::KeyR) => Key::Character('r'),
        PhysicalKey::Code(KeyCode::KeyS) => Key::Character('s'),
        PhysicalKey::Code(KeyCode::KeyT) => Key::Character('t'),
        PhysicalKey::Code(KeyCode::KeyU) => Key::Character('u'),
        PhysicalKey::Code(KeyCode::KeyV) => Key::Character('v'),
        PhysicalKey::Code(KeyCode::KeyW) => Key::Character('w'),
        PhysicalKey::Code(KeyCode::KeyX) => Key::Character('x'),
        PhysicalKey::Code(KeyCode::KeyY) => Key::Character('y'),
        PhysicalKey::Code(KeyCode::KeyZ) => Key::Character('z'),
        PhysicalKey::Code(KeyCode::Digit0) => Key::Character('0'),
        PhysicalKey::Code(KeyCode::Digit1) => Key::Character('1'),
        PhysicalKey::Code(KeyCode::Digit2) => Key::Character('2'),
        PhysicalKey::Code(KeyCode::Digit3) => Key::Character('3'),
        PhysicalKey::Code(KeyCode::Digit4) => Key::Character('4'),
        PhysicalKey::Code(KeyCode::Digit5) => Key::Character('5'),
        PhysicalKey::Code(KeyCode::Digit6) => Key::Character('6'),
        PhysicalKey::Code(KeyCode::Digit7) => Key::Character('7'),
        PhysicalKey::Code(KeyCode::Digit8) => Key::Character('8'),
        PhysicalKey::Code(KeyCode::Digit9) => Key::Character('9'),
        PhysicalKey::Code(KeyCode::F1) => Key::F(1),
        PhysicalKey::Code(KeyCode::F2) => Key::F(2),
        PhysicalKey::Code(KeyCode::F3) => Key::F(3),
        PhysicalKey::Code(KeyCode::F4) => Key::F(4),
        PhysicalKey::Code(KeyCode::F5) => Key::F(5),
        PhysicalKey::Code(KeyCode::F6) => Key::F(6),
        PhysicalKey::Code(KeyCode::F7) => Key::F(7),
        PhysicalKey::Code(KeyCode::F8) => Key::F(8),
        PhysicalKey::Code(KeyCode::F9) => Key::F(9),
        PhysicalKey::Code(KeyCode::F10) => Key::F(10),
        PhysicalKey::Code(KeyCode::F11) => Key::F(11),
        PhysicalKey::Code(KeyCode::F12) => Key::F(12),
        _ => Key::Unknown,
    }
}

pub(crate) fn tf_ensure_caret_visible(state: &mut TextFieldState, is_multiline: bool) {
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let wrap_width = state.inner_width;

    if is_multiline {
        let (cx, cy, _) = caret_xy_for_byte(&state.text, font_px, wrap_width, state.caret_index());
        let iw = state.inner_width;
        let ih = state.inner_height;
        state.ensure_caret_visible_xy(cx, cy, iw, ih, dp_to_px(2.0));
    } else {
        let caret_idx = state.caret_index();
        let (display, caret_display_off) = if let Some(vt) = &state.visual_transformation {
            let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
            let tfmd = vt.filter(&annotated);
            let off = repose_core::original_offset_to_display(&state.text, tfmd.text.as_str(), caret_idx);
            (tfmd.text.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = measure_text(&display, font_px, None, 400, 0);
        let caret_x_px = m.positions.get(caret_display_off).copied().unwrap_or(0.0);
        state.ensure_caret_visible(caret_x_px, wrap_width, dp_to_px(2.0));
    }
}

/// Place caret in textfield at pointer position and begin drag selection.
/// Handles both single-line and multiline textfields.
/// `pos_px`: absolute pointer position in pixels
/// `scale`: display scale factor
/// `shift`: whether shift key is held (extends selection)
pub(crate) fn tf_place_caret_at_pointer(
    state: &mut TextFieldState,
    hit_rect: Rect,
    is_multiline: bool,
    pos_px: (f32, f32),
    scale: f32,
    shift: bool,
) {
    let padding_px = TF_PADDING_X_DP * scale;
    let inner_x_px = hit_rect.x + padding_px;
    let inner_y_px = hit_rect.y + 8.0 * scale;
    let content_x_px = (pos_px.0 - inner_x_px + state.scroll_offset).max(0.0);
    let content_y_px = (pos_px.1 - inner_y_px + state.scroll_offset_y).max(0.0);
    let font_px = dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;

    let idx = if is_multiline {
        index_for_xy_bytes_vt(
            state,
            font_px,
            hit_rect.w - 2.0 * padding_px,
            content_x_px,
            content_y_px,
        )
    } else {
        index_for_x_bytes_vt(state, font_px, content_x_px)
    };
    state.begin_drag(idx, shift);

    // Ensure caret visible
    let caret_idx = state.caret_index();
    let wrap_w = hit_rect.w - 2.0 * padding_px;
    if is_multiline {
        let (cx, cy, _) = caret_xy_for_byte(&state.text, font_px, wrap_w, caret_idx);
        let iw = state.inner_width;
        let ih = state.inner_height;
        state.ensure_caret_visible_xy(cx, cy, iw, ih, 2.0 * scale);
    } else {
        let (display, caret_display_off) = if let Some(vt) = &state.visual_transformation {
            let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
            let tfmd = vt.filter(&annotated);
            let off = repose_core::original_offset_to_display(&state.text, tfmd.text.as_str(), caret_idx);
            (tfmd.text.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = measure_text(&display, font_px, None, 400, 0);
        let cx = m.positions.get(caret_display_off).copied().unwrap_or(0.0);
        state.ensure_caret_visible(cx, wrap_w, 2.0 * scale);
    }
}

/// Dispatch wheel/touch-scroll to scroll consumers under `pos`, propagating
/// leftovers to parent hit regions.
///
/// Returns `(any_consumed, updated_capture)`.  Feed the new capture back on
/// subsequent calls during the same touch gesture.
pub(crate) fn dispatch_scroll(
    frame: &Frame,
    pos: Vec2,
    mut delta: Vec2,
    scroll_capture: Option<u64>,
) -> (bool, Option<u64>) {
    let mut new_capture = scroll_capture;

    // If a scrollable has captured the gesture, try it first (position-independent).
    if let Some(cid) = scroll_capture {
        if let Some(cb) = frame
            .hit_regions
            .iter()
            .find(|h| h.id == cid)
            .and_then(|h| h.on_scroll.as_ref())
        {
            let before = delta;
            let leftover = cb(before);
            let consumed_x = (before.x - leftover.x).abs() > 0.001;
            let consumed_y = (before.y - leftover.y).abs() > 0.001;
            if consumed_x || consumed_y {
                delta = leftover;
                if delta.x.abs() <= 0.001 && delta.y.abs() <= 0.001 {
                    return (true, Some(cid));
                }
            } else {
                new_capture = None; // captured region exhausted - fall through
            }
        }
    }

    // Normal position-based dispatch for remaining/uncaptured delta.
    let mut any_consumed = false;
    for hit in frame
        .hit_regions
        .iter()
        .rev()
        .filter(|h| h.rect.contains(pos))
    {
        if let Some(cb) = &hit.on_scroll {
            let before = delta;
            let leftover = cb(before);
            let consumed_x = (before.x - leftover.x).abs() > 0.001;
            let consumed_y = (before.y - leftover.y).abs() > 0.001;
            if consumed_x || consumed_y {
                any_consumed = true;
                if new_capture.is_none() {
                    new_capture = Some(hit.id);
                }
            }
            delta = leftover;
            if delta.x.abs() <= 0.001 && delta.y.abs() <= 0.001 {
                break;
            }
        }
    }
    (any_consumed, new_capture)
}

#[macro_export]
macro_rules! handle_text_undo_redo {
    ($app:expr, $key_event:expr) => {{
        let mut __handled = false;
        if $key_event.state == ElementState::Pressed && !$key_event.repeat && $app.modifiers.command
        {
            match $key_event.physical_key {
                PhysicalKey::Code(KeyCode::KeyZ) if $app.modifiers.shift => {
                    if let Some(fid) = $app.sched.focused {
                        let key = $app.tf_key_of(fid);
                        if let Some(state_rc) = $app.textfield_states.get(&key) {
                            let mut st = state_rc.borrow_mut();
                            if st.can_redo() {
                                st.redo();
                                $app.notify_text_change(fid, st.text.clone());
                                __handled = true;
                            }
                        }
                    }
                }
                PhysicalKey::Code(KeyCode::KeyZ) => {
                    if let Some(fid) = $app.sched.focused {
                        let key = $app.tf_key_of(fid);
                        if let Some(state_rc) = $app.textfield_states.get(&key) {
                            let mut st = state_rc.borrow_mut();
                            if st.can_undo() {
                                st.undo();
                                $app.notify_text_change(fid, st.text.clone());
                                __handled = true;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        __handled
    }};
}

pub(crate) fn process_render_commands(
    backend: &mut repose_render_wgpu::WgpuBackend,
    cmds: Vec<RenderCommand>,
) {
    for cmd in cmds {
        match cmd {
            RenderCommand::SetImageEncoded {
                handle,
                bytes,
                srgb,
            } => {
                let _ = backend.set_image_from_bytes(handle, &bytes, srgb);
            }
            RenderCommand::SetImageRgba8 {
                handle,
                w,
                h,
                rgba,
                srgb,
            } => {
                let _ = backend.set_image_rgba8(handle, w, h, &rgba, srgb);
            }
            RenderCommand::SetImageNv12 {
                handle,
                w,
                h,
                y,
                uv,
                full_range,
            } => {
                let _ = backend.set_image_nv12(handle, w, h, &y, &uv, full_range);
            }
            RenderCommand::RemoveImage { handle } => {
                backend.remove_image(handle);
            }
        }
    }
}
