#![allow(non_snake_case)]

mod components;
pub use components::*;

use std::cell::RefCell;
use std::rc::Rc;

use repose_core::*;
use repose_ui::lazy::{LazyRow, LazyRowState};
use repose_ui::{
    Box, Column, Row, Spacer, Stack, Surface, Text, TextField, TextStyle, ViewExt,
    anim::animate_f32, overlay::OverlayHandle, overlay::SnackbarAction,
};
use web_time::Duration;

pub fn AlertDialog(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    title: View,
    text: View,
    confirm_button: View,
    dismiss_button: Option<View>,
) -> View {
    if !visible {
        return Box(Modifier::new());
    }

    let th = theme();
    Stack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .fill_max_size()
            .background(th.scrim.with_alpha(170))
            .clickable()
            .on_pointer_down(move |_| on_dismiss())),
        Surface(
            Modifier::new()
                .min_width(280.0)
                .max_width(560.0)
                .background(th.surface_container_high)
                .clip_rounded(th.shapes.extra_large)
                .padding(24.0),
            Column(Modifier::new()).child((
                title,
                Box(Modifier::new().size(1.0, 16.0)),
                text,
                Spacer(),
                Row(Modifier::new()).child((
                    dismiss_button.unwrap_or(Box(Modifier::new())),
                    Spacer(),
                    confirm_button,
                )),
            )),
        ),
    ))
}

pub fn BottomSheet(
    visible: bool,
    on_dismiss: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
) -> View {
    let max_h = animate_f32(
        "sheet_height",
        if visible { 500.0 } else { 0.0 },
        AnimationSpec::tween(Duration::from_millis(300), Easing::EaseOut),
    );

    Column(Modifier::new()).child((
        Box(if visible {
            modifier
                .clone()
                .fill_max_width()
                .max_height(max_h)
                .clip_rounded(4.0)
        } else {
            Modifier::new().width(0.0).height(0.0)
        })
        .child(if visible {
            content
        } else {
            Box(Modifier::new())
        }),
        if visible {
            Box(Modifier::new()
                .width(1.0)
                .height(0.0)
                .fill_max_width()
                .hit_passthrough()
                .on_pointer_down(move |_| on_dismiss()))
        } else {
            Box(Modifier::new())
        },
    ))
}

pub fn NavigationBar(selected_index: usize, items: Vec<NavItem>) -> View {
    Row(Modifier::new()
        .fill_max_size()
        .min_height(80.0)
        .background(theme().surface_container)
        .padding(8.0))
    .child(
        items
            .into_iter()
            .enumerate()
            .map(|(i, item)| NavigationBarItem(item, i == selected_index))
            .collect::<Vec<_>>(),
    )
}

pub struct NavItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

fn NavigationBarItem(item: NavItem, selected: bool) -> View {
    let th = theme();
    let color = if selected {
        th.primary
    } else {
        th.on_surface_variant
    };

    Column(
        Modifier::new()
            .flex_grow(1.0)
            .clickable()
            .on_pointer_down(move |_| (item.on_click)()),
    )
    .child((
        item.icon,
        Text(item.label)
            .color(color)
            .size(th.typography.label_medium)
            .single_line(),
    ))
}

pub fn Card(modifier: Modifier, content: View) -> View {
    let th = theme();
    Surface(
        modifier
            .background(th.surface_container_highest)
            .clip_rounded(th.shapes.medium),
        content,
    )
}

pub fn ElevatedCard(modifier: Modifier, content: View) -> View {
    let th = theme();
    Surface(
        modifier
            .background(th.surface_container_low)
            .state_elevation(StateElevation {
                default: th.elevation.level1,
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                disabled: 0.0,
            })
            .clip_rounded(th.shapes.medium),
        content,
    )
}

pub fn OutlinedCard(modifier: Modifier, content: View) -> View {
    let th = theme();
    Surface(
        modifier
            .background(th.surface)
            .border(1.0, th.outline_variant, th.shapes.medium)
            .clip_rounded(th.shapes.medium),
        content,
    )
}

fn card_state_colors(bg: Color) -> StateColors {
    let th = theme();
    StateColors {
        default: Color::TRANSPARENT,
        hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
        pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
        disabled: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
    }
}

pub fn ClickableCard(on_click: impl Fn() + 'static, modifier: Modifier, content: View) -> View {
    let th = theme();
    let bg = th.surface_container_highest;
    Surface(
        modifier
            .background(bg)
            .state_colors(card_state_colors(bg))
            .clip_rounded(th.shapes.medium)
            .clickable()
            .on_pointer_down(move |_| on_click()),
        content,
    )
}

pub fn ClickableElevatedCard(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
) -> View {
    let th = theme();
    let bg = th.surface;
    Surface(
        modifier
            .background(bg)
            .state_colors(card_state_colors(bg))
            .state_elevation(StateElevation {
                default: th.elevation.level1,
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                disabled: 0.0,
            })
            .clip_rounded(th.shapes.medium)
            .clickable()
            .on_pointer_down(move |_| on_click()),
        content,
    )
}

pub fn ClickableOutlinedCard(
    on_click: impl Fn() + 'static,
    modifier: Modifier,
    content: View,
) -> View {
    let th = theme();
    let bg = th.surface;
    Surface(
        modifier
            .background(bg)
            .state_colors(card_state_colors(bg))
            .border(1.0, th.outline_variant, th.shapes.medium)
            .clip_rounded(th.shapes.medium)
            .clickable()
            .on_pointer_down(move |_| on_click()),
        content,
    )
}

