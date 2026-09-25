#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use web_time::{Duration, Instant};

use repose_core::animation::{AnimationSpec, SpringSpec};
use repose_core::*;
use repose_ui::{
    Box, Column, Row, ViewExt, ZStack, anim::animate_f32_from, overlay::OverlayGuard,
    overlay::ambient_overlay,
};

use crate::ripple::{RippleConfig, ripple};

use super::*;

/// Configuration for [`BottomSheet`] / `ModalBottomSheet`.
#[derive(Clone, Debug)]
pub struct BottomSheetConfig {
    pub container_color: Color,
    pub content_color: Color,
    pub scrim_color: Color,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
    pub drag_handle_color: Color,
    pub shape_radius: Dp,
    pub max_width: Dp,
    pub drag_handle_width: Dp,
    pub drag_handle_height: Dp,
    pub peek_height: Dp,
    pub gestures_enabled: bool,
}

impl Default for BottomSheetConfig {
    fn default() -> Self {
        Self {
            container_color: BottomSheetDefaults::container_color(),
            content_color: BottomSheetDefaults::content_color(),
            scrim_color: BottomSheetDefaults::scrim_color(),
            tonal_elevation: BottomSheetDefaults::TONAL_ELEVATION,
            shadow_elevation: Dp::ZERO,
            drag_handle_color: BottomSheetDefaults::drag_handle_color(),
            shape_radius: BottomSheetDefaults::SHAPE_RADIUS,
            max_width: BottomSheetDefaults::MAX_WIDTH,
            drag_handle_width: BottomSheetDefaults::DRAG_HANDLE_WIDTH,
            drag_handle_height: BottomSheetDefaults::DRAG_HANDLE_HEIGHT,
            peek_height: BottomSheetDefaults::PEEK_HEIGHT,
            gestures_enabled: true,
        }
    }
}

pub fn BottomSheet(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
    config: BottomSheetConfig,
) -> View {
    let th = theme();
    let instance_id = remember(unique_component_id);
    let identity = match modifier.key {
        Some(key) => format!("bottom-sheet:key:{key}"),
        None => format!("bottom-sheet:instance:{instance_id}"),
    };

    let opacity = animate_f32_from(
        format!("{identity}:opacity"),
        if visible { 0.0 } else { 1.0 },
        if visible { 1.0 } else { 0.0 },
        th.motion.layout,
    );

    if !visible && opacity <= 0.01 {
        return Box(Modifier::new());
    }
    let sheet = Box(modifier
        .fill_max_width()
        .max_width(config.max_width)
        .min_height(config.peek_height)
        .alpha(opacity)
        .background(config.container_color)
        .state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            focused: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: Dp::ZERO,
        })
        .shadow(config.shadow_elevation, Dp::ZERO)
        .clip_rounded(config.shape_radius))
    .child(with_content_color(config.content_color, move || content));

    let dismiss_target = if config.gestures_enabled {
        Box(Modifier::new()
            .width(Dp(1.0))
            .height(Dp(0.0))
            .fill_max_width()
            .alpha(opacity)
            .hit_passthrough()
            .on_pointer_down(move |_| on_dismiss()))
    } else {
        Box(Modifier::new())
    };

    Column(Modifier::new().fill_max_width()).child((sheet, dismiss_target))
}

/// State for `ModalBottomSheet` - manages visibility and drag offset.
pub struct SheetState {
    visible: Signal<bool>,
    /// Drag offset in px (pointer space).
    drag_offset: Signal<f32>,
    /// Peek height in [`Dp`] magnitudes.
    peek_height: Signal<f32>,
    id: u64,
}

impl SheetState {
    pub fn new(peek_height: Dp) -> Self {
        Self {
            visible: signal(false),
            drag_offset: signal(0.0),
            peek_height: signal(peek_height.0),
            id: unique_component_id(),
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("sheet_{}_{}", self.id, suffix)
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set_neq(true);
    }

    pub fn dismiss(&self) {
        self.visible.set_neq(false);
        self.drag_offset.set_neq(0.0);
    }

    pub fn set_peek_height(&self, h: Dp) {
        self.peek_height.set_neq(h.0);
    }
}

fn modal_sheet_offset(value: f32, distance: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        distance.max(0.0)
    }
}

