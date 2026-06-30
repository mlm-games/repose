#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::overlay::OverlayHandle;
use repose_ui::{Box, Column, Row, Spacer, Text, ViewExt, ZStack};
use web_time::Duration;

use super::{AlertDialogDefaults, Button, ButtonConfig, TextButton};
use super::{DatePicker, DatePickerState};
use super::{TimePicker, TimePickerState};

static DIALOG_COUNTER: AtomicU64 = AtomicU64::new(0);

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
            id: DIALOG_COUNTER.fetch_add(1, Ordering::Relaxed),
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

/// A modal dialog rendered in the overlay layer with scrim and spring animation.
///
/// Unlike the inline `AlertDialog`, this version renders outside the layout tree
/// so it is never clipped by parent containers, scroll areas, or stacks.
///
/// Caller should create a `DialogState` and manage visibility via `show()`/`dismiss()`.
pub fn Dialog(
    state: Rc<DialogState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    content: View,
) -> View {
    let overlay_id = remember_with_key(state.key("oid"), || signal(0u64));

    // RefCell holding the latest content so the overlay builder reads fresh state each frame
    let current_content = remember_state_with_key(state.key("c"), || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    // Animated scale/alpha for enter/exit
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
        if overlay_id.get() == 0 {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let modifier = modifier.clone();
                let current_content = current_content.clone();
                move || {
                    let progress = *anim.borrow().get();
                    let alpha = progress.min(1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();

                    let dialog = Box(Modifier::new()
                        .min_width(280.0)
                        .max_width(560.0)
                        .then(modifier.clone())
                        .justify_content(JustifyContent::Center)
                        .background(th.surface_container_high)
                        .clip_rounded(th.shapes.extra_large)
                        .alpha(alpha))
                    .child(content);

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(th.scrim.with_alpha((85.0 * alpha) as u8))
                        .on_click({
                            let s = state.clone();
                            move || s.dismiss()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((
                        scrim,
                        Box(Modifier::new()
                            .fill_max_size()
                            .justify_content(JustifyContent::Center)
                            .align_items(AlignItems::Center)
                            .hit_passthrough())
                        .child(dialog),
                    ))
                }
            });

            let id = overlay.show_entry(builder, 900.0, false);
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
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
}

impl Default for AlertDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            scrim_color: AlertDialogDefaults::scrim_color(),
            min_width: AlertDialogDefaults::MIN_WIDTH,
            max_width: AlertDialogDefaults::MAX_WIDTH,
            horizontal_padding: AlertDialogDefaults::HORIZONTAL_PADDING,
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
    Dialog(
        state,
        overlay,
        Modifier::new()
            .min_width(config.min_width)
            .max_width(config.max_width)
            .then(config.modifier),
        super::alert_dialog_body(title, text, confirm_button, dismiss_button),
    )
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
) -> View {
    Dialog(
        state,
        overlay,
        Modifier::new(),
        Column(Modifier::new()).child((DatePicker(picker_state.clone(), on_confirm, on_dismiss),)),
    )
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
) -> View {
    Dialog(
        state,
        overlay,
        Modifier::new(),
        Column(Modifier::new()).child((TimePicker(picker_state.clone(), on_confirm, on_dismiss),)),
    )
}