pub fn Snackbar(
    message: impl Into<String>,
    action: Option<SnackbarAction>,
    base_modifier: Modifier,
) -> View {
    let msg = message.into();
    let th = theme();
    let bg = th.inverse_surface;
    let fg = th.inverse_on_surface;
    let action_color = th.inverse_primary;

    let modifier = Modifier::new()
        .background(bg)
        .clip_rounded(th.shapes.small)
        .then(base_modifier)
        .padding_values(PaddingValues {
            left: 16.0,
            right: 16.0,
            top: 12.0,
            bottom: 12.0,
        })
        .min_height(48.0)
        .min_width(280.0)
        .max_width(600.0);

    Surface(
        modifier,
        Row(Modifier::new().align_items(repose_core::AlignItems::Center)).child((
            Text(msg)
                .color(fg)
                .size(th.typography.body_medium)
                .max_lines(2)
                .overflow_ellipsize(),
            Spacer(),
            action
                .map(|a| {
                    let label = a.label.clone();
                    Box(Modifier::new()
                        .padding_values(PaddingValues {
                            left: 8.0,
                            right: 8.0,
                            top: 6.0,
                            bottom: 6.0,
                        })
                        .clip_rounded(th.shapes.extra_small)
                        .clickable()
                        .on_pointer_down(move |_| (a.on_click)()))
                    .child(
                        Text(label)
                            .color(action_color)
                            .size(th.typography.label_large)
                            .single_line(),
                    )
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn FilterChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
) -> View {
    let th = theme();
    let bg = if selected {
        th.secondary_container
    } else {
        th.surface
    };
    let label_color = if selected {
        th.on_secondary_container
    } else {
        th.on_surface_variant
    };
    let leading_color = if selected {
        th.on_secondary_container
    } else {
        th.primary
    };
    let trailing_color = if selected {
        th.on_secondary_container
    } else {
        th.on_surface_variant
    };

    Surface(
        Modifier::new()
            .background(bg)
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
                pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
                disabled: Color::TRANSPARENT,
            })
            .border(
                1.0,
                if selected {
                    Color::TRANSPARENT
                } else {
                    th.outline_variant
                },
                8.0,
            )
            .clip_rounded(8.0)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 8.0,
                bottom: 8.0,
            })
            .clickable()
            .on_pointer_down(move |_| on_click()),
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn AssistChip(
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
) -> View {
    let th = theme();
    Surface(
        Modifier::new()
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08),
                pressed: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .border(1.0, th.outline_variant, 8.0)
            .clip_rounded(8.0)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 8.0,
                bottom: 8.0,
            })
            .clickable()
            .on_pointer_down(move |_| on_click()),
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            leading_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(th.primary, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(th.on_surface, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(th.primary, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn SuggestionChip(on_click: impl Fn() + 'static, label: View, icon: Option<View>) -> View {
    let th = theme();
    Surface(
        Modifier::new()
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08),
                pressed: th.on_surface.with_alpha_f32(0.12),
                disabled: Color::TRANSPARENT,
            })
            .border(1.0, th.outline_variant, 8.0)
            .clip_rounded(8.0)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 8.0,
                bottom: 8.0,
            })
            .clickable()
            .on_pointer_down(move |_| on_click()),
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            icon.map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 8.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(with_content_color(th.primary, move || v))
            })
            .unwrap_or(Box(Modifier::new())),
            with_content_color(th.on_surface_variant, move || label),
        )),
    )
}

pub fn InputChip(
    selected: bool,
    on_click: impl Fn() + 'static,
    label: View,
    leading_icon: Option<View>,
    avatar: Option<View>,
    trailing_icon: Option<View>,
) -> View {
    let th = theme();
    let bg = if selected {
        th.secondary_container
    } else {
        th.surface
    };
    let label_color = if selected {
        th.on_secondary_container
    } else {
        th.on_surface_variant
    };
    let leading_color = if selected {
        th.primary
    } else {
        th.on_surface_variant
    };
    let trailing_color = if selected {
        th.on_secondary_container
    } else {
        th.on_surface_variant
    };

    Surface(
        Modifier::new()
            .background(bg)
            .state_colors(StateColors {
                default: Color::TRANSPARENT,
                hovered: th.on_surface.with_alpha_f32(0.08).composite_over(bg),
                pressed: th.on_surface.with_alpha_f32(0.12).composite_over(bg),
                disabled: Color::TRANSPARENT,
            })
            .border(
                1.0,
                if selected {
                    Color::TRANSPARENT
                } else {
                    th.outline_variant
                },
                8.0,
            )
            .clip_rounded(8.0)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 8.0,
                bottom: 8.0,
            })
            .clickable()
            .on_pointer_down(move |_| on_click()),
        Row(Modifier::new().align_items(AlignItems::Center)).child((
            avatar
                .or(leading_icon)
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 0.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(leading_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
            with_content_color(label_color, move || label),
            trailing_icon
                .map(|v| {
                    Box(Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }))
                    .child(with_content_color(trailing_color, move || v))
                })
                .unwrap_or(Box(Modifier::new())),
        )),
    )
}

pub fn Scaffold(
    top_bar: Option<View>,
    bottom_bar: Option<View>,
    floating_action_button: Option<View>,
    content: impl Fn(PaddingValues) -> View,
) -> View {
    Stack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .fill_max_size()
            .padding_values(PaddingValues {
                top: if top_bar.is_some() { 64.0 } else { 0.0 },
                bottom: if bottom_bar.is_some() { 80.0 } else { 0.0 },
                ..Default::default()
            }))
        .child(content(PaddingValues::default())),
        if let Some(bar) = top_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), Some(0.0), Some(0.0), None))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(bar) = bottom_bar {
            Box(Modifier::new()
                .absolute()
                .offset(Some(0.0), None, Some(0.0), Some(0.0)))
            .child(bar)
        } else {
            Box(Modifier::new())
        },
        if let Some(fab) = floating_action_button {
            Box(Modifier::new()
                .absolute()
                .offset(None, None, Some(16.0), Some(16.0)))
            .child(fab)
        } else {
            Box(Modifier::new())
        },
    ))
}

/// State controlling tooltip visibility.
pub struct TooltipState {
    visible: Signal<bool>,
}

impl TooltipState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            visible: signal(false),
        })
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

