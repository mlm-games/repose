#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use repose_core::timer::{TimerHandle, delay};
use repose_core::*;
use repose_ui::{
    Box, Column, Row, Text, TextStyle, ViewExt, ZStack, anim::animate_f32_from,
    overlay::OverlayGuard, overlay::ambient_overlay,
};
use web_time::Duration;

use super::util::apply_tonal_elevation;
use super::*;
use crate::{Icon, Symbol};

/// Configuration for [`DropdownMenu`].
#[derive(Clone, Debug)]
pub struct DropdownMenuConfig {
    pub container_color: Color,
    pub item_text_color: Color,
    pub disabled_item_text_color: Color,
    pub divider_color: Color,
    pub min_width: Dp,
    pub item_height: Dp,
    pub max_width: Dp,
    pub shadow_elevation: Option<Dp>,
    pub tonal_elevation: Dp,
    pub border: Option<(Dp, Color, Dp)>,
    pub shape_radius: Option<Dp>,
    pub offset_x: Dp,
    pub offset_y: Dp,
    pub vertical_margin: Dp,
}

impl Default for DropdownMenuConfig {
    fn default() -> Self {
        Self {
            container_color: DropdownMenuDefaults::container_color(),
            item_text_color: DropdownMenuDefaults::item_text_color(),
            disabled_item_text_color: DropdownMenuDefaults::disabled_item_text_color(),
            divider_color: DropdownMenuDefaults::divider_color(),
            min_width: DropdownMenuDefaults::MIN_WIDTH,
            item_height: DropdownMenuDefaults::ITEM_HEIGHT,
            max_width: DropdownMenuDefaults::MAX_WIDTH,
            shadow_elevation: None,
            tonal_elevation: Dp::ZERO,
            border: None,
            shape_radius: None,
            offset_x: Dp::ZERO,
            offset_y: Dp::ZERO,
            vertical_margin: DropdownMenuDefaults::VERTICAL_MARGIN,
        }
    }
}

/// A single item inside a `DropdownMenu`.
#[derive(Clone)]
pub struct DropdownMenuItem {
    pub text: String,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub on_click: Rc<dyn Fn()>,
    pub enabled: bool,
}

impl DropdownMenuItem {
    pub fn new(text: impl Into<String>, on_click: impl Fn() + 'static) -> Self {
        Self {
            text: text.into(),
            leading_icon: None,
            trailing_icon: None,
            on_click: Rc::new(on_click),
            enabled: true,
        }
    }

    pub fn leading_icon(mut self, icon: View) -> Self {
        self.leading_icon = Some(icon);
        self
    }

    pub fn trailing_icon(mut self, icon: View) -> Self {
        self.trailing_icon = Some(icon);
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// A menu divider line.
pub struct MenuDivider;

/// State for controlling `DropdownMenu` visibility.
pub struct MenuState {
    visible: Signal<bool>,
    anchor: Signal<Option<Vec2>>,
}

impl Default for MenuState {
    fn default() -> Self {
        Self::new()
    }
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            visible: signal(false),
            anchor: signal(None),
        }
    }

    pub fn is_open(&self) -> bool {
        self.visible.get()
    }

    /// Open anchored to the trigger element's measured rect.
    pub fn open(&self) {
        self.anchor.set(None);
        self.visible.set(true);
    }

