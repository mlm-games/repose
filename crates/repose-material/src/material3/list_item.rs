#![allow(non_snake_case)]

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use repose_ui::{Box, Column, Row, Text, TextStyle, ViewExt, anim::animate_color};

use super::*;

/// Colors for [`ListItem`] -> matches Compose Material3 `ListItemColors` with
/// 4 state groups (default, disabled, selected, dragged) × 6 slots each.
#[derive(Clone, Debug)]
pub struct ListItemColors {
    pub container_color: Color,
    pub headline_color: Color,
    pub supporting_color: Color,
    pub overline_color: Color,
    pub leading_icon_color: Color,
    pub trailing_icon_color: Color,

    pub disabled_container_color: Color,
    pub disabled_headline_color: Color,
    pub disabled_supporting_color: Color,
    pub disabled_overline_color: Color,
    pub disabled_leading_icon_color: Color,
    pub disabled_trailing_icon_color: Color,

    pub selected_container_color: Color,
    pub selected_headline_color: Color,
    pub selected_supporting_color: Color,
    pub selected_overline_color: Color,
    pub selected_leading_icon_color: Color,
    pub selected_trailing_icon_color: Color,

    pub dragged_container_color: Color,
    pub dragged_headline_color: Color,
    pub dragged_supporting_color: Color,
    pub dragged_overline_color: Color,
    pub dragged_leading_icon_color: Color,
    pub dragged_trailing_icon_color: Color,
}

impl ListItemColors {
    pub fn container(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_container_color
        } else if dragged {
            self.dragged_container_color
        } else if selected {
            self.selected_container_color
        } else {
            self.container_color
        }
    }
    pub fn headline(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_headline_color
        } else if dragged {
            self.dragged_headline_color
        } else if selected {
            self.selected_headline_color
        } else {
            self.headline_color
        }
    }
    pub fn supporting(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_supporting_color
        } else if dragged {
            self.dragged_supporting_color
        } else if selected {
            self.selected_supporting_color
        } else {
            self.supporting_color
        }
    }
    pub fn overline(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_overline_color
        } else if dragged {
            self.dragged_overline_color
        } else if selected {
            self.selected_overline_color
        } else {
            self.overline_color
        }
    }
    pub fn leading_icon(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_leading_icon_color
        } else if dragged {
            self.dragged_leading_icon_color
        } else if selected {
            self.selected_leading_icon_color
        } else {
            self.leading_icon_color
        }
    }
    pub fn trailing_icon(&self, enabled: bool, selected: bool, dragged: bool) -> Color {
        if !enabled {
            self.disabled_trailing_icon_color
        } else if dragged {
            self.dragged_trailing_icon_color
        } else if selected {
            self.selected_trailing_icon_color
        } else {
            self.trailing_icon_color
        }
    }
}

impl Default for ListItemColors {
    fn default() -> Self {
        Self {
            container_color: Color::TRANSPARENT,
            headline_color: ListItemDefaults::headline_color(),
            supporting_color: ListItemDefaults::supporting_color(),
            overline_color: ListItemDefaults::overline_color(),
            leading_icon_color: ListItemDefaults::leading_icon_color(),
            trailing_icon_color: ListItemDefaults::trailing_icon_color(),
            disabled_container_color: ListItemDefaults::disabled_container_color(),
            disabled_headline_color: ListItemDefaults::disabled_headline_color(),
            disabled_supporting_color: ListItemDefaults::disabled_supporting_color(),
            disabled_overline_color: ListItemDefaults::disabled_overline_color(),
            disabled_leading_icon_color: ListItemDefaults::disabled_leading_icon_color(),
            disabled_trailing_icon_color: ListItemDefaults::disabled_trailing_icon_color(),
            selected_container_color: ListItemDefaults::selected_container_color(),
            selected_headline_color: ListItemDefaults::selected_headline_color(),
            selected_supporting_color: ListItemDefaults::selected_supporting_color(),
            selected_overline_color: ListItemDefaults::selected_overline_color(),
            selected_leading_icon_color: ListItemDefaults::selected_leading_icon_color(),
            selected_trailing_icon_color: ListItemDefaults::selected_trailing_icon_color(),
            dragged_container_color: ListItemDefaults::dragged_container_color(),
            dragged_headline_color: ListItemDefaults::dragged_headline_color(),
            dragged_supporting_color: ListItemDefaults::dragged_supporting_color(),
            dragged_overline_color: ListItemDefaults::dragged_overline_color(),
            dragged_leading_icon_color: ListItemDefaults::dragged_leading_icon_color(),
            dragged_trailing_icon_color: ListItemDefaults::dragged_trailing_icon_color(),
        }
    }
}

