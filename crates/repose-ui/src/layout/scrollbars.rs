#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::*;

use crate::Interactions;
use crate::hit_testing::{HitContext, register_hit};

#[derive(Clone, Copy)]
pub(crate) enum ScrollbarAxis {
    V,
    H,
}

/// Paint-time inputs for one scrollbar. Bundled so the gesture, style,
/// direction, and fade knobs do not keep stretching the argument list.
pub(crate) struct ScrollbarParams<'a> {
    pub scene: &'a mut Scene,
    pub hits: &'a mut Vec<HitRegion>,
    pub interactions: &'a Interactions,
    pub vid: u64,
    pub vp: repose_core::Rect,
    pub content_len: f32,
    pub offset: f32,
    pub z: f32,
    pub axis: ScrollbarAxis,
    pub style: &'a ScrollbarStyle,
    pub alpha: f32,
    pub set_offset: Option<Rc<dyn Fn(f32)>>,
    pub get_offset: Option<Rc<dyn Fn() -> f32>>,
    pub reversed: bool,
    pub hit_context: &'a HitContext,
}

pub(crate) fn push_scrollbar(p: ScrollbarParams<'_>) {
    let ScrollbarParams {
        scene,
        hits,
        interactions,
        vid,
        vp,
        content_len,
        offset,
        z,
        axis,
        style,
        alpha,
        set_offset,
        get_offset,
        reversed,
        hit_context,
    } = p;
    let vp_len = match axis {
        ScrollbarAxis::V => vp.h,
        ScrollbarAxis::H => vp.w,
    };
    if content_len <= vp_len + 0.5 {
        return;
    }

    // M3 draws the vertical bar at the end edge respecting layout direction;
    // only the horizontal thumb position mirrors in RTL, never the vertical
    // one. `reversed` (Compose `reverseLayout`) mirrors the mapping itself.
    let rtl = locals::text_direction() == locals::TextDirection::Rtl;
    let mirror = reversed ^ (matches!(axis, ScrollbarAxis::H) && rtl);

    let thick = style.thickness.to_px().0.max(0.5);
    let main_inset = style.track_inset.to_px().0.max(0.0);
    let cross_inset = style.track_cross_inset.to_px().0.max(0.0);

    let (track_x, track_y, track_main, track_cross) = match axis {
        ScrollbarAxis::V => (
            if rtl {
                vp.x + cross_inset
            } else {
                vp.x + vp.w - thick - cross_inset
            },
            vp.y + main_inset,
            (vp.h - 2.0 * main_inset).max(0.0),
            thick,
        ),
        ScrollbarAxis::H => (
            vp.x + main_inset,
            vp.y + vp.h - thick - cross_inset,
            (vp.w - 2.0 * main_inset).max(0.0),
            thick,
        ),
    };
    if track_main <= 0.5 {
        return;
    }

    let min_thumb = style.min_thumb_length.to_px().0.max(0.5);

    let ratio = (vp_len / content_len).clamp(0.0, 1.0);
    let max_frac = style.thumb_max_length_fraction.clamp(0.0, 1.0);
    // An explicit fixed length wins over the minimum (it is a deliberate
    // override); the computed length honors the M3 min/max window.
    let thumb_len = match style.fixed_thumb_length.map(|length| length.to_px().0) {
        Some(fixed) => fixed.clamp(0.5, track_main),
        None => {
            // M3 draws nothing when the track cannot fit the minimum thumb.
            if track_main < min_thumb {
                return;
            }
            (track_main * ratio).clamp(min_thumb, (track_main * max_frac).max(min_thumb))
        }
    };
    let max_p = (track_main - thumb_len).max(0.0);
    let tpos = (offset / (content_len - vp_len).max(1.0)).clamp(0.0, 1.0);
    let raw_offset = tpos * max_p;
    let thumb_offset = if mirror { max_p - raw_offset } else { raw_offset };

    let (track_rect, thumb_rect) = match axis {
        ScrollbarAxis::V => (
            repose_core::Rect {
                x: track_x,
                y: track_y,
                w: track_cross,
                h: track_main,
            },
            repose_core::Rect {
                x: track_x,
                y: track_y + thumb_offset,
                w: track_cross,
                h: thumb_len,
            },
        ),
        ScrollbarAxis::H => (
            repose_core::Rect {
                x: track_x,
                y: track_y,
                w: track_main,
                h: track_cross,
            },
            repose_core::Rect {
                x: track_x + thumb_offset,
                y: track_y,
                w: thumb_len,
                h: track_cross,
            },
        ),
    };

    let tid = match axis {
        ScrollbarAxis::V => vid ^ 0x8000_0001,
        ScrollbarAxis::H => vid ^ 0x8000_0002,
    };
    let track_id = match axis {
        ScrollbarAxis::V => vid ^ 0x8000_0003,
        ScrollbarAxis::H => vid ^ 0x8000_0004,
    };
    let engaged = interactions.hover == Some(tid)
        || interactions.hover == Some(track_id)
        || interactions.pressed.contains(&tid)
        || interactions.pressed.contains(&track_id);
    let fade_key = vid
        ^ match axis {
            ScrollbarAxis::V => crate::scrollbar_fade::FADE_KEY_V,
            ScrollbarAxis::H => crate::scrollbar_fade::FADE_KEY_H,
        };
    let alpha =
        alpha * crate::scrollbar_fade::scrollbar_alpha(fade_key, offset, engaged, style);
    let visual_state = ControlVisualState {
        alpha,
        enabled: true,
        hovered: interactions.hover == Some(tid),
        pressed: interactions.pressed.contains(&tid),
        dragged: false,
        focused: false,
    };
    let track_state = ControlVisualState {
        pressed: false,
        dragged: false,
        ..visual_state
    };
    // Fully faded bars still hit-test (hover reveals) but paint nothing.
    if alpha >= 0.01 {
        if let Some(visual) = style.track_visuals.resolve(track_state) {
            visual.paint(scene, track_rect, track_state);
        } else {
            let radius = style
                .radius
                .map(|radius| radius.to_px().0)
                .unwrap_or(thick * 0.5);
            scene.nodes.push(SceneNode::Rect {
                rect: track_rect,
                brush: Brush::Solid({
                    let color = locals::theme().scrollbar_track;
                    Color(
                        color.0,
                        color.1,
                        color.2,
                        (color.3 as f32 * alpha.clamp(0.0, 1.0)) as u8,
                    )
                }),
                radius: [Px(radius); 4],
            });
        }
        if let Some(visual) = style.thumb_visuals.resolve(visual_state) {
            visual.paint(scene, thumb_rect, visual_state);
        } else {
            let radius = style
                .radius
                .map(|radius| radius.to_px().0)
                .unwrap_or(thick * 0.5);
            scene.nodes.push(SceneNode::Rect {
                rect: thumb_rect,
                brush: Brush::Solid({
                    let color = locals::theme().scrollbar_thumb;
                    Color(
                        color.0,
                        color.1,
                        color.2,
                        (color.3 as f32 * alpha.clamp(0.0, 1.0)) as u8,
                    )
                }),
                radius: [Px(radius); 4],
            });
        }
    }


    if let Some(s) = set_offset {
        let track_start = match axis {
            ScrollbarAxis::V => track_y,
            ScrollbarAxis::H => track_x,
        };
        let max_scroll = (content_len - vp_len).max(1.0);
        let vertical = matches!(axis, ScrollbarAxis::V);
        // Track-space position for a content offset, mirrored for reverse
        // layout / RTL-horizontal (Compose `SliderAdapter.position`).
        let mirror_pos = move |raw: f32| {
            let raw = raw.clamp(0.0, max_p);
            if mirror { max_p - raw } else { raw }
        };
        let mirror_pos = Rc::new(mirror_pos);

        // Geometry is frozen at paint time, but the content offset moves under
        // our own hands, so every drag delta and every repeat page re-reads it
        // live (Compose reads `adapter.scrollOffset` per delta). Without a
        // getter we fall back to the paint-time snapshot.
        let live_offset = {
            let get = get_offset.clone();
            Rc::new(move || get.as_ref().map(|f| f()).unwrap_or(offset))
        };
        let thumb_origin = {
            let mirror_pos = mirror_pos.clone();
            Rc::new(move |off: f32| {
                let t = if max_scroll > 0.0 {
                    (off / max_scroll).clamp(0.0, 1.0) * max_p
                } else {
                    0.0
                };
                track_start + mirror_pos(t)
            })
        };
        let axis_pos = {
            let context = hit_context.clone();
            Rc::new(move |pe: &PointerEvent| {
                let local = context.to_local(pe.position_in_window());
                if vertical { local.y } else { local.x }
            })
        };

        // Thumb drag (Compose `scrollbarDrag`): the press only grabs, it never
        // moves the thumb. Moves apply relative deltas against the live
        // offset; overshoot past the ends is banked (`unscrolled`) instead of
        // lost, so reversing out of an end doesn't jump.
        let drag = Rc::new(RefCell::new(ThumbDrag::default()));
        let on_thumb_down: Rc<dyn Fn(PointerEvent)> = {
            let drag = drag.clone();
            let axis_pos = axis_pos.clone();
            Rc::new(move |pe: PointerEvent| {
                let mut drag = drag.borrow_mut();
                drag.last = Some(axis_pos(&pe));
                drag.unscrolled = 0.0;
            })
        };
        // Installed unconditionally: moves dispatch to the press-time snapshot
        // for the whole gesture, so gating this on "already pressed at paint
        // time" would leave every drag without a move handler.
        let on_thumb_move: Rc<dyn Fn(PointerEvent)> = {
            let drag = drag.clone();
            let axis_pos = axis_pos.clone();
            let live_offset = live_offset.clone();
            let mirror_pos = mirror_pos.clone();
            let set = s.clone();
            Rc::new(move |pe: PointerEvent| {
                let pos = axis_pos(&pe);
                let mut drag = drag.borrow_mut();
                let Some(last) = drag.last else { return };
                let delta = pos - last;
                drag.last = Some(pos);
                if delta == 0.0 || max_p <= 0.0 {
                    return;
                }
                let scale = max_p / max_scroll;
                let current = mirror_pos(live_offset() * scale);
                let target = (current + delta + drag.unscrolled).clamp(0.0, max_p);
                let applied = target - current;
                let new_raw = if mirror { max_p - target } else { target };
                set(new_raw / scale);
                drag.unscrolled += delta - applied;
            })
        };
        let end_thumb: Rc<dyn Fn(PointerEvent)> = {
            let drag = drag.clone();
            Rc::new(move |pe: PointerEvent| {
                *drag.borrow_mut() = ThumbDrag::default();
                crate::scrollbar_repeat::cancel_track_repeat(
                    crate::scrollbar_repeat::pointer_key(pe.kind, pe.id),
                );
            })
        };
        let mut thumb_hit = HitRegion {
            id: tid,
            rect: thumb_rect,
            z_index: z + 1000.0,
            on_pointer_down: Some(on_thumb_down),
            on_pointer_move: Some(on_thumb_move),
            on_pointer_up: Some(end_thumb.clone()),
            on_pointer_cancel: Some(end_thumb),
            ..Default::default()
        };
        register_hit(&mut thumb_hit, hit_context, None);
        hits.push(thumb_hit);

        // Track press (Compose `scrollOnPressTrack` + `TrackPressScroller`):
        // a press outside the thumb pages one viewport toward the press
        // immediately, then auto-repeats until release. A press inside the
        // thumb bounds is the thumb's gesture, not the track's.
        let track_target = Rc::new(RefCell::new(None::<f32>));
        let page_toward: Rc<dyn Fn()> = {
            let set = s.clone();
            let live_offset = live_offset.clone();
            let thumb_origin = thumb_origin.clone();
            let track_target = track_target.clone();
            Rc::new(move || {
                let Some(target) = *track_target.borrow() else {
                    return;
                };
                let live = live_offset();
                let start = thumb_origin(live);
                let dir = if target < start {
                    -1.0
                } else if target > start + thumb_len {
                    1.0
                } else {
                    0.0
                };
                if dir != 0.0 {
                    // Track space runs opposite to content when mirrored.
                    let content_dir = if mirror { -dir } else { dir };
                    set(live + content_dir * vp_len);
                }
            })
        };
        let on_track_down: Rc<dyn Fn(PointerEvent)> = {
            let axis_pos = axis_pos.clone();
            let live_offset = live_offset.clone();
            let thumb_origin = thumb_origin.clone();
            let track_target = track_target.clone();
            let page_toward = page_toward.clone();
            Rc::new(move |pe: PointerEvent| {
                let pos = axis_pos(&pe);
                let start = thumb_origin(live_offset());
                if pos >= start && pos <= start + thumb_len {
                    return;
                }
                *track_target.borrow_mut() = Some(pos);
                page_toward();
                crate::scrollbar_repeat::begin_track_repeat(
                    crate::scrollbar_repeat::pointer_key(pe.kind, pe.id),
                    page_toward.clone(),
                );
            })
        };
        // Steering while held: the repeat pages toward the live point.
        let on_track_move: Rc<dyn Fn(PointerEvent)> = {
            let axis_pos = axis_pos.clone();
            let track_target = track_target.clone();
            Rc::new(move |pe: PointerEvent| {
                *track_target.borrow_mut() = Some(axis_pos(&pe));
            })
        };
        let end_track: Rc<dyn Fn(PointerEvent)> = {
            Rc::new(move |pe: PointerEvent| {
                crate::scrollbar_repeat::cancel_track_repeat(
                    crate::scrollbar_repeat::pointer_key(pe.kind, pe.id),
                );
            })
        };
        let mut track_hit = HitRegion {
            id: track_id,
            rect: track_rect,
            z_index: z + 999.0,
            on_pointer_down: Some(on_track_down),
            on_pointer_move: Some(on_track_move),
            on_pointer_up: Some(end_track.clone()),
            on_pointer_cancel: Some(end_track),
            ..Default::default()
        };
        register_hit(&mut track_hit, hit_context, None);
        hits.push(track_hit);
    }
}

/// Per-gesture thumb-drag state, shared by the press-time snapshot's down/move
/// closures for the duration of one gesture.
#[derive(Default)]
struct ThumbDrag {
    /// Press position along the scroll axis, for relative deltas.
    last: Option<f32>,
    /// Overshoot past the ends banked instead of lost (Compose
    /// `unscrolledDragDistance`).
    unscrolled: f32,
}