fn modal_sheet_drag_offset(value: f32, distance: f32) -> f32 {
    value.clamp(0.0, distance.max(0.0))
}

const MODAL_SHEET_POSITIONAL_THRESHOLD: Dp = Dp(56.0);
const MODAL_SHEET_VELOCITY_THRESHOLD: Dp = Dp(125.0);
const MODAL_SHEET_SPRING_VISIBILITY_THRESHOLD_PX: f32 = 0.01;

type DragSample = (f32, Instant);

fn update_drag_velocity(samples: &mut VecDeque<DragSample>, y: f32, now: Instant) -> f32 {
    samples.push_back((y, now));
    while samples.len() > 1
        && samples.front().is_some_and(|(_, time)| {
            now.saturating_duration_since(*time) > Duration::from_millis(100)
        })
    {
        samples.pop_front();
    }
    if samples.len() < 2 {
        return 0.0;
    }
    let origin = samples.front().map(|(_, time)| *time).unwrap_or(now);
    let count = samples.len() as f32;
    let mut sum_time = 0.0;
    let mut sum_position = 0.0;
    let mut sum_time_time = 0.0;
    let mut sum_time_position = 0.0;
    for (position, time) in samples.iter() {
        let time = time.saturating_duration_since(origin).as_secs_f32();
        sum_time += time;
        sum_position += *position;
        sum_time_time += time * time;
        sum_time_position += time * *position;
    }
    let denominator = sum_time_time - sum_time * sum_time / count;
    if denominator <= f32::EPSILON {
        return 0.0;
    }
    let velocity = (sum_time_position - sum_time * sum_position / count) / denominator;
    if velocity.is_finite() { velocity } else { 0.0 }
}

fn modal_sheet_release_target(
    offset: f32,
    distance: f32,
    velocity: f32,
    positional_threshold: f32,
    velocity_threshold: f32,
) -> f32 {
    let distance = distance.max(0.0);
    if distance <= f32::EPSILON {
        return 0.0;
    }
    let offset = offset.clamp(0.0, distance);
    let velocity = if velocity.is_finite() { velocity } else { 0.0 };
    let velocity_threshold = velocity_threshold.abs();
    if velocity == 0.0 {
        return if offset >= distance * 0.5 {
            distance
        } else {
            0.0
        };
    }
    if velocity.abs() >= velocity_threshold {
        return if velocity > 0.0 { distance } else { 0.0 };
    }
    if velocity > 0.0 {
        if offset >= positional_threshold.max(0.0) {
            distance
        } else {
            0.0
        }
    } else if distance - offset >= positional_threshold.max(0.0) {
        0.0
    } else {
        distance
    }
}

fn modal_sheet_spatial_spec(travel: f32) -> AnimationSpec {
    let settle_progress = (MODAL_SHEET_SPRING_VISIBILITY_THRESHOLD_PX
        / travel.abs().max(MODAL_SHEET_SPRING_VISIBILITY_THRESHOLD_PX))
    .min(1.0);
    AnimationSpec::spring(SpringSpec::new(0.9, 700.0).with_settle_progress(settle_progress))
}

fn modal_sheet_hide_spec(travel: f32) -> AnimationSpec {
    let settle_progress = (MODAL_SHEET_SPRING_VISIBILITY_THRESHOLD_PX
        / travel.abs().max(MODAL_SHEET_SPRING_VISIBILITY_THRESHOLD_PX))
    .min(1.0);
    AnimationSpec::spring(SpringSpec::new(1.0, 3800.0).with_settle_progress(settle_progress))
}

fn modal_sheet_effects_spec() -> AnimationSpec {
    AnimationSpec::spring(SpringSpec::new(1.0, 1600.0).with_settle_progress(0.01))
}