/// Configuration for [`ListItem`].
#[derive(Clone, Debug)]
pub struct ListItemConfig {
    pub modifier: Modifier,
    /// When false, renders disabled colors and suppresses clicks.
    pub enabled: bool,
    pub selected: bool,
    /// Renders the M3 dragged color/elevation roles (e.g. while reordering).
    pub dragged: bool,
    pub colors: ListItemColors,
    pub state_colors: StateColors,
    pub tonal_elevation: f32,
    /// Additional elevation applied while `dragged` (M3 drag lift, e.g. Level 4).
    pub dragged_elevation: f32,
    pub shadow_elevation: f32,
    pub shape_radius: f32,
    /// Per-corner radii `[BL, BR, TR, TL]`. When set, overrides `shape_radius`.
    pub shape_radii: Option<[f32; 4]>,
    pub horizontal_padding: f32,
    pub trailing_padding: f32,
    pub one_line_height: f32,
    pub two_line_height: f32,
    pub three_line_height: f32,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for ListItemConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            enabled: true,
            selected: false,
            dragged: false,
            colors: ListItemColors::default(),
            state_colors: ListItemDefaults::state_colors_default(),
            tonal_elevation: 0.0,
            dragged_elevation: 0.0,
            shadow_elevation: 0.0,
            shape_radius: 0.0,
            shape_radii: None,
            horizontal_padding: ListItemDefaults::HORIZONTAL_PADDING,
            trailing_padding: ListItemDefaults::TRAILING_PADDING,
            one_line_height: ListItemDefaults::ONE_LINE_HEIGHT,
            two_line_height: ListItemDefaults::TWO_LINE_HEIGHT,
            three_line_height: ListItemDefaults::THREE_LINE_HEIGHT,
            interaction_source: None,
        }
    }
}

static LISTITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

/// M3 List Item - a single row in a list with optional leading/trailing content,
/// overline text, and click handling.
pub fn ListItem(
    headline: impl Into<String>,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
    on_long_click: Option<Rc<dyn Fn()>>,
    config: ListItemConfig,
) -> View {
    let th = theme();
    let is_enabled = config.enabled;
    let is_selected = config.selected;
    let is_dragged = config.dragged;
    let c = &config.colors;
    let id = remember(|| LISTITEM_COUNTER.fetch_add(1, Ordering::Relaxed));
    let spec = th.motion.color;

    let hd_col = animate_color(
        format!("li_hd_{}", id),
        c.headline(is_enabled, is_selected, is_dragged),
        spec,
    );
    let sp_col = animate_color(
        format!("li_sp_{}", id),
        c.supporting(is_enabled, is_selected, is_dragged),
        spec,
    );
    let ol_col = animate_color(
        format!("li_ol_{}", id),
        c.overline(is_enabled, is_selected, is_dragged),
        spec,
    );
    let ld_col = animate_color(
        format!("li_ld_{}", id),
        c.leading_icon(is_enabled, is_selected, is_dragged),
        spec,
    );
    let tr_col = animate_color(
        format!("li_tr_{}", id),
        c.trailing_icon(is_enabled, is_selected, is_dragged),
        spec,
    );
    let bg = animate_color(
        format!("li_bg_{}", id),
        c.container(is_enabled, is_selected, is_dragged),
        spec,
    );

    let line_count = match (overline_text.is_some(), supporting_text.is_some()) {
        (true, true) => 3,
        (true, false) | (false, true) => 2,
        (false, false) => 1,
    };
    let min_h = match line_count {
        3 => config.three_line_height,
        2 => config.two_line_height,
        _ => config.one_line_height,
    };
    let top_bottom_padding = match line_count {
        3 => 12.0,
        _ => 8.0,
    };

    let vert_align = if min_h >= config.three_line_height {
        AlignItems::START
    } else {
        AlignItems::CENTER
    };

    let li_source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| remember(MutableInteractionSource::new));

    let mut modifier = Modifier::new()
        .min_width(200.0)
        .min_height(min_h)
        .background(bg);
    match config.shape_radii {
        Some(r) => modifier = modifier.clip_rounded_radii(r),
        None => modifier = modifier.clip_rounded(config.shape_radius),
    }
    modifier = modifier
        .state_colors(config.state_colors)
        .padding_values(PaddingValues {
            left: config.horizontal_padding,
            right: config.trailing_padding,
            top: top_bottom_padding,
            bottom: top_bottom_padding,
        })
        .align_items(vert_align)
        .interaction_source(&li_source)
        .then(config.modifier);

    if config.tonal_elevation > 0.0 || config.dragged_elevation > 0.0 {
        let dragged_elev = if config.dragged_elevation > 0.0 {
            config.dragged_elevation
        } else {
            config.tonal_elevation
        };
        modifier = modifier.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: dragged_elev,
            disabled: 0.0,
        });
    }
    if config.shadow_elevation > 0.0 {
        modifier = modifier.shadow(config.shadow_elevation, 0.0);
    }

    if on_click.is_some() || on_long_click.is_some() {
        modifier = modifier.clickable();
        if let Some(cb) = on_click {
            let cb = cb.clone();
            modifier = modifier.on_click(move || {
                if is_enabled {
                    cb();
                }
            });
        }
        if let Some(cb) = &on_long_click {
            let cb = cb.clone();
            modifier = modifier.on_long_click(move || {
                if is_enabled {
                    cb();
                }
            });
        }
    }

    let wrap_icon = |color: Color, v: View| -> View { with_content_color(color, move || v) };

    Row(modifier).child((
        leading
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 0.0,
                    right: 16.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(wrap_icon(ld_col, v))
            })
            .unwrap_or(Box(Modifier::new())),
        Column(
            Modifier::new()
                .flex_grow(1.0)
                .justify_content(JustifyContent::CENTER),
        )
        .child((
            overline_text
                .map(|ot| {
                    Text(ot)
                        .color(ol_col)
                        .size(th.typography.label_small)
                        .single_line()
                })
                .unwrap_or(Box(Modifier::new())),
            Text(headline)
                .color(hd_col)
                .size(th.typography.body_large)
                .single_line(),
            supporting_text
                .map(|st| {
                    Text(st)
                        .color(sp_col)
                        .size(th.typography.body_medium)
                        .max_lines(2)
                        .overflow_ellipsize()
                })
                .unwrap_or(Box(Modifier::new())),
        )),
        trailing
            .map(|v| {
                Box(Modifier::new().padding_values(PaddingValues {
                    left: 16.0,
                    right: 0.0,
                    top: 0.0,
                    bottom: 0.0,
                }))
                .child(wrap_icon(tr_col, v))
            })
            .unwrap_or(Box(Modifier::new())),
    ))
}

