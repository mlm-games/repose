#![allow(non_snake_case)]

use std::rc::Rc;

use repose_core::*;

use crate::Interactions;
use crate::hit_testing::{HitContext, register_hit};

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
    style: &ScrollbarStyle,
    alpha: f32,
    set_offset: Option<Rc<dyn Fn(f32)>>,
    hit_context: &HitContext,
) {
    let vp_len = match axis {
        ScrollbarAxis::V => vp.h,
        ScrollbarAxis::H => vp.w,
    };
    if content_len <= vp_len + 0.5 {
        return;
    }

    let thick = style.thickness.to_px().0.max(0.5);
    let main_inset = style.track_inset.to_px().0.max(0.0);

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
    let thumb_len = style
        .fixed_thumb_length
        .map(|length| length.to_px().0)
        .unwrap_or_else(|| (track_main * ratio).max(style.min_thumb_length.to_px().0))
        .clamp(0.5, track_main);
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

    let tid = match axis {
        ScrollbarAxis::V => vid ^ 0x8000_0001,
        ScrollbarAxis::H => vid ^ 0x8000_0002,
    };
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

    if let Some(s) = set_offset {
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

        let on_pd = {
            let s = s.clone();
            let m = map.clone();
            let context = hit_context.clone();
            let vertical = matches!(axis, ScrollbarAxis::V);
            Rc::new(move |pe: PointerEvent| {
                let local = context.to_local(pe.position_in_window());
                s(m(if vertical { local.y } else { local.x }))
            })
        };
        let on_pm = if interactions.pressed.contains(&tid) {
            let s = s.clone();
            let m = map.clone();
            let context = hit_context.clone();
            let vertical = matches!(axis, ScrollbarAxis::V);
            Some(Rc::new(move |pe: PointerEvent| {
                let local = context.to_local(pe.position_in_window());
                s(m(if vertical { local.y } else { local.x }))
            }) as Rc<dyn Fn(PointerEvent)>)
        } else {
            None
        };
        let mut hit = HitRegion {
            id: tid,
            rect: thumb_rect,
            z_index: z + 1000.0,
            on_pointer_down: Some(on_pd),
            on_pointer_move: on_pm,
            on_pointer_up: Some(Rc::new(|_| {})),
            ..Default::default()
        };
        register_hit(&mut hit, hit_context, None);
        hits.push(hit);
    }
}
