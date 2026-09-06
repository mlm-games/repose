#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use repose_core::NestedScrollConnection;
use repose_core::animation::AnimationSpec;
use repose_core::text::ImeAction;
use repose_core::*;
use repose_ui::{
    BasicTextField, Box, Column, Row, Spacer, Text, TextFieldState, TextStyle, ViewExt, ZStack,
    anim::animate_f32, overlay::OverlayGuard, overlay::OverlayHandle,
};

use super::app_bar::WindowInsets;
use super::util::apply_m3_clickable;
use super::util::apply_tonal_elevation;
use super::*;

use super::util::lerp_color;
/// Color slots for [`SearchBar`]. Matches Compose Material3 `SearchBarColors`.
#[derive(Clone, Copy, Debug)]
pub struct SearchBarColors {
    pub container_color: Color,
    pub active_container_color: Color,
    pub divider_color: Color,
    pub content_color: Color,
    pub placeholder_color: Color,
    pub scrim_color: Color,
}

impl SearchBarColors {
    pub fn container(&self, active: bool) -> Color {
        if active {
            self.active_container_color
        } else {
            self.container_color
        }
    }
}

impl Default for SearchBarColors {
    fn default() -> Self {
        Self {
            container_color: SearchBarDefaults::container_color(),
            active_container_color: SearchBarDefaults::active_container_color(),
            divider_color: SearchBarDefaults::divider_color(),
            content_color: SearchBarDefaults::content_color(),
            placeholder_color: SearchBarDefaults::placeholder_color(),
            scrim_color: SearchBarDefaults::scrim_color(),
        }
    }
}

/// Color slots for [`AppBarWithSearch`]. Scrolled/not-scrolled pairs.
#[derive(Clone, Copy, Debug)]
pub struct AppBarWithSearchColors {
    pub search_bar_colors: SearchBarColors,
    pub scrolled_search_bar_container_color: Color,
    pub app_bar_container_color: Color,
    pub scrolled_app_bar_container_color: Color,
    pub navigation_icon_content_color: Color,
    pub action_icon_content_color: Color,
}

impl AppBarWithSearchColors {
    pub fn search_bar_container(&self, scroll_fraction: f32) -> Color {
        lerp_color(
            self.search_bar_colors.container_color,
            self.scrolled_search_bar_container_color,
            scroll_fraction.clamp(0.0, 1.0),
        )
    }
    pub fn app_bar_container(&self, scroll_fraction: f32) -> Color {
        lerp_color(
            self.app_bar_container_color,
            self.scrolled_app_bar_container_color,
            scroll_fraction.clamp(0.0, 1.0),
        )
    }
}

impl Default for AppBarWithSearchColors {
    fn default() -> Self {
        Self {
            search_bar_colors: SearchBarColors::default(),
            scrolled_search_bar_container_color: SearchBarDefaults::scrolled_container_color(),
            app_bar_container_color: SearchBarDefaults::app_bar_container_color(),
            scrolled_app_bar_container_color: SearchBarDefaults::scrolled_app_bar_container_color(),
            navigation_icon_content_color: SearchBarDefaults::navigation_icon_content_color(),
            action_icon_content_color: SearchBarDefaults::action_icon_content_color(),
        }
    }
}

/// Configuration for [`SearchBar`].
#[derive(Clone, Debug)]
pub struct SearchBarConfig {
    pub modifier: Modifier,
    pub colors: SearchBarColors,
    pub height: f32,
    pub shape_radius: f32,
    pub active_shape_radius: f32,
    pub expanded_width: f32,
    pub collapsed_width: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub window_insets: WindowInsets,
    pub content_padding: PaddingValues,
    pub min_width: f32,
    pub max_width: f32,
}

impl Default for SearchBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: SearchBarColors::default(),
            height: SearchBarDefaults::HEIGHT,
            shape_radius: SearchBarDefaults::SHAPE_RADIUS,
            active_shape_radius: SearchBarDefaults::ACTIVE_SHAPE_RADIUS,
            expanded_width: SearchBarDefaults::EXPANDED_WIDTH,
            collapsed_width: SearchBarDefaults::COLLAPSED_WIDTH,
            tonal_elevation: SearchBarDefaults::TONAL_ELEVATION,
            shadow_elevation: SearchBarDefaults::SHADOW_ELEVATION,
            window_insets: WindowInsets::default(),
            content_padding: SearchBarDefaults::CONTENT_PADDING,
            min_width: SearchBarDefaults::MIN_WIDTH,
            max_width: SearchBarDefaults::MAX_WIDTH,
        }
    }
}

