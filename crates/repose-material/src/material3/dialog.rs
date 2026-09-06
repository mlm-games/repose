#![allow(non_snake_case)]

use std::rc::Rc;

use std::cell::RefCell;

use repose_core::*;
use repose_ui::overlay::{OverlayGuard, OverlayHandle};
use repose_ui::{Box, Column, ViewExt, ZStack, box_with_constraints_with_key};
use web_time::Duration;

use super::AlertDialogDefaults;
use super::{DatePicker, DatePickerConfig, DatePickerState};
use super::{TimePicker, TimePickerConfig, TimePickerState};

/// State controlling dialog visibility.
pub struct DialogState {
    visible: Signal<bool>,
    id: u64,
}

impl Default for DialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl DialogState {
    pub fn new() -> Self {
        Self {
            visible: signal(false),
            id: unique_component_id(),
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("dlg_{}_{}", self.id, suffix)
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// Configuration for dialog dismiss behavior.
/// Mirrors Compose's `DialogProperties`.
#[derive(Clone)]
pub struct DialogProperties {
    /// Called when the user attempts to dismiss the dialog
    /// (scrim click, Escape/Back press). When set, this overrides `state.dismiss()`.
    /// To make a dialog that never closes, pass `Some(Rc::new(|| {}))`.
    pub on_dismiss_request: Option<Rc<dyn Fn()>>,
    /// Whether clicking the scrim (outside the dialog surface) triggers dismissal.
    /// Default: `true`.
    pub dismiss_on_click_outside: bool,
    /// Whether pressing Escape (or Back gesture) triggers dismissal.
    /// Default: `true`.
    pub dismiss_on_back_press: bool,
    /// Compose `usePlatformDefaultWidth`. Default: true.
    pub use_platform_default_width: bool,
    /// Compose `usePlatformInsets` (+ IME). Default: true.
    pub use_platform_insets: bool,
}

impl Default for DialogProperties {
    fn default() -> Self {
        Self {
            on_dismiss_request: None,
            dismiss_on_click_outside: true,
            dismiss_on_back_press: true,
            use_platform_default_width: true,
            use_platform_insets: true,
        }
    }
}

fn preferred_dialog_width_dp(container_w: f32, container_h: f32) -> f32 {
    let smallest = container_w.min(container_h);
    if smallest >= 600.0 {
        super::DialogDefaults::PREFERRED_WIDTH_EXPANDED
    } else if smallest >= 480.0 {
        super::DialogDefaults::PREFERRED_WIDTH_MEDIUM
    } else {
        super::DialogDefaults::PREFERRED_WIDTH_COMPACT
    }
}

/// After merging caller modifiers, clamp size so the dialog can never escape the viewport.
fn clamp_dialog_modifier(mut m: Modifier, platform_max_w: f32, platform_max_h: f32) -> Modifier {
    let max_w = m
        .max_width
        .unwrap_or(platform_max_w)
        .min(platform_max_w)
        .max(0.0);
    let max_h = m
        .max_height
        .unwrap_or(platform_max_h)
        .min(platform_max_h)
        .max(0.0);
    m.max_width = Some(max_w);
    m.max_height = Some(max_h);

    // Compose Constraints: min cannot exceed max.
    if let Some(min_w) = m.min_width {
        m.min_width = Some(min_w.min(max_w).max(0.0));
    } else {
        m.min_width = Some(super::DialogDefaults::MIN_WIDTH.min(max_w).max(0.0));
    }
    if let Some(min_h) = m.min_height {
        m.min_height = Some(min_h.min(max_h).max(0.0));
    }
    m
}

/// A modal dialog rendered in the overlay layer with scrim and spring animation.
///
/// Unlike the inline `AlertDialog`, this version renders outside the layout tree
/// so it is never clipped by parent containers, scroll areas, or stacks.
///
/// Caller should create a `DialogState` and manage visibility via `show()`/`dismiss()`.
///
/// Focus behavior: dialog content is wrapped in a focus group, so Tab/Shift+Tab
/// cycles within the dialog instead of moving to background elements.
///
/// Escape handling: when the dialog content is focused and `dismiss_on_back_press`
/// is true, pressing Escape calls `on_dismiss_request` (or `state.dismiss()` if
/// no `on_dismiss_request` is set). Set `dismiss_on_back_press = false` or pass
/// `on_dismiss_request = Some(Rc::new(|| {}))` to prevent Escape from closing.
pub fn Dialog(
    state: Rc<DialogState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    properties: DialogProperties,
    content: View,
) -> View {
    let overlay_guard =
        remember_with_key(state.key("oguard"), || RefCell::new(None::<OverlayGuard>));

    let current_content = remember_state_with_key(state.key("c"), || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    // Store properties so the overlay closure reads fresh values each frame
    let props = remember_state_with_key(state.key("p"), || properties.clone());
    *props.borrow_mut() = properties;

    let current_modifier = remember_state_with_key(state.key("m"), Modifier::new);
    *current_modifier.borrow_mut() = modifier;

    let scroll_state: Rc<repose_core::scroll::ScrollState> =
        remember_with_key(state.key("scroll"), repose_core::scroll::ScrollState::new);

    let platform_state: Rc<RefCell<(f32, f32, PaddingValues)>> =
        remember_with_key(state.key("plat"), || {
            RefCell::new((
                super::DialogDefaults::MAX_WIDTH,
                800.0,
                PaddingValues::default(),
            ))
        });

    let spec = AnimationSpec::tween(Duration::from_millis(200), Easing::FastOutSlowIn);
    let anim = remember_state_with_key(state.key("anim"), || AnimatedValue::new(0.0, spec));
    let last_target = remember_state_with_key(state.key("atarget"), || f32::NAN);
    let anim_target = if state.is_visible() { 1.0 } else { 0.0 };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            a.set_spec(spec);
            a.set_target(anim_target);
            *lt = anim_target;
        }
        drop(lt);
        if a.update() {
            request_frame();
        }
    }

    let progress = *anim.borrow().get();
    let visible = state.is_visible() || progress > 0.01;

    if visible {
        if overlay_guard.borrow().is_none() {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let current_modifier = current_modifier.clone();
                let current_content = current_content.clone();
                let props = props.clone();
                let scroll_state = scroll_state.clone();
                move || {
                    let progress_outer = *anim.borrow().get();
                    let alpha_outer = progress_outer.min(1.0);
                    let scrim_color = AlertDialogDefaults::scrim_color();
                    let scrim_alpha = (scrim_color.3 as f32 / 255.0) * alpha_outer;
                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(scrim_color.with_alpha_f32(scrim_alpha.clamp(0.0, 1.0)))
                        .focusable(false)
                        .input_blocker()
                        .on_scroll(|_| Vec2::default())
                        .on_click({
                            let s = state.clone();
                            let props = props.clone();
                            move || {
                                let (dismiss, cb) = {
                                    let p = props.borrow();
                                    (p.dismiss_on_click_outside, p.on_dismiss_request.clone())
                                };
                                if dismiss {
                                    if let Some(cb) = cb {
                                        cb();
                                    } else {
                                        s.dismiss();
                                    }
                                }
                            }
                        }));

                    let p_for_measure = props.clone();
                    let platform_state_for_measure = platform_state.clone();
                    let props_snap = p_for_measure.borrow().clone();
                    let insets_snap = window_insets();
                    let measure_key = {
                        use std::hash::{Hash, Hasher};
                        let mut h = std::collections::hash_map::DefaultHasher::new();
                        props_snap.use_platform_default_width.hash(&mut h);
                        props_snap.use_platform_insets.hash(&mut h);
                        insets_snap.left.to_bits().hash(&mut h);
                        insets_snap.right.to_bits().hash(&mut h);
                        insets_snap.top.to_bits().hash(&mut h);
                        insets_snap.bottom.to_bits().hash(&mut h);
                        insets_snap.ime_bottom.to_bits().hash(&mut h);
                        h.finish()
                    };
                    let measure = box_with_constraints_with_key(
                        measure_key,
                        Modifier::new().fill_max_size().hit_passthrough(),
                        move |scope| {
                            let p = p_for_measure.borrow().clone();
                            let mut pad = PaddingValues::default();
                            if p.use_platform_insets {
                                let insets = window_insets();
                                pad.left = px_to_dp(insets.left);
                                pad.right = px_to_dp(insets.right);
                                pad.top = px_to_dp(insets.top);
                                pad.bottom = px_to_dp(insets.bottom) + px_to_dp(insets.ime_bottom);
                            }
                            let win_w = if scope.max_width.is_finite() && scope.max_width > 10.0 {
                                scope.max_width
                            } else {
                                1280.0
                            };
                            let win_h = if scope.max_height.is_finite() && scope.max_height > 10.0 {
                                scope.max_height
                            } else {
                                800.0
                            };
                            let avail_w = (win_w - pad.left - pad.right).max(0.0);
                            let avail_h = (win_h - pad.top - pad.bottom).max(0.0);
                            let platform_max_w = if p.use_platform_default_width {
                                preferred_dialog_width_dp(win_w, win_h)
                                    .min(avail_w)
                                    .min(super::DialogDefaults::MAX_WIDTH)
                            } else {
                                avail_w.min(super::DialogDefaults::MAX_WIDTH)
                            };
                            *platform_state_for_measure.borrow_mut() =
                                (platform_max_w, avail_h, pad);
                            Box(Modifier::new().size(0.0, 0.0))
                        },
                    );

                    let content = current_content.borrow().clone();
                    let progress = *anim.borrow().get();
                    let alpha = progress.min(1.0);
                    let scale = 0.8 + 0.2 * progress;
                    let th = theme();

                    let (platform_max_w, platform_max_h, pad) = *platform_state.borrow();

                    let dialog_mod = clamp_dialog_modifier(
                        Modifier::new()
                            .min_width(super::DialogDefaults::MIN_WIDTH)
                            .max_width(super::DialogDefaults::MAX_WIDTH)
                            .then(current_modifier.borrow().clone())
                            .justify_content(JustifyContent::CENTER)
                            .background(th.surface_container_high)
                            .clip_rounded(th.shapes.extra_large)
                            .alpha(alpha)
                            .scale(scale)
                            .focus_group()
                            .clickable()
                            .focusable(false)
                            .on_key_event({
                                let s = state.clone();
                                let props2 = props.clone();
                                move |ke| {
                                    use repose_core::input::{Key, KeyEventType};
                                    if ke.key == Key::Escape && ke.event_type == KeyEventType::Down
                                    {
                                        let (dismiss, cb) = {
                                            let p = props2.borrow();
                                            (p.dismiss_on_back_press, p.on_dismiss_request.clone())
                                        };
                                        if dismiss {
                                            if let Some(cb) = cb {
                                                cb();
                                            } else {
                                                s.dismiss();
                                            }
                                            return true;
                                        }
                                    }
                                    false
                                }
                            }),
                        platform_max_w,
                        platform_max_h,
                    );

                    let axis_binding = match scroll_state.to_binding() {
                        repose_core::scroll::ScrollBinding::Vertical(a) => a,
                        _ => unreachable!(),
                    };
                    let scrollable_body = Box(Modifier::new()
                        .fill_max_width()
                        .max_height(platform_max_h)
                        .vertical_scroll(axis_binding))
                    .child(content);

                    let dialog = Box(dialog_mod).child(scrollable_body);

                    let dialog_container = Box(Modifier::new()
                        .fill_max_size()
                        .padding_values(pad)
                        // Safe centering: an oversized dialog stays reachable
                        // instead of overflowing past both viewport edges.
                        .justify_content(JustifyContent::SAFE_CENTER)
                        .align_items(AlignItems::SAFE_CENTER)
                        .hit_passthrough())
                    .child(dialog);

                    ZStack(Modifier::new().fill_max_size().absolute()).child((
                        scrim,
                        measure,
                        dialog_container,
                    ))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 900.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

/// Configuration for alert dialog.
#[derive(Clone, Debug)]
pub struct AlertDialogConfig {
    pub modifier: Modifier,
    pub scrim_color: Color,
    pub min_width: f32,
    pub max_width: f32,
    pub horizontal_padding: f32,
    pub shape_radius: Option<f32>,
    pub container_color: Color,
    pub tonal_elevation: f32,
}

impl Default for AlertDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            scrim_color: AlertDialogDefaults::scrim_color(),
            min_width: AlertDialogDefaults::MIN_WIDTH,
            max_width: AlertDialogDefaults::MAX_WIDTH,
            horizontal_padding: AlertDialogDefaults::HORIZONTAL_PADDING,
            shape_radius: None,
            container_color: theme().surface_container_high,
            tonal_elevation: 0.0,
        }
    }
}

/// An improved AlertDialog using the overlay-based `Dialog`.
///
/// Shows a centered modal surface with title, text, confirm button, and optional
/// dismiss button. Managed via a shared `DialogState`.
pub fn AlertDialog(
    state: Rc<DialogState>,
    overlay: OverlayHandle,
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
    config: AlertDialogConfig,
) -> View {
    let content = Box(Modifier::new()
        .background(config.container_color)
        .clip_rounded(
            config
                .shape_radius
                .unwrap_or_else(|| theme().shapes.extra_large),
        ))
    .child(super::alert_dialog_body(
        title,
        text,
        confirm_button,
        dismiss_button,
    ));

    Dialog(
        state,
        overlay,
        Modifier::new()
            .min_width(config.min_width)
            .max_width(config.max_width)
            .then(config.modifier),
        DialogProperties::default(),
        content,
    )
}

/// Configuration for [`DatePickerDialog`].
#[derive(Clone)]
pub struct DatePickerDialogConfig {
    pub modifier: Modifier,
    pub shape_radius: Option<f32>,
    pub colors: super::DatePickerColors,
}

impl Default for DatePickerDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            shape_radius: None,
            colors: super::DatePickerColors::default(),
        }
    }
}