    /// Open anchored at an explicit window-space position in Dp
    /// (e.g. a cursor position: `px_to_dp` the event's
    /// `position_in_window()` first, since pointer positions are physical
    /// pixels). Takes precedence over the trigger rect while set.
    pub fn open_at(&self, screen_pos: Vec2) {
        self.anchor.set(Some(screen_pos));
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

const DDM_SCALE_FROM: f32 = 0.8;
const DDM_VERTICAL_PADDING: Dp = Dp(8.0);
const DDM_ITEM_H_PAD: Dp = Dp(12.0);
const DDM_ITEM_MIN_HEIGHT: Dp = Dp(48.0);
const DDM_MIN_OPEN_HEIGHT: Dp = Dp(48.0);
const DDM_ROOT_SCRIM_Z: f32 = 1000.0;
const DDM_CARD_Z: f32 = 1001.0;
const DDM_SUBMENU_SCRIM_Z: f32 = 1003.0;
const DDM_SUBMENU_CARD_Z: f32 = 1004.0;
/// Hit layer for everything inside the card. Must stay above the card and the
/// scrims, whose `on_scroll` barriers swallow the delta otherwise, so the
/// items scroller stays the first scroll consumer under the pointer.
const DDM_CONTENT_Z: f32 = 1005.0;
const DDM_SUBMENU_ARROW: Symbol = Symbol::new("chevron_right", '\u{E5CC}');

/// Either a menu item, a divider, or a nested submenu.
#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
pub enum DropdownMenuEntry {
    Item(DropdownMenuItem),
    Divider,
    Submenu(DropdownMenuSubmenu),
}

/// A labelled group of entries rendered in a cascading popup anchored to
/// its header row.
#[derive(Clone)]
pub struct DropdownMenuSubmenu {
    pub text: String,
    pub enabled: bool,
    pub children: Vec<DropdownMenuEntry>,
}

impl DropdownMenuSubmenu {
    pub fn new(text: impl Into<String>, children: Vec<DropdownMenuEntry>) -> Self {
        Self {
            text: text.into(),
            enabled: true,
            children,
        }
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// M3 Dropdown Menu anchored to a trigger element.
///
/// Renders as a single overlay entry with a transparent full-screen scrim and
/// positioned card, matching Compose's Popup behavior. The card is bounded in
/// height so vertical_scroll activates when content overflows.
///
/// Renders into the ambient overlay layer installed by the runtime.
pub fn DropdownMenu(
    state: Rc<MenuState>,
    modifier: Modifier,
    trigger: View,
    items: Vec<DropdownMenuEntry>,
    config: DropdownMenuConfig,
) -> View {
    let overlay = ambient_overlay();
    let th = theme();
    let ddm_id = remember(unique_component_id);
    let overlay_guard = remember_with_key(format!("ddm_oguard_{ddm_id}"), || {
        RefCell::new(None::<OverlayGuard>)
    });
    let trigger_rect = remember_state_with_key(format!("ddm_tr_{ddm_id}"), || None::<Rect>);
    let scroll_state: Rc<ScrollState> =
        remember_with_key(format!("ddm_scroll_{ddm_id}"), ScrollState::new);
    let root_popup_size = remember_state_with_key(format!("ddm_popup_size_{ddm_id}"), || {
        repose_core::Vec2 { x: 0.0, y: 0.0 }
    });
    let submenu_open = remember_state_with_key(format!("ddm_subopen_{ddm_id}"), || None::<String>);
    let submenu_anchor_rects = remember_state_with_key(
        format!("ddm_subanchor_{ddm_id}"),
        HashMap::<String, Rect>::new,
    );
    let submenu_popup_widths = remember_state_with_key(
        format!("ddm_subpopup_{ddm_id}"),
        HashMap::<String, f32>::new,
    );
    let submenu_guards = remember_state_with_key(
        format!("ddm_subguards_{ddm_id}"),
        HashMap::<String, OverlayGuard>::new,
    );
    let submenu_hovered =
        remember_state_with_key(format!("ddm_subhover_{ddm_id}"), || None::<String>);
    let submenu_popup_hovered: Rc<Cell<bool>> =
        remember_with_key(format!("ddm_subpopup_hover_{ddm_id}"), || Cell::new(false));
    let submenu_hover_timer = remember_state_with_key(
        format!("ddm_subhover_timer_{ddm_id}"),
        || None::<TimerHandle>,
    );
    let submenu_latched =
        remember_state_with_key(format!("ddm_sublatched_{ddm_id}"), || None::<String>);

    let current_items = remember_state_with_key(format!("ddm_items_{ddm_id}"), Vec::new);
    *current_items.borrow_mut() = items;
    let current_config = remember_state_with_key(format!("ddm_cfg_{ddm_id}"), || config.clone());
    *current_config.borrow_mut() = config;

    let trigger = Box(Modifier::new().on_globally_positioned({
        let tr = trigger_rect.clone();
        move |rect| {
            let changed = tr.borrow().as_ref() != Some(&rect);
            if changed {
                *tr.borrow_mut() = Some(rect);
                request_frame();
            }
        }
    }))
    .child(trigger);

    let anim = remember_state_with_key(format!("ddm_anim_{ddm_id}"), || {
        AnimatedValue::new(0.0, theme().motion.overlay)
    });
    let last_target = remember_state_with_key(format!("ddm_lt_{ddm_id}"), || f32::NAN);
    let anim_target = if state.is_open() { 1.0 } else { 0.0 };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
            a.set_target(anim_target);
            *lt = anim_target;
        }
        drop(lt);
        if a.update() {
            request_frame();
        }
    }

    let progress = *anim.borrow().get();
    let menu_visible = state.is_open() || progress > 0.01;
    if !state.is_open() {
        submenu_open.borrow_mut().take();
        submenu_anchor_rects.borrow_mut().clear();
        *root_popup_size.borrow_mut() = repose_core::Vec2 { x: 0.0, y: 0.0 };
        submenu_popup_widths.borrow_mut().clear();
        submenu_guards.borrow_mut().clear();
        submenu_hovered.borrow_mut().take();
        submenu_latched.borrow_mut().take();
        submenu_popup_hovered.set(false);
        submenu_hover_timer.borrow_mut().take();
    }

    if menu_visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
            let anim = anim.clone();
            let current_items = current_items.clone();
            let state = state.clone();
            let current_config = current_config.clone();
            let trigger_rect = trigger_rect.clone();
            let scroll_state = scroll_state.clone();
            let root_popup_size = root_popup_size.clone();
            let submenu_open = submenu_open.clone();
            let submenu_anchor_rects = submenu_anchor_rects.clone();
            let submenu_popup_widths = submenu_popup_widths.clone();
            let submenu_guards = submenu_guards.clone();
            let submenu_hovered = submenu_hovered.clone();
            let submenu_popup_hovered = submenu_popup_hovered.clone();
            let submenu_hover_timer = submenu_hover_timer.clone();
            let submenu_latched = submenu_latched.clone();
            let back_state = state.clone();

            *overlay_guard.borrow_mut() = Some(overlay.show_guard_with_back(
                Rc::new(move || {
                    let items = current_items.borrow().clone();
                    let config = current_config.borrow().clone();
                    let p = *anim.borrow().get();
                    let scale = DDM_SCALE_FROM + (1.0 - DDM_SCALE_FROM) * p;
                    let alpha = p;

                    let explicit_anchor = state.anchor.get();
                    let rect = explicit_anchor
                        .map(|pos| Rect {
                            x: pos.x,
                            y: pos.y,
                            w: 1.0,
                            h: 1.0,
                        })
                        .unwrap_or_else(|| (*trigger_rect.borrow()).unwrap_or_default());
                    let win_w = get_window_container_width();
                    let win_h = get_window_container_height();
                    let horizontal_margin = DropdownMenuDefaults::HORIZONTAL_MARGIN.0;
                    let vertical_margin = config.vertical_margin.0;

                    let space_below =
                        (win_h - vertical_margin) - (rect.y + rect.h + config.offset_y.0);
                    let space_above = rect.y + config.offset_y.0 - vertical_margin;

                    let estimated_h = estimate_dropdown_height(&items, &config)
                        .min(space_below.max(space_above))
                        .max(DDM_MIN_OPEN_HEIGHT.0);
                    let place_below = space_below >= estimated_h
                        || (space_above < estimated_h && space_below >= space_above);
                    let available_height = (if place_below {
                        space_below
                    } else {
                        space_above
                    })
                    .max(48.0);

                    // Keep the card on-screen horizontally (cursor menus near
                    // the right edge used to overflow off-window).
                    let viewport_width = (win_w - horizontal_margin * 2.0).max(1.0);
                    let constrained_min = config.min_width.0.min(viewport_width);
                    let measured_width = root_popup_size.borrow().x;
                    let menu_w = if measured_width > 0.0 {
                        measured_width
                    } else {
                        constrained_min.max(1.0)
                    };
                    let popup_x = (rect.x + config.offset_x.0).clamp(
                        horizontal_margin,
                        (win_w - horizontal_margin - menu_w).max(horizontal_margin),
                    );
                    let constrained_width =
                        config.max_width.0.min(viewport_width).max(constrained_min);

                    let mut adjusted_config = config.clone();
                    adjusted_config.min_width = Dp(constrained_min);
                    adjusted_config.max_width = Dp(constrained_width);

                    let content = render_dropdown_menu_content(
                        &th,
                        &items,
                        state.clone(),
                        &adjusted_config,
                        scroll_state.clone(),
                        submenu_open.clone(),
                        submenu_anchor_rects.clone(),
                        submenu_popup_widths.clone(),
                        submenu_guards.clone(),
                        submenu_hovered.clone(),
                        submenu_popup_hovered.clone(),
                        submenu_hover_timer.clone(),
                        submenu_latched.clone(),
                        available_height,
                        *ddm_id,
                    );

                    let transform_origin_y = if place_below { 0.0 } else { 1.0 };

                    let mut offset_modifier = Modifier::new();
                    if place_below {
                        offset_modifier = offset_modifier.offset(
                            Some(Dp(popup_x)),
                            Some(Dp(rect.y + rect.h + config.offset_y.0)),
                            None,
                            None,
                        );
                    } else {
                        let menu_bottom_y = rect.y + config.offset_y.0;
                        let offset_bottom = (win_h - menu_bottom_y).max(0.0);
                        offset_modifier = offset_modifier.offset(
                            Some(Dp(popup_x)),
                            None,
                            None,
                            Some(Dp(offset_bottom)),
                        );
                    }

                    let mut menu_modifier = offset_modifier
                        .absolute()
                        .z_index(DDM_CARD_Z)
                        .scale(scale)
                        .alpha(alpha)
                        .transform_origin(0.0, transform_origin_y);
                    let popup_size = root_popup_size.clone();
                    menu_modifier = menu_modifier.on_size_changed(move |size| {
                        if *popup_size.borrow() != size {
                            *popup_size.borrow_mut() = size;
                            request_frame();
                        }
                    });
                    let menu = Box(menu_modifier).child(content);

                    let scrim = Box(Modifier::new()
                        .z_index(DDM_ROOT_SCRIM_Z)
                        .fill_max_size()
                        .focusable(false)
                        .input_blocker()
                        .on_scroll(|_| Vec2::ZERO)
                        .on_click({
                            let state = state.clone();
                            let submenu_open = submenu_open.clone();
                            let submenu_hovered = submenu_hovered.clone();
                            let submenu_popup_hovered = submenu_popup_hovered.clone();
                            let submenu_hover_timer = submenu_hover_timer.clone();
                            let submenu_latched = submenu_latched.clone();
                            move || {
                                let has_child = submenu_open.borrow().is_some();
                                if has_child {
                                    submenu_open.borrow_mut().take();
                                    submenu_hovered.borrow_mut().take();
                                    submenu_latched.borrow_mut().take();
                                    submenu_popup_hovered.set(false);
                                    submenu_hover_timer.borrow_mut().take();
                                    request_frame();
                                } else {
                                    state.dismiss();
                                }
                            }
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, menu))
                }),
                901.0,
                false,
                Rc::new(move || {
                    back_state.dismiss();
                    true
                }),
            ));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(modifier).child(trigger)
}

fn estimate_dropdown_height(items: &[DropdownMenuEntry], config: &DropdownMenuConfig) -> f32 {
    let mut h = 2.0 * DDM_VERTICAL_PADDING.0;
    for entry in items {
        match entry {
            DropdownMenuEntry::Item(_) | DropdownMenuEntry::Submenu(_) => {
                h += config.item_height.max(DDM_ITEM_MIN_HEIGHT).0;
            }
            DropdownMenuEntry::Divider => h += 1.0 + 2.0 * 12.0,
        }
    }
    h
}

/// Window-space placement of a cascading submenu: its top-left corner and the
/// card height budget it gets. `est_h` must be the content's natural height: a
/// measured height feeds the budget back into the fit test and the card
/// oscillates between two sizes every frame.
fn place_submenu_popup(
    anchor: Rect,
    config: &DropdownMenuConfig,
    win_w: f32,
    win_h: f32,
    menu_w: f32,
    est_h: f32,
    horizontal_margin: f32,
) -> (f32, f32, f32) {
    let vertical_margin = config.vertical_margin.0;
    let est_h = est_h.max(DDM_MIN_OPEN_HEIGHT.0);
    let menu_w = menu_w.min((win_w - horizontal_margin * 2.0).max(1.0));
    let mut x = anchor.x + anchor.w + config.offset_x.0;
    if x + menu_w > win_w - horizontal_margin {
        x = (anchor.x - menu_w - config.offset_x.0).max(horizontal_margin);
    }
    let space_below = (win_h - vertical_margin) - (anchor.y + config.offset_y.0);
    let space_above = anchor.y + anchor.h + config.offset_y.0 - vertical_margin;
    let place_above = space_below < est_h && (space_above >= est_h || space_above > space_below);
    let y = if place_above {
        let top = (anchor.y + anchor.h + config.offset_y.0 - est_h).max(vertical_margin);
        let card = est_h.min(top - vertical_margin);
        top.min(win_h - card).max(vertical_margin)
    } else {
        (anchor.y + config.offset_y.0).max(vertical_margin)
    };
    let available_height = (if place_above {
        y - vertical_margin
    } else {
        win_h - vertical_margin - y
    })
    .max(DDM_MIN_OPEN_HEIGHT.0)
    .min((win_h - vertical_margin * 2.0).max(DDM_MIN_OPEN_HEIGHT.0));
    (x, y, available_height)
}

fn render_dropdown_item(
    th: &Theme,
    item: &DropdownMenuItem,
    state: Rc<MenuState>,
    config: &DropdownMenuConfig,
) -> View {
    let text_color = if item.enabled {
        config.item_text_color
    } else {
        config.disabled_item_text_color
    };
    let on_click = item.on_click.clone();
    let state = state.clone();
    let item_source: Rc<MutableInteractionSource> = remember(MutableInteractionSource::new);

    let mut modifier = Modifier::new()
        .z_index(DDM_CONTENT_Z)
        .fill_max_width()
        .min_height(config.item_height.max(DDM_ITEM_MIN_HEIGHT))
        .padding_values(PaddingValues {
            left: DDM_ITEM_H_PAD,
            right: DDM_ITEM_H_PAD,
            top: Dp::ZERO,
            bottom: Dp::ZERO,
        })
        .align_items(AlignItems::CENTER);

    if item.enabled {
        modifier = modifier
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: Color::TRANSPARENT,
                focused: Color::TRANSPARENT,
                pressed: Color::TRANSPARENT,
                dragged: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .interaction_source(&item_source)
            .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                color: Some(th.on_surface),
                bounded: true,
                ..Default::default()
            }))
            .clickable()
            .on_click(move || {
                on_click();
                state.dismiss();
            });
    }

