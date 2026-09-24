#![allow(non_snake_case)]

use std::rc::Rc;

use std::cell::{Cell, RefCell};

use repose_core::*;
use repose_ui::overlay::{OverlayGuard, ambient_overlay};
use repose_ui::{Box, Column, ViewExt, ZStack, box_with_constraints_with_key};
use web_time::Duration;

use super::AlertDialogDefaults;
use super::{DatePicker, DatePickerConfig, DatePickerState};
use super::{TimePicker, TimePickerConfig, TimePickerState};

/// State controlling dialog visibility.
pub struct DialogState {
    visible: Signal<bool>,
    id: u64,
    scrim_color: RefCell<Option<Color>>,
    opener_focus: Rc<RefCell<Option<FocusRequester>>>,
    focus_disposer: RefCell<Option<Dispose>>,
}

impl Default for DialogState {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for DialogState {
    fn drop(&mut self) {
        if let Some(disposer) = self.focus_disposer.borrow_mut().take() {
            disposer.run();
        }
    }
}

impl DialogState {
    pub fn new() -> Self {
        Self {
            visible: signal(false),
            id: unique_component_id(),
            scrim_color: RefCell::new(None),
            opener_focus: Rc::new(RefCell::new(None)),
            focus_disposer: RefCell::new(None),
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("dlg_{}_{}", self.id, suffix)
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn show(&self) {
        self.visible.set_neq(true);
    }

    pub fn dismiss(&self) {
        self.visible.set_neq(false);
    }

    pub fn set_opener_focus(&self, requester: FocusRequester) {
        *self.opener_focus.borrow_mut() = Some(requester);
    }

    pub fn show_from(&self, opener: FocusRequester) {
        self.set_opener_focus(opener);
        self.show();
    }

    pub(crate) fn restore_opener_focus(&self) -> bool {
        let Some(requester) = self.opener_focus.borrow_mut().take() else {
            return false;
        };
        FocusManager::new(Vec::new(), None).clear_focus(false);
        requester.request_focus();
        true
    }

    pub(crate) fn set_scrim_color(&self, color: Option<Color>) {
        *self.scrim_color.borrow_mut() = color;
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

fn preferred_dialog_width_dp(container_w: Dp, container_h: Dp) -> Dp {
    let smallest = container_w.min(container_h);
    if smallest.0 >= 600.0 {
        super::DialogDefaults::PREFERRED_WIDTH_EXPANDED
    } else if smallest.0 >= 480.0 {
        super::DialogDefaults::PREFERRED_WIDTH_MEDIUM
    } else {
        super::DialogDefaults::PREFERRED_WIDTH_COMPACT
    }
}

/// After merging caller modifiers, clamp size so the dialog can never escape the viewport.
fn clamp_dialog_modifier(mut m: Modifier, platform_max_w: Dp, platform_max_h: Dp) -> Modifier {
    let max_w = m
        .max_width
        .unwrap_or(platform_max_w)
        .min(platform_max_w)
        .max(Dp::ZERO);
    let max_h = m
        .max_height
        .unwrap_or(platform_max_h)
        .min(platform_max_h)
        .max(Dp::ZERO);
    m.max_width = Some(max_w);
    m.max_height = Some(max_h);

    // Compose Constraints: min cannot exceed max.
    if let Some(min_w) = m.min_width {
        m.min_width = Some(min_w.min(max_w).max(Dp::ZERO));
    } else {
        m.min_width = Some(super::DialogDefaults::MIN_WIDTH.min(max_w).max(Dp::ZERO));
    }
    if let Some(min_h) = m.min_height {
        m.min_height = Some(min_h.min(max_h).max(Dp::ZERO));
    }
    m
}

fn attach_focus_requester(
    view: &mut View,
    requester: &FocusRequester,
    accepted: Option<&Rc<Cell<bool>>>,
) -> bool {
    let modifier = &view.modifier;
    let enabled = !modifier.disabled
        && modifier
            .text_input
            .as_ref()
            .map(|input| input.enabled)
            .unwrap_or(true);
    let focusable = modifier
        .focusable
        .unwrap_or(modifier.click || modifier.on_action.is_some() || modifier.text_input.is_some());
    let candidate = enabled
        && focusable
        && (modifier.text_input.is_some()
            || modifier.on_action.is_some()
            || modifier.click
            || modifier.focusable == Some(true));
    if candidate {
        let previous_callback = view.modifier.on_focus_changed.take();
        let accepted = accepted.cloned();
        view.modifier.on_focus_changed = Some(Rc::new(move |focused| {
            if focused && let Some(accepted) = &accepted {
                accepted.set(false);
            }
            if let Some(callback) = &previous_callback {
                callback(focused);
            }
        }));
        view.modifier.focus_requester = Some(requester.clone());
        return true;
    }
    view.children
        .iter_mut()
        .any(|child| attach_focus_requester(child, requester, accepted))
}

fn dialog_preview_key(
    state: Rc<DialogState>,
    props: Rc<RefCell<DialogProperties>>,
    event: KeyEvent,
) -> bool {
    let action = repose_core::shortcuts::resolve_action(repose_core::shortcuts::KeyChord::new(
        event.key.clone(),
        event.modifiers,
    ));
    let is_back =
        event.key == Key::Escape || matches!(action, Some(repose_core::shortcuts::Action::Back));
    if is_back {
        if event.event_type == KeyEventType::Down && !event.is_repeat {
            let (dismiss, callback) = {
                let p = props.borrow();
                (p.dismiss_on_back_press, p.on_dismiss_request.clone())
            };
            if dismiss {
                if let Some(callback) = callback {
                    callback();
                } else {
                    state.dismiss();
                }
            }
        }
        return true;
    }
    if event.event_type != KeyEventType::Down || event.is_repeat {
        return false;
    }
    match action {
        Some(
            repose_core::shortcuts::Action::Copy
            | repose_core::shortcuts::Action::Cut
            | repose_core::shortcuts::Action::Paste
            | repose_core::shortcuts::Action::SelectAll
            | repose_core::shortcuts::Action::Undo
            | repose_core::shortcuts::Action::Redo
            | repose_core::shortcuts::Action::FocusNext
            | repose_core::shortcuts::Action::FocusPrevious
            | repose_core::shortcuts::Action::FocusLeft
            | repose_core::shortcuts::Action::FocusRight
            | repose_core::shortcuts::Action::FocusUp
            | repose_core::shortcuts::Action::FocusDown,
        ) => false,
        Some(_) => true,
        None => false,
    }
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
    modifier: Modifier,
    properties: DialogProperties,
    content: View,
) -> View {
    let overlay = ambient_overlay();
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
    let focus_requester = remember_with_key(state.key("focus"), FocusRequester::new);
    let focus_pending: Rc<Cell<bool>> =
        remember_with_key(state.key("focus_pending"), || Cell::new(false));
    let focus_requested: Rc<Cell<bool>> =
        remember_with_key(state.key("focus_requested"), || Cell::new(false));
    let was_visible: Rc<Cell<bool>> =
        remember_with_key(state.key("was_visible"), || Cell::new(false));
    let visible_now = state.is_visible();
    if visible_now && !was_visible.get() {
        focus_pending.set(true);
        focus_requested.set(false);
        *focus_requester.target.borrow_mut() = None;
    } else if !visible_now && was_visible.get() {
        let owned_focus = !focus_pending.get();
        focus_pending.set(true);
        focus_requested.set(false);
        *focus_requester.target.borrow_mut() = None;
        if owned_focus && !state.restore_opener_focus() {
            FocusManager::new(Vec::new(), None).clear_focus(false);
        }
    }
    was_visible.set(visible_now);
    let focus_cleanup = {
        let focus_requester = focus_requester.clone();
        let focus_pending = focus_pending.clone();
        let opener_focus = state.opener_focus.clone();
        move || {
            on_unmount(move || {
                *focus_requester.target.borrow_mut() = None;
                if !focus_pending.get() {
                    FocusManager::new(Vec::new(), None).clear_focus(false);
                    if let Some(requester) = opener_focus.borrow_mut().take() {
                        requester.request_focus();
                    }
                }
                request_frame();
            })
        }
    };
    let disposer = if current_scope().is_some() {
        effect_once_with_key(state.key("focus_lifecycle"), focus_cleanup)
    } else {
        focus_cleanup()
    };
    *state.focus_disposer.borrow_mut() = Some(disposer);

    let platform_state: Rc<RefCell<(Dp, Dp, PaddingValues)>> =
        remember_with_key(state.key("plat"), || {
            let properties = props.borrow();
            let insets = window_insets();
            let mut pad = PaddingValues::default();
            if properties.use_platform_insets {
                pad.left = Px(insets.left).to_dp();
                pad.right = Px(insets.right).to_dp();
                pad.top = Px(insets.top).to_dp();
                pad.bottom = Px(insets.bottom).to_dp() + Px(insets.ime_bottom).to_dp();
            }
            let win_w = {
                let w = get_window_container_width();
                if w.is_finite() && w > 10.0 {
                    Dp(w)
                } else {
                    Dp(1280.0)
                }
            };
            let win_h = {
                let h = get_window_container_height();
                if h.is_finite() && h > 10.0 {
                    Dp(h)
                } else {
                    Dp(800.0)
                }
            };
            let avail_w = (win_w - pad.left - pad.right).max(Dp::ZERO);
            let avail_h = (win_h - pad.top - pad.bottom).max(Dp::ZERO);
            let platform_max_w = if properties.use_platform_default_width {
                preferred_dialog_width_dp(win_w, win_h)
                    .min(avail_w)
                    .min(super::DialogDefaults::MAX_WIDTH)
            } else {
                avail_w.min(super::DialogDefaults::MAX_WIDTH)
            };
            RefCell::new((platform_max_w, avail_h, pad))
        });

    let spec = AnimationSpec::tween(Duration::from_millis(200), Easing::FastOutSlowIn);
    let anim_key = state.key("anim");
    let anim = remember_state_with_key(anim_key.clone(), || AnimatedValue::new(0.0, spec));
    let last_target = remember_state_with_key(state.key("atarget"), || f32::NAN);
    let anim_target = if visible_now { 1.0 } else { 0.0 };

    {
        repose_core::animation_driver::touch(&anim_key);
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            a.set_spec(spec);
            a.set_target(anim_target);
            *lt = anim_target;
            drop(lt);
            drop(a);
            let reg_anim = anim.clone();
            repose_core::animation_driver::register(
                anim_key.clone(),
                Rc::new(RefCell::new(move || reg_anim.borrow_mut().update())),
            );
            request_frame();
        } else {
            let needs_reregister =
                !repose_core::animation_driver::is_registered(&anim_key) && a.is_animating();
            drop(lt);
            drop(a);
            if needs_reregister {
                let reg_anim = anim.clone();
                repose_core::animation_driver::register(
                    anim_key.clone(),
                    Rc::new(RefCell::new(move || reg_anim.borrow_mut().update())),
                );
                request_frame();
            }
        }
    }

    let progress = *anim.borrow().get();
    // HACK (compared to jetpack compose): First-frame kick
    if visible_now && progress < 0.01 {
        request_frame();
    }
    let visible = visible_now || progress > 0.01;

    if visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let current_modifier = current_modifier.clone();
                let current_content = current_content.clone();
                let props = props.clone();
                let scroll_state = scroll_state.clone();
                let focus_requester = focus_requester.clone();
                let focus_pending = focus_pending.clone();
                let focus_requested = focus_requested.clone();
                move || {
                    let progress_outer = *anim.borrow().get();
                    let alpha_outer = progress_outer.min(1.0);
                    let scrim_color = state
                        .scrim_color
                        .borrow()
                        .clone()
                        .unwrap_or_else(AlertDialogDefaults::scrim_color);
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
                                pad.left = Px(insets.left).to_dp();
                                pad.right = Px(insets.right).to_dp();
                                pad.top = Px(insets.top).to_dp();
                                pad.bottom =
                                    Px(insets.bottom).to_dp() + Px(insets.ime_bottom).to_dp();
                            }
                            let win_w = if scope.max_width.is_finite() && scope.max_width.0 > 10.0 {
                                scope.max_width
                            } else {
                                Dp(1280.0)
                            };
                            let win_h = if scope.max_height.is_finite() && scope.max_height.0 > 10.0
                            {
                                scope.max_height
                            } else {
                                Dp(800.0)
                            };
                            let avail_w = (win_w - pad.left - pad.right).max(Dp::ZERO);
                            let avail_h = (win_h - pad.top - pad.bottom).max(Dp::ZERO);
                            let platform_max_w = if p.use_platform_default_width {
                                preferred_dialog_width_dp(win_w, win_h)
                                    .min(avail_w)
                                    .min(super::DialogDefaults::MAX_WIDTH)
                            } else {
                                avail_w.min(super::DialogDefaults::MAX_WIDTH)
                            };
                            *platform_state_for_measure.borrow_mut() =
                                (platform_max_w, avail_h, pad);
                            Box(Modifier::new().size(Dp(0.0), Dp(0.0)))
                        },
                    );

                    let mut content = current_content.borrow().clone();
                    let progress = *anim.borrow().get();
                    let alpha = progress.min(1.0);
                    let scale = 0.8 + 0.2 * progress;
                    let th = theme();
                    let content = if attach_focus_requester(
                        &mut content,
                        &focus_requester,
                        Some(&focus_pending),
                    ) {
                        content
                    } else {
                        let focus_pending = focus_pending.clone();
                        Box(Modifier::new()
                            .focusable(true)
                            .focus_requester((*focus_requester).clone())
                            .on_focus_changed(move |focused| {
                                if focused {
                                    focus_pending.set(false);
                                }
                            }))
                        .child(content)
                    };
                    if focus_pending.get() && !focus_requested.get() {
                        if focus_requester.target.borrow().is_some() {
                            focus_requester.request_focus();
                            focus_requested.set(true);
                        } else {
                            request_frame();
                        }
                    }

                    let (platform_max_w, platform_max_h, pad) = *platform_state.borrow();

                    let dialog_mod = clamp_dialog_modifier(
                        Modifier::new()
                            .min_width(super::DialogDefaults::MIN_WIDTH)
                            .max_width(super::DialogDefaults::MAX_WIDTH)
                            .then(current_modifier.borrow().clone())
                            .justify_content(JustifyContent::CENTER)
                            .background(th.surface_container_high)
                            .clip_rounded(th.shapes.extra_large)
                            .graphics_layer(1.0)
                            .alpha(alpha)
                            .scale(scale)
                            .transform_origin(0.5, 0.5)
                            .focus_group()
                            .clickable()
                            .focusable(true)
                            .semantics(Semantics {
                                role: Role::Container,
                                label: Some("Dialog".into()),
                                ..Default::default()
                            })
                            .on_preview_key_event({
                                let s = state.clone();
                                let p = props.clone();
                                move |ke| dialog_preview_key(s.clone(), p.clone(), ke)
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
                    let focus_probe = {
                        let focus_requester = focus_requester.clone();
                        let focus_pending = focus_pending.clone();
                        let focus_requested = focus_requested.clone();
                        Box(Modifier::new()
                            .size(Dp(0.0), Dp(0.0))
                            .hit_passthrough()
                            .on_globally_positioned(move |_| {
                                if focus_pending.get()
                                    && !focus_requested.get()
                                    && focus_requester.target.borrow().is_some()
                                {
                                    focus_requester.request_focus();
                                    focus_requested.set(true);
                                }
                            }))
                    };

                    let dialog_container = Box(Modifier::new()
                        .fill_max_size()
                        .padding_values(pad)
                        // Safe centering: an oversized dialog stays reachable
                        // instead of overflowing past both viewport edges.
                        .justify_content(JustifyContent::SAFE_CENTER)
                        .align_items(AlignItems::SAFE_CENTER)
                        .hit_passthrough())
                    .child((dialog, focus_probe));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((
                        scrim,
                        measure,
                        dialog_container,
                    ))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 1000.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

/// Configuration for alert dialog (dimensions in [`Dp`]).
#[derive(Clone, Debug)]
pub struct AlertDialogConfig {
    pub modifier: Modifier,
    pub scrim_color: Color,
    pub min_width: Dp,
    pub max_width: Dp,
    pub horizontal_padding: Dp,
    pub shape_radius: Option<Dp>,
    pub container_color: Color,
    pub tonal_elevation: Dp,
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
            tonal_elevation: Dp::ZERO,
        }
    }
}

/// An improved AlertDialog using the overlay-based `Dialog`.
///
/// Shows a centered modal surface with title, text, confirm button, and optional
/// dismiss button. Managed via a shared `DialogState`.
pub fn AlertDialog(
    state: Rc<DialogState>,
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
    config: AlertDialogConfig,
) -> View {
    state.set_scrim_color(Some(config.scrim_color));

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
        config.horizontal_padding,
    ));

    Dialog(
        state,
        Modifier::new()
            .min_width(config.min_width)
            .max_width(config.max_width)
            .state_elevation(StateElevation {
                default: config.tonal_elevation,
                hovered: config.tonal_elevation,
                focused: config.tonal_elevation,
                pressed: config.tonal_elevation,
                dragged: config.tonal_elevation,
                disabled: Dp::ZERO,
            })
            .then(config.modifier),
        DialogProperties::default(),
        content,
    )
}

/// Configuration for [`DatePickerDialog`].
#[derive(Clone)]
pub struct DatePickerDialogConfig {
    pub modifier: Modifier,
    pub shape_radius: Option<Dp>,
    pub colors: super::DatePickerColors,
    pub confirm_label: String,
    pub dismiss_label: String,
}

impl Default for DatePickerDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            shape_radius: None,
            colors: super::DatePickerColors::default(),
            confirm_label: super::DatePickerDefaults::CONFIRM_LABEL.to_string(),
            dismiss_label: super::DatePickerDefaults::DISMISS_LABEL.to_string(),
        }
    }
}

