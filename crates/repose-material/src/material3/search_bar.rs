#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use repose_core::NestedScrollConnection;
use repose_core::animation::AnimationSpec;
use repose_core::text::ImeAction;
use repose_core::*;
use repose_ui::{
    BasicTextField, Box, Column, Row, Spacer, Text, TextFieldState, TextStyle, ViewExt, ZStack,
    overlay::OverlayGuard, overlay::ambient_overlay,
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
    pub height: Dp,
    pub shape_radius: Dp,
    pub active_shape_radius: Dp,
    pub expanded_width: Dp,
    pub collapsed_width: Dp,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
    pub window_insets: WindowInsets,
    pub content_padding: PaddingValues,
    pub min_width: Dp,
    pub max_width: Dp,
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
    pub collapsed_shape_radius: Dp,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
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
    pub shape_radius: Dp,
    pub dropdown_shape_radius: Dp,
    pub dropdown_gap_size: Dp,
    pub dropdown_scrim_color: Color,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
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
    pub height: Dp,
    pub shape_radius: Dp,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
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

fn is_key_down(event: &KeyEvent) -> bool {
    event.event_type == KeyEventType::Down && !event.is_repeat
}

fn is_back_event(event: &KeyEvent) -> bool {
    if !is_key_down(event) {
        return false;
    }
    let action = repose_core::shortcuts::resolve_action(repose_core::shortcuts::KeyChord::new(
        event.key.clone(),
        event.modifiers,
    ));
    event.key == Key::Escape || matches!(action, Some(repose_core::shortcuts::Action::Back))
}

/// Scroll behavior for [`AppBarWithSearch`] -> collapses/expands on scroll.
pub struct SearchBarScrollBehavior {
    pub collapsed_offset: Signal<f32>,
    pub height: Dp,
    pub collapsed_height: Dp,
    _pending: Rc<Cell<f32>>,
}

impl SearchBarScrollBehavior {
    pub fn new(height: Dp, collapsed_height: Dp) -> Self {
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
            let new = (cur - delta.y).clamp(-max_offset.0, 0.0);
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
    /// Tracked via `on_globally_positioned` on the collapsed bar.
    /// Used by expanded docked variants for popup placement.
    pub collapsed_layout_rect: Signal<(f32, f32, f32, f32)>,
    id: u64,
    anim: Rc<RefCell<AnimatedValue<f32>>>,
    content_anim: Rc<RefCell<AnimatedValue<f32>>>,
    anim_signal: Signal<f32>,
    content_signal: Signal<f32>,
    anim_key: String,
    content_key: String,
    anim_target: Cell<f32>,
    content_target: Cell<f32>,
    overlay_focus: Rc<RefCell<Option<(u64, FocusRequester)>>>,
    restore_focus: Rc<RefCell<Option<FocusRequester>>>,
}

impl Default for SearchBarState {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SearchBarState {
    fn drop(&mut self) {
        repose_core::animation_driver::unregister(&self.anim_key);
        repose_core::animation_driver::unregister(&self.content_key);
        if let Some((_, requester)) = self.overlay_focus.borrow_mut().take() {
            *requester.target.borrow_mut() = None;
            request_frame();
        }
    }
}

impl SearchBarState {
    pub fn new() -> Self {
        let id = unique_component_id();
        Self {
            query: signal(String::new()),
            expanded: signal(false),
            active: signal(false),
            expands_to_full_screen: signal(false),
            collapsed_layout_rect: signal((0.0, 0.0, 0.0, 0.0)),
            id,
            anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            content_anim: Rc::new(RefCell::new(AnimatedValue::new(
                0.0,
                AnimationSpec::spring_gentle(),
            ))),
            anim_signal: signal(0.0),
            content_signal: signal(0.0),
            anim_key: format!("search:{id}:container"),
            content_key: format!("search:{id}:content"),
            anim_target: Cell::new(0.0),
            content_target: Cell::new(0.0),
            overlay_focus: Rc::new(RefCell::new(None)),
            restore_focus: Rc::new(RefCell::new(None)),
        }
    }

    pub fn key(&self, suffix: &str) -> String {
        format!("search_{}_{}", self.id, suffix)
    }

    pub fn query(&self) -> String {
        self.query.get()
    }

    pub fn set_query(&self, q: impl Into<String>) {
        self.query.set_neq(q.into());
    }

    pub fn is_expanded(&self) -> bool {
        self.expanded.get()
    }

    fn drive(&self, key: &str, animation: &Rc<RefCell<AnimatedValue<f32>>>, output: &Signal<f32>) {
        repose_core::animation_driver::touch(key);
        let active = animation.borrow().is_animating();
        if active && !repose_core::animation_driver::is_registered(key) {
            let animation = animation.clone();
            let output = output.clone();
            repose_core::animation_driver::register(
                key.to_string(),
                Rc::new(RefCell::new(move || {
                    let still = animation.borrow_mut().update();
                    let value = *animation.borrow().get();
                    output.set_neq(value);
                    still
                })),
            );
        }
        if active {
            request_frame();
        }
    }

    fn set_animation_target(
        &self,
        key: &str,
        animation: &Rc<RefCell<AnimatedValue<f32>>>,
        output: &Signal<f32>,
        target_cell: &Cell<f32>,
        target: f32,
    ) {
        let changed = target_cell.get().is_nan() || (target_cell.get() - target).abs() > 1e-6;
        if changed {
            animation.borrow_mut().set_target(target);
            target_cell.set(target);
        }
        self.drive(key, animation, output);
    }

    pub fn expand(&self) {
        self.expanded.set_neq(true);
        self.set_animation_target(
            &self.anim_key,
            &self.anim,
            &self.anim_signal,
            &self.anim_target,
            1.0,
        );
        self.set_animation_target(
            &self.content_key,
            &self.content_anim,
            &self.content_signal,
            &self.content_target,
            1.0,
        );
    }

    fn close_focus(&self) {
        if let Some((_, requester)) = self.overlay_focus.borrow_mut().take() {
            requester.free_focus();
            *requester.target.borrow_mut() = None;
        }
        FocusManager::new(Vec::new(), None).clear_focus(false);
        if let Some(requester) = self.restore_focus.borrow_mut().take() {
            requester.request_focus();
        }
    }

    pub(crate) fn overlay_closed(&self, owner: u64) {
        let owns_overlay = self
            .overlay_focus
            .borrow()
            .as_ref()
            .is_some_and(|(current, _)| *current == owner);
        if owns_overlay {
            self.close_focus();
        }
    }

    pub fn collapse(&self) {
        self.expanded.set_neq(false);
        self.active.set_neq(false);
        self.set_animation_target(
            &self.content_key,
            &self.content_anim,
            &self.content_signal,
            &self.content_target,
            0.0,
        );
        self.set_animation_target(
            &self.anim_key,
            &self.anim,
            &self.anim_signal,
            &self.anim_target,
            0.0,
        );
        self.close_focus();
    }

    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    pub fn activate(&self) {
        self.active.set_neq(true);
        self.expanded.set_neq(true);
        self.set_animation_target(
            &self.anim_key,
            &self.anim,
            &self.anim_signal,
            &self.anim_target,
            1.0,
        );
        self.set_animation_target(
            &self.content_key,
            &self.content_anim,
            &self.content_signal,
            &self.content_target,
            1.0,
        );
    }

    pub fn deactivate(&self) {
        self.collapse();
    }

    pub fn progress(&self) -> f32 {
        self.drive(&self.anim_key, &self.anim, &self.anim_signal);
        self.anim_signal.get().clamp(0.0, 1.0)
    }

    pub fn content_progress(&self) -> f32 {
        self.drive(&self.content_key, &self.content_anim, &self.content_signal);
        self.content_signal.get().clamp(0.0, 1.0)
    }

    /// Whether the animation is currently running.
    pub fn is_animating(&self) -> bool {
        self.anim.borrow().is_animating() || self.content_anim.borrow().is_animating()
    }

    /// Whether the search bar is currently expanded (with tolerance for spring overshoot).
    pub fn current_value(&self) -> SearchBarValue {
        if self.anim_signal.get() <= 0.02 {
            SearchBarValue::Collapsed
        } else {
            SearchBarValue::Expanded
        }
    }

    pub fn snap_to(&self, fraction: f32) {
        let fraction = fraction.clamp(0.0, 1.0);
        self.anim.borrow_mut().snap_to(fraction);
        self.anim_signal.set_neq(fraction);
        self.anim_target.set(fraction);
        request_frame();
    }

    pub(crate) fn set_overlay_focus(&self, owner: u64, requester: FocusRequester) {
        if let Some((previous_owner, previous)) =
            self.overlay_focus.borrow_mut().replace((owner, requester))
            && previous_owner != owner
        {
            previous.free_focus();
            *previous.target.borrow_mut() = None;
        }
    }

    pub fn set_restore_focus(&self, requester: FocusRequester) {
        *self.restore_focus.borrow_mut() = Some(requester);
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
    let query = state.as_ref().map(|state| state.query()).unwrap_or(query);
    let enabled = config.enabled;
    let field_instance = remember(unique_component_id);
    let field_key = state
        .as_ref()
        .map(|state| state.key("input"))
        .unwrap_or_else(|| format!("search-input:instance:{field_instance}"));
    let tf_state = remember_with_key(field_key.clone(), || RefCell::new(TextFieldState::new()));
    tf_state.borrow_mut().apply_controlled_value(&query);
    let restore_requester: Option<Rc<FocusRequester>> = state.as_ref().map(|state| {
        remember_with_key::<FocusRequester>(state.key("restore_focus"), FocusRequester::new)
    });

    let mut input_m = Modifier::new()
        .flex_grow(1.0)
        .padding(Dp(4.0))
        .required_width_in(SearchBarDefaults::MIN_WIDTH, SearchBarDefaults::MAX_WIDTH)
        .required_height_in(SearchBarDefaults::HEIGHT, SearchBarDefaults::HEIGHT)
        .interaction_source(&source)
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if is_back_event(&ev) {
                    if let Some(ref s) = s
                        && s.is_active()
                    {
                        s.deactivate();
                        return true;
                    }
                    return false;
                }
                if is_key_down(&ev)
                    && matches!(ev.key, Key::ArrowDown | Key::ArrowUp)
                    && let Some(ref s) = s
                    && !s.is_expanded()
                {
                    s.activate();
                    return true;
                }
                false
            }
        });
    if let Some(requester) = &restore_requester {
        input_m = input_m.focus_requester(requester.as_ref().clone());
    }
    if let Some(ref s) = state {
        let s2 = s.clone();
        let restore_requester = restore_requester.clone();
        let was_expanded = expanded;
        input_m = input_m.on_focus_changed(move |focused| {
            if focused {
                if let Some(requester) = &restore_requester {
                    s2.set_restore_focus(requester.as_ref().clone());
                }
                s2.activate();
            } else if !was_expanded && s2.is_active() {
                s2.deactivate();
            }
        });
    }

    let on_qc = if let Some(state) = state.clone() {
        let on_query_change = on_query_change.clone();
        Rc::new(move |text: String| {
            state.set_query(text.clone());
            on_query_change(text);
        }) as Rc<dyn Fn(String)>
    } else {
        on_query_change.clone()
    };
    let on_s = config.on_search.clone();

    // Always render the text field (focusable even when collapsed, matching CK).
    let read_only = !expanded;

    let display_color = if query.is_empty() {
        config.placeholder_color
    } else {
        config.text_color
    };

    // Build the row: [leading_icon] + text_field + [trailing_icon]
    let mut row_children: Vec<View> = Vec::new();
    if let Some(icon) = config.leading_icon {
        row_children.push(icon);
    }
    let on_qc2 = on_qc.clone();
    let text_field = BasicTextField(
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
    .size(repose_core::locals::theme().typography.body_large)
    .semantics(Semantics {
        role: Role::TextField,
        label: Some("Search".into()),
        focused,
        enabled,
        value: Some(query),
        ..Default::default()
    });
    row_children.push(text_field);
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
    let active = state.is_active() || state.is_expanded();
    let bar_color = colors.container(active);
    let insets = config.window_insets;

    let mut bar_m = modifier
        .fill_max_width()
        .min_width(config.min_width)
        .max_width(config.max_width)
        .height(config.height + Px(insets.top).to_dp() + Px(insets.bottom).to_dp())
        .state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: th.elevation.level2,
            focused: th.elevation.level2,
            pressed: th.elevation.level3,
            dragged: th.elevation.level3,
            disabled: Dp::ZERO,
        })
        .shadow(config.shadow_elevation, Dp::ZERO)
        .padding_values(PaddingValues {
            left: config.content_padding.left + Px(insets.left).to_dp(),
            right: config.content_padding.right + Px(insets.right).to_dp(),
            top: config.content_padding.top + Px(insets.top).to_dp(),
            bottom: config.content_padding.bottom + Px(insets.bottom).to_dp(),
        })
        .on_key_event({
            let s = state.clone();
            move |ev| {
                if is_back_event(&ev) && s.is_active() {
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
        .background(bar_color)
        .clip_rounded(if state.is_active() || state.is_expanded() {
            config.active_shape_radius
        } else {
            config.shape_radius
        })
        .then(track_collapsed_layout(&state));

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_color);

    Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(Dp(24.0), Dp(24.0)))),
            Box(Modifier::new().width(Dp(8.0)).fill_max_height()),
            with_content_color(colors.content_color, move || input_field),
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
    let instance_id = remember(unique_component_id);
    let animation_identity = search_animation_key(&modifier, "with-content", *instance_id);
    let th = theme();
    let insets = config.window_insets;
    let width_target = if expanded {
        config.expanded_width.0
    } else {
        config.collapsed_width.0
    };
    let width = animate_search_value(
        format!("{animation_identity}:width"),
        width_target,
        width_target,
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

    let mut bar_m = Modifier::new()
        .width(Dp(width))
        .min_width(config.min_width)
        .max_width(config.max_width)
        .height(config.height)
        .shadow(config.shadow_elevation, Dp::ZERO)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if expanded && is_back_event(&ev) {
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
    let content_alpha = animate_search_value(
        format!("{animation_identity}:content-alpha"),
        if expanded { 1.0 } else { 0.0 },
        if expanded { 1.0 } else { 0.0 },
        th.motion.color,
    );

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(Dp(24.0), Dp(24.0)))),
            Box(Modifier::new().width(Dp(8.0)).fill_max_height()),
            with_content_color(config.colors.content_color, move || input_field),
            trailing_icon.unwrap_or(Box(Modifier::new())),
        )),
    );

    let show_content = expanded || content_alpha > 0.01;
    let content_view = if show_content {
        Column(Modifier::new()).child((
            bar,
            Box(Modifier::new()
                .width(Dp(width))
                .max_height(SearchBarDefaults::DOCKED_HEIGHT)
                .alpha(content_alpha)
                .background(bar_bg)
                .clip_rounded(th.shapes.extra_small))
            .child(content),
        ))
    } else {
        bar
    };
    Column(modifier.clone().padding_values(PaddingValues {
        left: Px(insets.left).to_dp(),
        right: Px(insets.right).to_dp(),
        top: Px(insets.top).to_dp(),
        bottom: Px(insets.bottom).to_dp(),
    }))
    .child(content_view)
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
    let instance_id = remember(unique_component_id);
    let animation_identity = search_animation_key(&modifier, "docked", *instance_id);
    let th = theme();
    let active = expanded;
    let colors = config.colors;

    let content_target = if expanded {
        get_window_container_height() * 2.0 / 3.0
    } else {
        0.0
    };
    let content_height = animate_search_value(
        format!("{animation_identity}:height"),
        content_target,
        content_target,
        theme().motion.expand,
    );
    let content_alpha = animate_search_value(
        format!("{animation_identity}:alpha"),
        if expanded { 1.0 } else { 0.0 },
        if expanded { 1.0 } else { 0.0 },
        theme().motion.color,
    );
    let bar_bg = if active {
        colors.active_container_color
    } else {
        colors.container_color
    };

    let clear_source: Rc<MutableInteractionSource> = remember_with_key(
        format!("{animation_identity}:clear-source"),
        MutableInteractionSource::new,
    );
    let clear_btn = if active {
        Box(apply_m3_clickable(
            Modifier::new()
                .size(Dp(24.0), Dp(24.0))
                .clip_rounded(Dp(12.0)),
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
        .child(Text("✕").size(Sp(16.0)).color(colors.placeholder_color))
    } else {
        Box(Modifier::new())
    };

    let mut bar_m = Modifier::new()
        .z_index(1.0)
        .min_width(config.min_width)
        .max_width(config.max_width)
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
            disabled: Dp::ZERO,
        })
        .shadow(config.shadow_elevation, Dp::ZERO)
        .padding_values(config.content_padding)
        .on_key_event({
            let cb = on_expanded_change.clone();
            move |ev| {
                if expanded && is_back_event(&ev) {
                    if let Some(ref cb) = cb {
                        cb(false);
                        return true;
                    }
                }
                false
            }
        })
        .background(bar_bg)
        .clip_rounded(if active {
            config.active_shape_radius
        } else {
            config.shape_radius
        });

    bar_m = apply_tonal_elevation(bar_m, config.tonal_elevation, bar_bg);

    let bar = Box(bar_m).child(
        Row(Modifier::new()
            .fill_max_size()
            .align_items(AlignItems::CENTER))
        .child((
            leading_icon.unwrap_or(Box(Modifier::new().size(Dp(24.0), Dp(24.0)))),
            Box(Modifier::new().width(Dp(12.0)).fill_max_height()),
            with_content_color(colors.content_color, move || input_field),
            clear_btn,
        )),
    );

    let show_content = expanded || content_height > 1.0;
    let content_view = if show_content {
        Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
            bar,
            Box(Modifier::new()
                .min_width(SearchBarDefaults::MIN_WIDTH)
                .height(Dp(content_height))
                .alpha(content_alpha)
                .clip_rounded(th.shapes.small)
                .background(colors.container_color)
                .state_elevation(StateElevation {
                    default: config.tonal_elevation,
                    hovered: config.tonal_elevation,
                    focused: config.tonal_elevation,
                    pressed: config.tonal_elevation,
                    dragged: config.tonal_elevation,
                    disabled: Dp::ZERO,
                }))
            .child(
                Column(Modifier::new().min_width(SearchBarDefaults::MIN_WIDTH)).child((
                    Box(Modifier::new()
                        .min_width(SearchBarDefaults::MIN_WIDTH)
                        .height(Dp(1.0))
                        .background(colors.divider_color)),
                    content,
                )),
            ),
        ))
    } else {
        bar
    };
    let insets = config.window_insets;
    Column(
        modifier
            .clone()
            .min_width(SearchBarDefaults::MIN_WIDTH)
            .padding_values(PaddingValues {
                left: Px(insets.left).to_dp(),
                right: Px(insets.right).to_dp(),
                top: Px(insets.top).to_dp(),
                bottom: Px(insets.bottom).to_dp(),
            }),
    )
    .child(content_view)
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

