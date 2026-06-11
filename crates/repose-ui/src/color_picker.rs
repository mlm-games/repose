#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;

use crate::{Box, Column, Row, Stack, Text, TextStyle, ViewExt};

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = h % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let h = if d == 0.0 {
        0.0
    } else if mx == r {
        60.0 * ((g - b) / d % 6.0)
    } else if mx == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if mx == 0.0 { 0.0 } else { d / mx };
    (h, s, mx)
}

fn color_from_hsv(h: f32, s: f32, v: f32) -> Color {
    let (r, g, b) = hsv_to_rgb(h, s, v);
    Color::from_rgb(r, g, b)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)) as u8
}

fn lerp_color(c0: Color, c1: Color, t: f32) -> Color {
    Color::from_rgb(
        lerp_u8(c0.0, c1.0, t),
        lerp_u8(c0.1, c1.1, t),
        lerp_u8(c0.2, c1.2, t),
    )
}

fn spacer(h: f32) -> View {
    Box(Modifier::new().height(h))
}

static CP_COUNTER: AtomicU64 = AtomicU64::new(0);

type Painter = Rc<dyn Fn(&mut Scene, repose_core::Rect)>;

fn gradient_painter(stops: Vec<(f32, Color)>) -> Painter {
    Rc::new(move |scene: &mut Scene, rect: repose_core::Rect| {
        if stops.len() < 2 {
            return;
        }
        let segments = 80usize;
        let seg_w = rect.w / segments as f32;
        for i in 0..segments {
            let t = i as f32 / segments as f32;
            let mut idx = 0;
            for j in 0..stops.len() - 1 {
                if t >= stops[j].0 && t <= stops[j + 1].0 {
                    idx = j;
                    break;
                }
            }
            if idx >= stops.len() - 1 {
                idx = stops.len() - 2;
            }
            let c0 = &stops[idx];
            let c1 = &stops[idx + 1];
            let local_t = if (c1.0 - c0.0).abs() < 1e-6 {
                0.0
            } else {
                (t - c0.0) / (c1.0 - c0.0)
            };
            let col = lerp_color(c0.1, c1.1, local_t);
            scene.nodes.push(SceneNode::Rect {
                rect: repose_core::Rect {
                    x: rect.x + i as f32 * seg_w,
                    y: rect.y,
                    w: seg_w + 1.0,
                    h: rect.h,
                },
                brush: Brush::Solid(col),
                radius: 0.0,
            });
        }
    })
}

