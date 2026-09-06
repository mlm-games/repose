//! Material 3 Ripple indication.
//!
//! Animation specs matching Compose `RippleAnimation`:
//!   - Fade in:  alpha 0->1, 75ms, LinearEasing
//!   - Expand:   radius 0.3·maxDim -> targetRadius, 225ms, FastOutSlowInEasing
//!   - Center:   pressPos -> layoutCenter, 225ms, LinearEasing (bounded only)
//!   - Fade out: alpha 1->0, 150ms, LinearEasing (after fade-in completes)
//!
//! Compose's RippleAnimation.always completes fade-in to 1.0 before fading out,
//! even if the press was released before fade-in finished.
//! See RippleAnimation.kt `if (finishRequested && !finishedFadingIn) alpha = 1f`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use repose_core::animation::{AnimatedValue, AnimationSpec, Easing};
use repose_core::animation_driver;
use repose_core::{
    Color, Indication, IndicationDrawNode, IndicationNodeFactory, InteractionSource, PressId, Rect,
    Scene, SceneNode, Vec2, remember_state_with_key, request_frame,
};

const FADE_IN_MS: u64 = 75;
const RADIUS_MS: u64 = 225;
const FADE_OUT_MS: u64 = 150;

/// StateTokens.PressedStateLayerOpacity -> fixed press opacity multiplier matching Compose.
const PRESS_ALPHA: f32 = 0.10;
/// StateTokens.FocusStateLayerOpacity -> Material 3 focus state layer opacity.
const FOCUS_ALPHA: f32 = 0.12;
/// StateTokens.HoverStateLayerOpacity -> Material 3 hover state layer opacity.
const HOVER_ALPHA: f32 = 0.08;

#[derive(Clone, Debug)]
pub struct RippleConfig {
    pub bounded: bool,
    pub radius: Option<f32>,
    pub color: Option<Color>,
    pub enable_press: bool,
    pub enable_focus: bool,
    pub enable_hover: bool,
    pub press_offset: Option<Vec2>,
}

impl Default for RippleConfig {
    fn default() -> Self {
        Self {
            bounded: true,
            radius: None,
            color: None,
            enable_press: true,
            enable_focus: true,
            enable_hover: true,
            press_offset: None,
        }
    }
}

pub fn ripple(config: RippleConfig) -> Rc<dyn IndicationNodeFactory> {
    Rc::new(RippleNodeFactory { config })
}

/// Default factory for `LocalIndication` (color resolved from `content_color()` at draw time).
pub fn default_ripple() -> Rc<dyn IndicationNodeFactory> {
    ripple(RippleConfig::default())
}

#[derive(Clone, Debug)]
pub struct RippleNodeFactory {
    pub config: RippleConfig,
}

impl Indication for RippleNodeFactory {}

impl IndicationNodeFactory for RippleNodeFactory {
    fn create(&self, interaction_source: &InteractionSource) -> Box<dyn IndicationDrawNode> {
        Box::new(RippleDrawNode::new(
            interaction_source.clone(),
            self.config.clone(),
        ))
    }
}

struct RippleDrawNode {
    interaction_source: InteractionSource,
    config: RippleConfig,
}

impl RippleDrawNode {
    fn new(interaction_source: InteractionSource, config: RippleConfig) -> Self {
        Self {
            interaction_source,
            config,
        }
    }

    fn anim_base(&self) -> String {
        format!("rp:{:p}", self.interaction_source.stable_id())
    }

    fn register_driver(key: &str, anim: Rc<RefCell<AnimatedValue<f32>>>) {
        animation_driver::register(
            key.to_string(),
            Rc::new(RefCell::new(move || anim.borrow_mut().update())),
        );
        request_frame();
    }
}