    let mut row_children: Vec<View> = Vec::new();
    if let Some(icon) = item.leading_icon.clone() {
        row_children.push(icon);
        row_children.push(Box(Modifier::new().width(DDM_ITEM_H_PAD)));
    }
    row_children.push(
        Box(Modifier::new().flex_grow(1.0)).child(
            Text(item.text.clone())
                .color(text_color)
                .size(th.typography.label_large)
                .single_line()
                .overflow_ellipsize(),
        ),
    );
    if let Some(icon) = item.trailing_icon.clone() {
        row_children.push(Box(Modifier::new().width(DDM_ITEM_H_PAD)));
        row_children.push(icon);
    }
    Row(modifier).child(row_children)
}

#[derive(Clone)]
struct DropdownSubmenuHost {
    parent_state: Rc<MenuState>,
    open_child: Rc<RefCell<Option<String>>>,
    anchor_rects: Rc<RefCell<HashMap<String, Rect>>>,
    popup_widths: Rc<RefCell<HashMap<String, f32>>>,
    hovered_child: Rc<RefCell<Option<String>>>,
    popup_hovered: Rc<Cell<bool>>,
    hover_timer: Rc<RefCell<Option<TimerHandle>>>,
    latched_child: Rc<RefCell<Option<String>>>,
    guards: Weak<RefCell<HashMap<String, OverlayGuard>>>,
}

