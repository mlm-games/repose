//! Selectable text. `SelectableText` mirrors Compose's `SelectionContainer`
//! for a single `Text` view: drag to select a byte range, and a highlight
//! rect is painted on top of the glyphs. Single-line selection is
//! pixel-accurate; multi-line tracks the range but the highlight is drawn
//! per-line.

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_core::{Brush, PointerEvent, Rect, Scene, SceneNode, View};

use crate::Text;
use crate::textfield::{caret_xy_for_byte, index_for_xy_bytes};

pub fn SelectableText(
    text: impl Into<String>,
    font_size_dp: f32,
    on_selection_change: impl Fn(Option<(usize, usize)>) + 'static,
) -> View {
    let text: String = text.into();
    let text_for_handlers = text.clone();
    let text_for_paint = text.clone();

    let selection: Rc<RefCell<Option<(usize, usize)>>> =
        remember_with_key("sel:range", || RefCell::new(None));
    let anchor: Rc<RefCell<usize>> = remember_with_key("sel:anchor", || RefCell::new(0));
    let dragging: Rc<RefCell<bool>> = remember_with_key("sel:dragging", || RefCell::new(false));
    let last_rect: Rc<RefCell<Rect>> =
        remember_with_key("sel:last_rect", || RefCell::new(Rect::default()));

    let callback = Rc::new(on_selection_change);

    let cb_down = callback.clone();
    let on_down = {
        let text = text_for_handlers.clone();
        let selection = selection.clone();
        let anchor = anchor.clone();
        let dragging = dragging.clone();
        let last_rect = last_rect.clone();
        move |ev: PointerEvent| {
            let r = *last_rect.borrow();
            if r.w <= 0.0 || r.h <= 0.0 {
                return;
            }
            let font_px = dp_to_px(font_size_dp) * text_scale().0;
            let lx = (ev.position.x - r.x).max(0.0);
            let ly = (ev.position.y - r.y).max(0.0);
            let wrap_w = r.w.max(1.0);
            let byte = index_for_xy_bytes(&text, font_px, wrap_w, lx, ly);
            *anchor.borrow_mut() = byte;
            *selection.borrow_mut() = Some((byte, byte));
            *dragging.borrow_mut() = true;
            cb_down(Some((byte, byte)));
        }
    };

    let cb_move = callback.clone();
    let text_for_primary = text_for_handlers.clone();
    let on_move = {
        let text = text_for_handlers.clone();
        let selection = selection.clone();
        let anchor = anchor.clone();
        let dragging = dragging.clone();
        let last_rect = last_rect.clone();
        move |ev: PointerEvent| {
            if !*dragging.borrow() {
                return;
            }
            let r = *last_rect.borrow();
            if r.w <= 0.0 || r.h <= 0.0 {
                return;
            }
            let font_px = dp_to_px(font_size_dp) * text_scale().0;
            let lx = (ev.position.x - r.x).max(0.0);
            let ly = (ev.position.y - r.y).max(0.0);
            let wrap_w = r.w.max(1.0);
            let byte = index_for_xy_bytes(&text, font_px, wrap_w, lx, ly);
            let a = *anchor.borrow();
            let sel = Some((a.min(byte), a.max(byte)));
            *selection.borrow_mut() = sel;
            cb_move(sel);
            if let Some((s, e)) = sel
                && e > s
            {
                repose_core::clipboard::set_primary_selection(&text[s..e]);
            }
        }
    };

    let cb_up = callback.clone();
    let on_up = {
        let text = text_for_primary.clone();
        let selection = selection.clone();
        let dragging = dragging.clone();
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
            cb_up(sel);
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
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rect.x + sx,
                        y: rect.y + sy,
                        w: (rect.w - sx).max(2.0),
                        h: line_h,
                    },
                    brush: brush,
                    radius: [0.0; 4],
                });
                if eli > sli + 1 {
                    scene.nodes.push(SceneNode::Rect {
                        rect: Rect {
                            x: rect.x,
                            y: rect.y + (sli as f32 + 1.0) * line_h,
                            w: rect.w,
                            h: (eli as f32 - sli as f32 - 1.0) * line_h,
                        },
                        brush: brush,
                        radius: [0.0; 4],
                    });
                }
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

    let mut v = Text(text_for_paint);
    v.modifier = v
        .modifier
        .on_pointer_down(on_down)
        .on_pointer_move(on_move)
        .on_pointer_up(on_up)
        .painter(painter)
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
                _ => false,
            }
        });
    v
}