/// Configuration for [`ExpandedFullScreenSearchBar`].
#[derive(Clone, Debug)]
pub struct ExpandedFullScreenSearchBarConfig {
    pub modifier: Modifier,
    pub colors: SearchBarColors,
    pub collapsed_shape_radius: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub window_insets: WindowInsets,
    pub scrim_color: Color,
}

impl Default for ExpandedFullScreenSearchBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: SearchBarColors::default(),
            collapsed_shape_radius: SearchBarDefaults::SHAPE_RADIUS,
            tonal_elevation: SearchBarDefaults::TONAL_ELEVATION,
            shadow_elevation: SearchBarDefaults::SHADOW_ELEVATION,
            window_insets: WindowInsets::default(),
            scrim_color: SearchBarDefaults::scrim_color(),
        }
    }
}

/// Configuration for [`ExpandedDockedSearchBar`].
#[derive(Clone, Debug)]
pub struct ExpandedDockedSearchBarConfig {
    pub modifier: Modifier,
    pub colors: SearchBarColors,
    pub shape_radius: f32,
    pub dropdown_shape_radius: f32,
    pub dropdown_gap_size: f32,
    pub dropdown_scrim_color: Color,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
}

impl Default for ExpandedDockedSearchBarConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: SearchBarColors::default(),
            shape_radius: SearchBarDefaults::DOCKED_SHAPE_RADIUS,
            dropdown_shape_radius: SearchBarDefaults::DROPDOWN_SHAPE_RADIUS,
            dropdown_gap_size: SearchBarDefaults::DROPDOWN_GAP_SIZE,
            dropdown_scrim_color: SearchBarDefaults::dropdown_scrim_color(),
            tonal_elevation: SearchBarDefaults::TONAL_ELEVATION,
            shadow_elevation: SearchBarDefaults::SHADOW_ELEVATION,
        }
    }
}

/// Configuration for [`AppBarWithSearch`].
#[derive(Clone, Debug)]
pub struct AppBarWithSearchConfig {
    pub modifier: Modifier,
    pub colors: AppBarWithSearchColors,
    pub height: f32,
    pub shape_radius: f32,
    pub tonal_elevation: f32,
    pub shadow_elevation: f32,
    pub content_padding: PaddingValues,
    pub window_insets: WindowInsets,
    pub scroll_fraction: f32,
    pub scroll_offset: f32,
}

impl Default for AppBarWithSearchConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            colors: AppBarWithSearchColors::default(),
            height: SearchBarDefaults::HEIGHT,
            shape_radius: SearchBarDefaults::SHAPE_RADIUS,
            tonal_elevation: SearchBarDefaults::TONAL_ELEVATION,
            shadow_elevation: SearchBarDefaults::SHADOW_ELEVATION,
            content_padding: SearchBarDefaults::CONTENT_PADDING,
            window_insets: WindowInsets::default(),
            scroll_fraction: 0.0,
            scroll_offset: 0.0,
        }
    }
}

/// Scroll behavior for [`AppBarWithSearch`] -> collapses/expands on scroll.
pub struct SearchBarScrollBehavior {
    pub collapsed_offset: Signal<f32>,
    pub height: f32,
    pub collapsed_height: f32,
    _pending: Rc<Cell<f32>>,
}

impl SearchBarScrollBehavior {
    pub fn new(height: f32, collapsed_height: f32) -> Self {
        Self {
            collapsed_offset: signal(0.0),
            height,
            collapsed_height,
            _pending: Rc::new(Cell::new(0.0)),
        }
    }

    pub fn offset(&self) -> f32 {
        self.collapsed_offset.get()
    }

    pub fn nested_scroll_connection(&self) -> NestedScrollConnection {
        let offset = self.collapsed_offset.clone();
        let max_offset = self.height - self.collapsed_height;
        NestedScrollConnection::new().on_pre_scroll(move |delta: Vec2, _source| {
            let cur = offset.get();
            let new = (cur - delta.y).clamp(-max_offset, 0.0);
            let consumed = cur - new;
            offset.set(new);
            request_frame();
            Vec2 {
                x: 0.0,
                y: consumed,
            }
        })
    }
}

/// Possible values of [`SearchBarState`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SearchBarValue {
    Collapsed,
    Expanded,
}