/// M3 Date Picker Dialog - wraps [`DatePicker`] inside a modal [`Dialog`]
/// with confirm/cancel buttons. Equivalent to Compose's `DatePickerDialog`.
///
/// The `on_confirm` callback fires when the OK button is pressed.
/// The `on_dismiss` callback fires on Cancel, Escape, or scrim tap.
pub fn DatePickerDialog(
    state: Rc<DialogState>,
    picker_state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: DatePickerDialogConfig,
) -> View {
    state.set_scrim_color(None);
    let dismiss_state = state.clone();
    let dismiss_callback = on_dismiss.clone();
    let dismiss = Rc::new(move || {
        dismiss_state.dismiss();
        dismiss_callback();
    }) as Rc<dyn Fn()>;
    let confirm_state = state.clone();
    let confirm_callback = on_confirm.clone();
    let confirm = Rc::new(move |year, month, day| {
        confirm_state.dismiss();
        confirm_callback(year, month, day);
    }) as Rc<dyn Fn(i32, u32, u32)>;
    let content = Box(Modifier::new()
        .background(config.colors.container_color)
        .clip_rounded(
            config
                .shape_radius
                .unwrap_or_else(|| theme().shapes.extra_large),
        ))
    .child(Column(Modifier::new()).child((DatePicker(
        picker_state.clone(),
        confirm,
        dismiss.clone(),
        DatePickerConfig {
            colors: config.colors,
            confirm_label: config.confirm_label,
            dismiss_label: config.dismiss_label,
            ..DatePickerConfig::default()
        },
    ),)));

    Dialog(
        state,
        config.modifier,
        DialogProperties {
            on_dismiss_request: Some(dismiss.clone()),
            ..DialogProperties::default()
        },
        content,
    )
}

