#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::Ordering;

use repose_core::*;
use repose_ui::{
    Box, Row, TextStyle, ViewExt, ZStack,
    anim::{animate_color, animate_f32},
};

use super::util::FILTERCHIP_COUNTER;
use super::*;

/// Configuration for [`NavigationDrawer`].
#[derive(Clone, Debug)]
pub struct NavigationDrawerConfig {
    pub modifier: Modifier,
    pub container_color: Color,
    pub content_color: Color,
    pub scrim_color: Color,
    pub tonal_elevation: f32,
    pub width: f32,
    pub shape_radius: f32,
}

impl Default for NavigationDrawerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            container_color: NavigationDrawerDefaults::container_color(),
            content_color: NavigationDrawerDefaults::content_color(),
            scrim_color: NavigationDrawerDefaults::scrim_color(),
            tonal_elevation: NavigationDrawerDefaults::TONAL_ELEVATION,
            width: NavigationDrawerDefaults::WIDTH,
            shape_radius: NavigationDrawerDefaults::SHAPE_RADIUS,
        }
    }
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
    config: NavigationDrawerConfig,
) -> View {
    let _th = theme();

    let drawer_offset = animate_f32(
        "modal_drawer_offset",
        if drawer_state.is_open() { 0.0 } else { -360.0 },
        theme().motion.spring,
    );

    let mut drawer_m = Modifier::new()
        .absolute()
        .offset(Some(drawer_offset), Some(0.0), None, Some(0.0))
        .fill_max_height()
        .width(config.width)
        .background(config.container_color)
        .clip_rounded(config.shape_radius);

    if config.tonal_elevation > 0.0 {
        drawer_m = drawer_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    ZStack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .fill_max_size()
            .background(config.content_color))
        .child(content),
        if drawer_state.is_open() {
            Box(Modifier::new()
                .fill_max_size()
                .background(config.scrim_color)
                .clickable()
                .on_pointer_down({
                    let ds = drawer_state.clone();
                    move |_| ds.dismiss()
                }))
            .child(Box(Modifier::new()))
        } else {
            Box(Modifier::new())
        },
        Box(drawer_m).child(drawer_content),
    ))
}

/// M3 Dismissible Navigation Drawer - slides alongside content without scrim.
/// Uses [`DrawerState`] to control open/close.
pub fn DismissibleNavigationDrawer(
    drawer_state: Rc<DrawerState>,
    drawer_content: View,
    content: View,
    config: NavigationDrawerConfig,
) -> View {
    let _th = theme();
    let drawer_offset = animate_f32(
        "dismissible_drawer_offset",
        if drawer_state.is_open() { 0.0 } else { -360.0 },
        theme().motion.spring,
    );

    let mut drawer_m = Modifier::new()
        .absolute()
        .offset(Some(drawer_offset), Some(0.0), None, Some(0.0))
        .fill_max_height()
        .width(config.width)
        .background(config.container_color)
        .clip_rounded(config.shape_radius);

    if config.tonal_elevation > 0.0 {
        drawer_m = drawer_m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: 0.0,
        });
    }

    ZStack(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .fill_max_size()
            .background(config.content_color))
        .child(content),
        Box(drawer_m).child(drawer_content),
    ))
}

/// M3 Permanent Navigation Drawer - always visible alongside content.
pub fn PermanentNavigationDrawer(
    drawer_content: View,
    content: View,
    config: NavigationDrawerConfig,
) -> View {
    Row(Modifier::new().fill_max_size()).child((
        Box(Modifier::new()
            .width(config.width)
            .fill_max_height()
            .background(config.container_color))
        .child(
            Box(Modifier::new())
                .color(config.content_color)
                .child(drawer_content),
        ),
        Box(Modifier::new().flex_grow(1.0)).child(content),
    ))
}

/// A destination entry inside a NavigationDrawer.
#[derive(Clone)]
pub struct NavigationDrawerItemConfig {
    pub modifier: Modifier,
    pub icon: Option<View>,
    pub badge: Option<View>,
    pub enabled: bool,
    pub shape_radius: f32,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for NavigationDrawerItemConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            icon: None,
            badge: None,
            enabled: true,
            shape_radius: repose_core::locals::theme().shapes.large,
            interaction_source: None,
        }
    }
}

pub fn NavigationDrawerItem(
    label: View,
    selected: bool,
    on_click: impl Fn() + 'static,
    config: NavigationDrawerItemConfig,
) -> View {
    let th = theme();
    let id = remember(|| FILTERCHIP_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;
    let bg = animate_color(
        format!("ndi_bg_{}", id),
        if selected {
            th.secondary_container
        } else {
            Color::TRANSPARENT
        },
        spec,
    );
    let fg = animate_color(
        format!("ndi_fg_{}", id),
        if selected {
            th.on_secondary_container
        } else {
            th.on_surface_variant
        },
        spec,
    );

    let nd_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut m = Modifier::new()
        .fill_max_width()
        .padding_values(PaddingValues {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        })
        .min_height(56.0)
        .background(bg)
        .state_colors(StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            dragged: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        })
        .clip_rounded(config.shape_radius)
        .interaction_source(&nd_source)
        .then(config.modifier);

    if config.enabled {
        m = m.clickable().on_click(on_click);
    }

    Box(m).child(with_content_color(fg, || {
        Row(Modifier::new()
            .align_items(AlignItems::CENTER)
            .padding_values(PaddingValues {
                left: 16.0,
                right: 24.0,
                top: 0.0,
                bottom: 0.0,
            }))
        .child((
            config
                .icon
                .unwrap_or(Box(Modifier::new().width(24.0).height(24.0))),
            Box(Modifier::new().width(12.0).height(1.0)),
            Box(Modifier::new().flex_grow(1.0)).child(label),
            config.badge.unwrap_or(Box(Modifier::new())),
        ))
    }))
}