fn schedule_submenu_close(
    open_child: &Rc<RefCell<Option<String>>>,
    hovered_child: &Rc<RefCell<Option<String>>>,
    popup_hovered: &Rc<Cell<bool>>,
    latched_child: &Rc<RefCell<Option<String>>>,
    hover_timer: &Rc<RefCell<Option<TimerHandle>>>,
    text: &str,
) {
    hover_timer.borrow_mut().take();
    let open_child = open_child.clone();
    let hovered_child = hovered_child.clone();
    let popup_hovered = popup_hovered.clone();
    let latched_child = latched_child.clone();
    let text = text.to_string();
    *hover_timer.borrow_mut() = Some(delay(Duration::from_millis(120), move || {
        let should_close = !popup_hovered.get()
            && hovered_child.borrow().is_none()
            && latched_child.borrow().as_deref() != Some(text.as_str())
            && open_child.borrow().as_deref() == Some(text.as_str());
        if should_close {
            open_child.borrow_mut().take();
            request_frame();
        }
    }));
}

fn render_dropdown_submenu(
    th: &Theme,
    sub: &DropdownMenuSubmenu,
    parent: &DropdownSubmenuHost,
    config: &DropdownMenuConfig,
    ddm_id: u64,
) -> View {
    let header_color = if sub.enabled {
        config.item_text_color
    } else {
        config.disabled_item_text_color
    };
    let open = sub.enabled && parent.open_child.borrow().as_ref() == Some(&sub.text);
    let parent = parent.clone();
    let text = sub.text.clone();
    let enabled = sub.enabled;
    if !enabled {
        if parent.open_child.borrow().as_ref() == Some(&text) {
            parent.open_child.borrow_mut().take();
        }
        if parent.latched_child.borrow().as_ref() == Some(&text) {
            parent.latched_child.borrow_mut().take();
        }
        if parent.hovered_child.borrow().as_ref() == Some(&text) {
            parent.hovered_child.borrow_mut().take();
        }
    }
    let header_source: Rc<MutableInteractionSource> = remember(MutableInteractionSource::new);
    let mut header_modifier = Modifier::new()
        .z_index(DDM_CONTENT_Z)
        .fill_max_width()
        .min_height(config.item_height.max(DDM_ITEM_MIN_HEIGHT))
        .padding_values(PaddingValues {
            left: DDM_ITEM_H_PAD,
            right: DDM_ITEM_H_PAD,
            top: Dp::ZERO,
            bottom: Dp::ZERO,
        })
        .align_items(AlignItems::CENTER);
    if enabled {
        let parent_toggle = parent.clone();
        let text_toggle = text.clone();
        let parent_enter = parent.clone();
        let text_enter = text.clone();
        let parent_leave = parent.clone();
        let text_leave = text.clone();
        header_modifier = header_modifier
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: Color::TRANSPARENT,
                focused: Color::TRANSPARENT,
                pressed: th.on_surface.with_alpha_f32(0.12),
                dragged: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .interaction_source(&header_source)
            .indication(crate::ripple::ripple(crate::ripple::RippleConfig {
                color: Some(th.on_surface),
                bounded: true,
                ..Default::default()
            }))
            .clickable()
            .on_click(move || {
                parent_toggle.hover_timer.borrow_mut().take();
                let mut slot = parent_toggle.open_child.borrow_mut();
                let mut latched = parent_toggle.latched_child.borrow_mut();
                if latched.as_ref() == Some(&text_toggle) {
                    *latched = None;
                    *slot = None;
                } else {
                    *latched = Some(text_toggle.clone());
                    *slot = Some(text_toggle.clone());
                }
                request_frame();
            })
            .on_pointer_enter(move |event| {
                if event.kind == PointerKind::Touch {
                    return;
                }
                parent_enter.hover_timer.borrow_mut().take();
                *parent_enter.hovered_child.borrow_mut() = Some(text_enter.clone());
                *parent_enter.open_child.borrow_mut() = Some(text_enter.clone());
                request_frame();
            })
            .on_pointer_leave(move |event| {
                if event.kind == PointerKind::Touch {
                    return;
                }
                let was_hovered = parent_leave.hovered_child.borrow().as_ref() == Some(&text_leave);
                if was_hovered {
                    parent_leave.hovered_child.borrow_mut().take();
                }
                schedule_submenu_close(
                    &parent_leave.open_child,
                    &parent_leave.hovered_child,
                    &parent_leave.popup_hovered,
                    &parent_leave.latched_child,
                    &parent_leave.hover_timer,
                    &text_leave,
                );
                request_frame();
            });
    }
    let header = Row(header_modifier).child((
        Box(Modifier::new().flex_grow(1.0)).child(
            Text(sub.text.clone())
                .color(header_color)
                .size(th.typography.label_large)
                .single_line()
                .overflow_ellipsize(),
        ),
        Box(Modifier::new().width(DDM_ITEM_H_PAD)),
        Icon(DDM_SUBMENU_ARROW).color(header_color).size(Sp(20.0)),
    ));

    let animation_key = format!("ddm_sub_progress_{ddm_id}_{}", sub.text);
    let animation_target = if open { 1.0 } else { 0.0 };
    let progress = animate_f32_from(
        animation_key.clone(),
        0.0,
        animation_target,
        th.motion.overlay,
    );
    let visible = open || progress > 0.01;
    let anchor_rect = parent.anchor_rects.borrow().get(&sub.text).cloned();
    let guard_key = format!("ddm_sub_{ddm_id}_{}", sub.text);
    let Some(guards) = parent.guards.upgrade() else {
        return header;
    };
    if !visible {
        guards.borrow_mut().remove(&guard_key);
        return Box(Modifier::new().on_globally_positioned({
            let parent = parent.clone();
            let text = sub.text.clone();
            move |rect| {
                if parent.anchor_rects.borrow().get(&text) != Some(&rect) {
                    parent.anchor_rects.borrow_mut().insert(text.clone(), rect);
                    request_frame();
                }
            }
        }))
        .child(header);
    }
    let Some(initial_anchor) = anchor_rect else {
        return Box(Modifier::new().on_globally_positioned({
            let parent = parent.clone();
            let text = sub.text.clone();
            move |rect| {
                parent.anchor_rects.borrow_mut().insert(text.clone(), rect);
                request_frame();
            }
        }))
        .child(header);
    };

    let overlay = ambient_overlay();
    if !guards.borrow().contains_key(&guard_key)
        && let Some(overlay) = overlay
    {
        let parent_state = parent.parent_state.clone();
        let back_parent = parent.clone();
        let back_text = sub.text.clone();
        let parent = parent.clone();
        let text = sub.text.clone();
        let children = sub.children.clone();
        let config = config.clone();
        let th = *th;
        let popup_hovered = parent.popup_hovered.clone();
        let popup_hovered_child = parent.hovered_child.clone();
        let popup_hover_timer = parent.hover_timer.clone();
        let popup_latched = parent.latched_child.clone();
        let popup_text = text.clone();
        let guard = overlay.show_guard_with_back(
            Rc::new(move || {
                let animation_target = if parent.open_child.borrow().as_ref() == Some(&text) {
                    1.0
                } else {
                    0.0
                };
                let progress = animate_f32_from(
                    animation_key.clone(),
                    0.0,
                    animation_target,
                    th.motion.overlay,
                );
                let scale = DDM_SCALE_FROM + (1.0 - DDM_SCALE_FROM) * progress;
                let alpha = progress;
                let win_w = get_window_container_width();
                let win_h = get_window_container_height();
                let horizontal_margin = DropdownMenuDefaults::HORIZONTAL_MARGIN.0;
                // Unlike the height, this may be a measurement: the card is laid
                // out by taffy against the whole overlay, so its width never
                // depends on the x it is placed at.
                let measured_w = parent
                    .popup_widths
                    .borrow()
                    .get(&text)
                    .copied()
                    .unwrap_or(0.0);
                let menu_w = if measured_w > 0.0 {
                    measured_w
                } else {
                    config.min_width.0.max(1.0)
                };
                let anchor = parent
                    .anchor_rects
                    .borrow()
                    .get(&text)
                    .copied()
                    .unwrap_or(initial_anchor);
                let (x, y, available_height) = place_submenu_popup(
                    anchor,
                    &config,
                    win_w,
                    win_h,
                    menu_w,
                    estimate_dropdown_height(&children, &config),
                    horizontal_margin,
                );
                let items: Vec<View> = children
                    .iter()
                    .map(|entry| match entry {
                        DropdownMenuEntry::Item(item) => {
                            render_dropdown_item(&th, item, parent_state.clone(), &config)
                        }
                        DropdownMenuEntry::Submenu(nested) => {
                            render_dropdown_submenu(&th, nested, &parent, &config, ddm_id)
                        }
                        DropdownMenuEntry::Divider => render_dropdown_divider(&config),
                    })
                    .collect();
                let scroll_state: Rc<ScrollState> =
                    remember_with_key(format!("ddm_sub_scroll_{ddm_id}_{text}"), ScrollState::new);
                let binding = scroll_state.to_binding();
                let axis_binding = match &binding {
                    ScrollBinding::Vertical(axis) => axis.clone(),
                    _ => unreachable!(),
                };
                let popup_enter_hovered = popup_hovered.clone();
                let popup_enter_timer = popup_hover_timer.clone();
                let popup_leave_hovered = popup_hovered.clone();
                let popup_leave_hovered_child = popup_hovered_child.clone();
                let popup_leave_timer = popup_hover_timer.clone();
                let popup_leave_latched = popup_latched.clone();
                let popup_leave_open = parent.open_child.clone();
                let popup_leave_text = popup_text.clone();
                let items_column = Box(Modifier::new()
                    .fill_max_width()
                    .max_height(Dp(
                        (available_height - 2.0 * DDM_VERTICAL_PADDING.0).max(0.0)
                    ))
                    .z_index(DDM_CONTENT_Z)
                    .vertical_scroll(axis_binding))
                .child(Column(Modifier::new().fill_max_width()).with_children(items));
                let popup_widths = parent.popup_widths.clone();
                let popup_width_text = text.clone();
                let card_modifier = render_dropdown_card_modifier(&th, &config)
                    .z_index(DDM_SUBMENU_CARD_Z)
                    .on_pointer_enter(move |event| {
                        if event.kind == PointerKind::Touch {
                            return;
                        }
                        popup_enter_hovered.set(true);
                        popup_enter_timer.borrow_mut().take();
                    })
                    .on_pointer_leave(move |event| {
                        if event.kind == PointerKind::Touch {
                            return;
                        }
                        popup_leave_hovered.set(false);
                        schedule_submenu_close(
                            &popup_leave_open,
                            &popup_leave_hovered_child,
                            &popup_leave_hovered,
                            &popup_leave_latched,
                            &popup_leave_timer,
                            &popup_leave_text,
                        );
                    })
                    .on_size_changed(move |size| {
                        let mut widths = popup_widths.borrow_mut();
                        if widths.get(&popup_width_text) != Some(&size.x) {
                            widths.insert(popup_width_text.clone(), size.x);
                            drop(widths);
                            request_frame();
                        }
                    });
                let card = Box(card_modifier).child(items_column);
                let popup = Box(Modifier::new()
                    .absolute()
                    .z_index(DDM_SUBMENU_CARD_Z)
                    .offset(Some(Dp(x)), Some(Dp(y)), None, None)
                    .scale(scale)
                    .alpha(alpha)
                    .transform_origin(0.0, 0.0))
                .child(card);
                let scrim = Box(Modifier::new()
                    .z_index(DDM_SUBMENU_SCRIM_Z)
                    .fill_max_size()
                    .on_click({
                        let open_child = parent.open_child.clone();
                        let text = text.clone();
                        let hovered_child = parent.hovered_child.clone();
                        let latched_child = parent.latched_child.clone();
                        let hover_timer = parent.hover_timer.clone();
                        let popup_hovered = parent.popup_hovered.clone();
                        move || {
                            if open_child.borrow().as_deref() == Some(text.as_str()) {
                                open_child.borrow_mut().take();
                                hovered_child.borrow_mut().take();
                                latched_child.borrow_mut().take();
                                popup_hovered.set(false);
                                hover_timer.borrow_mut().take();
                                request_frame();
                            }
                        }
                    }));
                ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, popup))
            }),
            902.0,
            true,
            Rc::new(move || {
                let was_open = back_parent.open_child.borrow().as_ref() == Some(&back_text);
                if was_open {
                    back_parent.open_child.borrow_mut().take();
                    let was_latched =
                        back_parent.latched_child.borrow().as_ref() == Some(&back_text);
                    if was_latched {
                        back_parent.latched_child.borrow_mut().take();
                    }
                    let was_hovered =
                        back_parent.hovered_child.borrow().as_ref() == Some(&back_text);
                    if was_hovered {
                        back_parent.hovered_child.borrow_mut().take();
                    }
                    back_parent.popup_hovered.set(false);
                    back_parent.hover_timer.borrow_mut().take();
                    request_frame();
                }
                was_open
            }),
        );
        guards.borrow_mut().insert(guard_key, guard);
    }
    Box(Modifier::new().on_globally_positioned({
        let parent = parent.clone();
        let text = sub.text.clone();
        move |rect| {
            if parent.anchor_rects.borrow().get(&text) != Some(&rect) {
                parent.anchor_rects.borrow_mut().insert(text.clone(), rect);
                request_frame();
            }
        }
    }))
    .child(header)
}