/// State for `SearchBar` -> manages expanded/collapsed progress, query text,
/// active state, and collapsed layout coordinates for popup anchoring.
pub struct SearchBarState {
    pub query: Signal<String>,
    pub expanded: Signal<bool>,
    pub active: Signal<bool>,
    /// Whether this search bar expands to full-screen (vs docked).
    /// Used by AppBarWithSearch to hide the collapsed bar when expanded.
    pub expands_to_full_screen: Signal<bool>,
    /// Container animation (shape, size, position)
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Content fade animation -> fades FIRST on collapse before container shrinks
    content_anim: Rc<RefCell<AnimatedValue<f32>>>,
    /// Tracked via `on_globally_positioned` on the collapsed bar.
    /// Used by expanded docked variants for popup placement.
    pub collapsed_layout_rect: Signal<(f32, f32, f32, f32)>,
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self::new()
    }
}

impl SearchBarState {
    pub fn new() -> Self {
        Self {
            query: signal(String::new()),
            expanded: signal(false),
            active: signal(false),
            expands_to_full_screen: signal(false),
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            content_anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            collapsed_layout_rect: signal((0.0, 0.0, 0.0, 0.0)),
        }
    }

    pub fn query(&self) -> String {
        self.query.get()
    }

    pub fn set_query(&self, q: impl Into<String>) {
        self.query.set(q.into());
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    pub fn expand(&self) {
        self.expanded.set(true);
        self.anim.borrow_mut().set_target(1.0);
        self.content_anim.borrow_mut().set_target(1.0);
        request_frame();
    }

    pub fn collapse(&self) {
        self.expanded.set(false);
        self.active.set(false);
        // Content fades first; container follows in progress()
        self.content_anim.borrow_mut().set_target(0.0);
        self.anim.borrow_mut().set_target(0.0);
        request_frame();
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn activate(&self) {
        self.active.set(true);
        self.expanded.set(true);
        self.anim.borrow_mut().set_target(1.0);
        self.content_anim.borrow_mut().set_target(1.0);
        request_frame();
    }

    pub fn deactivate(&self) {
        if self.expanded.get() {
            self.expanded.set(false);
            self.content_anim.borrow_mut().set_target(0.0);
            self.anim.borrow_mut().set_target(0.0);
        }
        self.active.set(false);
        FocusManager::new(vec![], None).clear_focus(false);
        request_frame();
    }

    /// Container animation progress: 0.0 = collapsed, 1.0 = expanded.
    /// Ticks the underlying AnimatedValue and requests frames while animating.
    pub fn progress(&self) -> f32 {
        let mut a = self.anim.borrow_mut();
        let still = a.update();
        if still {
            request_frame();
        }
        a.get().clamp(0.0, 1.0)
    }

    /// Content fade progress -> fades ahead of container on collapse.
    pub fn content_progress(&self) -> f32 {
        let mut a = self.content_anim.borrow_mut();
        let still = a.update();
        if still {
            request_frame();
        }
        a.get().clamp(0.0, 1.0)
    }

    /// Whether the animation is currently running.
    pub fn is_animating(&self) -> bool {
        self.anim.borrow().is_animating() || self.content_anim.borrow().is_animating()
    }

    /// Whether the search bar is currently expanded (with tolerance for spring overshoot).
    pub fn current_value(&self) -> SearchBarValue {
        if *self.anim.borrow().get() <= 0.02 {
            SearchBarValue::Collapsed
        } else {
            SearchBarValue::Expanded
        }
    }

    /// Snap the container progress to a specific fraction (0.0 = collapsed, 1.0 = expanded).
    pub fn snap_to(&self, fraction: f32) {
        self.anim.borrow_mut().snap_to(fraction.clamp(0.0, 1.0));
        request_frame();
    }
}

#[derive(Clone)]
pub struct SearchBarInputFieldConfig {
    pub state: Option<Rc<SearchBarState>>,
    pub on_search: Option<Rc<dyn Fn(String)>>,
    pub enabled: bool,
    pub text_color: Color,
    pub placeholder_color: Color,
    pub leading_icon: Option<View>,
    pub trailing_icon: Option<View>,
    pub interaction_source: Option<MutableInteractionSource>,
}

impl Default for SearchBarInputFieldConfig {
    fn default() -> Self {
        let th = theme();
        Self {
            state: None,
            on_search: None,
            enabled: true,
            text_color: th.on_surface,
            placeholder_color: th.on_surface_variant,
            leading_icon: None,
            trailing_icon: None,
            interaction_source: None,
        }
    }
}

/// Build a search bar input field with proper M3 SearchBar styling.
/// Equivalent to Compose Material3's `SearchBarDefaults.InputField`.
/// When `state` is provided, focus gain triggers expand and Escape triggers collapse.
/// Always renders a `UiTextField` (focusable even in collapsed state, matching CK).
pub fn SearchBarInputField(
    placeholder: String,
    query: String,
    on_query_change: Rc<dyn Fn(String)>,
    expanded: bool,
    config: SearchBarInputFieldConfig,
) -> View {
    let source: Rc<MutableInteractionSource> = config
        .interaction_source
        .clone()
        .map(Rc::new)
        .unwrap_or_else(|| Rc::new(MutableInteractionSource::new()));
    let focused = source.source().collect_is_focused();
    let state = config.state;
    let enabled = config.enabled;

    let mut input_m = Modifier::new()
        .flex_grow(1.0)
        .padding(4.0)
        .required_width_in(SearchBarDefaults::MIN_WIDTH, SearchBarDefaults::MAX_WIDTH)
        .required_height_in(SearchBarDefaults::HEIGHT, SearchBarDefaults::HEIGHT)
        .interaction_source(&source)
        .semantics(Semantics {
            role: Role::TextField,
            label: Some("Search".into()),
            focused: expanded || focused,
            enabled,
            ..Default::default()
        })
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    if let Some(ref s) = s
                        && s.is_active()
                    {
                        s.deactivate();
                    }
                    true
                } else if ev.key == Key::ArrowDown || ev.key == Key::ArrowUp {
                    if let Some(ref s) = s
                        && !s.is_expanded()
                    {
                        s.activate();
                    }
                    true
                } else {
                    false
                }
            }
        });
    if let Some(ref s) = state {
        let s2 = s.clone();
        input_m = input_m.on_focus_changed(move |focused| {
            if focused {
                s2.activate();
            }
        });
    }

    let on_qc = on_query_change.clone();
    let on_s = config.on_search.clone();

    // Always render the text field (focusable even when collapsed, matching CK).
    let read_only = !expanded;

    let display_color = if query.is_empty() {
        config.placeholder_color
    } else {
        config.text_color
    };

    let tf_state = remember_with_key("SearchBarInputField_tf_state", || {
        RefCell::new(TextFieldState::new())
    });
    if tf_state.borrow().text != query {
        tf_state.borrow_mut().text = query.clone();
    }

    // Build the row: [leading_icon] + text_field + [trailing_icon]
    let mut row_children: Vec<View> = Vec::new();
    if let Some(icon) = config.leading_icon {
        row_children.push(icon);
    }
    let on_qc2 = on_qc.clone();
    row_children.push(
        BasicTextField(
            tf_state.clone(),
            input_m,
            placeholder,
            repose_ui::TextFieldConfig {
                on_change: Some(Rc::new(move |text| on_qc2(text))),
                on_submit: on_s.clone(),
                enabled,
                read_only,
                line_limits: TextFieldLineLimits::SingleLine,
                keyboard_options: KeyboardOptions {
                    ime_action: ImeAction::Search,
                    ..KeyboardOptions::DEFAULT
                },
                ..Default::default()
            },
        )
        .color(display_color)
        .size(repose_core::locals::theme().typography.body_large),
    );
    if let Some(icon) = config.trailing_icon {
        row_children.push(icon);
    }

    if row_children.len() == 1 {
        row_children.into_iter().next().unwrap()
    } else {
        Row(Modifier::new()
            .fill_max_width()
            .align_items(AlignItems::CENTER))
        .child(row_children)
    }
}