/// A compact HSV color picker with hue, saturation, and value sliders.
pub fn ColorPicker(
    color: Color,
    on_change: impl Fn(Color) + 'static,
) -> View {
    let id = CP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let (hue, sat, val) = rgb_to_hsv(color.0, color.1, color.2);

    let drag_active = remember_state_with_key(format!("cp_drag_{}", id), || false);
    let drag_target = remember_state_with_key(format!("cp_tgt_{}", id), || 0u8);
    let drag_start_x = remember_state_with_key(format!("cp_dsx_{}", id), || 0.0f32);
    let drag_start_comp = remember_state_with_key(format!("cp_dsc_{}", id), || 0.0f32);

    let oc = Rc::new(on_change);

    let slider_w = 200.0f32;
    let slider_h = 20.0f32;
    let swatch_size = 40.0f32;
    let thumb_w = 4.0f32;

    let th = locals::theme();

    // Hue gradient stops (rainbow)
    let hue_stops = vec![
        (0.0, Color::from_rgb(255, 0, 0)),
        (1.0 / 6.0, Color::from_rgb(255, 255, 0)),
        (2.0 / 6.0, Color::from_rgb(0, 255, 0)),
        (3.0 / 6.0, Color::from_rgb(0, 255, 255)),
        (4.0 / 6.0, Color::from_rgb(0, 0, 255)),
        (5.0 / 6.0, Color::from_rgb(255, 0, 255)),
        (1.0, Color::from_rgb(255, 0, 0)),
    ];
    let hue_painter = gradient_painter(hue_stops);

    // Saturation gradient: gray → full hue
    let base_hue = color_from_hsv(hue, 1.0, 1.0);
    let gray = Color::from_rgb(
        lerp_u8(base_hue.0, 128, 0.5),
        lerp_u8(base_hue.1, 128, 0.5),
        lerp_u8(base_hue.2, 128, 0.5),
    );
    let sat_stops = vec![(0.0, gray), (1.0, base_hue)];
    let sat_painter = gradient_painter(sat_stops);

    // Value gradient: black → color at current S and V=1
    let col_at_sat = color_from_hsv(hue, sat, 1.0);
    let val_stops = vec![(0.0, Color::BLACK), (1.0, col_at_sat)];
    let val_painter = gradient_painter(val_stops);

    let hue_frac = hue / 360.0;
    let sat_frac = sat;
    let val_frac = val;

    // Share drag handlers across all three sliders
    let make_drag_start = {
        let da = drag_active.clone();
        let dt = drag_target.clone();
        let dsx = drag_start_x.clone();
        let dsc = drag_start_comp.clone();
        move |target: u8, comp: f32| {
            let da = da.clone();
            let dt = dt.clone();
            let dsx = dsx.clone();
            let dsc = dsc.clone();
            move |pe: PointerEvent| {
                *da.borrow_mut() = true;
                *dt.borrow_mut() = target;
                *dsx.borrow_mut() = pe.position.x;
                *dsc.borrow_mut() = comp;
            }
        }
    };

    let make_drag_move: Rc<dyn Fn(PointerEvent)> = {
        let da = drag_active.clone();
        let dt = drag_target.clone();
        let dsx = drag_start_x.clone();
        let dsc = drag_start_comp.clone();
        let oc = oc.clone();
        Rc::new(move |pe: PointerEvent| {
            if !*da.borrow() {
                return;
            }
            let target = *dt.borrow();
            let start_x = *dsx.borrow();
            let start_comp = *dsc.borrow();
            let dx = pe.position.x - start_x;
            let new_comp = (start_comp + dx * 0.005).clamp(0.0, 1.0);
            let (mut h, mut s, mut v) = (hue, sat, val);
            match target {
                0 => h = new_comp * 360.0,
                1 => s = new_comp,
                2 => v = new_comp,
                _ => {}
            }
            (oc)(color_from_hsv(h, s, v));
        })
    };

    let make_drag_end: Rc<dyn Fn(PointerEvent)> = {
        let da = drag_active.clone();
        Rc::new(move |_pe: PointerEvent| {
            *da.borrow_mut() = false;
        })
    };

    let header = Row(Modifier::new().align_items(AlignItems::Center)).child((
        Box(Modifier::new().width(swatch_size).height(swatch_size).background(color).border(1.0, th.outline, 4.0).clip_rounded(4.0)),
        Box(Modifier::new().width(8.0).height(1.0)),
        Text(format!("#{:02X}{:02X}{:02X}", color.0, color.1, color.2)).size(12.0).color(th.on_surface),
    ));

    let hue_slider = Stack(Modifier::new().width(slider_w).height(slider_h)).child((
        Box(Modifier::new().fill_max_size().painter(move |s: &mut Scene, r: repose_core::Rect| (hue_painter)(s, r))).child(Box(Modifier::new())),
        Box(Modifier::new().absolute().offset(Some(hue_frac * slider_w - thumb_w * 0.5), None, None, None).width(thumb_w).height(slider_h).background(Color::WHITE).border(1.0, th.outline, 2.0)),
    ))
    .modifier(
        Modifier::new()
            .on_pointer_down(make_drag_start(0, hue_frac))
            .on_pointer_move({
                let f = make_drag_move.clone();
                move |pe| f(pe)
            })
            .on_pointer_up({
                let f = make_drag_end.clone();
                move |pe| f(pe)
            }),
    );

    let sat_slider = Stack(Modifier::new().width(slider_w).height(slider_h)).child((
        Box(Modifier::new().fill_max_size().painter(move |s: &mut Scene, r: repose_core::Rect| (sat_painter)(s, r))).child(Box(Modifier::new())),
        Box(Modifier::new().absolute().offset(Some(sat_frac * slider_w - thumb_w * 0.5), None, None, None).width(thumb_w).height(slider_h).background(Color::WHITE).border(1.0, th.outline, 2.0)),
    ))
    .modifier(
        Modifier::new()
            .on_pointer_down(make_drag_start(1, sat_frac))
            .on_pointer_move({
                let f = make_drag_move.clone();
                move |pe| f(pe)
            })
            .on_pointer_up({
                let f = make_drag_end.clone();
                move |pe| f(pe)
            }),
    );

    let val_slider = Stack(Modifier::new().width(slider_w).height(slider_h)).child((
        Box(Modifier::new().fill_max_size().painter(move |s: &mut Scene, r: repose_core::Rect| (val_painter)(s, r))).child(Box(Modifier::new())),
        Box(Modifier::new().absolute().offset(Some(val_frac * slider_w - thumb_w * 0.5), None, None, None).width(thumb_w).height(slider_h).background(Color::WHITE).border(1.0, th.outline, 2.0)),
    ))
    .modifier(
        Modifier::new()
            .on_pointer_down(make_drag_start(2, val_frac))
            .on_pointer_move({
                let f = make_drag_move.clone();
                move |pe| f(pe)
            })
            .on_pointer_up({
                let f = make_drag_end.clone();
                move |pe| f(pe)
            }),
    );

    Column(Modifier::new().width(240.0))
        .child(header)
        .child(spacer(8.0))
        .child(Text("Hue").size(11.0).color(th.on_surface_variant))
        .child(spacer(2.0))
        .child(hue_slider)
        .child(spacer(4.0))
        .child(Text("Saturation").size(11.0).color(th.on_surface_variant))
        .child(spacer(2.0))
        .child(sat_slider)
        .child(spacer(4.0))
        .child(Text("Value").size(11.0).color(th.on_surface_variant))
        .child(spacer(2.0))
        .child(val_slider)
}