/// Configuration for [`TimePickerDialog`].
#[derive(Clone)]
pub struct TimePickerDialogConfig {
    pub modifier: Modifier,
    pub shape_radius: Option<Dp>,
    pub container_color: Color,
    pub colors: super::TimePickerColors,
    pub confirm_label: String,
    pub dismiss_label: String,
}

impl Default for TimePickerDialogConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            shape_radius: None,
            container_color: theme().surface_container_high,
            colors: super::TimePickerColors::default(),
            confirm_label: super::TimePickerDefaults::CONFIRM_LABEL.to_string(),
            dismiss_label: super::TimePickerDefaults::DISMISS_LABEL.to_string(),
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
    picker_state: Rc<TimePickerState>,
    on_confirm: Rc<dyn Fn(u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
    config: TimePickerDialogConfig,
) -> View {
    state.set_scrim_color(None);
    let dismiss_state = state.clone();
    let dismiss_callback = on_dismiss.clone();
    let dismiss = Rc::new(move || {
        dismiss_state.dismiss();
        dismiss_callback();
    }) as Rc<dyn Fn()>;
    let confirm_state = state.clone();
    let confirm_callback = on_confirm.clone();
    let confirm = Rc::new(move |hour, minute| {
        confirm_state.dismiss();
        confirm_callback(hour, minute);
    }) as Rc<dyn Fn(u32, u32)>;
    let content = Box(Modifier::new()
        .background(config.container_color)
        .clip_rounded(
            config
                .shape_radius
                .unwrap_or_else(|| theme().shapes.extra_large),
        ))
    .child(Column(Modifier::new()).child((TimePicker(
        picker_state.clone(),
        confirm,
        dismiss.clone(),
        TimePickerConfig {
            colors: config.colors,
            confirm_label: config.confirm_label,
            dismiss_label: config.dismiss_label,
            ..TimePickerConfig::default()
        },
    ),)));

    Dialog(
        state,
        config.modifier,
        DialogProperties {
            on_dismiss_request: Some(dismiss.clone()),
            ..DialogProperties::default()
        },
        content,
    )
}