fn search_animation_key(modifier: &Modifier, name: &str, instance_id: u64) -> String {
    match modifier.key {
        Some(key) => format!("search:{name}:key:{key}"),
        None => format!("search:{name}:instance:{instance_id}"),
    }
}

fn animate_search_value(key: String, initial: f32, target: f32, spec: AnimationSpec) -> f32 {
    let anim_key = format!("search:driver:{key}");
    let animation = remember_state_with_key(anim_key.clone(), || AnimatedValue::new(initial, spec));
    let last_target = remember_state_with_key(format!("search:driver-target:{key}"), || target);
    let output = remember_with_key(format!("search:driver-output:{key}"), || signal(initial));
    repose_core::animation_driver::touch(&anim_key);
    let changed = last_target.borrow().is_nan() || (*last_target.borrow() - target).abs() > 1e-6;
    if changed {
        animation.borrow_mut().set_spec(spec);
        animation.borrow_mut().set_target(target);
        *last_target.borrow_mut() = target;
    }
    if animation.borrow().is_animating() && !repose_core::animation_driver::is_registered(&anim_key)
    {
        let animation = animation.clone();
        let output = output.clone();
        repose_core::animation_driver::register(
            anim_key.clone(),
            Rc::new(RefCell::new(move || {
                let still = animation.borrow_mut().update();
                let value = *animation.borrow().get();
                output.set_neq(value);
                still
            })),
        );
    }
    if animation.borrow().is_animating() {
        request_frame();
    }
    output.get()
}