fn render_dropdown_divider(config: &DropdownMenuConfig) -> View {
    Box(Modifier::new()
        .fill_max_width()
        .height(Dp(1.0))
        .margin(Dp(12.0))
        .background(config.divider_color))
}
fn render_dropdown_menu_content(
    th: &Theme,
    items: &[DropdownMenuEntry],
    state: Rc<MenuState>,
    config: &DropdownMenuConfig,
    scroll_state: Rc<ScrollState>,
    submenu_open: Rc<RefCell<Option<String>>>,
    submenu_anchor_rects: Rc<RefCell<HashMap<String, Rect>>>,
    submenu_popup_widths: Rc<RefCell<HashMap<String, f32>>>,
    submenu_guards: Rc<RefCell<HashMap<String, OverlayGuard>>>,
    submenu_hovered: Rc<RefCell<Option<String>>>,
    submenu_popup_hovered: Rc<Cell<bool>>,
    submenu_hover_timer: Rc<RefCell<Option<TimerHandle>>>,
    submenu_latched: Rc<RefCell<Option<String>>>,
    max_height: f32,
    ddm_id: u64,
) -> View {
    let host = DropdownSubmenuHost {
        parent_state: state,
        open_child: submenu_open,
        anchor_rects: submenu_anchor_rects,
        popup_widths: submenu_popup_widths,
        hovered_child: submenu_hovered,
        popup_hovered: submenu_popup_hovered,
        hover_timer: submenu_hover_timer,
        latched_child: submenu_latched,
        guards: Rc::downgrade(&submenu_guards),
    };
    let children: Vec<View> = items
        .iter()
        .map(|entry| match entry {
            DropdownMenuEntry::Item(item) => {
                render_dropdown_item(th, item, host.parent_state.clone(), config)
            }
            DropdownMenuEntry::Submenu(sub) => {
                render_dropdown_submenu(th, sub, &host, config, ddm_id)
            }
            DropdownMenuEntry::Divider => render_dropdown_divider(config),
        })
        .collect();

    let binding = scroll_state.to_binding();
    let axis_binding = match &binding {
        ScrollBinding::Vertical(a) => a.clone(),
        _ => unreachable!(),
    };

    let items_column = Box(Modifier::new()
        .fill_max_width()
        .max_height(Dp((max_height - 2.0 * DDM_VERTICAL_PADDING.0).max(0.0)))
        .z_index(DDM_CONTENT_Z)
        .vertical_scroll(axis_binding))
    .child(Column(Modifier::new().fill_max_width()).with_children(children));

    Box(render_dropdown_card_modifier(th, config)).child(items_column)
}