/// M3 Date Picker Dialog - wraps [`DatePicker`] inside a modal [`Dialog`]
/// with confirm/cancel buttons. Equivalent to Compose's `DatePickerDialog`.
///
/// The `on_confirm` callback fires when a day is clicked or the OK button is pressed.
/// The `on_dismiss` fires on Cancel or scrim tap.
pub fn DatePickerDialog(
    state: Rc<DialogState>,
    overlay: OverlayHandle,
    picker_state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: DatePickerDialogConfig,
) -> View {
    let content = Box(Modifier::new()
        .background(config.colors.container_color)
        .clip_rounded(
            config
                .shape_radius
                .unwrap_or_else(|| theme().shapes.extra_large),
        ))
    .child(Column(Modifier::new()).child((DatePicker(
        picker_state.clone(),
        on_confirm,
        on_dismiss,
        DatePickerConfig {
            colors: config.colors,
            ..DatePickerConfig::default()
        },
    ),)));

    Dialog(
        state,
        overlay,
        config.modifier,
        DialogProperties::default(),
        content,
    )
}

/// Configuration for [`TimePickerDialog`].
#[derive(Clone)]
pub struct TimePickerDialogConfig {
    pub modifier: Modifier,
    pub shape_radius: Option<f32>,
    pub container_color: Color,
    pub colors: super::TimePickerColors,
}

