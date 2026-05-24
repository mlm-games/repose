#![allow(non_snake_case)]

mod components;
pub use components::*;

use std::rc::Rc;

use repose_core::*;
use repose_ui::{
    Box, Column, Row, Spacer, Stack, Surface, Text, TextStyle, ViewExt, anim::animate_f32,
    overlay::SnackbarAction,
};

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
                .size(280.0, 200.0)
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
    let offset = animate_f32(
        "sheet_offset",
        if visible { 0.0 } else { 800.0 },
        AnimationSpec::spring_gentle(),
    );

    Stack(Modifier::new().fill_max_size()).child((
        if visible {
            Box(Modifier::new()
                .fill_max_size()
                .background(theme().scrim.with_alpha(85))
                .on_pointer_down(move |_| on_dismiss()))
        } else {
            Box(Modifier::new())
        },
        Box(modifier
            .absolute()
            .offset(None, Some(offset), Some(0.0), Some(0.0)))
        .child(content),
    ))
}

pub fn NavigationBar(selected_index: usize, items: Vec<NavItem>) -> View {
    Row(Modifier::new()
        .fill_max_size()
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
        .min_width(280.0);

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

pub fn SuggestionChip(
    on_click: impl Fn() + 'static,
    label: View,
    icon: Option<View>,
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
            icon
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
            .clip_rounded(th.shapes.large)
        )
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
                }),
            )
            .child((
                icon.unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
                Box(Modifier::new().width(12.0).height(1.0)),
                Box(Modifier::new().flex_grow(1.0)).child(label),
                badge.unwrap_or(Box(Modifier::new())),
            ))
        }),
    )
}