fn render_dropdown_card_modifier(th: &Theme, config: &DropdownMenuConfig) -> Modifier {
    let shadow_elevation = config.shadow_elevation.unwrap_or(th.elevation.level2);

    let mut card_modifier = Modifier::new()
        .graphics_layer(1.0)
        .z_index(DDM_CARD_Z)
        .focus_group()
        .input_blocker()
        .on_scroll(|_| Vec2::ZERO)
        .shadow(shadow_elevation, Dp::ZERO)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .padding_values(PaddingValues {
            left: Dp::ZERO,
            right: Dp::ZERO,
            top: DDM_VERTICAL_PADDING,
            bottom: DDM_VERTICAL_PADDING,
        })
        .background(config.container_color)
        .clip_rounded(config.shape_radius.unwrap_or(th.shapes.extra_small));

    card_modifier = apply_tonal_elevation(
        card_modifier,
        config.tonal_elevation,
        config.container_color,
    );

    if let Some((border_width, border_color, border_radius)) = config.border {
        card_modifier = card_modifier.border(border_width, border_color, border_radius);
    }

    card_modifier
}

#[cfg(test)]
mod keyed_lifetime_tests {
    use super::*;
    use repose_core::runtime::ComposeGuard;
    use repose_core::scope::Scope;
    use repose_ui::overlay::OverlayHandle;