impl IndicationDrawNode for RippleDrawNode {
    fn draw(&self, scene: &mut Scene, rect: Rect, radius: [f32; 4], alpha: f32) {
        let base_color = self
            .config
            .color
            .unwrap_or_else(repose_core::locals::content_color);

        // M3 state layers (focus, hover) rendered as circles. Drawn even while
        // pressed (Compose draws stateLayer then drawRipples together);
        // priority: focus > hover.
        let is_pressed = self.interaction_source.collect_is_pressed();
        let is_hovered = self.interaction_source.collect_is_hovered();
        let is_focused = self.interaction_source.collect_is_focus_visible();
        let bounded = self.config.bounded;

        let center_scene = Vec2 {
            x: rect.x + rect.w * 0.5,
            y: rect.y + rect.h * 0.5,
        };
        let target_radius = self.config.radius.unwrap_or_else(|| {
            let diag = (rect.w * rect.w + rect.h * rect.h).sqrt();
            if bounded {
                diag * 0.5 + 10.0
            } else {
                diag * 0.5
            }
        });

        let layer_alpha = if self.config.enable_focus && is_focused {
            Some(FOCUS_ALPHA)
        } else if self.config.enable_hover && is_hovered {
            Some(HOVER_ALPHA)
        } else {
            None
        };

        if let Some(layer_a) = layer_alpha {
            let layer_a = layer_a * alpha;
            if layer_a > 0.001 {
                let layer_rect = Rect {
                    x: center_scene.x - target_radius,
                    y: center_scene.y - target_radius,
                    w: target_radius * 2.0,
                    h: target_radius * 2.0,
                };
                let brush = base_color.with_alpha_f32(layer_a).into();
                if bounded {
                    scene.nodes.push(SceneNode::PushClip {
                        rect,
                        radius,
                        op: repose_core::ClipOp::Intersect,
                    });
                }
                scene.nodes.push(SceneNode::Ellipse {
                    rect: layer_rect,
                    brush,
                });
                if bounded {
                    scene.nodes.push(SceneNode::PopClip);
                }
            }
        }

        if !self.config.enable_press {
            return;
        }

        let base = self.anim_base();
        let start_radius = rect.w.max(rect.h) * 0.3;

        let current_pid = self.interaction_source.collect_last_press_id();
        let press_pos = self.interaction_source.collect_last_press_position();

        let k_alpha = format!("{}:a", base);
        let k_rad = format!("{}:r", base);
        let k_ctr = format!("{}:c", base);

        let alpha_anim = remember_state_with_key(&k_alpha, || {
            AnimatedValue::new(
                0.0f32,
                AnimationSpec::tween(Duration::from_millis(FADE_IN_MS), Easing::Linear),
            )
        });
        let rad_anim = remember_state_with_key(&k_rad, || {
            AnimatedValue::new(
                0.0f32,
                AnimationSpec::tween(Duration::from_millis(RADIUS_MS), Easing::FastOutSlowIn),
            )
        });
        let ctr_anim = remember_state_with_key(&k_ctr, || {
            AnimatedValue::new(
                0.0f32,
                AnimationSpec::tween(Duration::from_millis(RADIUS_MS), Easing::Linear),
            )
        });

        // Phase: 0=idle, 1=rising(fade-in), 2=visible, 3=fading-out
        let k_phase = format!("{}:ph", base);
        let phase = remember_state_with_key(&k_phase, || 0u8);

        // Track last processed press ID to detect new presses
        let k_last_pid = format!("{}:lpid", base);
        let last_pid = remember_state_with_key(&k_last_pid, || None::<PressId>);

        // Pending release flag -> set when release occurs before fade-in completes
        let k_release_pending = format!("{}:rpend", base);
        let release_pending = remember_state_with_key(&k_release_pending, || false);

        let prev_pid = *last_pid.borrow();

        animation_driver::touch(&format!("{}:drv:a", base));
        animation_driver::touch(&format!("{}:drv:r", base));
        animation_driver::touch(&format!("{}:drv:c", base));

        // Use last_press_id which persists after release (unlike is_pressed which is transient
        // because press+release can both happen before the next frame renders).
        let new_press = current_pid.is_some() && current_pid != prev_pid;

        if new_press {
            *last_pid.borrow_mut() = current_pid;
            *phase.borrow_mut() = 1;
            *release_pending.borrow_mut() = false;

            let spec_in = AnimationSpec::tween(Duration::from_millis(FADE_IN_MS), Easing::Linear);
            let spec_rad =
                AnimationSpec::tween(Duration::from_millis(RADIUS_MS), Easing::FastOutSlowIn);
            let spec_ctr = AnimationSpec::tween(Duration::from_millis(RADIUS_MS), Easing::Linear);

            {
                let mut a = alpha_anim.borrow_mut();
                a.snap_to(0.0);
                a.set_spec(spec_in);
                a.set_target(1.0);
            }
            Self::register_driver(&format!("{}:drv:a", base), alpha_anim.clone());

            {
                let mut r = rad_anim.borrow_mut();
                r.snap_to(0.0);
                r.set_spec(spec_rad);
                r.set_target(1.0);
            }
            Self::register_driver(&format!("{}:drv:r", base), rad_anim.clone());

            {
                let mut c = ctr_anim.borrow_mut();
                c.snap_to(0.0);
                c.set_spec(spec_ctr);
                c.set_target(1.0);
            }
            Self::register_driver(&format!("{}:drv:c", base), ctr_anim.clone());
        }

        // Compare against *last_pid.borrow(), not prev_pid, because prev_pid was
        // captured before the new-press block and would be None on first detection.
        if *phase.borrow() != 0
            && !is_pressed
            && current_pid.is_some()
            && current_pid == *last_pid.borrow()
        {
            *release_pending.borrow_mut() = true;
        }

        if *phase.borrow() != 0 && !animation_driver::is_registered(&format!("{}:drv:a", base)) {
            *phase.borrow_mut() = 0;
            *last_pid.borrow_mut() = current_pid;
            *release_pending.borrow_mut() = false;
            alpha_anim.borrow_mut().snap_to(0.0);
            rad_anim.borrow_mut().snap_to(0.0);
            ctr_anim.borrow_mut().snap_to(0.0);
            return;
        }

        let fade_pct = *alpha_anim.borrow().get();

        if *phase.borrow() == 1 && fade_pct >= 1.0 {
            // Fade-in complete -> move to visible or start fade-out
            if *release_pending.borrow() {
                *phase.borrow_mut() = 3;
                let spec_out =
                    AnimationSpec::tween(Duration::from_millis(FADE_OUT_MS), Easing::Linear);
                alpha_anim.borrow_mut().set_target(0.0);
                alpha_anim.borrow_mut().set_spec(spec_out);
                Self::register_driver(&format!("{}:drv:a", base), alpha_anim.clone());
            } else {
                *phase.borrow_mut() = 2;
            }
        }

        if *phase.borrow() == 2 && *release_pending.borrow() {
            // Release happened while visible -> start fade-out
            *phase.borrow_mut() = 3;
            let spec_out = AnimationSpec::tween(Duration::from_millis(FADE_OUT_MS), Easing::Linear);
            alpha_anim.borrow_mut().set_target(0.0);
            alpha_anim.borrow_mut().set_spec(spec_out);
            Self::register_driver(&format!("{}:drv:a", base), alpha_anim.clone());
        }

        if *phase.borrow() == 3 && fade_pct <= 0.01 {
            // Fade-out complete -> reset
            *phase.borrow_mut() = 0;
            // Keep last_pid = current_pid so the stale press ID doesn't
            // retrigger new_press as a false positive on the next frame.
            *last_pid.borrow_mut() = current_pid;
            alpha_anim.borrow_mut().snap_to(0.0);
            rad_anim.borrow_mut().snap_to(0.0);
            ctr_anim.borrow_mut().snap_to(0.0);
            return;
        }

        if *phase.borrow() == 0 {
            return;
        }
        let snap_finish = *release_pending.borrow() && *phase.borrow() == 1;
        if fade_pct <= 0.01 && !snap_finish {
            return;
        }

        // Compose RippleAnimation.draw() snaps alpha to 1.0 when finish() is called
        // during fade-in: `if (finishRequested && !finishedFadingIn) alpha = 1f`.
        let draw_alpha = if snap_finish { 1.0f32 } else { fade_pct };

        let rad_pct = *rad_anim.borrow().get();
        let ctr_pct = *ctr_anim.borrow().get();
        let current_radius = start_radius + (target_radius - start_radius) * rad_pct;

        let origin_scene = match press_pos {
            Some(pos) => {
                let effective = if let Some(off) = self.config.press_offset {
                    Vec2 {
                        x: pos.x - off.x,
                        y: pos.y - off.y,
                    }
                } else {
                    pos
                };
                let ox = rect.x + effective.x;
                let oy = rect.y + effective.y;
                if bounded {
                    Vec2 {
                        x: ox + (center_scene.x - ox) * ctr_pct,
                        y: oy + (center_scene.y - oy) * ctr_pct,
                    }
                } else {
                    center_scene
                }
            }
            None => center_scene,
        };

        // Compose: color.copy(alpha = PressAlpha * animatedAlpha)
        let ripple_alpha = PRESS_ALPHA * draw_alpha * alpha;
        if ripple_alpha <= 0.001 {
            return;
        }

        let draw_color = base_color.with_alpha_f32(ripple_alpha);

        if bounded {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius,
                op: repose_core::ClipOp::Intersect,
            });
            scene.nodes.push(SceneNode::Ellipse {
                rect: Rect {
                    x: origin_scene.x - current_radius,
                    y: origin_scene.y - current_radius,
                    w: current_radius * 2.0,
                    h: current_radius * 2.0,
                },
                brush: draw_color.into(),
            });
            scene.nodes.push(SceneNode::PopClip);
        } else {
            scene.nodes.push(SceneNode::Ellipse {
                rect: Rect {
                    x: origin_scene.x - current_radius,
                    y: origin_scene.y - current_radius,
                    w: current_radius * 2.0,
                    h: current_radius * 2.0,
                },
                brush: draw_color.into(),
            });
        }
    }
}