/// M3 Modal Bottom Sheet - slides up from the bottom with a drag handle.
///
/// Renders as an overlay so it is not clipped by parent containers.
/// Shows on `state.show()`, dismisses on `state.dismiss()` or scrim tap.
pub fn ModalBottomSheet(
    state: Rc<SheetState>,
    modifier: Modifier,
    content: View,
    config: BottomSheetConfig,
) -> View {
    let overlay = ambient_overlay();
    // Peek heights are Dp; the slide animation runs in px (pointer space).
    let peek_h = Dp(state.peek_height.get().max(config.peek_height.0));
    let mbs_id = state.key("modal");
    let sheet_height: Rc<Cell<f32>> =
        remember_with_key(format!("mbs_height_{mbs_id}"), || Cell::new(0.0));
    let viewport_height = repose_core::locals::get_window_container_height().max(0.0);
    let measured_height = sheet_height.get().max(0.0);
    let anim_distance = Dp(peek_h.0.max(viewport_height).max(measured_height) + 1.0);
    let anim_distance_px = remember_with_key(format!("mbs_distance_{mbs_id}"), || {
        Cell::new(anim_distance.to_px().0)
    });
    anim_distance_px.set(anim_distance.to_px().0);
    let overlay_guard = remember_with_key(format!("mbs_oguard_{mbs_id}"), || {
        RefCell::new(None::<OverlayGuard>)
    });

    // Fresh content each composition (builder captures content once).
    let current_content =
        remember_state_with_key(format!("mbs_c_{mbs_id}"), || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    let current_modifier = remember_state_with_key(format!("mbs_mod_{mbs_id}"), Modifier::new);
    *current_modifier.borrow_mut() = modifier;
    let current_config = remember_state_with_key(format!("mbs_cfg_{mbs_id}"), || config.clone());
    *current_config.borrow_mut() = config;

    // Drag state -> offset_at_drag_start is the anim value when the drag began
    let drag_anchor_y: Rc<RefCell<f32>> =
        remember_state_with_key(format!("mbs_drag_y_{mbs_id}"), || 0.0);
    let offset_at_drag_start: Rc<RefCell<f32>> =
        remember_state_with_key(format!("mbs_drag_base_{mbs_id}"), || 0.0);
    let is_dragging: Rc<RefCell<bool>> =
        remember_state_with_key(format!("mbs_drag_{mbs_id}"), || false);
    let drag_samples: Rc<RefCell<VecDeque<DragSample>>> =
        remember_with_key(format!("mbs_drag_samples_{mbs_id}"), || {
            RefCell::new(VecDeque::new())
        });
    let drag_velocity: Rc<Cell<f32>> =
        remember_with_key(format!("mbs_drag_velocity_{mbs_id}"), || Cell::new(0.0));
    let drag_release_in_flight: Rc<Cell<bool>> =
        remember_with_key(format!("mbs_drag_release_{mbs_id}"), || Cell::new(false));

    let positional_threshold_px = MODAL_SHEET_POSITIONAL_THRESHOLD.to_px().0;
    let velocity_threshold_px = MODAL_SHEET_VELOCITY_THRESHOLD.to_px().0;

    let dh_source: Rc<MutableInteractionSource> = remember_with_key(
        format!("mbs_dh_src_{mbs_id}"),
        MutableInteractionSource::new,
    );

    let anim = remember_state_with_key(format!("mbs_anim_{mbs_id}"), || {
        AnimatedValue::new(
            anim_distance_px.get(),
            modal_sheet_spatial_spec(anim_distance_px.get()),
        )
    });
    let last_target = remember_state_with_key(format!("mbs_anim_target_{mbs_id}"), || f32::NAN);
    let last_spatial_motion =
        remember_with_key(format!("mbs_anim_spatial_{mbs_id}"), || Cell::new(true));
    let anim_key = format!("mbs_anim_driver_{mbs_id}");
    repose_core::animation_driver::touch(&anim_key);

    if state.is_visible() && drag_release_in_flight.get() {
        drag_release_in_flight.set(false);
    }
    let release_in_flight = drag_release_in_flight.get();
    let anim_target = if state.is_visible() {
        0.0
    } else {
        anim_distance_px.get()
    };
    let spatial_motion = state.is_visible() || release_in_flight;

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan()
            || (*lt - anim_target).abs() > 1e-6
            || last_spatial_motion.get() != spatial_motion
        {
            let velocity = a.current_velocity();
            let travel = anim_target - *a.get();
            a.set_spec(if spatial_motion {
                modal_sheet_spatial_spec(travel)
            } else {
                modal_sheet_hide_spec(travel)
            });
            a.set_target_with_velocity(anim_target, velocity);
            *lt = anim_target;
            last_spatial_motion.set(spatial_motion);
        }
        let still_animating = a.is_animating();
        if still_animating && !repose_core::animation_driver::is_registered(&anim_key) {
            let registered_anim = anim.clone();
            let registered_release_target = drag_release_in_flight.clone();
            repose_core::animation_driver::register(
                anim_key.clone(),
                Rc::new(RefCell::new(move || {
                    let still = registered_anim.borrow_mut().update();
                    if !still {
                        registered_release_target.set(false);
                    }
                    still
                })),
            );
        }
        if still_animating {
            request_frame();
        }
    }

    let scrim_target = if *is_dragging.borrow() {
        *anim.borrow().get() < anim_distance_px.get() * 0.5
    } else {
        state.is_visible() && !drag_release_in_flight.get()
    };
    let scrim_progress =
        remember_with_key(format!("mbs_scrim_progress_{mbs_id}"), || Cell::new(0.0));
    scrim_progress.set(animate_f32_from(
        format!("mbs_scrim_{mbs_id}"),
        if scrim_target { 1.0 } else { 0.0 },
        if scrim_target { 1.0 } else { 0.0 },
        modal_sheet_effects_spec(),
    ));

    let sheet_visible = state.is_visible()
        || anim.borrow().is_animating()
        || *is_dragging.borrow()
        || (!state.is_visible() && scrim_progress.get() > 0.01);

    if sheet_visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let anim_distance_px = anim_distance_px.clone();
                let sheet_height = sheet_height.clone();
                let current_modifier = current_modifier.clone();
                let current_content = current_content.clone();
                let current_config = current_config.clone();
                let drag_anchor_y = drag_anchor_y.clone();
                let offset_at_drag_start = offset_at_drag_start.clone();
                let is_dragging = is_dragging.clone();
                let drag_samples = drag_samples.clone();
                let drag_velocity = drag_velocity.clone();
                let drag_release_in_flight = drag_release_in_flight.clone();
                let last_target = last_target.clone();
                let last_spatial_motion = last_spatial_motion.clone();
                let scrim_progress = scrim_progress.clone();

                let dh_source = dh_source.clone();
                move || {
                    let modifier = current_modifier.borrow().clone();
                    let config = current_config.borrow().clone();
                    let off = modal_sheet_offset(*anim.borrow().get(), anim_distance_px.get());
                    let content = current_content.borrow().clone();
                    let sheet_peek_height = Dp(state.peek_height.get().max(config.peek_height.0));

                    let mut sheet_mod = modifier
                        .clone()
                        .fill_max_width()
                        .max_width(config.max_width)
                        .align_self(AlignSelf::CENTER)
                        .translate(0.0, off)
                        .background(config.container_color)
                        .state_elevation(StateElevation {
                            default: config.tonal_elevation,
                            hovered: config.tonal_elevation,
                            focused: config.tonal_elevation,
                            pressed: config.tonal_elevation,
                            dragged: config.tonal_elevation,
                            disabled: Dp::ZERO,
                        })
                        .shadow(config.shadow_elevation, Dp::ZERO)
                        .clip_rounded(config.shape_radius)
                        .on_size_changed({
                            let sheet_height = sheet_height.clone();
                            let anim = anim.clone();
                            let anim_distance_px = anim_distance_px.clone();
                            let state = state.clone();
                            let drag_release_in_flight = drag_release_in_flight.clone();
                            let last_target = last_target.clone();
                            let last_spatial_motion = last_spatial_motion.clone();
                            let viewport_height = Dp(viewport_height);

                            move |size| {
                                if size.y.is_finite() {
                                    let height = size.y.max(0.0);
                                    if (sheet_height.get() - height).abs() > f32::EPSILON {
                                        sheet_height.set(height);
                                        let distance = Dp(height
                                            .max(viewport_height.0)
                                            .max(sheet_peek_height.0)
                                            + 1.0)
                                        .to_px()
                                        .0;
                                        anim_distance_px.set(distance);
                                        if !state.is_visible() {
                                            let current = *anim.borrow().get();
                                            let velocity = anim.borrow().current_velocity();
                                            if anim.borrow().is_animating()
                                                || drag_release_in_flight.get()
                                            {
                                                let spatial = last_spatial_motion.get()
                                                    || drag_release_in_flight.get();
                                                let mut animation = anim.borrow_mut();
                                                animation.set_spec(if spatial {
                                                    modal_sheet_spatial_spec(distance - current)
                                                } else {
                                                    modal_sheet_hide_spec(distance - current)
                                                });
                                                animation
                                                    .set_target_with_velocity(distance, velocity);
                                                *last_target.borrow_mut() = distance;
                                                last_spatial_motion.set(spatial);
                                            } else {
                                                anim.borrow_mut().snap_to(distance);
                                            }
                                        }
                                        request_frame();
                                    }
                                }
                            }
                        })
                        .focus_group()
                        .semantics(Semantics {
                            role: Role::Dialog,
                            label: Some("Bottom sheet".into()),
                            ..Default::default()
                        });

                    if config.gestures_enabled {
                        let drag_distance = anim_distance_px.clone();
                        sheet_mod = sheet_mod
                            .on_pointer_down({
                                let anim = anim.clone();
                                let state = state.clone();
                                let drag_release_in_flight = drag_release_in_flight.clone();
                                let drag_anchor_y = drag_anchor_y.clone();
                                let offset_at_drag_start = offset_at_drag_start.clone();
                                let is_dragging = is_dragging.clone();

                                let drag_samples = drag_samples.clone();
                                let drag_velocity = drag_velocity.clone();
                                let drag_distance = anim_distance_px.clone();
                                move |ev| {
                                    if !matches!(
                                        ev.event,
                                        PointerEventKind::Down(PointerButton::Primary)
                                    ) {
                                        return;
                                    }
                                    if !state.is_visible() {
                                        drag_release_in_flight.set(false);
                                        state.show();
                                    }
                                    let y = ev.position_in_window().y;
                                    *drag_anchor_y.borrow_mut() = y;
                                    *offset_at_drag_start.borrow_mut() = modal_sheet_offset(
                                        *anim.borrow().get(),
                                        drag_distance.get(),
                                    );

                                    let now = Instant::now();
                                    drag_samples.borrow_mut().clear();
                                    drag_velocity.set(update_drag_velocity(
                                        &mut drag_samples.borrow_mut(),
                                        y,
                                        now,
                                    ));
                                    *is_dragging.borrow_mut() = true;
                                }
                            })
                            .on_pointer_move({
                                let anim = anim.clone();
                                let drag_anchor_y = drag_anchor_y.clone();
                                let offset_at_drag_start = offset_at_drag_start.clone();
                                let is_dragging = is_dragging.clone();

                                let drag_samples = drag_samples.clone();
                                let drag_velocity = drag_velocity.clone();
                                let drag_distance = anim_distance_px.clone();
                                move |ev| {
                                    if !*is_dragging.borrow() {
                                        return;
                                    }
                                    let y = ev.position_in_window().y;
                                    let now = Instant::now();
                                    let velocity = update_drag_velocity(
                                        &mut drag_samples.borrow_mut(),
                                        y,
                                        now,
                                    );
                                    drag_velocity.set(velocity);

                                    let delta = y - *drag_anchor_y.borrow();
                                    let start_off = *offset_at_drag_start.borrow();
                                    let total = modal_sheet_drag_offset(
                                        start_off + delta,
                                        drag_distance.get(),
                                    );
                                    anim.borrow_mut().snap_to(total);
                                    request_frame();
                                }
                            })
                            .on_pointer_up({
                                let anim = anim.clone();
                                let is_dragging = is_dragging.clone();
                                let state = state.clone();
                                let anim_distance_px = drag_distance.clone();

                                let drag_samples = drag_samples.clone();
                                let drag_velocity = drag_velocity.clone();
                                let drag_release_in_flight = drag_release_in_flight.clone();
                                let last_target = last_target.clone();
                                let last_spatial_motion = last_spatial_motion.clone();

                                move |ev| {
                                    if !*is_dragging.borrow() {
                                        return;
                                    }
                                    let y = ev.position_in_window().y;
                                    let now = Instant::now();
                                    let velocity = update_drag_velocity(
                                        &mut drag_samples.borrow_mut(),
                                        y,
                                        now,
                                    );
                                    drag_velocity.set(velocity);
                                    let release_velocity = drag_velocity.get();
                                    *is_dragging.borrow_mut() = false;
                                    drag_samples.borrow_mut().clear();

                                    let distance = anim_distance_px.get();
                                    let current_off =
                                        modal_sheet_drag_offset(*anim.borrow().get(), distance);
                                    let target = modal_sheet_release_target(
                                        current_off,
                                        distance,
                                        release_velocity,
                                        positional_threshold_px,
                                        velocity_threshold_px,
                                    );
                                    {
                                        let mut animation = anim.borrow_mut();
                                        animation.set_spec(modal_sheet_spatial_spec(
                                            target - current_off,
                                        ));
                                        animation
                                            .set_target_with_velocity(target, release_velocity);
                                    }
                                    *last_target.borrow_mut() = target;
                                    last_spatial_motion.set(true);
                                    if target > 0.0 {
                                        drag_release_in_flight.set(true);
                                        state.dismiss();
                                    } else {
                                        drag_release_in_flight.set(false);
                                    }
                                    request_frame();
                                }
                            })
                            .on_pointer_cancel({
                                let anim = anim.clone();
                                let state = state.clone();
                                let is_dragging = is_dragging.clone();

                                let drag_samples = drag_samples.clone();
                                let drag_release_in_flight = drag_release_in_flight.clone();
                                let last_target = last_target.clone();
                                let last_spatial_motion = last_spatial_motion.clone();

                                let anim_distance_px = anim_distance_px.clone();

                                move |_| {
                                    if !*is_dragging.borrow() {
                                        return;
                                    }
                                    *is_dragging.borrow_mut() = false;
                                    drag_samples.borrow_mut().clear();

                                    let distance = anim_distance_px.get();
                                    let current =
                                        modal_sheet_drag_offset(*anim.borrow().get(), distance);
                                    let target = modal_sheet_release_target(
                                        current,
                                        distance,
                                        0.0,
                                        positional_threshold_px,
                                        velocity_threshold_px,
                                    );
                                    {
                                        let mut animation = anim.borrow_mut();
                                        animation
                                            .set_spec(modal_sheet_spatial_spec(target - current));
                                        animation.set_target_with_velocity(target, 0.0);
                                    }
                                    *last_target.borrow_mut() = target;
                                    last_spatial_motion.set(true);
                                    if target > 0.0 {
                                        drag_release_in_flight.set(true);
                                        state.dismiss();
                                    } else {
                                        drag_release_in_flight.set(false);
                                    }
                                    request_frame();
                                }
                            });
                    }

                    let sheet_body = Box(sheet_mod).child(
                        Column(Modifier::new().fill_max_width()).child((
                            Row(Modifier::new()
                                .fill_max_width()
                                .justify_content(JustifyContent::CENTER))
                            .child(Box(Modifier::new()
                                .margin_vertical(Dp(22.0))
                                .width(config.drag_handle_width)
                                .height(config.drag_handle_height)
                                .background(config.drag_handle_color)
                                .clip_rounded(Dp(2.0))
                                .interaction_source(&dh_source)
                                .indication(ripple(RippleConfig {
                                    color: Some(config.content_color),
                                    bounded: true,
                                    ..Default::default()
                                }))
                                .on_pointer_down(|_| {}))),
                            with_content_color(config.content_color, move || content),
                        )),
                    );

                    let sheet = Column(
                        Modifier::new()
                            .fill_max_size()
                            .justify_content(JustifyContent::FLEX_END)
                            .align_items(AlignItems::CENTER),
                    )
                    .child(sheet_body);

                    let scrim_alpha =
                        (config.scrim_color.3 as f32 * scrim_progress.get().clamp(0.0, 1.0)) as u8;
                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.scrim_color.with_alpha(scrim_alpha))
                        .input_blocker()
                        .focusable(false)
                        .on_scroll(|_| Vec2::default())
                        .on_pointer_down({
                            let s = state.clone();
                            move |_| s.dismiss()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, sheet))
                }
            });

            let back_state = state.clone();
            let back_handler: Rc<dyn Fn() -> bool> = Rc::new(move || {
                back_state.dismiss();
                true
            });
            *overlay_guard.borrow_mut() =
                Some(overlay.show_guard_with_back(builder, 800.0, false, back_handler));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::runtime::ComposeGuard;
    use repose_core::scope::Scope;
    use repose_ui::Text;
    use repose_ui::layout::LayoutEngine;
    use repose_ui::overlay::{OverlayHandle, with_ambient_overlay};
    use std::collections::HashMap;

    #[test]
    fn modal_sheet_release_target_uses_velocity_direction() {
        assert_eq!(
            modal_sheet_release_target(30.0, 600.0, 100.0, 56.0, 125.0),
            0.0
        );
        assert_eq!(
            modal_sheet_release_target(100.0, 600.0, 0.0, 56.0, 125.0),
            0.0
        );
        assert_eq!(
            modal_sheet_release_target(301.0, 600.0, 0.0, 56.0, 125.0),
            600.0
        );
        assert_eq!(
            modal_sheet_release_target(590.0, 600.0, -50.0, 56.0, 125.0),
            600.0
        );
        assert_eq!(
            modal_sheet_release_target(500.0, 600.0, -50.0, 56.0, 125.0),
            0.0
        );
        assert_eq!(
            modal_sheet_release_target(100.0, 600.0, 200.0, 56.0, 125.0),
            600.0
        );
    }

    #[test]
    fn modal_sheet_offset_preserves_spring_overshoot() {
        assert_eq!(modal_sheet_offset(-100.0, 600.0), -100.0);
        assert_eq!(modal_sheet_offset(250.0, 600.0), 250.0);
        assert_eq!(modal_sheet_offset(700.0, 600.0), 700.0);
        assert_eq!(modal_sheet_drag_offset(-100.0, 600.0), 0.0);
        assert_eq!(modal_sheet_drag_offset(700.0, 600.0), 600.0);
    }

    #[test]
    fn modal_sheet_is_bottom_anchored_with_intrinsic_height() {
        let overlay = OverlayHandle::new();
        let state = Rc::new(SheetState::new(Dp(56.0)));
        state.show();
        let config = BottomSheetConfig::default();
        let container_color = config.container_color;
        let scope = Scope::new();
        let guard = ComposeGuard::begin();
        scope.run(|| {
            let view = with_ambient_overlay(overlay.clone(), || {
                ModalBottomSheet(
                    state.clone(),
                    Modifier::new(),
                    Text("Sheet"),
                    config.clone(),
                )
            });
            let root = overlay.host(Modifier::new().fill_max_size(), view);
            let (scene, _, _) = LayoutEngine::new().layout_frame(
                &root,
                (800, 600),
                &HashMap::new(),
                &repose_ui::Interactions::default(),
                None,
            );
            let sheet = scene
                .nodes
                .iter()
                .find_map(|node| match node {
                    SceneNode::Rect {
                        rect,
                        brush: Brush::Solid(color),
                        ..
                    } if *color == container_color => Some(*rect),
                    _ => None,
                })
                .expect("modal sheet surface");
            assert!(sheet.h < 600.0);
            assert!((sheet.y + sheet.h - 600.0).abs() < 1.0);
            assert!((sheet.x - (800.0 - sheet.w) * 0.5).abs() < 1.0);
        });
        drop(guard);
        scope.dispose();
    }
}