/// M3 Selectable List Item -> single-selection variant with `selected` state and
/// `Role::RadioButton` semantics.
pub fn SelectableListItem(
    headline: impl Into<String>,
    selected: bool,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
    mut config: ListItemConfig,
) -> View {
    config.selected = selected;
    let mut m = Modifier::new().semantics(Semantics::new(Role::RadioButton));
    m = m.then(config.modifier);
    config.modifier = m;
    ListItem(
        headline,
        supporting_text,
        overline_text,
        leading,
        trailing,
        on_click,
        None,
        config,
    )
}

/// M3 Toggleable List Item -> multi-selection variant with `checked` state and
/// `Role::Checkbox` semantics. Clicking toggles the checked state.
pub fn ToggleableListItem(
    headline: impl Into<String>,
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    config: ListItemConfig,
) -> View {
    let mut cfg = config.clone();
    cfg.selected = checked;
    let cb = Rc::new(on_checked_change);
    let cb2 = cb.clone();
    let mut m = Modifier::new().semantics(Semantics::new(Role::Checkbox));
    m = m.then(cfg.modifier);
    cfg.modifier = m;
    ListItem(
        headline,
        supporting_text,
        overline_text,
        leading,
        trailing,
        Some(Rc::new(move || (cb2)(!checked))),
        None,
        cfg,
    )
}

/// Compute per-index corner radii `[BL, BR, TR, TL]` for a segmented list item.
fn segmented_item_radii(index: usize, count: usize, r: f32) -> [f32; 4] {
    if count <= 1 {
        [r, r, r, r]
    } else if index == 0 {
        [0.0, 0.0, r, r]
    } else if index == count - 1 {
        [r, r, 0.0, 0.0]
    } else {
        [0.0, 0.0, 0.0, 0.0]
    }
}

/// M3 Segmented List Item -> clickable variant with segmented (per-index) corner radii.
pub fn SegmentedListItem(
    index: usize,
    count: usize,
    headline: impl Into<String>,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
    mut config: ListItemConfig,
) -> View {
    config.shape_radii = Some(segmented_item_radii(index, count, config.shape_radius));
    ListItem(
        headline,
        supporting_text,
        overline_text,
        leading,
        trailing,
        on_click,
        None,
        config,
    )
}

/// M3 Segmented List Item -> single-selection variant.
pub fn SegmentedSelectableListItem(
    index: usize,
    count: usize,
    headline: impl Into<String>,
    selected: bool,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    on_click: Option<Rc<dyn Fn()>>,
    mut config: ListItemConfig,
) -> View {
    config.selected = selected;
    config.shape_radii = Some(segmented_item_radii(index, count, config.shape_radius));
    let mut m = Modifier::new().semantics(Semantics::new(Role::RadioButton));
    m = m.then(config.modifier);
    config.modifier = m;
    ListItem(
        headline,
        supporting_text,
        overline_text,
        leading,
        trailing,
        on_click,
        None,
        config,
    )
}

/// M3 Segmented List Item -> multi-selection (toggleable) variant.
pub fn SegmentedToggleableListItem(
    index: usize,
    count: usize,
    headline: impl Into<String>,
    checked: bool,
    on_checked_change: impl Fn(bool) + 'static,
    supporting_text: Option<String>,
    overline_text: Option<String>,
    leading: Option<View>,
    trailing: Option<View>,
    config: ListItemConfig,
) -> View {
    let mut cfg = config.clone();
    cfg.selected = checked;
    cfg.shape_radii = Some(segmented_item_radii(index, count, cfg.shape_radius));
    let cb2 = Rc::new(on_checked_change);
    let mut m = Modifier::new().semantics(Semantics::new(Role::Checkbox));
    m = m.then(cfg.modifier);
    cfg.modifier = m;
    ListItem(
        headline,
        supporting_text,
        overline_text,
        leading,
        trailing,
        Some(Rc::new(move || (cb2)(!checked))),
        None,
        cfg,
    )
}
