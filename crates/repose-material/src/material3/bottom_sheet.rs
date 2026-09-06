#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::animation::AnimationSpec;
use repose_core::*;
use repose_ui::{
    Box, Column, Row, ViewExt, ZStack, anim::animate_f32_from, overlay::OverlayGuard,
    overlay::OverlayHandle,
};

use super::*;

/// Configuration for [`BottomSheet`] / `ModalBottomSheet`.
#[derive(Clone, Debug)]
pub struct BottomSheetConfig {
    pub container_color: Color,
    pub content_color: Color,
    pub scrim_color: Color,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub drag_handle_color: Color,
    pub shape_radius: f32,
    pub max_width: f32,
    pub drag_handle_width: f32,
    pub drag_handle_height: f32,
    pub peek_height: f32,
    pub gestures_enabled: bool,
}

impl Default for BottomSheetConfig {
    fn default() -> Self {
        Self {
            container_color: BottomSheetDefaults::container_color(),
            content_color: BottomSheetDefaults::content_color(),
            scrim_color: BottomSheetDefaults::scrim_color(),
            tonal_elevation: BottomSheetDefaults::TONAL_ELEVATION,
            shadow_elevation: 0.0,
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
    let id = remember(unique_component_id);

    let opacity = animate_f32_from(
        format!("bs_opacity_{id}"),
        if visible { 0.0 } else { 1.0 },
        if visible { 1.0 } else { 0.0 },
        th.motion.layout,
    );

    let keep = visible || opacity > 0.01;
    if !keep {
        return Box(Modifier::new());
    }
    Column(Modifier::new().fill_max_width()).child((
        Box(modifier
            .alpha(opacity)
            .background(config.container_color)
            .clip_rounded(config.shape_radius))
        .child(with_content_color(config.content_color, move || content)),
        Box(Modifier::new()
            .width(1.0)
            .height(0.0)
            .fill_max_width()
            .alpha(opacity)
            .hit_passthrough()
            .on_pointer_down(move |_| on_dismiss())),
    ))
}

/// State for `ModalBottomSheet` - manages visibility and drag offset.
pub struct SheetState {
    visible: Signal<bool>,
    drag_offset: Signal<f32>,
    peek_height: Signal<f32>,
}

impl SheetState {
    pub fn new(peek_height: f32) -> Self {
        Self {
            visible: signal(false),
            drag_offset: signal(0.0),
            peek_height: signal(peek_height),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
        self.drag_offset.set(0.0);
    }

    pub fn set_peek_height(&self, h: f32) {
        self.peek_height.set(h);
    }
}

/// M3 Modal Bottom Sheet - slides up from the bottom with a drag handle.
///
/// Renders as an overlay so it is not clipped by parent containers.
/// Shows on `state.show()`, dismisses on `state.dismiss()` or scrim tap.
pub fn ModalBottomSheet(
    state: Rc<SheetState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    content: View,
    config: BottomSheetConfig,
) -> View {
    let th = theme();
    let peek_h = state.peek_height.get().max(config.peek_height);
    let anim_distance = peek_h.max(48.0).max(400.0);
    let overlay_guard = remember_with_key("mbs_oguard", || RefCell::new(None::<OverlayGuard>));

    // Fresh content each composition (builder captures content once).
    let current_content = remember_state_with_key("mbs_c", || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    // Drag state -> offset_at_drag_start is the anim value when the drag began
    let drag_anchor_y: Rc<RefCell<f32>> = remember_state_with_key("mbs_drag_y", || 0.0);
    let offset_at_drag_start: Rc<RefCell<f32>> = remember_state_with_key("mbs_drag_base", || 0.0);
    let is_dragging: Rc<RefCell<bool>> = remember_state_with_key("mbs_drag", || false);

    // Animated offset: anim_distance px (off-screen) -> 0px (visible)
    let anim = remember_state_with_key("mbs_anim", || {
        AnimatedValue::new(anim_distance, theme().motion.spring)
    });
    let last_target = remember_state_with_key("mbs_anim_target", || f32::NAN);
    let anim_target = if state.is_visible() {
        0.0
    } else {
        anim_distance
    };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            if state.is_visible() {
                a.set_spec(th.motion.spring);
            } else {
                a.set_spec(AnimationSpec::fast());
            }
            a.set_target(anim_target);
            *lt = anim_target;
        }
        drop(lt);
        let still_animating = a.update();
        if still_animating {
            request_frame();
        }
    }

    let offset = *anim.borrow().get();
    let sheet_visible = state.is_visible() || offset < anim_distance - 10.0;

    if sheet_visible {
        if overlay_guard.borrow().is_none() {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let modifier = modifier.clone();
                let current_content = current_content.clone();
                let drag_anchor_y = drag_anchor_y.clone();
                let offset_at_drag_start = offset_at_drag_start.clone();
                let is_dragging = is_dragging.clone();
                let anim_distance = anim_distance;
                move || {
                    let off = *anim.borrow().get();
                    let content = current_content.borrow().clone();

                    let mut sheet_mod = modifier
                        .clone()
                        .fill_max_width()
                        .max_width(dp_to_px(config.max_width))
                        .translate(0.0, off)
                        .background(config.container_color)
                        .clip_rounded(config.shape_radius);

                    if config.gestures_enabled {
                        sheet_mod = sheet_mod
                            .on_pointer_down({
                                let anim = anim.clone();
                                let drag_anchor_y = drag_anchor_y.clone();
                                let offset_at_drag_start = offset_at_drag_start.clone();
                                let is_dragging = is_dragging.clone();
                                move |ev| {
                                    *drag_anchor_y.borrow_mut() = ev.position.y;
                                    *offset_at_drag_start.borrow_mut() = *anim.borrow().get();
                                    *is_dragging.borrow_mut() = true;
                                }
                            })
                            .on_pointer_move({
                                let anim = anim.clone();
                                let drag_anchor_y = drag_anchor_y.clone();
                                let offset_at_drag_start = offset_at_drag_start.clone();
                                let is_dragging = is_dragging.clone();
                                move |ev| {
                                    if !*is_dragging.borrow() {
                                        return;
                                    }
                                    let delta = ev.position.y - *drag_anchor_y.borrow();
                                    let start_off = *offset_at_drag_start.borrow();
                                    let total = (start_off + delta).max(0.0);
                                    anim.borrow_mut().snap_to(total);
                                    request_frame();
                                }
                            })
                            .on_pointer_up({
                                let anim = anim.clone();
                                let is_dragging = is_dragging.clone();
                                let state = state.clone();
                                let anim_distance = anim_distance;
                                move |_| {
                                    *is_dragging.borrow_mut() = false;
                                    let current_off = *anim.borrow().get();
                                    let threshold = anim_distance * 0.3;
                                    if current_off > threshold {
                                        anim.borrow_mut().set_target(anim_distance);
                                        state.dismiss();
                                    } else {
                                        anim.borrow_mut().set_target(0.0);
                                    }
                                }
                            });
                    }

                    let sheet_body = Box(sheet_mod).child(
                        Column(Modifier::new().fill_max_width()).child((
                            Row(Modifier::new()
                                .fill_max_width()
                                .justify_content(JustifyContent::CENTER))
                            .child(Box(Modifier::new()
                                .margin_vertical(22.0)
                                .width(config.drag_handle_width)
                                .height(config.drag_handle_height)
                                .background(config.drag_handle_color)
                                .clip_rounded(2.0))),
                            content,
                        )),
                    );

                    let sheet = Box(Modifier::new()
                        .fill_max_size()
                        .justify_content(JustifyContent::CENTER)
                        .align_items(AlignItems::FLEX_END))
                    .child(sheet_body);

                    let scrim_alpha = if state.is_visible() {
                        config.scrim_color.3
                    } else {
                        let t = (off / anim_distance).clamp(0.0, 1.0);
                        (config.scrim_color.3 as f32 * (1.0 - t)) as u8
                    };
                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.scrim_color.with_alpha(scrim_alpha))
                        .input_blocker()
                        .on_scroll(|_| Vec2::default())
                        .on_pointer_down({
                            let s = state.clone();
                            move |_| s.dismiss()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, sheet))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 900.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}