/// Record the collapsed bar's layout rect on the state. Returns a modifier
/// that should be applied to the collapsed bar.
fn track_collapsed_layout(state: &Rc<SearchBarState>) -> Modifier {
    let s = state.clone();
    Modifier::new().on_globally_positioned(move |rect| {
        s.collapsed_layout_rect
            .set((rect.x, rect.y, rect.w, rect.h));
    })
}

/// M3 Collapsed Search Bar -> renders ONLY the collapsed bar surface wrapping
/// the provided `input_field`. Does NOT manage expanded content.
///
/// Equivalent to CK's `SearchBar(state, inputField)` overload -> a passive
/// Surface that does NOT handle clicks or ripple. The click/focus->expand
/// behavior is managed by the `InputField` (via `SearchBarInputField`).
///
/// Pressing <kbd>Escape</kbd> deactivates the search bar (cross-platform back).
///
/// Use [`ExpandedFullScreenSearchBar`] / [`ExpandedDockedSearchBar`] for the
/// expanded state, or [`SearchBarWithContent`] for an all-in-one variant.
pub fn SearchBar(
    state: Rc<SearchBarState>,
    input_field: View,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: SearchBarConfig,
) -> View {
    let th = theme();
    let colors = config.colors;

    let mut bar_m = modifier
        .fill_max_width()
        .height(config.height)
        .state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: th.elevation.level2,
            focused: th.elevation.level2,
            pressed: th.elevation.level3,
            dragged: th.elevation.level3,
            disabled: 0.0,
        })
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if ev.key == Key::Escape && s.is_active() {
                    s.deactivate();
                    true
                } else {
                    false
                }
            }
        })
        .on_focus_changed({
            let s = state.clone();
            move |focused| {
                if focused {
                    s.activate();
                }
            }
        })
        .semantics(Semantics {
            role: Role::TextField,
            label: Some("Search".into()),
            focused: state.is_active(),
            ..Default::default()
        })
        .background(colors.container_color)
        .clip_rounded(config.shape_radius)
        .then(track_collapsed_layout(&state));

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, colors.container_color);

    Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(8.0).fill_max_height()),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    )
}

