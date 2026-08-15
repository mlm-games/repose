#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{Box, Column, Row, Text, TextStyle, ViewExt, ZStack, overlay::OverlayHandle};

use super::util::apply_tonal_elevation;
use super::*;

/// Configuration for [`DropdownMenu`].
#[derive(Clone, Debug)]
pub struct DropdownMenuConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub item_text_color: Color,
    pub disabled_item_text_color: Color,
    pub divider_color: Color,
    pub min_width: f32,
    pub item_height: f32,
    pub max_width: f32,
    pub shadow_elevation: Option<f32>,
    pub tonal_elevation: f32,
    pub border: Option<(f32, Color, f32)>,
    pub shape_radius: Option<f32>,
    pub offset_x: f32,
    pub offset_y: f32,
    pub vertical_margin: f32,
}

impl Default for DropdownMenuConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: DropdownMenuDefaults::container_color(),
            item_text_color: DropdownMenuDefaults::item_text_color(),
            disabled_item_text_color: DropdownMenuDefaults::disabled_item_text_color(),
            divider_color: DropdownMenuDefaults::divider_color(),
            min_width: DropdownMenuDefaults::MIN_WIDTH,
            item_height: DropdownMenuDefaults::ITEM_HEIGHT,
            max_width: DropdownMenuDefaults::MAX_WIDTH,
            shadow_elevation: None,
            tonal_elevation: 0.0,
            border: None,
            shape_radius: None,
            offset_x: 0.0,
            offset_y: 0.0,
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

    pub fn open(&self) {
        self.visible.set(true);
    }

    pub fn open_at(&self, screen_pos: Vec2) {
        self.anchor.set(Some(screen_pos));
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

static DROPDOWN_COUNTER: AtomicU64 = AtomicU64::new(0);

const DDM_SCALE_FROM: f32 = 0.8;
const DDM_VERTICAL_PADDING: f32 = 8.0;
const DDM_ITEM_H_PAD: f32 = 12.0;
const DDM_ITEM_MIN_HEIGHT: f32 = 48.0;
const DDM_MIN_OPEN_HEIGHT: f32 = 48.0;

/// Either a menu item or a divider.
#[derive(Clone)]
pub enum DropdownMenuEntry {
    Item(DropdownMenuItem),
    Divider,
}

/// M3 Dropdown Menu anchored to a trigger element.
///
/// Renders as a single overlay entry with a transparent full-screen scrim and
/// positioned card, matching Compose's Popup behavior. The card is bounded in
/// height so vertical_scroll activates when content overflows.
pub fn DropdownMenu(
    state: Rc<MenuState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    trigger: View,
    items: Vec<DropdownMenuEntry>,
    config: DropdownMenuConfig,
) -> View {
    let th = theme();
    let ddm_id = remember(|| DROPDOWN_COUNTER.fetch_add(1, Ordering::Relaxed));
    let overlay_id = remember_with_key(format!("ddm_oid_{ddm_id}"), || signal(0u64));
    let trigger_rect = remember_state_with_key(format!("ddm_tr_{ddm_id}"), Rect::default);
    let scroll_state: Rc<ScrollState> =
        remember_with_key(format!("ddm_scroll_{ddm_id}"), ScrollState::new);

    let trigger = Box(Modifier::new().on_globally_positioned({
        let tr = trigger_rect.clone();
        move |rect| {
            *tr.borrow_mut() = rect;
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

    if menu_visible {
        if overlay_id.get() == 0 {
            let anim = anim.clone();
            let th = th;
            let items = items.clone();
            let state = state.clone();
            let config = config.clone();
            let trigger_rect = trigger_rect.clone();
            let scroll_state = scroll_state.clone();

            let id = overlay.show_entry(
                Rc::new(move || {
                    let p = *anim.borrow().get();
                    let scale = DDM_SCALE_FROM + (1.0 - DDM_SCALE_FROM) * p;
                    let alpha = p;

                    let rect = *trigger_rect.borrow();
                    let win_h = get_window_container_height();
                    let hm = config.vertical_margin;

                    let space_below = (win_h - hm) - (rect.y + rect.h);
                    let space_above = rect.y - hm;

                    let estimated_h = estimate_dropdown_height(&items, &config)
                        .min(space_below.max(space_above))
                        .max(DDM_MIN_OPEN_HEIGHT);
                    let place_below = space_below >= estimated_h
                        || (space_above < estimated_h && space_below >= space_above);
                    let available_height = (if place_below {
                        space_below
                    } else {
                        space_above
                    })
                    .max(48.0);

                    let popup_x = rect.x + config.offset_x;
                    let constrained_width = config.max_width;

                    let mut adjusted_config = config.clone();
                    adjusted_config.max_width = constrained_width;

                    let content = render_dropdown_menu_content(
                        &th,
                        &items,
                        state.clone(),
                        &adjusted_config,
                        scroll_state.clone(),
                        available_height,
                    );

                    let transform_origin_y = if place_below { 0.0 } else { 1.0 };

                    let mut offset_modifier = Modifier::new();
                    if place_below {
                        offset_modifier = offset_modifier.offset(
                            Some(popup_x),
                            Some(rect.y + rect.h + config.offset_y),
                            None,
                            None,
                        );
                    } else {
                        let menu_bottom_y = rect.y + config.offset_y;
                        let offset_bottom = (win_h - menu_bottom_y).max(0.0);
                        offset_modifier =
                            offset_modifier.offset(Some(popup_x), None, None, Some(offset_bottom));
                    }

                    let menu = Box(offset_modifier
                        .absolute()
                        .scale(scale)
                        .alpha(alpha)
                        .transform_origin(0.0, transform_origin_y))
                    .child(content);

                    let scrim = Box(Modifier::new().fill_max_size().on_pointer_down({
                        let s = state.clone();
                        move |_| s.dismiss()
                    }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, menu))
                }),
                901.0,
                false,
            );
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
    }

    Box(modifier).child(trigger)
}

fn estimate_dropdown_height(items: &[DropdownMenuEntry], config: &DropdownMenuConfig) -> f32 {
    let mut h = 2.0 * DDM_VERTICAL_PADDING;
    for entry in items {
        match entry {
            DropdownMenuEntry::Item(_) => {
                h += config.item_height.max(DDM_ITEM_MIN_HEIGHT);
            }
            // Divider: 1px line + 12px horizontal margins (also vertical here).
            DropdownMenuEntry::Divider => h += 1.0 + 2.0 * 12.0,
        }
    }
    h
}

fn render_dropdown_menu_content(
    th: &Theme,
    items: &[DropdownMenuEntry],
    state: Rc<MenuState>,
    config: &DropdownMenuConfig,
    scroll_state: Rc<ScrollState>,
    max_height: f32,
) -> View {
    let children: Vec<View> = items
        .iter()
        .map(|entry| match entry {
            DropdownMenuEntry::Item(item) => {
                let text_color = if item.enabled {
                    config.item_text_color
                } else {
                    config.disabled_item_text_color
                };
                let on_click = item.on_click.clone();
                let state = state.clone();
                let item_source: Rc<MutableInteractionSource> =
                    remember(MutableInteractionSource::new);

                let mut modifier = Modifier::new()
                    .fill_max_width()
                    .min_height(config.item_height.max(DDM_ITEM_MIN_HEIGHT))
                    .padding_values(PaddingValues {
                        left: DDM_ITEM_H_PAD,
                        right: DDM_ITEM_H_PAD,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .align_items(AlignItems::CENTER);

                if item.enabled {
                    modifier = modifier
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            focused: th.on_surface.with_alpha_f32(0.12),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            dragged: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        })
                        .interaction_source(&item_source)
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
                            .single_line(),
                    ),
                );
                if let Some(icon) = item.trailing_icon.clone() {
                    row_children.push(Box(Modifier::new().width(DDM_ITEM_H_PAD)));
                    row_children.push(icon);
                }
                Row(modifier).child(row_children)
            }
            DropdownMenuEntry::Divider => Box(Modifier::new()
                .fill_max_width()
                .height(1.0)
                .margin(12.0)
                .background(config.divider_color)),
        })
        .collect();

    let binding = scroll_state.to_binding();
    let axis_binding = match &binding {
        ScrollBinding::Vertical(a) => a.clone(),
        _ => unreachable!(),
    };

    let items_column = Box(Modifier::new()
        .fill_max_width()
        .max_height((max_height - 2.0 * DDM_VERTICAL_PADDING).max(0.0))
        .vertical_scroll(axis_binding))
    .child(Column(Modifier::new().fill_max_width()).with_children(children));

    let shadow_elevation = config.shadow_elevation.unwrap_or(th.elevation.level2);

    let mut card_modifier = Modifier::new()
        .shadow(shadow_elevation, 0.0)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .padding_values(PaddingValues {
            left: 0.0,
            right: 0.0,
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

    Box(card_modifier).child(items_column)
}