    fn entry_count(overlay: &OverlayHandle) -> usize {
        overlay.entry_count()
    }

    fn compose_menu(scope: &Scope, overlay: &OverlayHandle, state: &Rc<MenuState>) {
        let guard = ComposeGuard::begin();
        repose_ui::overlay::with_ambient_overlay(overlay.clone(), || {
            scope.run(|| {
                let _ = DropdownMenu(
                    state.clone(),
                    Modifier::new(),
                    Box(Modifier::new()),
                    vec![DropdownMenuEntry::Item(DropdownMenuItem::new("A", || {}))],
                    DropdownMenuConfig {
                        min_width: Dp(100.0),
                        ..Default::default()
                    },
                );
            });
        });
        drop(guard);
    }

    /// A host that stops composing a `DropdownMenu` (a context menu closing, say)
    /// must not strand the overlay entry. The guard lives in a keyed slot, so
    /// this only works if a slot abandoned inside a still-composed scope is
    /// collected rather than pinned to the scope's lifetime.
    #[test]
    fn dropdown_entry_released_when_host_stops_composing_it() {
        let scope = Scope::new();
        let overlay = OverlayHandle::new();
        let state = Rc::new(MenuState::new());
        state.open();
        compose_menu(&scope, &overlay, &state);
        assert_eq!(entry_count(&overlay), 1, "menu should be showing");

        state.dismiss();
        for _ in 0..4 {
            let guard = ComposeGuard::begin();
            scope.run(|| {
                let _ = View::new(0, ViewKind::Box);
            });
            drop(guard);
        }
        assert_eq!(
            entry_count(&overlay),
            0,
            "overlay entry outlived the host that owned it"
        );
    }