/// Wraps `content` with a tooltip label shown above it when `state` is visible.
///
/// Usage:
/// ```ignore
/// let tip = TooltipState::new();
/// TooltipBox("I'm a tooltip", tip.clone(), Modifier::new(), Button("Hover me", {
///     let tip = tip.clone();
///     move || tip.show()
/// }));
/// ```
pub fn TooltipBox(
    text: impl Into<String>,
    state: Rc<TooltipState>,
    modifier: Modifier,
    content: View,
) -> View {
    let text: Rc<str> = Rc::from(text.into());
    let th = theme();

    let tooltip = state.is_visible().then(|| {
        let bg = th.inverse_surface;
        let fg = th.inverse_on_surface;
        Box(Modifier::new()
            .background(bg)
            .clip_rounded(th.shapes.extra_small)
            .padding_values(PaddingValues {
                left: 8.0,
                right: 8.0,
                top: 4.0,
                bottom: 4.0,
            })
            .absolute()
            .offset(None, Some(-28.0), None, None)
            .align_self(AlignSelf::Center)
            .render_z_index(10000.0))
        .child(
            Text((*text).to_string())
                .color(fg)
                .size(th.typography.label_medium)
                .single_line(),
        )
    });

    Stack(modifier).child((
        Box(Modifier::new().fill_max_size()).child(content),
        tooltip.unwrap_or(Box(Modifier::new())),
    ))
}

/// State controlling drawer open/close.
pub struct DrawerState {
    visible: Signal<bool>,
}

impl DrawerState {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            visible: signal(false),
        })
    }

    pub fn is_open(&self) -> bool {
        self.visible.get()
    }

    pub fn open(&self) {
        self.visible.set(true);
    }

    pub fn dismiss(&self) {
        self.visible.set(false);
    }
}

/// A modal navigation drawer that slides in from the left with a scrim overlay.
pub fn ModalNavigationDrawer(
    drawer_state: Rc<DrawerState>,
    drawer_content: View,
    content: View,
) -> View {
    let th = theme();

    let drawer_offset = animate_f32(
        "modal_drawer_offset",
        if drawer_state.is_open() { 0.0 } else { -360.0 },
        AnimationSpec::spring_gentle(),
    );

    Stack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new().fill_max_size()).child(content),
        if drawer_state.is_open() {
            Box(Modifier::new()
                .fill_max_size()
                .background(th.scrim.with_alpha(82))
                .clickable()
                .on_pointer_down({
                    let ds = drawer_state.clone();
                    move |_| ds.dismiss()
                }))
            .child(Box(Modifier::new()))
        } else {
            Box(Modifier::new())
        },
        Box(Modifier::new()
            .absolute()
            .offset(Some(drawer_offset), Some(0.0), None, Some(0.0))
            .fill_max_height()
            .width(300.0)
            .background(th.surface_container_low)
            .clip_rounded(th.shapes.large))
        .child(drawer_content),
    ))
}

