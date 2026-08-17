#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ripple::{RippleConfig, ripple};
use repose_core::*;
use repose_ui::Box;

use super::*;

/// Configuration for [`Slider`] and [`RangeSlider`].
#[derive(Clone)]
pub struct SliderConfig {
    // Debug impl is manual because on_value_change_finished contains a closure
    pub modifier: Modifier,
    /// When false, renders disabled colors and does not respond to input.
    pub enabled: bool,
    pub active_track_color: Color,
    pub inactive_track_color: Color,
    pub thumb_color: Color,
    pub active_tick_color: Color,
    pub inactive_tick_color: Color,
    pub disabled_thumb_color: Color,
    pub disabled_active_track_color: Color,
    pub disabled_inactive_track_color: Color,
    pub disabled_active_tick_color: Color,
    pub disabled_inactive_tick_color: Color,
    pub state_colors: StateColors,
    pub on_value_change_finished: Option<Rc<dyn Fn()>>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl std::fmt::Debug for SliderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SliderConfig")
            .field("modifier", &self.modifier)
            .field("enabled", &self.enabled)
            .field("active_track_color", &self.active_track_color)
            .field("inactive_track_color", &self.inactive_track_color)
            .field("thumb_color", &self.thumb_color)
            .field("active_tick_color", &self.active_tick_color)
            .field("inactive_tick_color", &self.inactive_tick_color)
            .field("disabled_thumb_color", &self.disabled_thumb_color)
            .field(
                "disabled_active_track_color",
                &self.disabled_active_track_color,
            )
            .field(
                "disabled_inactive_track_color",
                &self.disabled_inactive_track_color,
            )
            .field(
                "disabled_active_tick_color",
                &self.disabled_active_tick_color,
            )
            .field(
                "disabled_inactive_tick_color",
                &self.disabled_inactive_tick_color,
            )
            .field("state_colors", &self.state_colors)
            .field(
                "on_value_change_finished",
                &self.on_value_change_finished.as_ref().map(|_| ".."),
            )
            .field(
                "interaction_source",
                &self.interaction_source.as_ref().map(|_| ".."),
            )
            .finish()
    }
}

impl Default for SliderConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            active_track_color: SliderDefaults::active_track_color(),
            inactive_track_color: SliderDefaults::inactive_track_color(),
            thumb_color: SliderDefaults::thumb_color(),
            active_tick_color: SliderDefaults::active_tick_color(),
            inactive_tick_color: SliderDefaults::inactive_tick_color(),
            disabled_thumb_color: SliderDefaults::disabled_thumb_color(),
            disabled_active_track_color: SliderDefaults::disabled_active_track_color(),
            disabled_inactive_track_color: SliderDefaults::disabled_inactive_track_color(),
            disabled_active_tick_color: SliderDefaults::disabled_active_tick_color(),
            disabled_inactive_tick_color: SliderDefaults::disabled_inactive_tick_color(),
            state_colors: SliderDefaults::state_colors_default(),
            on_value_change_finished: None,
            interaction_source: None,
        }
    }
}

static SLIDER_COUNTER: AtomicU64 = AtomicU64::new(0);

fn snap_step(v: f32, min: f32, max: f32, step: Option<f32>) -> f32 {
    let v = v.clamp(min, max);
    if let Some(s) = step.filter(|s| *s > 0.0) {
        let t = ((v - min) / s).round();
        (min + t * s).clamp(min, max)
    } else {
        v
    }
}

fn value_from_x(x: f32, rect: Rect, min: f32, max: f32, step: Option<f32>) -> f32 {
    let w = rect.w.max(1.0);
    let t = ((x - rect.x) / w).clamp(0.0, 1.0);
    let v = min + t * (max - min);
    snap_step(v, min, max, step)
}