fn attach_focus_requester(view: &mut View, requester: &FocusRequester) -> bool {
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
        view.modifier.focus_requester = Some(requester.clone());
        return true;
    }
    view.children
        .iter_mut()
        .any(|child| attach_focus_requester(child, requester))
}

/// Set the window container width (in dp) used for dropdown constraints.
pub fn set_window_container_width(w: f32) {
    repose_core::locals::set_window_container_width(w);
}

/// M3 Expanded Full-Screen Search Bar -> rendered in an overlay covering the
/// entire window. Uses the state's own `progress()` for animation.
/// Equivalent to CK's `ExpandedFullScreenSearchBar(state, inputField, ...)`.
///
/// Renders into the ambient overlay layer installed by the runtime.
pub fn ExpandedFullScreenSearchBar(
    state: Rc<SearchBarState>,
    input_field: View,
    modifier: Modifier,
    config: ExpandedFullScreenSearchBarConfig,
    content: View,
) -> View {
    let overlay = ambient_overlay();
    // Mark as full-screen so AppBarWithSearch can hide the collapsed bar
    state.expands_to_full_screen.set_neq(true);

    let overlay_instance = remember(unique_component_id);
    let efs_id = format!("{}_{}", state.key("expanded-full"), overlay_instance);
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
    let input_fr = remember_with_key(format!("{efs_id}:input"), FocusRequester::new);
    let focus_requested =
        remember_with_key(format!("{efs_id}:focus-requested"), || Cell::new(false));
    if current_scope().is_some() {
        let cleanup_state = state.clone();
        let cleanup_owner = *overlay_instance;
        effect_once_with_key(format!("{efs_id}:unmount"), move || {
            on_unmount(move || cleanup_state.overlay_closed(cleanup_owner))
        });
    }
    if !expanded {
        state.overlay_closed(*overlay_instance);
    }
    if visible && expanded {
        state.set_overlay_focus(*overlay_instance, (*input_fr).clone());
    } else {
        focus_requested.set(false);
        *input_fr.target.borrow_mut() = None;
    }

    if visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
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
                    let mut input_field = current_input.borrow().clone();
                    let config = current_config.borrow().clone();
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let th = theme();
                    let content = current_content.borrow().clone();

                    let attached = attach_focus_requester(&mut input_field, &input_fr);
                    let inp = if attached {
                        input_field
                    } else {
                        Box(Modifier::new()
                            .focusable(true)
                            .focus_requester((*input_fr).clone()))
                        .child(input_field)
                    };
                    if state.is_expanded() && !focus_requested.get() {
                        if input_fr.target.borrow().is_some() {
                            input_fr.request_focus();
                            focus_requested.set(true);
                        } else {
                            request_frame();
                        }
                    }

                    let header = Box(Modifier::new()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .padding_values(PaddingValues {
                            left: Dp(16.0),
                            right: Dp(16.0),
                            top: Dp(0.0),
                            bottom: Dp(0.0),
                        })
                        .background(config.colors.container(state.is_expanded()))
                        .clip_rounded(config.collapsed_shape_radius)
                        .state_elevation(StateElevation {
                            default: config.tonal_elevation,
                            hovered: config.tonal_elevation,
                            focused: config.tonal_elevation,
                            pressed: config.tonal_elevation,
                            dragged: config.tonal_elevation,
                            disabled: Dp::ZERO,
                        })
                        .shadow(config.shadow_elevation, Dp::ZERO)
                        .alpha(alpha))
                    .child(inp);

                    let body = Box(Modifier::new()
                        .fill_max_width()
                        .flex_grow(1.0)
                        .alpha(c_alpha)
                        .background(th.surface))
                    .child(content);

                    let insets = config.window_insets;
                    let full = Column(
                        modifier
                            .clone()
                            .fill_max_size()
                            .padding_values(PaddingValues {
                                left: Px(insets.left).to_dp(),
                                right: Px(insets.right).to_dp(),
                                top: Px(insets.top).to_dp(),
                                bottom: Px(insets.bottom).to_dp(),
                            })
                            .focus_group()
                            .on_preview_key_event({
                                let state = state.clone();
                                move |event: KeyEvent| {
                                    if state.is_expanded() && is_back_event(&event) {
                                        state.collapse();
                                        true
                                    } else {
                                        false
                                    }
                                }
                            }),
                    )
                    .child((header, body));

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(
                            config
                                .scrim_color
                                .with_alpha((config.scrim_color.3 as f32 * alpha).round() as u8),
                        )
                        .input_blocker()
                        .focusable(false)
                        .on_scroll(|_| Vec2::default())
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));
                    let focus_probe = {
                        let state = state.clone();
                        let input_fr = input_fr.clone();
                        let focus_requested = focus_requested.clone();
                        Box(Modifier::new()
                            .size(Dp(0.0), Dp(0.0))
                            .hit_passthrough()
                            .on_globally_positioned(move |_| {
                                if state.is_expanded()
                                    && !focus_requested.get()
                                    && input_fr.target.borrow().is_some()
                                {
                                    input_fr.request_focus();
                                    focus_requested.set(true);
                                }
                            }))
                    };

                    ZStack(Modifier::new().fill_max_size().absolute()).child((
                        scrim,
                        full,
                        focus_probe,
                    ))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 850.0, false));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    Box(Modifier::new())
}