/// M3 Search Bar that manages expanded content with animated width and
/// suggestions dropdown. Equivalent to CK's
/// `SearchBar(inputField, expanded, onExpandedChange, ..., content)` overload.
///
/// The bar itself is a passive surface (no click handling) -> expansion is
/// driven by the `InputField`'s focus tracking inside `input_field`.
pub fn SearchBarWithContent(
    input_field: View,
    expanded: bool,
    on_expanded_change: Rc<dyn Fn(bool)>,
    modifier: Modifier,
    leading_icon: Option<View>,
    trailing_icon: Option<View>,
    config: SearchBarConfig,
    content: View,
) -> View {
    let th = theme();
    let width = animate_f32(
        "sbwc_w",
        if expanded {
            config.expanded_width
        } else {
            config.collapsed_width
        },
        theme().motion.expand,
    );

    let bar_bg = if expanded {
        config.colors.active_container_color
    } else {
        config.colors.container_color
    };
    let shape = if expanded {
        config.active_shape_radius
    } else {
        config.shape_radius
    };

    let mut bar_m = modifier
        .clone()
        .width(width)
        .min_width(config.min_width)
        .max_width(config.max_width)
        .height(config.height)
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    cb(false);
                    true
                } else {
                    false
                }
            }
        })
        .background(bar_bg)
        .clip_rounded(shape);

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_bg);

    // Content fades with separate alpha so content can fade before collapse
    let content_alpha = animate_f32("sbwc_a", if expanded { 1.0 } else { 0.0 }, th.motion.color);

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(8.0).fill_max_height()),
            input_field,
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    );

    let show_content = expanded || content_alpha > 0.01;
    if show_content || expanded {
        Column(modifier).child((
            bar,
            Box(Modifier::new()
                .width(width)
                .max_height(SearchBarDefaults::DOCKED_HEIGHT)
                .alpha(content_alpha)
                .background(config.colors.container_color)
                .clip_rounded(th.shapes.extra_small))
            .child(content),
        ))
    } else {
        bar
    }
}