pub fn Slider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
    config: SliderConfig,
) -> View {
    assert!(range.0 <= range.1, "Slider range start must be <= end");
    if let Some(s) = step {
        assert!(s > 0.0, "Slider step must be positive");
    }
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("ms_rect_{}", id), Rect::default);
    let drag_active = remember_mutable_with_key(format!("ms_da_{}", id), || false);
    let hovered = remember(|| Signal::new(false));
    let focused = remember(|| Signal::new(false));

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let hovered_sig = hovered.clone();
    let focused_sig = focused.clone();
    let sc = config.state_colors;

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t = ((value - min) / range_size).clamp(0.0, 1.0);

    let is_enabled = config.enabled;

    let act_trk = if !is_enabled {
        config.disabled_active_track_color
    } else {
        config.active_track_color
    };
    let inact_trk = if !is_enabled {
        config.disabled_inactive_track_color
    } else {
        config.inactive_track_color
    };
    let act_tick = if !is_enabled {
        config.disabled_active_tick_color
    } else {
        config.active_tick_color
    };
    let inact_tick = if !is_enabled {
        config.disabled_inactive_tick_color
    } else {
        config.inactive_tick_color
    };
    let thumb_col = if !is_enabled {
        config.disabled_thumb_color
    } else {
        config.thumb_color
    };

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    let sl_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    let mut host = Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .interaction_source(&sl_source)
        .indication(ripple(RippleConfig {
            color: Some(thumb_col),
            bounded: false,
            radius: Some(20.0),
            ..Default::default()
        }));
    if !is_enabled {
        host = host.enabled(false);
    }
    host = host.focusable(true).on_focus_changed({
        let f = focused.clone();
        move |focused| f.set(focused)
    });
    Box(host
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(SliderDefaults::TRACK_HEIGHT);
            let thumb_w = dp_to_px(SliderDefaults::THUMB_WIDTH);
            let thumb_h = dp_to_px(SliderDefaults::THUMB_HEIGHT);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = thumb_w * 0.5 + dp_to_px(ProgressIndicatorDefaults::SLIDER_THUMB_TRACK_GAP);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            let kx = if step.is_some() && !tick_frac.is_empty() {
                let is_first = (t - tick_frac[0]).abs() < 1e-6;
                let is_last = (t - tick_frac[tick_frac.len() - 1]).abs() < 1e-6;
                if is_first || is_last {
                    track_x + t * track_w
                } else {
                    track_x + (track_w - track_h) * t + corner
                }
            } else {
                track_x + t * track_w
            };

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            let inactive_x = track_x.max(kx + gap);
            let inactive_w = (track_x + track_w - inactive_x).max(0.0);
            if inactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: inactive_x,
                        y: cy - track_h * 0.5,
                        w: inactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(inact_trk)),
                    radius: [corner; 4],
                });
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(inact_tick)),
                });
            }
            let fill_w = (kx - gap - track_x).max(0.0);
            if fill_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: fill_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(act_trk)),
                    radius: [corner; 4],
                });
            }
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for (i, &tf) in tick_frac.iter().enumerate() {
                let tx = tick_start + tf * (tick_end - tick_start);
                // skip ticks that fall on the stop indicator (last)
                if i == tick_frac.len() - 1 {
                    continue;
                }
                if tx >= kx - gap && tx <= kx + gap {
                    continue;
                }
                let on_active = tx <= kx - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active { act_tick } else { inact_tick })),
                });
            }
            let da = *drag_active_p.get();
            let hv = hovered_sig.get();
            let fs = focused_sig.get();
            let tw = if da { thumb_w * 0.5 } else { thumb_w };
            scene.nodes.push(SceneNode::Rect {
                rect: Rect {
                    x: kx - tw * 0.5,
                    y: cy - thumb_h * 0.5,
                    w: tw,
                    h: thumb_h,
                },
                brush: Brush::Solid(mul_c(thumb_col)),
                radius: [tw * 0.5; 4],
            });

            // Compose: circular state layer centered on the thumb (not on handle geometry)
            let sc_target = if !is_enabled {
                Color::TRANSPARENT
            } else if da {
                sc.pressed
            } else if fs {
                sc.focused
            } else if hv {
                sc.hovered
            } else {
                sc.default
            };
            if sc_target.3 > 0 {
                let sl = dp_to_px(SliderDefaults::STATE_LAYER_SIZE);
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: kx - sl * 0.5,
                        y: cy - sl * 0.5,
                        w: sl,
                        h: sl,
                    },
                    brush: Brush::Solid(mul_c(sc_target)),
                });
            }
        })
        .on_pointer_enter({
            let h = hovered.clone();
            let en = is_enabled;
            move |_pe: PointerEvent| {
                if en {
                    h.set(true);
                }
            }
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| h.set(false)
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let en = is_enabled;
            move |pe: PointerEvent| {
                if !en {
                    return;
                }
                drag_active.set(true);
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position_in_window().x, r, min, max, step));
                pe.consume();
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let en = is_enabled;
            move |pe: PointerEvent| {
                if !en {
                    return;
                }
                if !*drag_active.get() {
                    return;
                }
                let r = *track_rect.borrow();
                (oc)(value_from_x(pe.position_in_window().x, r, min, max, step));
                pe.consume();
            }
        })
        .on_pointer_up({
            let drag_active = drag_active.clone();
            let on_finished = config.on_value_change_finished.clone();
            move |_pe: PointerEvent| {
                let was = *drag_active.get();
                drag_active.set(false);
                if was {
                    if let Some(ref cb) = on_finished {
                        (cb)();
                    }
                }
            }
        })
        .on_pointer_cancel({
            let drag_active = drag_active.clone();
            let on_finished = config.on_value_change_finished.clone();
            move |_pe: PointerEvent| {
                if *drag_active.get() {
                    drag_active.set(false);
                    if let Some(ref cb) = on_finished {
                        (cb)();
                    }
                }
            }
        })
        .on_scroll({
            let oc = oc.clone();
            let en = is_enabled;
            move |d: Vec2| -> Vec2 {
                if !en {
                    return d;
                }
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let new_val = snap_step(value + (dir as f32) * step_val, min, max, step);
                if (new_val - value).abs() > 1e-6 {
                    (oc)(new_val);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        })
        .on_key_event({
            let oc = oc.clone();
            let en = is_enabled;
            move |ke: KeyEvent| -> bool {
                if !en || ke.event_type != KeyEventType::Down {
                    return false;
                }
                let step_val = step.unwrap_or(((max - min) / 100.0).max(1e-6)).max(1e-6);
                let page = step_val * 10.0;
                let new_val = match ke.key {
                    Key::ArrowLeft | Key::ArrowDown => snap_step(value - step_val, min, max, step),
                    Key::ArrowRight | Key::ArrowUp => snap_step(value + step_val, min, max, step),
                    Key::PageDown => snap_step(value - page, min, max, step),
                    Key::PageUp => snap_step(value + page, min, max, step),
                    Key::Home => min,
                    Key::End => max,
                    _ => return false,
                };
                if (new_val - value).abs() > 1e-6 {
                    (oc)(new_val);
                }
                true
            }
        })
        .then(config.modifier))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: is_enabled,
        selectable_group: false,
    })
}