/// A destination entry inside a NavigationDrawer.
pub fn NavigationDrawerItem(
    label: View,
    selected: bool,
    on_click: impl Fn() + 'static,
    icon: Option<View>,
    badge: Option<View>,
) -> View {
    let th = theme();
    let bg = if selected {
        th.secondary_container
    } else {
        Color::TRANSPARENT
    };
    let fg = if selected {
        th.on_secondary_container
    } else {
        th.on_surface_variant
    };

    Surface(
        Modifier::new()
            .fill_max_width()
            .padding_values(PaddingValues {
                left: 12.0,
                right: 12.0,
                top: 0.0,
                bottom: 0.0,
            })
            .min_height(56.0)
            .background(bg)
            .clip_rounded(28.0)
            .clickable()
            .on_pointer_down(move |_| on_click()),
        with_content_color(fg, move || {
            Row(Modifier::new()
                .align_items(AlignItems::Center)
                .padding_values(PaddingValues {
                    left: 16.0,
                    right: 24.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
            .child((
                icon.unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
                Box(Modifier::new().width(12.0).height(1.0)),
                Box(Modifier::new().flex_grow(1.0)).child(label),
                badge.unwrap_or(Box(Modifier::new())),
            ))
        }),
    )
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

/// M3 Dropdown Menu anchored to a trigger element.
///
/// Renders a full-screen scrim in the overlay (to dismiss taps outside the card)
/// while keeping the menu positioned inline below the trigger for correct position.
/// Items can be `DropdownMenuItem` or `MenuDivider`.
pub fn DropdownMenu(
    state: Rc<MenuState>,
    overlay: OverlayHandle,
    modifier: Modifier,
    trigger: View,
    items: Vec<DropdownMenuEntry>,
) -> View {
    let th = theme();
    let overlay_id = remember_with_key("ddm_oid", || signal(0u64));

    if state.is_open() {
        if overlay_id.get() == 0 {
            let scrim = Box(Modifier::new().fill_max_size().absolute().on_pointer_down({
                let s = state.clone();
                move |_| s.dismiss()
            }));
            let id = overlay.show_with(scrim, 899.0, true);
            overlay_id.set(id);
        }
    } else {
        let prev = overlay_id.get();
        if prev != 0 {
            let _ = overlay.dismiss(prev);
            overlay_id.set(0);
        }
    }

    Stack(modifier).child((
        trigger,
        if state.is_open() {
            Box(Modifier::new()
                .absolute()
                .offset(None, Some(40.0), None, None)
                .render_z_index(900.0))
            .child(render_dropdown_menu_content(&th, &items, state.clone()))
        } else {
            Box(Modifier::new())
        },
    ))
}

/// Either a menu item or a divider.
#[derive(Clone)]
pub enum DropdownMenuEntry {
    Item(DropdownMenuItem),
    Divider,
}

fn render_dropdown_menu_content(
    th: &Theme,
    items: &[DropdownMenuEntry],
    state: Rc<MenuState>,
) -> View {
    let children: Vec<View> = items
        .iter()
        .map(|entry| match entry {
            DropdownMenuEntry::Item(item) => {
                let text_color = if item.enabled {
                    th.on_surface
                } else {
                    th.on_surface.with_alpha_f32(0.38)
                };
                let on_click = item.on_click.clone();
                let state = state.clone();
                let mut modifier = Modifier::new()
                    .fill_max_width()
                    .min_height(40.0)
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 0.0,
                        bottom: 0.0,
                    })
                    .align_items(AlignItems::Center);

                if item.enabled {
                    modifier = modifier
                        .state_colors(StateColors {
                            default: Color::TRANSPARENT,
                            hovered: th.on_surface.with_alpha_f32(0.08),
                            pressed: th.on_surface.with_alpha_f32(0.12),
                            disabled: Color::TRANSPARENT,
                        })
                        .clickable()
                        .on_pointer_down(move |_| {
                            on_click();
                            state.dismiss();
                        });
                }

                Row(modifier).child((
                    item.leading_icon
                        .clone()
                        .unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
                    Box(Modifier::new().size(12.0, 1.0)),
                    Box(Modifier::new().flex_grow(1.0)).child(
                        Text(item.text.clone())
                            .color(text_color)
                            .size(th.typography.body_large)
                            .single_line(),
                    ),
                    item.trailing_icon.clone().unwrap_or(Box(Modifier::new())),
                ))
            }
            DropdownMenuEntry::Divider => Box(Modifier::new()
                .fill_max_width()
                .height(1.0)
                .margin(12.0)
                .background(th.outline_variant)),
        })
        .collect();

    Surface(
        Modifier::new()
            .background(th.surface_container)
            .clip_rounded(th.shapes.small)
            .state_elevation(StateElevation {
                default: th.elevation.level2,
                hovered: th.elevation.level3,
                pressed: th.elevation.level3,
                disabled: 0.0,
            })
            .min_width(112.0)
            .padding(4.0),
        Column(Modifier::new()).with_children(children),
    )
}

/// State for `SearchBar` - manages expanded/collapsed and query text.
pub struct SearchBarState {
    pub query: Signal<String>,
    pub expanded: Signal<bool>,
    pub active: Signal<bool>,
}

impl SearchBarState {
    pub fn new() -> Self {
        Self {
            query: signal(String::new()),
            expanded: signal(false),
            active: signal(false),
        }
    }

    pub fn query(&self) -> String {
        self.query.get()
    }

    pub fn set_query(&self, q: String) {
        self.query.set(q);
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn expand(&self) {
        self.expanded.set(true);
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
        self.active.set(false);
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn activate(&self) {
        self.active.set(true);
        self.expanded.set(true);
    }

    pub fn deactivate(&self) {
        self.active.set(false);
    }
}

/// M3 Search Bar - an expandable search input with leading icon and optional
/// suggestions that expand below the search bar (pushing content down).
pub fn SearchBar(
    state: Rc<SearchBarState>,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    placeholder: impl Into<String>,
    on_query_change: Option<Rc<dyn Fn(String)>>,
    content: View,
) -> View {
    let th = theme();
    let placeholder = placeholder.into();
    let expanded = state.is_expanded();
    let query = state.query();
    let active = state.is_active();

    let width = animate_f32(
        "searchbar_width",
        if expanded { 360.0 } else { 240.0 },
        AnimationSpec::tween(Duration::from_millis(200), Easing::EaseOut),
    );

    let input_field: View = if active {
        TextField(
            placeholder.clone(),
            Modifier::new().flex_grow(1.0).padding(4.0),
            Some({
                let s = state.clone();
                let cb = on_query_change.clone();
                move |text| {
                    s.set_query(text);
                    if let Some(ref cb) = cb {
                        cb(s.query());
                    }
                }
            }),
            None::<fn(String)>,
        )
        .color(th.on_surface)
        .size(th.typography.body_large)
    } else {
        Box(Modifier::new().flex_grow(1.0)).child(
            Text(if query.is_empty() {
                placeholder.clone()
            } else {
                query.clone()
            })
            .color(if query.is_empty() {
                th.on_surface_variant
            } else {
                th.on_surface
            })
            .size(th.typography.body_large)
            .single_line(),
        )
    };

    let bar_modifier = modifier.clone();
    let bar = Surface(
        bar_modifier
            .width(width)
            .height(56.0)
            .background(if active {
                th.surface_container_high
            } else {
                th.surface_container
            })
            .state_elevation(StateElevation {
                default: if active { th.elevation.level3 } else { 0.0 },
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                disabled: 0.0,
            })
            .clip_rounded(th.shapes.large)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 0.0,
                bottom: 0.0,
            })
            .clickable()
            .on_pointer_down({
                let s = state.clone();
                move |_| s.activate()
            }),
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::Center))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().size(8.0, 1.0)),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    );

    if expanded {
        Stack(modifier).child((
            bar,
            Box(Modifier::new()
                .width(width)
                .max_height(400.0)
                .background(th.surface_container)
                .clip_rounded(th.shapes.small))
            .child(content),
        ))
    } else {
        bar
    }
}