/// M3 Docked Search Bar -> bounded-width variant with animated suggestions
/// dropdown (height + alpha).  Equivalent to CK's
/// `DockedSearchBar(inputField, expanded, onExpandedChange, ..., content)`.
/// The bar itself is a passive Surface -> expansion is driven by `InputField`.
pub fn DockedSearchBar(
    input_field: View,
    expanded: bool,
    on_expanded_change: Option<Rc<dyn Fn(bool)>>,
    modifier: Modifier,
    leading_icon: Option<View>,
    config: SearchBarConfig,
    content: View,
) -> View {
    let th = theme();
    let active = expanded;
    let colors = config.colors;

    let content_target = if expanded {
        get_window_container_height() * 2.0 / 3.0
    } else {
        0.0
    };
    let content_height = animate_f32("docked_sh", content_target, theme().motion.expand);
    let content_alpha = animate_f32(
        "docked_sa",
        if expanded { 1.0 } else { 0.0 },
        theme().motion.color,
    );
    let bar_bg = if active {
        colors.active_container_color
    } else {
        colors.container_color
    };

    let clear_source: Rc<MutableInteractionSource> = remember(MutableInteractionSource::new);
    let clear_btn = if active {
        Box(apply_m3_clickable(
            Modifier::new().size(24.0, 24.0).clip_rounded(12.0),
            &clear_source,
            colors.placeholder_color,
            true,
            {
                let cb = on_expanded_change.clone();
                move || {
                    if let Some(ref cb) = cb {
                        cb(false);
                    }
                }
            },
        ))
        .child(Text("✕").size(16.0).color(colors.placeholder_color))
    } else {
        Box(Modifier::new())
    };

    let mut bar_m = modifier
        .z_index(1.0)
        .min_width(SearchBarDefaults::MIN_WIDTH)
        .height(config.height)
        .state_elevation(StateElevation {
            default: if active {
                th.elevation.level3
            } else {
                config.tonal_elevation
            },
            hovered: th.elevation.level2,
            focused: th.elevation.level2,
            pressed: th.elevation.level3,
            dragged: th.elevation.level3,
            disabled: 0.0,
        })
        .shadow(config.shadow_elevation, 0.0)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if ev.key == Key::Escape {
                    if let Some(ref cb) = cb {
                        cb(false);
                    }
                    true
                } else {
                    false
                }
            }
        })
        .background(bar_bg)
        .clip_rounded(config.shape_radius);

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_bg);

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(24.0, 24.0))),
            Box(Modifier::new().width(12.0).fill_max_height()),
            input_field,
            clear_btn,
        )),
    );

    let show_content = expanded || content_height > 1.0;
    if show_content {
        Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
            bar,
            Box(Modifier::new()
                .min_width(SearchBarDefaults::MIN_WIDTH)
                .height(content_height)
                .alpha(content_alpha)
                .clip_rounded(th.shapes.small)
                .background(colors.container_color)
                .state_elevation(StateElevation {
                    default: th.elevation.level3,
                    hovered: th.elevation.level3,
                    focused: th.elevation.level3,
                    pressed: th.elevation.level3,
                    dragged: th.elevation.level3,
                    disabled: 0.0,
                }))
            .child(
                Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
                    Box(Modifier::new()
                        .min_width(SearchBarDefaults::MIN_WIDTH)
                        .height(1.0)
                        .background(colors.divider_color)),
                    content,
                )),
            ),
        ))
    } else {
        bar
    }
}

/// Platform-agnostic window container height. On Skiko this would read
/// `LocalWindowInfo`, on Android `LocalConfiguration`. Defaults to 800 dp.
/// The `LayoutEngine` keeps this current from the physical viewport + density.
pub fn set_window_container_height(h: f32) {
    repose_core::locals::set_window_container_height(h);
}

fn get_window_container_height() -> f32 {
    repose_core::locals::get_window_container_height()
}

/// Set the window container width (in dp) used for dropdown constraints.
pub fn set_window_container_width(w: f32) {
    repose_core::locals::set_window_container_width(w);
}

