use crate::*;
use repose_core::input::{PointerButton, PointerEvent, PointerEventKind, PointerId, PointerKind};
use repose_core::locals::dp_to_px;
use repose_ui::TextFieldState;
use repose_ui::textfield::{
    TF_FONT_DP, TF_PADDING_X_DP, caret_xy_for_byte, index_for_x_bytes, index_for_xy_bytes,
    measure_text,
};

/// Like `index_for_x_bytes` but applies visual transformation if active on the state.
/// The returned offset is in the original text's byte space.
pub(crate) fn index_for_x_bytes_vt(state: &TextFieldState, font_px: f32, x_px: f32) -> usize {
    if let Some(vt) = &state.visual_transformation {
        let tfmd = vt.filter(&state.text);
        let display_idx = index_for_x_bytes(&tfmd.text, font_px, x_px);
        (tfmd.offset_map)(display_idx)
    } else {
        index_for_x_bytes(&state.text, font_px, x_px)
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
        let tfmd = vt.filter(&state.text);
        let display_idx = index_for_xy_bytes(&tfmd.text, font_px, wrap_w, x_px, y_px);
        (tfmd.offset_map)(display_idx)
    } else {
        index_for_xy_bytes(&state.text, font_px, wrap_w, x_px, y_px)
    }
}

use std::cell::RefCell;
use std::rc::Rc;

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
    PointerEvent {
        id: PointerId(0),
        kind: PointerKind::Mouse,
        event,
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_touch(event: PointerEventKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind: PointerKind::Touch,
        event,
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_down_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind,
        event: PointerEventKind::Down(PointerButton::Primary),
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
}

pub(crate) fn pe_up_primary(kind: PointerKind, pos: Vec2, mods: Modifiers) -> PointerEvent {
    PointerEvent {
        id: PointerId(0),
        kind,
        event: PointerEventKind::Up(PointerButton::Primary),
        position: pos,
        pressure: 1.0,
        modifiers: mods,
    }
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
            let tfmd = vt.filter(&state.text);
            let off = repose_core::original_offset_to_display(&state.text, &tfmd.text, caret_idx);
            (tfmd.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = measure_text(&display, font_px, None);
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
            let tfmd = vt.filter(&state.text);
            let off = repose_core::original_offset_to_display(&state.text, &tfmd.text, caret_idx);
            (tfmd.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = measure_text(&display, font_px, None);
        let cx = m.positions.get(caret_display_off).copied().unwrap_or(0.0);
        state.ensure_caret_visible(cx, wrap_w, 2.0 * scale);
    }
}

pub(crate) fn touch_slop_px(scale: f32) -> f32 {
    6.0 * scale
}

/// Delegate focus movement to the core spatial focus algorithm.
/// Now lives in `repose_core::spatial_focus_next`.
pub(crate) fn focus_in_direction(
    chain: &[u64],
    hit_regions: &[HitRegion],
    current: Option<u64>,
    dir: FocusDirection,
) -> Option<u64> {
    repose_core::spatial_focus_next(chain, hit_regions, current, dir)
}

pub(crate) fn is_dnd_target(hit: &HitRegion) -> bool {
    hit.on_drop.is_some()
        || hit.on_drag_enter.is_some()
        || hit.on_drag_over.is_some()
        || hit.on_drag_leave.is_some()
}

pub(crate) fn dnd_target_id_at(frame: &Frame, pos: Vec2) -> Option<u64> {
    frame
        .hit_regions
        .iter()
        .rev()
        .filter(|h| h.rect.contains(pos))
        .find(|h| is_dnd_target(h))
        .map(|h| h.id)
}

/// Dispatch wheel/touch-scroll to the top-most scroll consumer under `pos`.
/// Returns `true` if something consumed the scroll.
pub(crate) fn dispatch_scroll(frame: &Frame, pos: Vec2, delta: Vec2) -> bool {
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
                return true;
            }
        }
    }
    false
}

/// Shared state for runner-provided "auto root scroll".
#[derive(Default)]
pub(crate) struct RootScrollState {
    pub viewport_h: f32,
    pub content_h: f32,
    pub offset_y: f32,
}

impl RootScrollState {
    #[inline]
    pub fn max_offset(&self) -> f32 {
        (self.content_h - self.viewport_h).max(0.0)
    }
}

/// Wrap any `child` view in a vertical scroll container backed by `RootScrollState`.
/// This is how the runner can provide "overflow-y: auto" semantics for the whole app.
pub(crate) fn wrap_root_scroll(child: View, st: Rc<RefCell<RootScrollState>>) -> View {
    let st_get = st.clone();
    let get_scroll_offset = Some(Rc::new(move || st_get.borrow().offset_y) as Rc<dyn Fn() -> f32>);

    let st_set = st.clone();
    let set_scroll_offset = Some(Rc::new(move |y: f32| {
        let mut s = st_set.borrow_mut();
        s.offset_y = y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_vp = st.clone();
    let set_viewport_height = Some(Rc::new(move |h: f32| {
        let mut s = st_vp.borrow_mut();
        s.viewport_h = h.max(0.0);
        s.offset_y = s.offset_y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_ch = st.clone();
    let set_content_height = Some(Rc::new(move |h: f32| {
        let mut s = st_ch.borrow_mut();
        s.content_h = h.max(0.0);
        s.offset_y = s.offset_y.clamp(0.0, s.max_offset());
    }) as Rc<dyn Fn(f32)>);

    let st_scroll = st.clone();
    let on_scroll = Some(Rc::new(move |delta: Vec2| -> Vec2 {
        let mut s = st_scroll.borrow_mut();
        let max_off = s.max_offset();
        if max_off <= 0.5 {
            return delta; // nothing to consume
        }

        let before = s.offset_y;
        let target = (s.offset_y - delta.y).clamp(0.0, max_off);
        s.offset_y = target;

        let consumed = before - target;
        Vec2 {
            x: delta.x,
            y: delta.y - consumed, // leftover
        }
    }) as Rc<dyn Fn(Vec2) -> Vec2>);

    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll,
            set_viewport_height,
            set_content_height,
            get_scroll_offset,
            set_scroll_offset,
            show_scrollbar: true,
        },
    )
    .modifier(Modifier::new().fill_max_size())
    .with_children(vec![child])
}