impl Default for TimePickerDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            shape_radius: None,
            container_color: theme().surface_container_high,
            colors: super::TimePickerColors::default(),
        }
    }
}

/// M3 Time Picker Dialog - wraps [`TimePicker`] inside a modal [`Dialog`]
/// with confirm/cancel buttons. Equivalent to Compose's `TimePickerDialog`.
///
/// The `on_confirm` callback fires when OK is pressed.
/// The `on_dismiss` fires on Cancel or scrim tap.
pub fn TimePickerDialog(
    state: Rc<DialogState>,
    overlay: OverlayHandle,
    picker_state: Rc<TimePickerState>,
    on_confirm: Rc<dyn Fn(u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: TimePickerDialogConfig,
) -> View {
    let content = Box(Modifier::new()
        .background(config.container_color)
        .clip_rounded(
            config
                .shape_radius
                .unwrap_or_else(|| theme().shapes.extra_large),
        ))
    .child(Column(Modifier::new()).child((TimePicker(
        picker_state.clone(),
        on_confirm,
        on_dismiss,
        TimePickerConfig {
            colors: config.colors,
            ..TimePickerConfig::default()
        },
    ),)));

    Dialog(
        state,
        overlay,
        config.modifier,
        DialogProperties::default(),
        content,
    )
}