/// M3 Docked Search Bar - full-width variant anchored to the top of the screen.
/// Shows a search bar with animated suggestions dropdown.
pub fn DockedSearchBar(
    state: Rc<SearchBarState>,
    modifier: Modifier,
    leading_icon: Option<View>,
    placeholder: impl Into<String>,
    on_query_change: Option<Rc<dyn Fn(String)>>,
    content: View,
) -> View {
    let th = theme();
    let placeholder = placeholder.into();
    let expanded = state.is_expanded();
    let query = state.query();
    let active = state.is_active();

    let content_target = if expanded { 400.0 } else { 0.0 };
    let content_height = animate_f32(
        "docked_sh",
        content_target,
        AnimationSpec::tween(Duration::from_millis(250), Easing::FastOutSlowIn),
    );
    let content_alpha = animate_f32(
        "docked_sa",
        if expanded { 1.0 } else { 0.0 },
        AnimationSpec::tween(Duration::from_millis(200), Easing::FastOutSlowIn),
    );

    let input_field: View = if active {
        TextField(
            placeholder.clone(),
            Modifier::new().flex_grow(1.0),
            Some({
                let s = state.clone();
                let cb = on_query_change.clone();
                move |text| {
                    s.set_query(text);
                    if let Some(ref cb) = cb {
                        cb(s.query());
                    }
                }
            }),
            None::<fn(String)>,
        )
        .color(th.on_surface)
        .size(th.typography.body_large)
    } else {
        Box(Modifier::new().flex_grow(1.0)).child(if query.is_empty() {
            Text(placeholder.clone())
                .color(th.on_surface_variant)
                .size(th.typography.body_large)
                .single_line()
        } else {
            Text(query.clone())
                .color(th.on_surface)
                .size(th.typography.body_large)
                .single_line()
        })
    };

    let bar = Surface(
        modifier
            .fill_max_width()
            .height(56.0)
            .background(if active {
                th.surface_container_high
            } else {
                th.surface_container
            })
            .state_elevation(StateElevation {
                default: if active { th.elevation.level3 } else { 0.0 },
                hovered: th.elevation.level2,
                pressed: th.elevation.level3,
                disabled: 0.0,
            })
            .clip_rounded(th.shapes.large)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 16.0,
                top: 0.0,
                bottom: 0.0,
            })
            .clickable()
            .on_pointer_down({
                let s = state.clone();
                move |_| {
                    if !s.is_active() {
                        s.activate()
                    }
                }
            }),
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::Center))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().size(12.0, 1.0)),
            input_field,
            if active {
                Box(Modifier::new()
                    .size(24.0, 24.0)
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| {
                            s.set_query(String::new());
                            s.collapse();
                        }
                    }))
                .child(Text("✕").size(16.0).color(th.on_surface_variant))
            } else {
                Box(Modifier::new())
            },
        )),
    );

    let show_content = expanded || content_height > 1.0;
    if show_content {
        Column(Modifier::new().fill_max_width()).child((
            bar,
            Box(Modifier::new()
                .fill_max_width()
                .height(content_height)
                .alpha(content_alpha)
                .clip_rounded(th.shapes.small)
                .background(th.surface_container)
                .state_elevation(StateElevation {
                    default: th.elevation.level3,
                    hovered: th.elevation.level3,
                    pressed: th.elevation.level3,
                    disabled: 0.0,
                }))
            .child(
                Column(Modifier::new().fill_max_width()).child((
                    Box(Modifier::new()
                        .fill_max_width()
                        .height(1.0)
                        .background(th.outline_variant)),
                    content,
                )),
            ),
        ))
    } else {
        bar
    }
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
) -> View {
    let th = theme();
    let peek = state.peek_height.get();
    let overlay_id = remember_with_key("mbs_oid", || signal(0u64));

    // Animated offset: 600px (off-screen) → 0px (visible)
    let anim = remember_state_with_key("mbs_anim", || {
        AnimatedValue::new(600.0, AnimationSpec::spring_gentle())
    });
    let last_target = remember_state_with_key("mbs_anim_target", || f32::NAN);
    let anim_target = if state.is_visible() { 0.0 } else { 600.0 };

    {
        let mut a = anim.borrow_mut();
        let mut lt = last_target.borrow_mut();
        if lt.is_nan() || (*lt - anim_target).abs() > 1e-6 {
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
    let sheet_visible = state.is_visible() || offset < 590.0;

    if sheet_visible {
        if overlay_id.get() == 0 {
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let anim = anim.clone();
                let modifier = modifier.clone();
                let content = content.clone();
                let th = th.clone();
                move || {
                    let off = *anim.borrow().get();
                    let modif = modifier.clone();
                    let peek_h = state.peek_height.get();
                    let sheet = Box(modif
                        .fill_max_width()
                        .absolute()
                        .offset(None, Some(off), Some(0.0), Some(0.0))
                        .min_height(peek_h.max(48.0))
                        .background(th.surface_container_low)
                        .clip_rounded(th.shapes.large))
                    .child(
                        Column(Modifier::new().fill_max_width()).child((
                            Row(Modifier::new()
                                .fill_max_width()
                                .justify_content(JustifyContent::Center))
                            .child(Box(Modifier::new()
                                .margin_vertical(8.0)
                                .width(32.0)
                                .height(4.0)
                                .background(th.on_surface_variant.with_alpha_f32(0.4))
                                .clip_rounded(2.0))),
                            content.clone(),
                        )),
                    );

                    let visible = state.is_visible();
                    let scrim = if visible {
                        Box(Modifier::new()
                            .fill_max_size()
                            .background(th.scrim.with_alpha(85))
                            .on_pointer_down({
                                let s = state.clone();
                                move |_| s.dismiss()
                            }))
                    } else {
                        Box(Modifier::new())
                    };

                    Stack(Modifier::new().fill_max_size().absolute()).child((scrim, sheet))
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

/// State for `PullToRefresh` - tracks pull progress and refresh trigger.
pub struct PullToRefreshState {
    refreshing: Signal<bool>,
    pull_offset: Signal<f32>,
    enabled: bool,
}

impl PullToRefreshState {
    pub fn new() -> Self {
        Self {
            refreshing: signal(false),
            pull_offset: signal(0.0),
            enabled: true,
        }
    }

    pub fn is_refreshing(&self) -> bool {
        self.refreshing.get()
    }

    pub fn set_refreshing(&self, v: bool) {
        self.refreshing.set(v);
    }

    pub fn pull_offset(&self) -> f32 {
        self.pull_offset.get()
    }
}

/// Wraps scrollable content with a pull-to-refresh indicator.
///
/// Renders a small spinner at the top when the user pulls down past a threshold,
/// or shows the current pull offset as a visual indicator.
pub fn PullToRefresh(
    state: Rc<PullToRefreshState>,
    modifier: Modifier,
    on_refresh: Rc<dyn Fn()>,
    content: View,
) -> View {
    let th = theme();
    let pull = state.pull_offset.get();
    let refreshing = state.is_refreshing();

    Stack(modifier).child((
        // Indicator area (hidden behind content via z-ordering)
        if pull > 0.0 || refreshing {
            Box(Modifier::new()
                .fill_max_width()
                .height(pull.clamp(0.0, 64.0))
                .absolute()
                .offset(None, Some(-pull), None, None)
                .align_items(AlignItems::Center)
                .justify_content(JustifyContent::Center))
            .child(if refreshing {
                Text("↻")
                    .size(24.0)
                    .color(th.primary)
                    .modifier(Modifier::new())
            } else {
                let p = (pull / 64.0).min(1.0);
                Text("↓")
                    .size((16.0 + p * 8.0).min(24.0))
                    .color(th.primary.with_alpha_f32(p))
                    .modifier(Modifier::new())
            })
        } else {
            Box(Modifier::new())
        },
        // Content shifted down by pull offset
        Box(Modifier::new()
            .absolute()
            .offset(None, Some(pull), None, None))
        .child(content),
    ))
}

/// State for `DatePicker` - manages selected date.
pub struct DatePickerState {
    pub year: Signal<i32>,
    pub month: Signal<u32>, // 1-12
    pub day: Signal<u32>,
}

impl DatePickerState {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self {
            year: signal(year),
            month: signal(month.clamp(1, 12)),
            day: signal(day.clamp(1, 31)),
        }
    }

    pub fn selected_date(&self) -> (i32, u32, u32) {
        (self.year.get(), self.month.get(), self.day.get())
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

/// M3 Date Picker dialog - a simple date picker with month navigation.
pub fn DatePicker(
    state: Rc<DatePickerState>,
    on_confirm: Rc<dyn Fn(i32, u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
) -> View {
    let th = theme();
    let (year, month, day) = state.selected_date();
    let dim = days_in_month(year, month);

    let month_names = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];

    // Previous month
    let prev_month = {
        let s = state.clone();
        move || {
            if s.month.get() == 1 {
                s.year.set(s.year.get() - 1);
                s.month.set(12);
            } else {
                s.month.set(s.month.get() - 1);
            }
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };

    // Next month
    let next_month = {
        let s = state.clone();
        move || {
            if s.month.get() == 12 {
                s.year.set(s.year.get() + 1);
                s.month.set(1);
            } else {
                s.month.set(s.month.get() + 1);
            }
            let d = days_in_month(s.year.get(), s.month.get());
            if s.day.get() > d {
                s.day.set(d);
            }
        }
    };

    Surface(
        Modifier::new()
            .width(328.0)
            .background(th.surface_container_high)
            .clip_rounded(th.shapes.extra_large)
            .padding(16.0),
        Column(Modifier::new()).child((
            // Month/Year header with nav
            Row(Modifier::new()
                .fill_max_width()
                .align_items(AlignItems::Center))
            .child((
                IconButton(
                    Box(Modifier::new()).child(Text("◀").color(th.on_surface).size(16.0)),
                    prev_month,
                ),
                Spacer(),
                Text(format!("{} {}", month_names[(month - 1) as usize], year))
                    .size(th.typography.title_medium)
                    .color(th.on_surface),
                Spacer(),
                IconButton(
                    Box(Modifier::new()).child(Text("▶").color(th.on_surface).size(16.0)),
                    next_month,
                ),
            )),
            Box(Modifier::new().size(1.0, 16.0)),
            // Day grid
            Column(Modifier::new()).child({
                let mut rows: Vec<View> = Vec::new();
                // Day-of-week headers
                let dow_headers: Vec<View> = ["M", "T", "W", "T", "F", "S", "S"]
                    .iter()
                    .map(|d| {
                        Box(Modifier::new()
                            .width(40.0)
                            .height(40.0)
                            .align_items(AlignItems::Center)
                            .justify_content(JustifyContent::Center))
                        .child(
                            Text(d.to_string())
                                .size(th.typography.label_small)
                                .color(th.on_surface_variant),
                        )
                    })
                    .collect();
                rows.push(Row(Modifier::new()).with_children(dow_headers));

                // Simple grid: 6 rows of 7 days
                for w in 0..6 {
                    let mut week: Vec<View> = Vec::new();
                    for d in 0..7 {
                        let day_num = w * 7 + d + 1;
                        if day_num <= dim as i32 {
                            let is_selected = day_num == day as i32;
                            let s = state.clone();
                            let on_confirm = on_confirm.clone();
                            let on_dismiss_fn = on_dismiss.clone();
                            week.push(
                                Box(Modifier::new()
                                    .width(40.0)
                                    .height(40.0)
                                    .background(if is_selected {
                                        th.primary
                                    } else {
                                        Color::TRANSPARENT
                                    })
                                    .clip_rounded(20.0)
                                    .align_items(AlignItems::Center)
                                    .justify_content(JustifyContent::Center)
                                    .clickable()
                                    .on_pointer_down(move |_| {
                                        s.day.set(day_num as u32);
                                        on_confirm(s.year.get(), s.month.get(), day_num as u32);
                                        on_dismiss_fn();
                                    }))
                                .child(
                                    Text(day_num.to_string())
                                        .size(th.typography.body_medium)
                                        .color(if is_selected {
                                            th.on_primary
                                        } else {
                                            th.on_surface
                                        }),
                                ),
                            );
                        } else {
                            week.push(Box(Modifier::new().width(40.0).height(40.0)));
                        }
                    }
                    rows.push(Row(Modifier::new()).with_children(week));
                }
                rows
            }),
        )),
    )
}

/// State for `TimePicker` - manages selected hour and minute.
pub struct TimePickerState {
    pub hour: Signal<u32>,
    pub minute: Signal<u32>,
    pub is_am: Signal<bool>,
}

impl TimePickerState {
    pub fn new(hour: u32, minute: u32) -> Self {
        let h = hour % 12;
        let am = hour < 12;
        Self {
            hour: signal(if h == 0 { 12 } else { h }),
            minute: signal(minute.min(59)),
            is_am: signal(am),
        }
    }

    pub fn selected_time(&self) -> (u32, u32) {
        let mut h = self.hour.get();
        if !self.is_am.get() {
            h = (h % 12) + 12;
        } else if h == 12 {
            h = 0;
        }
        (h, self.minute.get())
    }
}

/// M3 Time Picker - a simple time picker with hour/minute fields and AM/PM toggle.
pub fn TimePicker(
    state: Rc<TimePickerState>,
    on_confirm: Rc<dyn Fn(u32, u32)>,
    on_dismiss: Rc<dyn Fn()>,
) -> View {
    let th = theme();
    let hour = state.hour.get();
    let minute = state.minute.get();
    let is_am = state.is_am.get();

    let hour_str = format!("{:02}", hour);
    let min_str = format!("{:02}", minute);

    Surface(
        Modifier::new()
            .width(256.0)
            .background(th.surface_container_high)
            .clip_rounded(th.shapes.extra_large)
            .padding(24.0),
        Column(Modifier::new().align_items(AlignItems::Center)).child((
            // Time display
            Row(Modifier::new().align_items(AlignItems::Center)).child((
                Box(Modifier::new()
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| s.hour.set((s.hour.get() % 12) + 1)
                    })
                    .padding(8.0))
                .child(Text(hour_str).size(48.0).color(th.on_surface).single_line()),
                Text(":")
                    .size(48.0)
                    .color(th.on_surface_variant)
                    .single_line(),
                Box(Modifier::new()
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| s.minute.set((s.minute.get() + 1) % 60)
                    })
                    .padding(8.0))
                .child(Text(min_str).size(48.0).color(th.on_surface).single_line()),
            )),
            Box(Modifier::new().size(1.0, 16.0)),
            // AM/PM toggle
            Row(Modifier::new().align_items(AlignItems::Center)).child((
                Box(Modifier::new()
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 4.0,
                        bottom: 4.0,
                    })
                    .background(if is_am {
                        th.primary
                    } else {
                        Color::TRANSPARENT
                    })
                    .clip_rounded(8.0)
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| {
                            if !s.is_am.get() {
                                s.is_am.set(true);
                                let h = s.hour.get();
                                s.hour.set(if h == 12 { 12 } else { (h + 12) % 24 });
                                if s.hour.get() == 0 {
                                    s.hour.set(12);
                                }
                            }
                        }
                    }))
                .child(
                    Text("AM").size(th.typography.label_large).color(if is_am {
                        th.on_primary
                    } else {
                        th.on_surface
                    }),
                ),
                Box(Modifier::new().width(8.0).height(1.0)),
                Box(Modifier::new()
                    .padding_values(PaddingValues {
                        left: 12.0,
                        right: 12.0,
                        top: 4.0,
                        bottom: 4.0,
                    })
                    .background(if !is_am {
                        th.primary
                    } else {
                        Color::TRANSPARENT
                    })
                    .clip_rounded(8.0)
                    .clickable()
                    .on_pointer_down({
                        let s = state.clone();
                        move |_| {
                            if s.is_am.get() {
                                s.is_am.set(false);
                                let h = s.hour.get();
                                s.hour.set(if h == 12 { 12 } else { (h + 12) % 24 });
                                if s.hour.get() == 0 {
                                    s.hour.set(12);
                                }
                            }
                        }
                    }))
                .child(
                    Text("PM").size(th.typography.label_large).color(if !is_am {
                        th.on_primary
                    } else {
                        th.on_surface
                    }),
                ),
            )),
            Box(Modifier::new().size(1.0, 16.0)),
            Row(Modifier::new().fill_max_width()).child((
                Spacer(),
                Box(Modifier::new().padding(8.0).clickable().on_pointer_down({
                    let on_dismiss = on_dismiss.clone();
                    move |_| on_dismiss()
                }))
                .child(
                    Text("Cancel")
                        .color(th.primary)
                        .size(th.typography.label_large)
                        .single_line(),
                ),
                Box(Modifier::new().width(8.0).height(1.0)),
                Box(Modifier::new()
                    .padding(8.0)
                    .clickable()
                    .on_pointer_down(move |_| {
                        let (h, m) = state.selected_time();
                        on_confirm(h, m);
                        on_dismiss();
                    }))
                .child(
                    Text("OK")
                        .color(th.primary)
                        .size(th.typography.label_large)
                        .single_line(),
                ),
            )),
        )),
    )
}