/// M3 Expanded Docked Search Bar -> rendered as an overlay popup anchored below
/// the collapsed search bar using `collapsed_layout_rect`.
/// Equivalent to CK's `ExpandedDockedSearchBar(state, inputField, ...)`.
///
/// Renders into the ambient overlay layer installed by the runtime.
pub fn ExpandedDockedSearchBar(
    state: Rc<SearchBarState>,
    input_field: View,
    modifier: Modifier,
    config: ExpandedDockedSearchBarConfig,
    content: View,
) -> View {
    let overlay = ambient_overlay();
    // Docked search bar does NOT expand to full-screen
    state.expands_to_full_screen.set_neq(false);

    let overlay_instance = remember(unique_component_id);
    let eds_id = format!("{}_{}", state.key("expanded-docked"), overlay_instance);
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
    let input_fr = remember_with_key(format!("{eds_id}:input"), FocusRequester::new);
    let focus_requested =
        remember_with_key(format!("{eds_id}:focus-requested"), || Cell::new(false));
    if current_scope().is_some() {
        let cleanup_state = state.clone();
        let cleanup_owner = *overlay_instance;
        effect_once_with_key(format!("{eds_id}:unmount"), move || {
            on_unmount(move || cleanup_state.overlay_closed(cleanup_owner))
        });
    }
    if !expanded {
        state.overlay_closed(*overlay_instance);
    }
    if visible && expanded {
        state.set_overlay_focus(*overlay_instance, (*input_fr).clone());
    } else {
        focus_requested.set(false);
        *input_fr.target.borrow_mut() = None;
    }

    if visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
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
                    let mut input_field = current_input.borrow().clone();
                    let config = current_config.borrow().clone();
                    let progress = state.progress();
                    let content_alpha = state.content_progress();
                    let alpha = progress.clamp(0.0, 1.0);
                    let c_alpha = content_alpha.clamp(0.0, 1.0);
                    let content = current_content.borrow().clone();
                    let (_cx, _cy, _cw, _ch) = state.collapsed_layout_rect.get();

                    let attached = attach_focus_requester(&mut input_field, &input_fr);
                    let inp = if attached {
                        input_field
                    } else {
                        Box(Modifier::new()
                            .focusable(true)
                            .focus_requester((*input_fr).clone()))
                        .child(input_field)
                    };
                    if state.is_expanded() && !focus_requested.get() {
                        if input_fr.target.borrow().is_some() {
                            input_fr.request_focus();
                            focus_requested.set(true);
                        } else {
                            request_frame();
                        }
                    }

                    let header = Box(Modifier::new()
                        .fill_max_width()
                        .height(SearchBarDefaults::HEIGHT)
                        .alpha(alpha)
                        .background(config.colors.container(state.is_expanded()))
                        .clip_rounded(config.shape_radius)
                        .state_elevation(StateElevation {
                            default: config.tonal_elevation,
                            hovered: config.tonal_elevation,
                            focused: config.tonal_elevation,
                            pressed: config.tonal_elevation,
                            dragged: config.tonal_elevation,
                            disabled: Dp::ZERO,
                        })
                        .shadow(config.shadow_elevation, Dp::ZERO))
                    .child(inp);

                    let dropdown = Box(Modifier::new()
                        .fill_max_width()
                        .max_height(Dp(get_window_container_height() * 2.0 / 3.0))
                        .alpha(c_alpha)
                        .clip_rounded(config.dropdown_shape_radius)
                        .background(config.colors.container(state.is_expanded()))
                        .state_elevation(StateElevation {
                            default: config.tonal_elevation,
                            hovered: config.tonal_elevation,
                            focused: config.tonal_elevation,
                            pressed: config.tonal_elevation,
                            dragged: config.tonal_elevation,
                            disabled: Dp::ZERO,
                        })
                        .shadow(config.shadow_elevation, Dp::ZERO))
                    .child(
                        Column(Modifier::new().fill_max_width()).child((
                            Box(Modifier::new()
                                .fill_max_width()
                                .height(Dp(1.0))
                                .background(config.colors.divider_color)),
                            content,
                        )),
                    );

                    let docked_width = Dp(_cw).max(SearchBarDefaults::MIN_WIDTH);
                    let popup_left = Dp(_cx);
                    let popup_top = Dp(_cy) + Dp(_ch) + config.dropdown_gap_size;

                    let col = Column(Modifier::new().fill_max_width()).child((header, dropdown));

                    let positioned = Box(modifier
                        .clone()
                        .absolute()
                        .offset(Some(popup_left), Some(popup_top), None, None)
                        .width(docked_width)
                        .focus_group()
                        .on_preview_key_event({
                            let state = state.clone();
                            move |event: KeyEvent| {
                                if state.is_expanded() && is_back_event(&event) {
                                    state.collapse();
                                    true
                                } else {
                                    false
                                }
                            }
                        }))
                    .child(col);

                    let scrim = Box(Modifier::new()
                        .fill_max_size()
                        .background(config.dropdown_scrim_color)
                        .input_blocker()
                        .focusable(false)
                        .on_scroll(|_| Vec2::default())
                        .on_click({
                            let s = state.clone();
                            move || s.collapse()
                        }));

                    let focus_probe = {
                        let state = state.clone();
                        let input_fr = input_fr.clone();
                        let focus_requested = focus_requested.clone();
                        Box(Modifier::new()
                            .size(Dp(0.0), Dp(0.0))
                            .hit_passthrough()
                            .on_globally_positioned(move |_| {
                                if state.is_expanded()
                                    && !focus_requested.get()
                                    && input_fr.target.borrow().is_some()
                                {
                                    input_fr.request_focus();
                                    focus_requested.set(true);
                                }
                            }))
                    };

                    ZStack(Modifier::new().fill_max_size().absolute()).child((
                        scrim,
                        positioned,
                        focus_probe,
                    ))
                }
            });

            *overlay_guard.borrow_mut() = Some(overlay.show_guard(builder, 850.0, false));
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
        Dp::ZERO
    } else {
        config.tonal_elevation
    };
    let shadow_elevation = if is_container_transparent {
        Dp::ZERO
    } else {
        config.shadow_elevation
    };

    // Hide the collapsed bar when full-screen expanded (CK parity via expandsToFullScreen)
    let hide_collapsed = state.expands_to_full_screen.get() && state.is_expanded();
    let collapsed_alpha = if hide_collapsed { 0.0 } else { 1.0 };

    let bar_m = Modifier::new()
        .fill_max_width()
        .height(config.height + Px(insets.top).to_dp())
        .translate(0.0, config.scroll_offset)
        .background(app_bar_bg)
        .state_elevation(StateElevation {
            default: tonal_elevation,
            hovered: tonal_elevation,
            focused: tonal_elevation,
            pressed: tonal_elevation,
            dragged: tonal_elevation,
            disabled: Dp::ZERO,
        })
        .then(config.modifier.clone())
        .semantics(Semantics::new(Role::Container).with_selectable_group());

    let row = Row(Modifier::new()
        .fill_max_size()
        .align_items(AlignItems::CENTER)
        .padding_values(PaddingValues {
            left: config.content_padding.left + Px(insets.left).to_dp(),
            right: config.content_padding.right + Px(insets.right).to_dp(),
            top: Px(insets.top).to_dp(),
            bottom: Dp(0.0),
        }))
    .child({
        let mut children: Vec<View> = Vec::new();
        if let Some(nav) = navigation_icon {
            children.push(with_content_color(
                config.colors.navigation_icon_content_color,
                move || nav,
            ));
            children.push(Box(Modifier::new().width(Dp(4.0))));
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
                height: (config.height - Dp(8.0)).max(Dp(1.0)),
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
                children.push(with_content_color(
                    config.colors.action_icon_content_color,
                    move || a,
                ));
            }
        }
        children
    });

    Box(bar_m.shadow(shadow_elevation, Dp::ZERO)).child(row)
}