pub fn RangeSlider(
    start: f32,
    end: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32, f32) + 'static,
    config: SliderConfig,
) -> View {
    assert!(range.0 <= range.1, "Slider range start must be <= end");
    if let Some(s) = step {
        assert!(s > 0.0, "Slider step must be positive");
    }
    let id = *remember(|| SLIDER_COUNTER.fetch_add(1, Ordering::Relaxed));
    let track_rect = remember_state_with_key(format!("mrs_rect_{}", id), Rect::default);
    let drag_active = remember_mutable_with_key(format!("mrs_da_{}", id), || false);
    let active_thumb = remember_mutable_with_key(format!("mrs_at_{}", id), || false);
    let hovered = remember(|| Signal::new(false));
    let focused = remember(|| Signal::new(false));

    let min = range.0;
    let max = range.1;
    let oc = Rc::new(on_change);
    let range_size = (max - min).max(1e-6);
    let t0 = ((start - min) / range_size).clamp(0.0, 1.0);
    let t1 = ((end - min) / range_size).clamp(0.0, 1.0);
    let sc = config.state_colors;
    let is_enabled = config.enabled;

    let act_trk = if !is_enabled {
        config.disabled_active_track_color
    } else {
        config.active_track_color
    };
    let inact_trk = if !is_enabled {
        config.disabled_inactive_track_color
    } else {
        config.inactive_track_color
    };
    let act_tick = if !is_enabled {
        config.disabled_active_tick_color
    } else {
        config.active_tick_color
    };
    let inact_tick = if !is_enabled {
        config.disabled_inactive_tick_color
    } else {
        config.inactive_tick_color
    };
    let thumb_col = if !is_enabled {
        config.disabled_thumb_color
    } else {
        config.thumb_color
    };

    let tick_frac: Vec<f32> = if let Some(s) = step {
        let n = ((max - min) / s.max(1e-6)).round() as usize;
        (0..=n).map(|i| i as f32 / n as f32).collect()
    } else {
        Vec::new()
    };

    let track_rect_p = track_rect.clone();
    let drag_active_p = drag_active.clone();
    let active_thumb_p = active_thumb.clone();
    let hovered_sig = hovered.clone();
    let focused_sig = focused.clone();

    let sl_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));
    let mut host = Modifier::new()
        .min_width(200.0)
        .height(44.0)
        .interaction_source(&sl_source)
        .indication(ripple(RippleConfig {
            color: Some(thumb_col),
            bounded: false,
            radius: Some(20.0),
            ..Default::default()
        }));
    if !is_enabled {
        host = host.enabled(false);
    }
    host = host.focusable(true).on_focus_changed({
        let f = focused.clone();
        move |focused| f.set(focused)
    });
    Box(host
        .painter(move |scene: &mut Scene, rect: Rect, alpha: f32| {
            let mul_c = |c: Color| {
                Color(
                    c.0,
                    c.1,
                    c.2,
                    ((c.3 as f32) * alpha).clamp(0.0, 255.0) as u8,
                )
            };
            let track_h = dp_to_px(SliderDefaults::TRACK_HEIGHT);
            let thumb_w = dp_to_px(SliderDefaults::THUMB_WIDTH);
            let thumb_h = dp_to_px(SliderDefaults::THUMB_HEIGHT);
            let dot_r = dp_to_px(2.0);
            let corner = track_h * 0.5;
            let gap = thumb_w * 0.5 + dp_to_px(ProgressIndicatorDefaults::SLIDER_THUMB_TRACK_GAP);
            let pad = thumb_w * 0.5;
            let track_x = rect.x + pad;
            let track_w = (rect.w - thumb_w).max(0.0);
            let cy = rect.y + rect.h * 0.5;

            let thumb_pos = |tf: f32, fracs: &[f32]| {
                if step.is_some() && !fracs.is_empty() {
                    let is_first = (tf - fracs[0]).abs() < 1e-6;
                    let is_last = (tf - fracs[fracs.len() - 1]).abs() < 1e-6;
                    if is_first || is_last {
                        track_x + tf * track_w
                    } else {
                        track_x + (track_w - track_h) * tf + corner
                    }
                } else {
                    track_x + tf * track_w
                }
            };
            let k0 = thumb_pos(t0, &tick_frac);
            let k1 = thumb_pos(t1, &tick_frac);
            let active_l = k0.min(k1);
            let active_r = k0.max(k1);

            *track_rect_p.borrow_mut() = Rect {
                x: track_x,
                y: rect.y,
                w: track_w,
                h: rect.h,
            };

            let linactive_w = (active_l - gap - track_x).max(0.0);
            if linactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: track_x,
                        y: cy - track_h * 0.5,
                        w: linactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(inact_trk)),
                    radius: [corner; 4],
                });
                let sx0 = track_x + corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx0 - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(inact_tick)),
                });
            }
            let rinactive_x = (active_r + gap).min(track_x + track_w);
            let rinactive_w = (track_x + track_w - rinactive_x).max(0.0);
            if rinactive_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: rinactive_x,
                        y: cy - track_h * 0.5,
                        w: rinactive_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(inact_trk)),
                    radius: [corner; 4],
                });
                let sx = track_x + track_w - corner;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: sx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(inact_tick)),
                });
            }
            let active_w = (active_r - gap - (active_l + gap)).max(0.0);
            if active_w > 0.0 {
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: active_l + gap,
                        y: cy - track_h * 0.5,
                        w: active_w,
                        h: track_h,
                    },
                    brush: Brush::Solid(mul_c(act_trk)),
                    radius: [corner; 4],
                });
            }
            let tick_start = track_x + corner;
            let tick_end = track_x + track_w - corner;
            for (i, &tf) in tick_frac.iter().enumerate() {
                let tx = tick_start + tf * (tick_end - tick_start);
                // skip ticks that fall on the stop indicators (first and last)
                if i == 0 || i == tick_frac.len() - 1 {
                    continue;
                }
                let in_lgap = tx >= active_l - gap && tx <= active_l + gap;
                let in_rgap = tx >= active_r - gap && tx <= active_r + gap;
                if in_lgap || in_rgap {
                    continue;
                }
                let on_active = tx >= active_l + gap && tx <= active_r - gap;
                scene.nodes.push(SceneNode::Ellipse {
                    rect: Rect {
                        x: tx - dot_r,
                        y: cy - dot_r,
                        w: dot_r * 2.0,
                        h: dot_r * 2.0,
                    },
                    brush: Brush::Solid(mul_c(if on_active { act_tick } else { inact_tick })),
                });
            }
            let da = *drag_active_p.get();
            let at = *active_thumb_p.get();
            let hv = hovered_sig.get();
            let fs = focused_sig.get();
            let thumbs = [k0, k1];
            for (idx, &kx) in thumbs.iter().enumerate() {
                let is_active = da && (if idx == 0 { !at } else { at });
                let tw = if is_active { thumb_w * 0.5 } else { thumb_w };
                scene.nodes.push(SceneNode::Rect {
                    rect: Rect {
                        x: kx - tw * 0.5,
                        y: cy - thumb_h * 0.5,
                        w: tw,
                        h: thumb_h,
                    },
                    brush: Brush::Solid(mul_c(thumb_col)),
                    radius: [tw * 0.5; 4],
                });
                let sc_target = if !is_enabled {
                    Color::TRANSPARENT
                } else if is_active {
                    sc.pressed
                } else if fs {
                    sc.focused
                } else if hv {
                    sc.hovered
                } else {
                    sc.default
                };
                if sc_target.3 > 0 {
                    let sl = dp_to_px(SliderDefaults::STATE_LAYER_SIZE);
                    scene.nodes.push(SceneNode::Ellipse {
                        rect: Rect {
                            x: kx - sl * 0.5,
                            y: cy - sl * 0.5,
                            w: sl,
                            h: sl,
                        },
                        brush: Brush::Solid(mul_c(sc_target)),
                    });
                }
            }
        })
        .on_pointer_enter({
            let h = hovered.clone();
            let en = is_enabled;
            move |_pe: PointerEvent| {
                if en {
                    h.set(true);
                }
            }
        })
        .on_pointer_leave({
            let h = hovered.clone();
            move |_pe: PointerEvent| h.set(false)
        })
        .on_pointer_down({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |pe: PointerEvent| {
                if !en {
                    return;
                }
                drag_active.set(true);
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position_in_window().x, r, min, max, step);
                let use_end = (v - end).abs() < (v - start).abs();
                active_thumb.set(use_end);
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
                pe.consume();
            }
        })
        .on_pointer_move({
            let oc = oc.clone();
            let track_rect = track_rect.clone();
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |pe: PointerEvent| {
                if !en {
                    return;
                }
                if !*drag_active.get() {
                    return;
                }
                let r = *track_rect.borrow();
                let v = value_from_x(pe.position_in_window().x, r, min, max, step);
                let use_end = *active_thumb.get();
                let (a, b) = if use_end {
                    (start, v.max(start))
                } else {
                    (v.min(end), end)
                };
                (oc)(a, b);
                pe.consume();
            }
        })
        .on_pointer_up({
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            let on_finished = config.on_value_change_finished.clone();
            move |_pe: PointerEvent| {
                let was = *drag_active.get();
                drag_active.set(false);
                active_thumb.set(false);
                if was {
                    if let Some(ref cb) = on_finished {
                        (cb)();
                    }
                }
            }
        })
        .on_pointer_cancel({
            let drag_active = drag_active.clone();
            let active_thumb = active_thumb.clone();
            let on_finished = config.on_value_change_finished.clone();
            move |_pe: PointerEvent| {
                if *drag_active.get() {
                    drag_active.set(false);
                    active_thumb.set(false);
                    if let Some(ref cb) = on_finished {
                        (cb)();
                    }
                }
            }
        })
        .on_scroll({
            let oc = oc.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |d: Vec2| -> Vec2 {
                if !en {
                    return d;
                }
                let dir = if d.y < -0.5 {
                    1
                } else if d.y > 0.5 {
                    -1
                } else {
                    0
                };
                if dir == 0 {
                    return d;
                }
                let step_val = step.unwrap_or(1.0).max(1e-6);
                let use_end = *active_thumb.get();
                let (mut a, mut b) = (start, end);
                if use_end {
                    b = snap_step(end + (dir as f32) * step_val, min, max, step).max(a);
                } else {
                    a = snap_step(start + (dir as f32) * step_val, min, max, step).min(b);
                }
                if (a - start).abs() > 1e-6 || (b - end).abs() > 1e-6 {
                    (oc)(a, b);
                    Vec2 { x: d.x, y: 0.0 }
                } else {
                    d
                }
            }
        })
        .on_key_event({
            let oc = oc.clone();
            let active_thumb = active_thumb.clone();
            let en = is_enabled;
            move |ke: KeyEvent| -> bool {
                if !en || ke.event_type != KeyEventType::Down {
                    return false;
                }
                let step_val = step.unwrap_or(((max - min) / 100.0).max(1e-6)).max(1e-6);
                let page = step_val * 10.0;
                let use_end = *active_thumb.get();
                let (a, b) = if use_end {
                    let new_b = match ke.key {
                        Key::ArrowLeft | Key::ArrowDown => {
                            snap_step(end - step_val, min, max, step)
                        }
                        Key::ArrowRight | Key::ArrowUp => {
                            snap_step(end + step_val, min, max, step)
                        }
                        Key::PageDown => snap_step(end - page, min, max, step),
                        Key::PageUp => snap_step(end + page, min, max, step),
                        Key::Home => start,
                        Key::End => max,
                        _ => return false,
                    };
                    (start, new_b.max(start))
                } else {
                    let new_a = match ke.key {
                        Key::ArrowLeft | Key::ArrowDown => {
                            snap_step(start - step_val, min, max, step)
                        }
                        Key::ArrowRight | Key::ArrowUp => {
                            snap_step(start + step_val, min, max, step)
                        }
                        Key::PageDown => snap_step(start - page, min, max, step),
                        Key::PageUp => snap_step(start + page, min, max, step),
                        Key::Home => min,
                        Key::End => end,
                        _ => return false,
                    };
                    (new_a.min(end), end)
                };
                if (a - start).abs() > 1e-6 || (b - end).abs() > 1e-6 {
                    (oc)(a, b);
                }
                true
            }
        })
        .then(config.modifier))
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: is_enabled,
        selectable_group: false,
    })
}