/// A destination entry inside a NavigationRail.
pub struct NavRailItem {
    pub icon: View,
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
    pub badge: Option<View>,
}

/// M3 Navigation Rail — a compact vertical navigation sidebar.
///
/// Typically placed on the left side of the screen. Contains navigation items
/// (icon + label) and optional header / FAB sections.
pub fn NavigationRail(
    selected_index: usize,
    items: Vec<NavRailItem>,
    header: Option<View>,
    fab: Option<View>,
) -> View {
    let th = theme();

    let mut rail_children: Vec<View> = Vec::new();

    let has_header = header.is_some();
    let has_fab = fab.is_some();

    if let Some(h) = header {
        rail_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 12.0,
                    bottom: 12.0,
                })
                .align_self(AlignSelf::Center))
            .child(h),
        );
    }

    if let Some(f) = fab {
        rail_children.push(
            Box(Modifier::new()
                .padding_values(PaddingValues {
                    left: 12.0,
                    right: 12.0,
                    top: 8.0,
                    bottom: 8.0,
                })
                .align_self(AlignSelf::Center))
            .child(f),
        );
    }

    // Divider after header/FAB area
    if has_header || has_fab {
        rail_children.push(Box(Modifier::new()
            .size(80.0, 1.0)
            .fill_max_width()
            .background(th.outline_variant)));
    }

    for (i, item) in items.into_iter().enumerate() {
        let selected = i == selected_index;
        let fg = if selected {
            th.on_secondary_container
        } else {
            th.on_surface_variant
        };
        let bg = if selected {
            th.secondary_container
        } else {
            Color::TRANSPARENT
        };
        let cb = item.on_click.clone();

        rail_children.push(
            Column(
                Modifier::new()
                    .fill_max_width()
                    .padding_values(PaddingValues {
                        left: 4.0,
                        right: 4.0,
                        top: 4.0,
                        bottom: 4.0,
                    })
                    .align_items(AlignItems::Center)
                    .justify_content(JustifyContent::Center)
                    .state_colors(StateColors {
                        default: Color::TRANSPARENT,
                        hovered: th.on_surface.with_alpha_f32(0.08),
                        pressed: th.on_surface.with_alpha_f32(0.12),
                        disabled: Color::TRANSPARENT,
                    })
                    .clip_rounded(16.0)
                    .clickable()
                    .on_pointer_down(move |_| cb()),
            )
            .child((
                Stack(Modifier::new()).child((
                    Box(Modifier::new().size(24.0, 24.0))
                        .child(with_content_color(fg, move || item.icon)),
                    item.badge
                        .map(|b| {
                            Box(Modifier::new()
                                .absolute()
                                .offset(None, None, None, Some(0.0)))
                            .child(b)
                        })
                        .unwrap_or(Box(Modifier::new())),
                )),
                Box(Modifier::new().size(1.0, 4.0)),
                Text(item.label)
                    .color(fg)
                    .size(th.typography.label_medium)
                    .single_line(),
            )),
        );
    }

    Column(
        Modifier::new()
            .width(80.0)
            .fill_max_height()
            .background(th.surface)
            .align_items(AlignItems::Center),
    )
    .with_children(rail_children)
}

