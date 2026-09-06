//! Selectable text as a fluent modifier on `Text` views.
//!
//! Mirrors Compose-style selection for a single `Text`:
//! - Drag to select (byte range, multi-line highlight)
//! - Double-tap -> select word under pointer
//! - Triple-tap -> select all
//! - Shift+click / shift+drag -> extend from previous anchor
//! - Primary selection (middle-click paste) + Ctrl/Cmd+C via `Action::Copy`

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_core::{Brush, CursorIcon, PointerEvent, Rect, Scene, SceneNode, View};
use web_time::{Duration, Instant};

use crate::textfield::{caret_xy_for_byte, index_for_xy_bytes, word_range};
use crate::{Text, TextStyle};

const TRIPLE_TAP_MS: u64 = 500;
const TAP_SLOP_PX: f32 = 12.0;

/// Fluent selectable modifier for `Text` views.
///
/// Call after styling: `Text(...).size(16.0).selectable(|sel| ...)`.
pub trait SelectableTextExt {
    fn selectable(self, on_selection_change: impl Fn(Option<(usize, usize)>) + 'static) -> View;
}

impl SelectableTextExt for View {
    fn selectable(self, on_selection_change: impl Fn(Option<(usize, usize)>) + 'static) -> View {
        let (text, font_size_dp) = match &self.kind {
            ViewKind::Text {
                text, font_size, ..
            } => (text.clone(), *font_size),
            _ => return self,
        };
        make_selectable(self, text, font_size_dp, on_selection_change)
    }
}

/// Backward-compatible free function.
pub fn SelectableText(
    text: impl Into<String>,
    font_size_dp: f32,
    on_selection_change: impl Fn(Option<(usize, usize)>) + 'static,
) -> View {
    let text: String = text.into();
    let v = Text(text.clone()).size(font_size_dp);
    make_selectable(v, text, font_size_dp, on_selection_change)
}

fn make_selectable(
    mut v: View,
    text: String,
    font_size_dp: f32,
    on_selection_change: impl Fn(Option<(usize, usize)>) + 'static,
) -> View {
    let text_for_handlers = text.clone();
    let text_for_paint = text.clone();

    let selection: Rc<RefCell<Option<(usize, usize)>>> = remember(|| RefCell::new(None));
    let anchor: Rc<RefCell<usize>> = remember(|| RefCell::new(0));
    let dragging: Rc<RefCell<bool>> = remember(|| RefCell::new(false));
    let last_rect: Rc<RefCell<Rect>> = remember(|| RefCell::new(Rect::default()));

    // Tap counting for double/triple
    let last_tap_time: Rc<RefCell<Option<Instant>>> = remember(|| RefCell::new(None));
    let last_tap_pos: Rc<RefCell<Option<(f32, f32)>>> = remember(|| RefCell::new(None));
    let tap_count: Rc<RefCell<u8>> = remember(|| RefCell::new(0));

    let callback = Rc::new(on_selection_change);

    let set_sel = {
        let selection = selection.clone();
        let callback = callback.clone();
        move |s: Option<(usize, usize)>| {
            *selection.borrow_mut() = s;
            callback(s);
        }
    };

    let on_down = {
        let text = text_for_handlers.clone();
        let selection = selection.clone();
        let anchor = anchor.clone();
        let dragging = dragging.clone();
        let last_rect = last_rect.clone();
        let last_tap_time = last_tap_time.clone();
        let last_tap_pos = last_tap_pos.clone();
        let tap_count = tap_count.clone();
        let set_sel = set_sel.clone();
        move |ev: PointerEvent| {
            let r = *last_rect.borrow();
            if r.w <= 0.0 || r.h <= 0.0 {
                return;
            }
            let font_px = dp_to_px(font_size_dp) * text_scale().0;
            let lx = ev.position.x.max(0.0);
            let ly = ev.position.y.max(0.0);
            let wrap_w = r.w.max(1.0);
            let byte = index_for_xy_bytes(&text, font_px, wrap_w, lx, ly);

            // Tap counting
            let now = Instant::now();
            let pos = (ev.position.x, ev.position.y);
            let mut count = *tap_count.borrow();
            let mut is_multi = false;
            if let (Some(t), Some(p)) = (*last_tap_time.borrow(), *last_tap_pos.borrow()) {
                let dt = now.saturating_duration_since(t);
                let dist = ((pos.0 - p.0).powi(2) + (pos.1 - p.1).powi(2)).sqrt();
                // Taps within the triple-tap timeout keep
                // incrementing (double-tap is the fast path of the same window).
                if dt < Duration::from_millis(TRIPLE_TAP_MS) && dist < TAP_SLOP_PX {
                    count = count.saturating_add(1);
                    is_multi = true;
                } else {
                    count = 1;
                }
            } else {
                count = 1;
            }
            *tap_count.borrow_mut() = count;
            *last_tap_time.borrow_mut() = Some(now);
            *last_tap_pos.borrow_mut() = Some(pos);

            let shift = ev.modifiers.shift;

            if count >= 3 {
                // Triple-tap: select all
                let sel = Some((0, text.len()));
                *anchor.borrow_mut() = 0;
                set_sel(sel);
                *dragging.borrow_mut() = false;
                if !text.is_empty() {
                    repose_core::clipboard::set_primary_selection(&text);
                }
                let _ = is_multi;
                return;
            }

            if count == 2 {
                // Double-tap: select word
                let (s, e) = word_range(&text, byte);
                let sel = Some((s, e));
                *anchor.borrow_mut() = s;
                set_sel(sel);
                *dragging.borrow_mut() = true;
                if e > s {
                    repose_core::clipboard::set_primary_selection(&text[s..e]);
                }
                let _ = is_multi;
                return;
            }

            // Single tap / start drag
            if shift {
                let a = selection
                    .borrow()
                    .map(|(s, e)| {
                        let _ = (s, e);
                        *anchor.borrow()
                    })
                    .unwrap_or(*anchor.borrow());
                let sel = Some((a.min(byte), a.max(byte)));
                set_sel(sel);
            } else {
                *anchor.borrow_mut() = byte;
                set_sel(Some((byte, byte)));
            }
            *dragging.borrow_mut() = true;
            let _ = is_multi;
        }
    };

    let on_move = {
        let text = text_for_handlers.clone();
        let anchor = anchor.clone();
        let dragging = dragging.clone();
        let last_rect = last_rect.clone();
        let set_sel = set_sel.clone();
        move |ev: PointerEvent| {
            if !*dragging.borrow() {
                return;
            }
            let r = *last_rect.borrow();
            if r.w <= 0.0 || r.h <= 0.0 {
                return;
            }
            let font_px = dp_to_px(font_size_dp) * text_scale().0;
            let lx = ev.position.x.max(0.0);
            let ly = ev.position.y.max(0.0);
            let wrap_w = r.w.max(1.0);
            let byte = index_for_xy_bytes(&text, font_px, wrap_w, lx, ly);
            let a = *anchor.borrow();
            let sel = Some((a.min(byte), a.max(byte)));
            set_sel(sel);
            if let Some((s, e)) = sel
                && e > s
            {
                repose_core::clipboard::set_primary_selection(&text[s..e]);
            }
        }
    };

    let on_up = {
        let text = text_for_handlers.clone();
        let selection = selection.clone();
        let dragging = dragging.clone();
        let set_sel = set_sel.clone();
        move |_ev: PointerEvent| {
            *dragging.borrow_mut() = false;
            let sel = *selection.borrow();
            if let Some((a, b)) = sel {
                let s = a.min(b);
                let e = a.max(b);
                if e > s {
                    repose_core::clipboard::set_primary_selection(&text[s..e]);
                }
            }
            set_sel(sel);
        }
    };

    let painter = {
        let text = text_for_paint.clone();
        let selection = selection.clone();
        let last_rect = last_rect.clone();
        move |scene: &mut Scene, rect: Rect, _alpha: f32| {
            *last_rect.borrow_mut() = rect;

            let (s, e) = match *selection.borrow() {
                Some((a, b)) if a != b => {
                    if a < b {
                        (a, b)
                    } else {
                        (b, a)
                    }
                }
                _ => return,
            };
            if e == 0 || e <= s {
                return;
            }

            let font_px = dp_to_px(font_size_dp) * text_scale().0;
            let wrap_w = rect.w.max(1.0);
            let (sx, sy, sli) = caret_xy_for_byte(&text, font_px, wrap_w, s);
            let (ex, ey, eli) = caret_xy_for_byte(&text, font_px, wrap_w, e);
            let th = theme();
            let brush = Brush::Solid(th.primary.with_alpha(96));
            let line_h = font_px * 1.2;

            if sli == eli {
                let x = sx.min(ex);
                let w = (ex - sx).abs().max(2.0);
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x + x,
                        y: rect.y + sy,
                        w,
                        h: line_h,
                    },
                    brush,
                    radius: [0.0; 4],
                });
            } else {
                // First partial line
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x + sx,
                        y: rect.y + sy,
                        w: (rect.w - sx).max(2.0),
                        h: line_h,
                    },
                    brush,
                    radius: [0.0; 4],
                });
                // Full middle lines
                if eli > sli + 1 {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: rect.x,
                            y: rect.y + (sli as f32 + 1.0) * line_h,
                            w: rect.w,
                            h: (eli as f32 - sli as f32 - 1.0) * line_h,
                        },
                        brush,
                        radius: [0.0; 4],
                    });
                }
                // Last partial line
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x,
                        y: rect.y + ey,
                        w: ex.max(2.0),
                        h: line_h,
                    },
                    brush,
                    radius: [0.0; 4],
                });
            }
        }
    };

    v.modifier = v
        .modifier
        .on_pointer_down(on_down)
        .on_pointer_move(on_move)
        .on_pointer_up(on_up)
        .painter(painter)
        .cursor(CursorIcon::Text)
        .on_action({
            let selection = selection.clone();
            let text = text_for_handlers.clone();
            move |action| match action {
                repose_core::shortcuts::Action::Copy => {
                    let sel = *selection.borrow();
                    if let Some((a, b)) = sel {
                        let s = a.min(b);
                        let e = a.max(b);
                        if e > s {
                            repose_core::clipboard::copy_to_clipboard(&text[s..e]);
                            return true;
                        }
                    }
                    false
                }
                repose_core::shortcuts::Action::SelectAll => {
                    let len = text.len();
                    *selection.borrow_mut() = Some((0, len));
                    if len > 0 {
                        repose_core::clipboard::set_primary_selection(&text);
                    }
                    true
                }
                _ => false,
            }
        });

    v
}