/// M3 Expanded Full-Screen Search Bar -> rendered in an overlay covering the
/// entire window. Uses the state's own `progress()` for animation.
/// Equivalent to CK's `ExpandedFullScreenSearchBar(state, inputField, ...)`.
pub fn ExpandedFullScreenSearchBar(
    state: Rc<SearchBarState>,
    overlay: OverlayHandle,
    input_field: View,
    modifier: Modifier,
    config: ExpandedFullScreenSearchBarConfig,
    content: View,
) -> View {
    // Mark as full-screen so AppBarWithSearch can hide the collapsed bar
    state.expands_to_full_screen.set(true);

    let efs_id = remember(unique_component_id);
    let overlay_guard = remember_with_key(format!("efs_oguard_{efs_id}"), || {
        RefCell::new(None::<OverlayGuard>)
    });
    let current_content =
        remember_state_with_key(format!("efs_cc_{efs_id}"), || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    let current_modifier = remember_state_with_key(format!("efs_mod_{efs_id}"), Modifier::new);
    *current_modifier.borrow_mut() = modifier;
    let current_input =
        remember_state_with_key(format!("efs_input_{efs_id}"), || input_field.clone());
    *current_input.borrow_mut() = input_field;
    let current_config = remember_state_with_key(format!("efs_cfg_{efs_id}"), || config.clone());
    *current_config.borrow_mut() = config;

    let progress = state.progress();
    let _content_alpha = state.content_progress();

    let expanded = state.is_expanded();
    let visible = expanded || progress > 0.01;

    if visible {
        if overlay_guard.borrow().is_none() {
            let input_fr = FocusRequester::new();
            let focus_requested = Rc::new(Cell::new(false));
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let current_modifier = current_modifier.clone();
                let current_input = current_input.clone();
                let current_content = current_content.clone();
                let current_config = current_config.clone();
                let input_fr = input_fr.clone();
                let focus_requested = focus_requested.clone();
                move || {
                    let modifier = current_modifier.borrow().clone();
                    let input_field = current_input.borrow().clone();
                    let config = current_config.borrow().clone();
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();

                    // Wrap input with focus requester and request focus.
                    let inp = Box(Modifier::new().focus_requester(input_fr.clone()))
                        .child(input_field.clone());
                    if !focus_requested.get() {
                        focus_requested.set(true);
                        input_fr.request_focus();
                    }

                    let header = Box(modifier
                        .clone()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .padding_values(PaddingValues {
                            left: 16.0,
                            right: 16.0,
                            top: 0.0,
                            bottom: 0.0,
                        })
                        .background(config.colors.container_color)
                        .alpha(alpha))
                    .child(inp);

                    let body = Box(Modifier::new()
                        .fill_max_width()
                        .flex_grow(1.0)
                        .alpha(c_alpha)
                        .background(th.surface))
                    .child(content);

                    let insets = config.window_insets;
                    let full = Column(Modifier::new().fill_max_size().padding_values(
                        PaddingValues {
                            left: insets.left,
                            right: insets.right,
                            top: insets.top,
                            bottom: insets.bottom,
                        },
                    ))
                    .child((header, body));

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.scrim_color.with_alpha((85.0 * alpha) as u8))
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, full))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 900.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

/// M3 Expanded Docked Search Bar -> rendered as an overlay popup anchored below
/// the collapsed search bar using `collapsed_layout_rect`.
/// Equivalent to CK's `ExpandedDockedSearchBar(state, inputField, ...)`.
pub fn ExpandedDockedSearchBar(
    state: Rc<SearchBarState>,
    overlay: OverlayHandle,
    input_field: View,
    modifier: Modifier,
    config: ExpandedDockedSearchBarConfig,
    content: View,
) -> View {
    // Docked search bar does NOT expand to full-screen
    state.expands_to_full_screen.set(false);

    let eds_id = remember(unique_component_id);
    let overlay_guard = remember_with_key(format!("eds_oguard_{eds_id}"), || {
        RefCell::new(None::<OverlayGuard>)
    });
    let current_content =
        remember_state_with_key(format!("eds_cc_{eds_id}"), || Box(Modifier::new()));
    *current_content.borrow_mut() = content;

    let current_modifier = remember_state_with_key(format!("eds_mod_{eds_id}"), Modifier::new);
    *current_modifier.borrow_mut() = modifier;
    let current_input =
        remember_state_with_key(format!("eds_input_{eds_id}"), || input_field.clone());
    *current_input.borrow_mut() = input_field;
    let current_config = remember_state_with_key(format!("eds_cfg_{eds_id}"), || config.clone());
    *current_config.borrow_mut() = config;

    let progress = state.progress();
    let _content_alpha = state.content_progress();
    let expanded = state.is_expanded();
    let visible = expanded || progress > 0.01;

    if visible {
        if overlay_guard.borrow().is_none() {
            let input_fr = FocusRequester::new();
            let focus_requested = Rc::new(Cell::new(false));
            let builder: Rc<dyn Fn() -> View> = Rc::new({
                let state = state.clone();
                let current_modifier = current_modifier.clone();
                let current_input = current_input.clone();
                let current_content = current_content.clone();
                let current_config = current_config.clone();
                let input_fr = input_fr.clone();
                let focus_requested = focus_requested.clone();
                move || {
                    let modifier = current_modifier.borrow().clone();
                    let input_field = current_input.borrow().clone();
                    let config = current_config.borrow().clone();
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();
                    let (_cx, _cy, _cw, _ch) = state.collapsed_layout_rect.get();

                    let inp = Box(Modifier::new().focus_requester(input_fr.clone()))
                        .child(input_field.clone());
                    if !focus_requested.get() {
                        focus_requested.set(true);
                        input_fr.request_focus();
                    }

                    let header = Box(modifier
                        .clone()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .alpha(alpha)
                        .background(config.colors.container_color)
                        .clip_rounded(config.shape_radius)
                        .state_elevation(StateElevation {
                            default: th.elevation.level3,
                            hovered: th.elevation.level2,
                            focused: th.elevation.level2,
                            pressed: th.elevation.level3,
                            dragged: th.elevation.level3,
                            disabled: 0.0,
                        }))
                    .child(inp);

                    let dropdown = Box(Modifier::new()
                        .fill_max_width()
                        .max_height(get_window_container_height() * 2.0 / 3.0)
                        .alpha(c_alpha)
                        .clip_rounded(config.dropdown_shape_radius)
                        .background(config.colors.container_color)
                        .state_elevation(StateElevation {
                            default: th.elevation.level3,
                            hovered: th.elevation.level3,
                            focused: th.elevation.level3,
                            pressed: th.elevation.level3,
                            dragged: th.elevation.level3,
                            disabled: 0.0,
                        }))
                    .child(
                        Column(Modifier::new().fill_max_width()).child((
                            Box(Modifier::new()
                                .fill_max_width()
                                .height(1.0)
                                .background(config.colors.divider_color)),
                            content,
                        )),
                    );

                    let docked_width = _cw.max(SearchBarDefaults::MIN_WIDTH);
                    let popup_left = _cx;
                    let popup_top = _cy + _ch + config.dropdown_gap_size;

                    let col = Column(Modifier::new().fill_max_width()).child((header, dropdown));

                    let positioned = Box(Modifier::new()
                        .absolute()
                        .offset(Some(popup_left), Some(popup_top), None, None)
                        .width(docked_width))
                    .child(col);

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.dropdown_scrim_color)
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));

                    ZStack(Modifier::new().fill_max_size().absolute()).child((scrim, positioned))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 900.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

/// M3 App Bar With Search -> integrates a search bar into a top app bar layout
/// with optional navigation icon, action buttons, scroll behavior, and window insets.
/// Wraps the internal `SearchBar` collapsed component.
pub fn AppBarWithSearch(
    state: Rc<SearchBarState>,
    input_field: View,
    navigation_icon: Option<View>,
    actions: Option<Vec<View>>,
    config: AppBarWithSearchConfig,
) -> View {
    let bg = config.colors.search_bar_container(config.scroll_fraction);
    let app_bar_bg = config.colors.app_bar_container(config.scroll_fraction);

    let insets = config.window_insets;

    // CK parity: when app bar container is transparent, disable tonal/shadow elevations
    let is_container_transparent = app_bar_bg.3 == 0;
    let tonal_elevation = if is_container_transparent {
        0.0
    } else {
        config.tonal_elevation
    };
    let shadow_elevation = if is_container_transparent {
        0.0
    } else {
        config.shadow_elevation
    };

    // Hide the collapsed bar when full-screen expanded (CK parity via expandsToFullScreen)
    let hide_collapsed = state.expands_to_full_screen.get() && state.is_expanded();
    let collapsed_alpha = if hide_collapsed { 0.0 } else { 1.0 };

    let bar_m = Modifier::new()
        .fill_max_width()
        .height(config.height + insets.top)
        .translate(0.0, config.scroll_offset)
        .background(app_bar_bg)
        .semantics(Semantics::new(Role::Container).with_selectable_group());

    let row = Row(Modifier::new()
        .fill_max_size()
        .align_items(AlignItems::CENTER)
        .padding_values(PaddingValues {
            left: config.content_padding.left + insets.left,
            right: config.content_padding.right + insets.right,
            top: insets.top,
            bottom: 0.0,
        }))
    .child({
        let mut children: Vec<View> = Vec::new();
        if let Some(nav) = navigation_icon {
            children.push(nav);
            children.push(Box(Modifier::new().width(4.0)));
        }
        // Wrap input_field in collapsed SearchBar (CK parity)
        let sb_colors = &config.colors.search_bar_colors;
        let collapsed_bar = SearchBar(
            state.clone(),
            input_field,
            Modifier::new().flex_grow(1.0).alpha(collapsed_alpha),
            None,
            None,
            SearchBarConfig {
                height: config.height - 8.0,
                shape_radius: config.shape_radius,
                colors: SearchBarColors {
                    container_color: bg,
                    active_container_color: bg,
                    divider_color: sb_colors.divider_color,
                    content_color: sb_colors.content_color,
                    placeholder_color: sb_colors.placeholder_color,
                    scrim_color: sb_colors.scrim_color,
                },
                tonal_elevation,
                shadow_elevation,
                ..Default::default()
            },
        );
        children.push(Box(Modifier::new().flex_grow(1.0)).child(collapsed_bar));
        if let Some(acts) = actions {
            children.push(Spacer());
            for a in acts {
                children.push(a);
            }
        }
        children
    });

    Box(bar_m.shadow(shadow_elevation, 0.0)).child(row)
}