/// State for `SwipeToDismiss` — tracks swipe offset with spring animation.
pub struct SwipeToDismissState {
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Set when `dismiss()` starts the spring; checked when the spring settles
    /// to fire `on_dismiss` exactly once per dismiss gesture.
    dismiss_handled: Rc<RefCell<bool>>,
}

impl SwipeToDismissState {
    pub fn new() -> Self {
        Self {
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            dismiss_handled: Rc::new(RefCell::new(true)),
        }
    }

    /// Read and tick the animated offset.  Call every frame that needs the value.
    /// Requests a re-draw while the spring is still running.
    pub fn offset(&self) -> f32 {
        let mut anim = self.anim.borrow_mut();
        if anim.update() {
            request_frame();
        }
        *anim.get()
    }

    /// Snap instantly to an offset (used during active drag).
    pub fn set_offset_instant(&self, off: f32) {
        self.anim.borrow_mut().snap_to(off);
        request_frame();
    }

    /// Whether the current position is past the dismiss threshold.
    pub fn is_dismissed(&self) -> bool {
        *self.anim.borrow().get() < -250.0
    }

    /// Animate to the dismissed position.
    pub fn dismiss(&self) {
        *self.dismiss_handled.borrow_mut() = false;
        self.anim.borrow_mut().set_target(-300.0);
        request_frame();
    }

