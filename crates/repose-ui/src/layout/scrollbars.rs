#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;

use crate::Interactions;



#[derive(Clone, Copy)]
pub(crate) enum ScrollbarAxis {
    V,
    H,
}

pub(crate) fn push_scrollbar(
    scene: &mut Scene,
    hits: &mut Vec<HitRegion>,
    interactions: &Interactions,
    vid: u64,
    vp: repose_core::Rect,
    content_len: f32,
    offset: f32,
    z: f32,
    axis: ScrollbarAxis,
    set_offset: Option<Rc<dyn Fn(f32)>>,
) {
    let vp_len = match axis {
        ScrollbarAxis::V => vp.h,
        ScrollbarAxis::H => vp.w,
    };
    if content_len <= vp_len + 0.5 {
        return;
    }

    let thick = dp_to_px(4.0);
    let main_inset = dp_to_px(2.0);

    let (track_x, track_y, track_main, track_cross) = match axis {
        ScrollbarAxis::V => (
            vp.x + vp.w - thick,
            vp.y + main_inset,
            (vp.h - 2.0 * main_inset).max(0.0),
            thick,
        ),
        ScrollbarAxis::H => (
            vp.x + main_inset,
            vp.y + vp.h - thick,
            (vp.w - 2.0 * main_inset).max(0.0),
            thick,
        ),
    };
    if track_main <= 0.5 {
        return;
    }

    let ratio = (vp_len / content_len).clamp(0.0, 1.0);
    let thumb_len = (track_main * ratio).max(dp_to_px(24.0)).min(track_main);
    let tpos = (offset / (content_len - vp_len).max(1.0)).clamp(0.0, 1.0);
    let thumb_offset = tpos * (track_main - thumb_len);

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

    scene.nodes.push(SceneNode::Rect {
        rect: track_rect,
        brush: Brush::Solid(locals::theme().scrollbar_track),
        radius: [thick * 0.5; 4],
    });
    scene.nodes.push(SceneNode::Rect {
        rect: thumb_rect,
        brush: Brush::Solid(locals::theme().scrollbar_thumb),
        radius: [thick * 0.5; 4],
    });

    if let Some(s) = set_offset {
        let tid = match axis {
            ScrollbarAxis::V => vid ^ 0x8000_0001,
            ScrollbarAxis::H => vid ^ 0x8000_0002,
        };
        let track_start = match axis {
            ScrollbarAxis::V => track_y,
            ScrollbarAxis::H => track_x,
        };
        let max_scroll = (content_len - vp_len).max(1.0);

        let map = Rc::new(move |pos: f32| -> f32 {
            let max_p = (track_main - thumb_len).max(0.0);
            let p = ((pos - track_start) - thumb_len * 0.5).clamp(0.0, max_p);
            (if max_p > 0.0 { p / max_p } else { 0.0 }) * max_scroll
        });

        let extract = match axis {
            ScrollbarAxis::V => (|pe: &PointerEvent| pe.position.y) as fn(&PointerEvent) -> f32,
            ScrollbarAxis::H => (|pe: &PointerEvent| pe.position.x) as fn(&PointerEvent) -> f32,
        };

        let on_pd = {
            let s = s.clone();
            let m = map.clone();
            Rc::new(move |pe: PointerEvent| s(m(extract(&pe))))
        };
        let on_pm = if interactions.pressed.contains(&tid) {
            let s = s.clone();
            let m = map.clone();
            Some(Rc::new(move |pe: PointerEvent| s(m(extract(&pe)))) as Rc<dyn Fn(PointerEvent)>)
        } else {
            None
        };
        hits.push(HitRegion {
            id: tid,
            rect: thumb_rect,
            z_index: z + 1000.0,
            on_pointer_down: Some(on_pd),
            on_pointer_move: on_pm,
            on_pointer_up: Some(Rc::new(|_| {})),
            ..Default::default()
        });
    }
}