    /// The counterpart that must keep working: a menu whose host stays composed
    /// releases the entry once the exit animation finishes.
    #[test]
    fn dropdown_entry_released_after_exit_animation() {
        let scope = Scope::new();
        let overlay = OverlayHandle::new();
        let state = Rc::new(MenuState::new());
        let mut clock = repose_core::animation::TestClock {
            t: web_time::Instant::now(),
        };
        repose_core::animation::set_clock(Box::new(repose_core::animation::TestClock {
            t: clock.t,
        }));
        state.open();
        for frame in 0..120u32 {
            compose_menu(&scope, &overlay, &state);
            if frame == 0 {
                state.dismiss();
            }
            clock.t += Duration::from_millis(16);
        }
        let left = entry_count(&overlay);
        repose_core::animation::set_clock(Box::new(repose_core::animation::SystemClock));
        assert_eq!(left, 0, "entry survived the exit animation");
    }
}

#[cfg(test)]
mod cached_scope_overlay_tests {
    use super::*;
    use repose_core::runtime::{ComposeGuard, Scheduler};
    use repose_core::scope::Scope;
    use repose_ui::overlay::OverlayHandle;

    thread_local! {
        static RUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }

    fn entry_count(overlay: &OverlayHandle) -> usize {
        overlay.entry_count()
    }

    /// A menu hosted inside a `scope!` that serves from cache does not re-run, so
    /// it never re-registers its overlay entry. The entry survives only because
    /// the remembered guard is preserved for cached scopes.
    ///
    /// Guards against a tempting refactor: pruning overlay entries that were not
    /// re-registered during a compose. `OverlayHandle::host` builds every
    /// registered entry unconditionally, so a compose-anchored sweep cannot
    /// distinguish an owner that re-composed from one being served from cache -
    /// it makes an open menu vanish the moment its scope goes clean.
    #[test]
    fn open_menu_inside_a_cached_scope_keeps_its_entry() {
        let scope = Scope::new();
        let mut scheduler = Scheduler::new();
        let overlay = OverlayHandle::new();
        let state = Rc::new(MenuState::new());
        state.open();

        for _ in 0..3 {
            let guard = ComposeGuard::begin();
            // Mirrors the runtime: compose under the ambient overlay, then host
            // the entries. `host` is what prunes unrefreshed entries.
            repose_ui::overlay::with_ambient_overlay(overlay.clone(), || {
                let content = scope.run(|| {
                    repose_core::scope!("cached_menu_scope", &mut scheduler, [0u32], {
                        RUNS.with(|c| c.set(c.get() + 1));
                        DropdownMenu(
                            state.clone(),
                            Modifier::new(),
                            Box(Modifier::new()),
                            vec![DropdownMenuEntry::Item(DropdownMenuItem::new("A", || {}))],
                            DropdownMenuConfig {
                                min_width: Dp(100.0),
                                ..Default::default()
                            },
                        )
                    })
                });
                let _ = overlay.host(Modifier::new(), content);
            });
            drop(guard);
            assert_eq!(
                RUNS.with(|c| c.get()),
                1,
                "the scope must actually be served from cache for this test to mean anything"
            );
        }
        assert_eq!(
            entry_count(&overlay),
            1,
            "an open menu must survive its scope being served from cache"
        );
    }
}

#[cfg(test)]
mod submenu_placement_tests {
    use super::*;

    fn config() -> DropdownMenuConfig {
        DropdownMenuConfig {
            min_width: Dp(220.0),
            max_width: Dp(280.0),
            ..Default::default()
        }
    }

    fn entries(count: usize) -> Vec<DropdownMenuEntry> {
        (0..count)
            .map(|_| DropdownMenuEntry::Item(DropdownMenuItem::new("A", || {})))
            .collect()
    }

    fn place(anchor_y: f32, win_h: f32, entries: &[DropdownMenuEntry]) -> (f32, f32, f32) {
        let config = config();
        place_submenu_popup(
            Rect {
                x: 220.0,
                y: anchor_y,
                w: 220.0,
                h: 48.0,
            },
            &config,
            1200.0,
            win_h,
            220.0,
            estimate_dropdown_height(entries, &config),
            DropdownMenuDefaults::HORIZONTAL_MARGIN.0,
        )
    }

    #[test]
    fn submenu_opens_below_its_header_row_when_it_fits() {
        assert_eq!(place(200.0, 760.0, &entries(6)), (440.0, 200.0, 512.0));
    }

    #[test]
    fn submenu_opens_above_its_header_row_when_it_does_not_fit_below() {
        assert_eq!(place(600.0, 760.0, &entries(6)), (440.0, 344.0, 296.0));
    }

    #[test]
    fn submenu_stays_inside_the_window_when_its_header_row_overhangs_the_bottom() {
        assert_eq!(place(716.0, 760.0, &entries(1)), (440.0, 696.0, 648.0));
    }
}