    /// Animate back to origin.
    pub fn reset(&self) {
        *self.dismiss_handled.borrow_mut() = true;
        self.anim.borrow_mut().set_target(0.0);
        request_frame();
    }

    /// Fire the dismiss callback once when the spring settles past threshold.
    fn try_handle_dismiss(&self, on_dismiss: &Option<Rc<dyn Fn()>>) {
        let anim = self.anim.borrow();
        if !anim.is_animating() && !*self.dismiss_handled.borrow() && *anim.get() < -150.0 {
            *self.dismiss_handled.borrow_mut() = true;
            if let Some(cb) = on_dismiss {
                cb();
            }
        }
    }
}

/// M3 SwipeToDismiss — wraps content that can be swiped left to reveal
/// a `background` action view.  On release past the threshold the content
/// springs to the dismissed position and `on_dismiss` fires **once**.
pub fn SwipeToDismiss(
    state: Rc<SwipeToDismissState>,
    on_dismiss: Option<Rc<dyn Fn()>>,
    background: View,
    content: View,
    modifier: Modifier,
) -> View {
    let offset = state.offset();
    state.try_handle_dismiss(&on_dismiss);

    let drag_start_x = remember_with_key("swipe_drag_start", || RefCell::new(None::<f32>));
    let drag_base = remember_with_key("swipe_drag_base", || RefCell::new(0.0f32));

    let st = state.clone();
    let on_down = {
        let d = drag_start_x.clone();
        let base = drag_base.clone();
        move |e: PointerEvent| {
            *d.borrow_mut() = Some(e.position.x);
            *base.borrow_mut() = *st.anim.borrow().get();
        }
    };

    let st = state.clone();
    let on_move = {
        let d = drag_start_x.clone();
        let base = drag_base.clone();
        move |e: PointerEvent| {
            if let Some(start) = *d.borrow() {
                let dx = e.position.x - start;
                st.set_offset_instant(*base.borrow() + dx);
            }
        }
    };

    let st = state.clone();
    let on_up = {
        let d = drag_start_x.clone();
        move |_e: PointerEvent| {
            *d.borrow_mut() = None;
            let off = *st.anim.borrow().get();
            if off > -50.0 {
                st.reset();
            } else {
                st.dismiss();
            }
        }
    };

    let display_offset = offset.max(-300.0).min(0.0);

    Stack(modifier.fill_max_width()).child((
        Box(Modifier::new().fill_max_width()).child(background),
        Box(Modifier::new()
            .fill_max_width()
            .translate(display_offset, 0.0)
            .on_pointer_down(on_down)
            .on_pointer_move(on_move)
            .on_pointer_up(on_up))
        .child(content),
    ))
}

/// M3 Carousel — a horizontally scrolling container with peek edges.
///
/// Uses a `LazyRow` internally. The first and last items are partially visible
/// (peek) to indicate there is more scrollable content.
pub fn Carousel<T, F>(
    items: Vec<T>,
    item_width: f32,
    peek_amount: f32,
    modifier: Modifier,
    state: Rc<LazyRowState>,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let padded_modifier = modifier.clone().padding_values(PaddingValues {
        left: peek_amount,
        right: peek_amount,
        top: 0.0,
        bottom: 0.0,
    });

    LazyRow(items, item_width, state, padded_modifier, item_builder)
}
