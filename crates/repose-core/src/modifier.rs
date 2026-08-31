use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use taffy::{AlignContent, AlignItems, AlignSelf, FlexDirection, FlexWrap, JustifyContent};

use crate::animation::AnimationSpec;
use crate::indication::IndicationNodeFactory;
use crate::{Brush, Color, PointerEvent, Size, Transform, Vec2};

/// State-driven colors. Priority: disabled > dragged > pressed > focused > hovered > default.
///
/// Priority (paint): disabled > dragged > pressed > focused > hovered > default.
#[derive(Clone, Copy, Debug)]
pub struct StateColors {
    pub default: Color,
    pub hovered: Color,
    /// Color while focused.
    pub focused: Color,
    pub pressed: Color,
    pub disabled: Color,
    /// While dragged (overrides hover/press/focus).
    pub dragged: Color,
}

/// State-driven elevation. Priority: disabled > dragged > pressed > focused > hovered > default.
#[derive(Clone, Copy, Debug)]
pub struct StateElevation {
    pub default: f32,
    pub hovered: f32,
    /// Applied between pressed and hovered in the paint priority order.
    pub focused: f32,
    pub pressed: f32,
    pub disabled: f32,
    /// Elevation while the component is being dragged (preferred over hovered/pressed/focused).
    pub dragged: f32,
}

impl StateColors {
    pub const fn transparent() -> Self {
        Self {
            default: Color::TRANSPARENT,
            hovered: Color::TRANSPARENT,
            focused: Color::TRANSPARENT,
            pressed: Color::TRANSPARENT,
            disabled: Color::TRANSPARENT,
            dragged: Color::TRANSPARENT,
        }
    }
}

impl StateElevation {
    pub const fn zero() -> Self {
        Self {
            default: 0.0,
            hovered: 0.0,
            focused: 0.0,
            pressed: 0.0,
            disabled: 0.0,
            dragged: 0.0,
        }
    }
}

macro_rules! merge_opts {
    ($dst:ident, $src:ident; $($f:ident),+ $(,)?) => {
        $( $dst.$f = $src.$f.or($dst.$f); )+
    };
}
macro_rules! merge_flags {
    ($dst:ident, $src:ident; $($f:ident),+ $(,)?) => {
        $( $dst.$f |= $src.$f; )+
    };
}

macro_rules! impl_option_fields {
    ($ty:ty, $fn:ident) => {
        impl $ty {
            $fn!(replace);
        }
    };
    ($ty:ident) => {
        impl $ty {
            /// Chain another modifier's settings onto this one.
            /// Useful for creating reusable modifier templates.
            pub fn then(mut self, other: Self) -> Self {
                merge_opts!(self, other;
                    key, size, width, height, required_size,
                    padding, padding_values,
                    min_width, min_height, max_width, max_height,
                    required_min_width, required_max_width,
                    required_min_height, required_max_height,
                    default_min_width, default_min_height,
                    fill_max, fill_max_w, fill_max_h,
                    background, state_colors, state_elevation, border,
                    flex_grow, flex_shrink, flex_basis, flex_wrap, flex_dir,
                    gap, row_gap, column_gap,
                    align_self, justify_content, align_items_container, align_content,
                    clip_rounded, clip_rect, overflow, render_z_index,
                    on_scroll,
                    nested_scroll_connection,
                    scroll,
                    on_pointer_down, on_pointer_move, on_pointer_up,
                    on_pointer_enter, on_pointer_leave,
                    on_click, on_double_click, on_long_click,
                    semantics, alpha, transform,
                    grid, grid_col_span, grid_row_span,
                    position_type,
                    offset_left, offset_right, offset_top, offset_bottom,
                    margin_left, margin_right, margin_top, margin_bottom,
                    aspect_ratio, intrinsic_width, intrinsic_height,
                    painter,
                    on_drag_start, on_drag_end, on_drag_enter, on_drag_over, on_drag_leave, on_drop,
                    drag_preview,
                    on_action, cursor, animate_content_size, focus_requester, on_focus_changed,
                    interaction_source, text_input,
                );
                        merge_flags!(self, other;
                    hit_passthrough, input_blocker, repaint_boundary, click, disabled,
                    propagate_min, focus_group,
                );

                if let Some(f) = other.focusable {
                    self.focusable = Some(f);
                }
                if other.indication.is_some() {
                    self.indication = other.indication;
                }
                if other.z_index != 0.0 {
                    self.z_index = other.z_index;
                }
                self
            }
        }
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ClipOp {
    /// Keep content inside the clip rect (default).
    #[default]
    Intersect,
    /// Remove content inside the clip rect (cutout).
    Difference,
}

/// Controls whether child content is clipped to the parent bounds.
///
/// Analogous to CSS `overflow`:
/// - `Clip` (default): content extending beyond the parent is hidden.
/// - `Visible`: content is allowed to overflow the parent bounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Overflow {
    #[default]
    Clip,
    Visible,
}

/// Rectangular clip with a clipping operation.
/// The rect is relative to the element bounds, in dp.
#[derive(Clone, Copy, Debug)]
pub struct ClipRect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub op: ClipOp,
}

#[derive(Clone, Debug)]
pub struct Border {
    pub width: f32,
    pub color: Color,
    pub radius: [f32; 4],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaddingValues {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
pub struct GridConfig {
    pub columns: usize,
    pub row_gap: f32,
    pub column_gap: f32,
}

/// Edge treatment for `Modifier::blur` -> controls how pixels at the edges
/// of the blurred region are handled.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BlurredEdgeTreatment {
    /// Clip the blur to the element's bounds and clamp edge pixels
    /// (extend the outermost pixels). This is the Compose default.
    Rectangle,
    /// Allow the blur to extend beyond the element's bounds.
    /// Edge pixels are treated as transparent (decal).
    Unbounded,
}

/// Gaussian blur parameters for `Modifier::blur`.
#[derive(Clone, Copy, Debug)]
pub struct BlurStyle {
    /// Horizontal blur radius in dp.
    pub radius_x: f32,
    /// Vertical blur radius in dp.
    pub radius_y: f32,
    /// Controls edge pixel behavior.
    pub edge_treatment: BlurredEdgeTreatment,
}

/// Constraints passed to the `Modifier::layout` callback.
/// Mirrors Compose's `Constraints` -> the element's size must fall within
/// `[min_width, max_width]` × `[min_height, max_height]`.
/// A dimension with `INFINITY` max means unbounded in that direction.
#[derive(Clone, Copy, Debug)]
pub struct LayoutConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

/// Drop-shadow parameters applied to a graphics layer.
///
/// `blur_radius` is the Gaussian blur radius in dp (1.0 = subtle, 8.0 = soft,
/// 16.0 = very diffuse). `offset_y` is the vertical offset of the shadow in dp
/// (positive = below the layer). `color` is the shadow color (premultiplied
/// alpha controls shadow darkness).
#[derive(Clone, Copy, Debug)]
pub struct ShadowSpec {
    pub blur_radius: f32,
    pub offset_y: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum PositionType {
    Relative,
    Absolute,
}

/// Configuration for a text input field.
#[derive(Clone)]
pub struct TextInputConfig {
    pub hint: String,
    pub multiline: bool,
    pub on_change: Option<Rc<dyn Fn(String)>>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    pub focus_tracker: Option<Rc<Cell<bool>>>,
    pub value: String,
    pub visual_transformation: Option<Rc<dyn crate::text::VisualTransformation>>,
    pub keyboard_type: crate::text::KeyboardType,
    pub capitalization: crate::text::KeyboardCapitalization,
    pub ime_action: crate::text::ImeAction,
    /// Platform keyboard auto-correct hint. `None` = follow platform default
    /// (except password keyboards, which never auto-correct).
    pub auto_correct_enabled: Option<bool>,
    /// When false, the text field is not editable, not focusable, and input is not selectable.
    pub enabled: bool,
    /// When true, the text field can be focused and text can be selected/copied, but not modified.
    pub read_only: bool,
    /// Maximum visible lines. Only effective when `multiline` is true.
    pub max_lines: Option<usize>,
    /// Minimum visible lines. Only effective when `multiline` is true.
    pub min_lines: usize,
    /// Override the cursor color. When None, uses the theme's `on_surface`.
    pub cursor_color: Option<Color>,
    /// Callback invoked after each text layout computation, providing layout details
    /// such as line count and content size.
    pub on_text_layout: Option<Rc<dyn Fn(&crate::text::TextLayoutResult)>>,
    /// Style for the text content (font size, color, weight, etc.).
    /// None = use defaults (16dp, theme color, NORMAL weight).
    pub text_style: Option<crate::text::TextStyle>,
    /// Per-action IME callbacks (onDone, onGo, onNext, etc.).
    /// None = use `on_submit` for all actions.
    pub keyboard_actions: Option<crate::text::KeyboardActions>,
    /// Interaction source for tracking focus/press/hover state.
    pub interaction_source: Option<InteractionSource>,
    /// Line limits (SingleLine or MultiLine). Overrides `multiline`/`max_lines`/`min_lines`.
    pub line_limits: Option<crate::text::TextFieldLineLimits>,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            hint: String::new(),
            multiline: false,
            on_change: None,
            on_submit: None,
            focus_tracker: None,
            value: String::new(),
            visual_transformation: None,
            keyboard_type: crate::text::KeyboardType::default(),
            capitalization: crate::text::KeyboardCapitalization::default(),
            ime_action: crate::text::ImeAction::default(),
            auto_correct_enabled: None,
            enabled: true,
            read_only: false,
            max_lines: None,
            min_lines: 1,
            cursor_color: None,
            on_text_layout: None,
            text_style: None,
            keyboard_actions: None,
            interaction_source: None,
            line_limits: None,
        }
    }
}

impl std::fmt::Debug for TextInputConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("TextInputConfig");
        s.field("hint", &self.hint);
        s.field("multiline", &self.multiline);
        if self.on_change.is_some() {
            s.field("on_change", &"…");
        }
        if self.on_submit.is_some() {
            s.field("on_submit", &"…");
        }
        if self.focus_tracker.is_some() {
            s.field("focus_tracker", &"…");
        }
        s.field("value", &self.value);
        if self.visual_transformation.is_some() {
            s.field("visual_transformation", &"…");
        }
        s.field("keyboard_type", &self.keyboard_type);
        s.field("capitalization", &self.capitalization);
        s.field("ime_action", &self.ime_action);
        s.field("auto_correct_enabled", &self.auto_correct_enabled);
        s.field("enabled", &self.enabled);
        s.field("read_only", &self.read_only);
        s.field("max_lines", &self.max_lines);
        s.field("min_lines", &self.min_lines);
        s.field("cursor_color", &self.cursor_color);
        if self.on_text_layout.is_some() {
            s.field("on_text_layout", &"…");
        }
        s.finish()
    }
}

/// Intrinsic sizing mode for [`Modifier::intrinsic_width`] and [`Modifier::intrinsic_height`].
/// When set, the node sizes itself to the intrinsic content size in that dimension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntrinsicSize {
    Min,
    Max,
}

static PRESS_COUNTER: AtomicU64 = AtomicU64::new(1);

/// A press identifier for linking Press -> Release/Cancel pairs.
pub type PressId = u64;

/// An interaction event that can be emitted by a [`MutableInteractionSource`].
///
/// Compose-like per-interaction-type hierarchy:
/// - `PressInteraction.Press(position)` / `Release(press)` / `Cancel(press)`
/// - `HoverInteraction.Enter` / `Exit`
/// - `FocusInteraction.Focus` / `Unfocus`
/// - `DragInteraction.Start` / `Stop` / `Cancel`
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interaction {
    /// A press started at the given position (in local coords).
    /// Carries a unique `PressId` so Release/Cancel can identify which press to end.
    Press(PressId, Vec2),
    /// The press with the given `PressId` was released.
    Release(PressId),
    /// The press with the given `PressId` was cancelled
    /// (e.g. gesture disambiguation, pointer leave during press).
    Cancel(PressId),
    HoverEnter,
    HoverLeave,
    Focus,
    Unfocus,
    DragStart,
    DragStop,
    DragCancel,
}

impl Interaction {
    /// Create a new `Press` with a fresh unique ID and the given position.
    #[inline]
    pub fn new_press(position: Vec2) -> Self {
        Interaction::Press(PRESS_COUNTER.fetch_add(1, Ordering::Relaxed), position)
    }
}

/// Read-only handle to a shared interaction state.
///
/// Use [`MutableInteractionSource::source`] to obtain a read handle, or
/// [`MutableInteractionSource::new`] + `.source()` to create a new source pair.
///
/// Multiple clones share the same underlying state.
#[derive(Clone)]
pub struct InteractionSource {
    pub(crate) state: Rc<RefCell<InteractionState>>,
}

impl InteractionSource {
    pub fn collect_is_pressed(&self) -> bool {
        !self.state.borrow().active_presses.is_empty()
    }
    pub fn collect_is_hovered(&self) -> bool {
        self.state.borrow().hovered
    }
    pub fn collect_is_focused(&self) -> bool {
        self.state.borrow().focused
    }

    /// Focused and keyboard/D-pad input mode (Compose `:focus-visible`).
    pub fn collect_is_focus_visible(&self) -> bool {
        crate::input::is_focus_visible(self.collect_is_focused())
    }

    pub fn collect_is_dragged(&self) -> bool {
        self.state.borrow().dragged > 0
    }
    pub fn collect_last_press_position(&self) -> Option<Vec2> {
        self.state.borrow().last_press_position
    }
    pub fn collect_last_press_id(&self) -> Option<PressId> {
        self.state.borrow().last_press_id
    }
    /// Stable identity: the pointer of the shared state Rc.
    pub fn stable_id(&self) -> *const () {
        Rc::as_ptr(&self.state) as *const ()
    }
    /// Get a mutable handle to the same underlying state.
    /// Both handles share the same `Rc<RefCell<..>>`, so mutations via
    /// the returned `MutableInteractionSource` are reflected here.
    pub fn to_mutable(&self) -> MutableInteractionSource {
        MutableInteractionSource {
            state: self.state.clone(),
        }
    }

    /// Convenience: hard-reset via read handle (same Rc).
    pub fn reset(&self) {
        self.to_mutable().reset();
    }

    /// Convenience: clear hover only via read handle (same Rc).
    pub fn reset_hover(&self) {
        self.to_mutable().reset_hover();
    }
}

/// Mutable handle to a shared interaction state.
///
/// Create one via [`MutableInteractionSource::new`], then pass the read-only
/// [`InteractionSource`] to modifiers via `.interaction_source(&source)`.
///
/// ```ignore
/// let src = remember(MutableInteractionSource::new);
/// m = m.clickable().interaction_source(&src).state_colors(...);
/// ```
#[derive(Clone)]
pub struct MutableInteractionSource {
    pub(crate) state: Rc<RefCell<InteractionState>>,
}

impl std::fmt::Debug for MutableInteractionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MutableInteractionSource")
            .finish_non_exhaustive()
    }
}

impl MutableInteractionSource {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(InteractionState::default())),
        }
    }

    /// Emit an interaction event, updating the shared state.
    pub fn emit(&self, interaction: Interaction) {
        let changed = {
            let mut s = self.state.borrow_mut();
            match interaction {
                Interaction::Press(id, pos) => {
                    let inserted = s.active_presses.insert(id);
                    s.last_press_id = Some(id);
                    s.last_press_position = Some(pos);
                    inserted
                }
                Interaction::Release(id) | Interaction::Cancel(id) => {
                    if s.active_presses.remove(&id) {
                        true
                    } else if id == 0 {
                        if let Some(any) = s.active_presses.iter().next().copied() {
                            s.active_presses.remove(&any);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
                Interaction::HoverEnter => {
                    let changed = !s.hovered;
                    s.hovered = true;
                    changed
                }
                Interaction::HoverLeave => {
                    let changed = s.hovered;
                    s.hovered = false;
                    // Leaving while pressed cancels all presses (Compose-like).
                    if !s.active_presses.is_empty() {
                        s.active_presses.clear();
                        true
                    } else {
                        changed
                    }
                }
                Interaction::Focus => {
                    let changed = !s.focused;
                    s.focused = true;
                    changed
                }
                Interaction::Unfocus => {
                    let changed = s.focused;
                    s.focused = false;
                    changed
                }
                Interaction::DragStart => {
                    let changed = s.dragged == 0;
                    s.dragged = s.dragged.saturating_add(1);
                    changed
                }
                Interaction::DragStop | Interaction::DragCancel => {
                    let was = s.dragged;
                    s.dragged = s.dragged.saturating_sub(1);
                    was != s.dragged
                }
            }
        };
        if changed {
            // So source-driven paint (ripple, state layers) cannot stick stale.
            crate::frame_clock::request_frame();
        }
    }

    /// Get a read-only handle to the shared state.
    pub fn source(&self) -> InteractionSource {
        InteractionSource {
            state: self.state.clone(),
        }
    }

    /// Hard-reset all interaction flags.
    pub fn reset(&self) {
        let mut s = self.state.borrow_mut();
        *s = InteractionState::default();
        crate::frame_clock::request_frame();
    }

    /// Clear hover only (keep press/focus/drag).
    pub fn reset_hover(&self) {
        let mut s = self.state.borrow_mut();
        if s.hovered {
            s.hovered = false;
            crate::frame_clock::request_frame();
        }
    }
}

impl Default for MutableInteractionSource {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Default)]
pub(crate) struct InteractionState {
    /// Active press IDs (Press → Release/Cancel pairing).
    active_presses: HashSet<PressId>,
    hovered: bool,
    focused: bool,
    dragged: u32,
    /// Most recent press position (used by ripple for origin).
    pub(crate) last_press_position: Option<Vec2>,
    /// Most recent press ID.
    pub(crate) last_press_id: Option<PressId>,
}

#[derive(Clone, Default)]
pub struct Modifier {
    /// Optional stable identity key for this view node.
    ///
    /// If set, `layout_and_paint` will prefer this over child index when assigning stable ViewIds.
    /// This is the “escape hatch” for dynamic lists / conditional UI where index-based identity
    /// would otherwise shift.
    pub key: Option<u64>,

    pub size: Option<Size>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub required_size: Option<Size>,
    pub fill_max: Option<f32>,
    pub fill_max_w: Option<f32>,
    pub fill_max_h: Option<f32>,
    pub padding: Option<f32>,
    pub padding_values: Option<PaddingValues>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    /// Like [`required_size`] but only for min width. Overrides parent min constraints.
    pub required_min_width: Option<f32>,
    /// Like [`required_size`] but only for max width. Overrides parent max constraints.
    pub required_max_width: Option<f32>,
    /// Like [`required_size`] but only for min height. Overrides parent min constraints.
    pub required_min_height: Option<f32>,
    /// Like [`required_size`] but only for max height. Overrides parent max constraints.
    pub required_max_height: Option<f32>,
    /// Minimum size that only applies when the incoming constraint is 0 (unconstrained).
    /// Use [`min_width`] for an unconditional minimum.
    pub default_min_width: Option<f32>,
    pub default_min_height: Option<f32>,
    pub background: Option<Brush>,
    pub state_colors: Option<StateColors>,
    pub state_elevation: Option<StateElevation>,

    pub border: Option<Border>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<f32>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_dir: Option<FlexDirection>,
    pub gap: Option<f32>,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub align_self: Option<AlignSelf>,
    pub justify_content: Option<JustifyContent>,
    pub align_items_container: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub clip_rounded: Option<[f32; 4]>,
    /// Rectangular clip with a clipping operation (Intersect or Difference).
    /// The rect is relative to the element bounds, in dp.
    pub clip_rect: Option<ClipRect>,
    /// Controls whether child content is clipped to the parent bounds.
    ///
    /// Defaults to `Clip`. When set to `Visible`, children can overflow
    /// the parent's rounded rect or clip rect boundary.
    pub overflow: Option<Overflow>,
    /// Z-index for hit-testing order (higher = receives events first).
    pub z_index: f32,
    /// Z-index for render order (higher = painted on top). If None, uses tree order.
    pub render_z_index: Option<f32>,
    /// If true, this view does not create hit regions.
    pub hit_passthrough: bool,
    /// If true, this view blocks pointer/touch input for hits below it.
    pub input_blocker: bool,
    pub repaint_boundary: bool,
    pub click: bool,
    /// When true, the component ignores pointer events and appears disabled.
    pub disabled: bool,
    /// When Some(true), the component can receive keyboard focus regardless of interactivity.
    /// When Some(false), the component cannot receive focus even if interactive.
    /// When None, focusability is determined implicitly by interactivity (click/pointer/dnd handlers).
    pub focusable: Option<bool>,
    /// When true, the Box passes its min constraints to children instead of removing them.
    pub propagate_min: bool,
    /// When true, this node and its children form a focus group: focus cycles within
    /// the group before moving outside it.
    pub focus_group: bool,
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    /// Scroll modifier binding. When set, the layout engine treats this view as
    /// a scroll container, applying clipping and offset to children.
    ///
    /// Use `Modifier::vertical_scroll()`, `Modifier::horizontal_scroll()`, or
    /// `Modifier::scrollable()` to set this.
    pub scroll: Option<crate::scroll::ScrollBinding>,
    /// Nested scroll connection for coordinated scrolling between this element
    /// and its scrollable descendants.
    ///
    /// When set on an ancestor of a scroll container (e.g. `ScrollArea`,
    /// `LazyColumn`), the scroll container automatically discovers this
    /// connection and dispatches pre/post scroll events to it during layout.
    ///
    /// Mirrors Compose's `Modifier.nestedScroll(NestedScrollConnection)`.
    pub nested_scroll_connection: Option<crate::nested_scroll::NestedScrollConnection>,
    pub on_pointer_down: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_cancel: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(PointerEvent)>>,
    /// Called when the element is clicked (pointer down then up within bounds).
    pub on_click: Option<Rc<dyn Fn()>>,
    /// Called when the element is double-clicked/tapped.
    pub on_double_click: Option<Rc<dyn Fn()>>,
    /// Called when the element is long-pressed.
    pub on_long_click: Option<Rc<dyn Fn()>>,
    /// Called when the element's global position changes after layout.
    /// Provides the element's rect in window coordinates.
    pub on_globally_positioned: Option<Rc<dyn Fn(crate::Rect)>>,
    /// Called when the element's size changes after layout.
    /// Provides the new size.
    pub on_size_changed: Option<Rc<dyn Fn(crate::Vec2)>>,
    /// Called when a key event is received while this element is focused.
    /// Return `true` to consume the event. This is the normal handler.
    pub on_key_event: Option<Rc<dyn Fn(crate::input::KeyEvent) -> bool>>,
    /// Called before `on_key_event` -> if the preview handler returns `true`,
    /// the event is consumed and `on_key_event` is NOT called.
    pub on_preview_key_event: Option<Rc<dyn Fn(crate::input::KeyEvent) -> bool>>,
    /// Apply a gaussian blur to this element's rendered content.
    /// When set, `graphics_layer` is auto-enabled if not already set.
    /// Use `Modifier::blur(radius)` for uniform blur, or
    /// `Modifier::blur_with_edge(rx, ry, edge)` for per-axis control.
    pub blur: Option<BlurStyle>,
    /// Custom layout callback. When set, the element's measurement is delegated
    /// to this function instead of the default Taffy-based layout.
    /// The callback receives `LayoutConstraints` (min/max width/height in dp).
    /// Returns `(width, height)` for this element.
    pub layout: Option<Rc<dyn Fn(LayoutConstraints) -> (f32, f32)>>,
    pub semantics: Option<crate::Semantics>,
    pub alpha: Option<f32>,
    pub graphics_layer: Option<f32>,
    pub shadow: Option<ShadowSpec>,
    pub transform: Option<Transform>,
    pub grid: Option<GridConfig>,
    pub grid_col_span: Option<u16>,
    pub grid_row_span: Option<u16>,
    pub position_type: Option<PositionType>,
    pub offset_left: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_top: Option<f32>,
    pub offset_bottom: Option<f32>,

    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub aspect_ratio: Option<f32>,
    /// Size this node's width to its min or max intrinsic content size.
    pub intrinsic_width: Option<IntrinsicSize>,
    /// Size this node's height to its min or max intrinsic content size.
    pub intrinsic_height: Option<IntrinsicSize>,
    pub painter: Option<Rc<dyn Fn(&mut crate::Scene, crate::Rect, f32)>>,

    // Drag-drop (internal)
    pub on_drag_start: Option<Rc<dyn Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload>>>,
    pub on_drag_end: Option<Rc<dyn Fn(crate::dnd::DragEnd)>>,
    pub on_drag_enter: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_over: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_leave: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drop: Option<Rc<dyn Fn(crate::dnd::DropEvent) -> bool>>,
    /// Compose-like `drawDragDecoration`: paints the floating preview while dragging.
    pub drag_preview: Option<crate::dnd::DragPreview>,

    pub on_action: Option<Rc<dyn Fn(crate::shortcuts::Action) -> bool>>,

    /// Cursor icon hint for desktop/web runners.
    pub cursor: Option<crate::CursorIcon>,

    /// If set, the size of this node will smoothly animate to its target size
    /// whenever content size changes. Uses the provided animation spec.
    pub animate_content_size: Option<AnimationSpec>,

    /// A `FocusRequester` handle that will be associated with this view.
    /// When the requester's `request_focus()` is called, keyboard focus will
    /// move to this view.
    pub focus_requester: Option<crate::runtime::FocusRequester>,

    /// Called when this view gains or loses focus. The boolean parameter is
    /// `true` when focused, `false` when unfocused.
    pub on_focus_changed: Option<Rc<dyn Fn(bool)>>,

    /// If set, this view reads its interaction state (hover/press) from this source
    /// in addition to the implicit view-ID-based matching. The source state is OR'd
    /// with the implicit state, enabling programmatic override of hover/press visuals.
    ///
    /// When set, the layout engine also auto-wires the source to emit PointerDown/Up
    /// and HoverEnter/Leave events into the source via the hit region's callbacks.
    pub interaction_source: Option<InteractionSource>,

    /// Text input configuration. When set, this box acts as a text input field.
    pub text_input: Option<TextInputConfig>,

    /// Indication (ripple/overlay) factory for visual feedback on interaction.
    pub indication: Option<Rc<dyn IndicationNodeFactory>>,
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Modifier");

        macro_rules! opt_val {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name.is_some() { s.field(stringify!($name), &self.$name); } )+
            };
        }
        if self.indication.is_some() {
            s.field("indication", &"…");
        }

        opt_val!(
            key,
            size,
            width,
            height,
            required_size,
            padding,
            padding_values,
            min_width,
            min_height,
            max_width,
            max_height,
            required_min_width,
            required_max_width,
            required_min_height,
            required_max_height,
            default_min_width,
            default_min_height,
            fill_max,
            fill_max_w,
            fill_max_h,
            background,
            state_colors,
            state_elevation,
            border,
            flex_grow,
            flex_shrink,
            flex_basis,
            flex_wrap,
            flex_dir,
            gap,
            row_gap,
            column_gap,
            align_self,
            justify_content,
            align_items_container,
            align_content,
            clip_rounded,
            clip_rect,
            render_z_index,
            semantics,
            alpha,
            transform,
            grid,
            grid_col_span,
            grid_row_span,
            position_type,
            offset_left,
            offset_right,
            offset_top,
            offset_bottom,
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
            aspect_ratio,
            intrinsic_width,
            intrinsic_height,
            cursor,
            animate_content_size,
            blur,
        );

        macro_rules! opt_cb {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name.is_some() { s.field(stringify!($name), &"…"); } )+
            };
        }
        opt_cb!(
            on_scroll,
            scroll,
            nested_scroll_connection,
            on_pointer_down,
            on_pointer_move,
            on_pointer_up,
            on_pointer_cancel,
            on_pointer_enter,
            on_pointer_leave,
            on_click,
            on_double_click,
            on_long_click,
            on_globally_positioned,
            on_size_changed,
            on_key_event,
            on_preview_key_event,
            painter,
            on_drag_start,
            on_drag_end,
            on_drag_enter,
            on_drag_over,
            on_drag_leave,
            on_drop,
            drag_preview,
            on_action,
            on_focus_changed,
            interaction_source,
            text_input,
            layout,
        );

        macro_rules! flag {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name { s.field(stringify!($name), &true); } )+
            };
        }
        flag!(
            hit_passthrough,
            input_blocker,
            repaint_boundary,
            click,
            disabled,
            propagate_min,
            focus_group,
        );

        if let Some(f) = self.focusable {
            s.field("focusable", &f);
        }
        if self.z_index != 0.0 {
            s.field("z_index", &self.z_index);
        }

        s.finish()
    }
}

impl_option_fields!(Modifier);

/// Content alignment for a container, applied as the flexbox cross-axis
/// (`align_items`) and main-axis (`justify_content`) pair. Mirrors Compose's
/// `Alignment` for `Box(contentAlignment = ...)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Alignment {
    TopStart,
    TopCenter,
    TopEnd,
    CenterStart,
    #[default]
    Center,
    CenterEnd,
    BottomStart,
    BottomCenter,
    BottomEnd,
}

impl Alignment {
    /// The corresponding (`align_items`, `justify_content`) pair for flexbox layout.
    pub fn to_flex(self) -> (AlignItems, JustifyContent) {
        use AlignItems as AI;
        use JustifyContent as JC;
        match self {
            Self::TopStart => (AI::START, JC::START),
            Self::TopCenter => (AI::START, JC::CENTER),
            Self::TopEnd => (AI::START, JC::END),
            Self::CenterStart => (AI::CENTER, JC::START),
            Self::Center => (AI::CENTER, JC::CENTER),
            Self::CenterEnd => (AI::CENTER, JC::END),
            Self::BottomStart => (AI::END, JC::START),
            Self::BottomCenter => (AI::END, JC::CENTER),
            Self::BottomEnd => (AI::END, JC::END),
        }
    }
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches a stable identity key to this view node.
    /// Use for dynamic lists / conditional UI where index-based identity can shift.
    pub fn key(mut self, key: u64) -> Self {
        self.key = Some(key);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some(Size {
            width: w,
            height: h,
        });
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
    /// Set a fixed size that overrides parent constraints.
    /// Unlike `size()` which is bounded by the parent's max constraints,
    /// `required_size()` forces the node to this size regardless of the parent,
    /// acting as both min and max.
    pub fn required_size(mut self, w: f32, h: f32) -> Self {
        self.required_size = Some(Size {
            width: w,
            height: h,
        });
        self
    }
    pub fn required_width_in(mut self, min: f32, max: f32) -> Self {
        self.required_min_width = Some(min.max(0.0));
        self.required_max_width = Some(max.max(0.0));
        self
    }
    pub fn required_height_in(mut self, min: f32, max: f32) -> Self {
        self.required_min_height = Some(min.max(0.0));
        self.required_max_height = Some(max.max(0.0));
        self
    }
    pub fn required_min_width(mut self, w: f32) -> Self {
        self.required_min_width = Some(w.max(0.0));
        self
    }
    pub fn required_max_width(mut self, w: f32) -> Self {
        self.required_max_width = Some(w.max(0.0));
        self
    }
    pub fn required_min_height(mut self, h: f32) -> Self {
        self.required_min_height = Some(h.max(0.0));
        self
    }
    pub fn required_max_height(mut self, h: f32) -> Self {
        self.required_max_height = Some(h.max(0.0));
        self
    }
    /// Minimum size that only takes effect when the incoming constraint is 0 (unconstrained).
    pub fn default_min_size(mut self, w: f32, h: f32) -> Self {
        self.default_min_width = Some(w.max(0.0));
        self.default_min_height = Some(h.max(0.0));
        self
    }
    /// Fill the available space in both dimensions.
    /// By default fills 100% (fraction = 1.0). Pass a fraction to fill partially.
    pub fn fill_max_size(mut self) -> Self {
        self.fill_max = Some(1.0);
        self
    }
    pub fn fill_max_size_frac(mut self, fraction: f32) -> Self {
        self.fill_max = Some(fraction.clamp(0.0, 1.0));
        self
    }
    /// Fill the available width. By default fills 100%.
    pub fn fill_max_width(mut self) -> Self {
        self.fill_max_w = Some(1.0);
        self
    }
    pub fn fill_max_width_frac(mut self, fraction: f32) -> Self {
        self.fill_max_w = Some(fraction.clamp(0.0, 1.0));
        self
    }
    /// Fill the available height. By default fills 100%.
    pub fn fill_max_height(mut self) -> Self {
        self.fill_max_h = Some(1.0);
        self
    }
    pub fn fill_max_height_frac(mut self, fraction: f32) -> Self {
        self.fill_max_h = Some(fraction.clamp(0.0, 1.0));
        self
    }
    pub fn padding(mut self, v: f32) -> Self {
        self.padding = Some(v);
        self
    }
    pub fn padding_values(mut self, padding: PaddingValues) -> Self {
        self.padding_values = Some(padding);
        self
    }
    /// Add padding equal to the current IME (soft keyboard) bottom inset.
    /// Combine with `system_bars_padding()` to handle both system bars and keyboard.
    pub fn ime_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let scale = crate::locals::effective_density_scale();
        let mut p = self.padding_values.unwrap_or_default();
        p.bottom += insets.ime_bottom / scale;
        self.padding_values = Some(p);
        self
    }
    /// Add padding equal to the current system bar insets (status bar top, nav bar bottom).
    pub fn system_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let scale = crate::locals::effective_density_scale();
        let mut p = self.padding_values.unwrap_or_default();
        p.top += insets.top / scale;
        p.bottom += insets.bottom / scale;
        self.padding_values = Some(p);
        self
    }
    /// Add status bar inset as top padding.
    pub fn status_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let scale = crate::locals::effective_density_scale();
        let mut p = self.padding_values.unwrap_or_default();
        p.top += insets.top / scale;
        self.padding_values = Some(p);
        self
    }
    /// Add navigation bar inset as bottom padding.
    pub fn navigation_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let scale = crate::locals::effective_density_scale();
        let mut p = self.padding_values.unwrap_or_default();
        p.bottom += insets.bottom / scale;
        self.padding_values = Some(p);
        self
    }
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_width = Some(w);
        self.min_height = Some(h);
        self
    }
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_width = Some(w);
        self.max_height = Some(h);
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w);
        self
    }
    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = Some(h);
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }
    /// Set a solid color background.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }
    /// Set a brush (solid, gradient, etc.) background.
    pub fn background_brush(mut self, brush: Brush) -> Self {
        self.background = Some(brush);
        self
    }
    pub fn border(mut self, width: f32, color: Color, radius: f32) -> Self {
        self.border = Some(Border {
            width,
            color,
            radius: [radius; 4],
        });
        self
    }
    pub fn border_radii(mut self, width: f32, color: Color, radii: [f32; 4]) -> Self {
        self.border = Some(Border {
            width,
            color,
            radius: radii,
        });
        self
    }
    pub fn flex_grow(mut self, v: f32) -> Self {
        self.flex_grow = Some(v);
        self
    }
    pub fn flex_shrink(mut self, v: f32) -> Self {
        self.flex_shrink = Some(v);
        self
    }
    pub fn flex_basis(mut self, v: f32) -> Self {
        self.flex_basis = Some(v);
        self
    }
    pub fn flex_wrap(mut self, w: FlexWrap) -> Self {
        self.flex_wrap = Some(w);
        self
    }
    pub fn flex_dir(mut self, d: FlexDirection) -> Self {
        self.flex_dir = Some(d);
        self
    }
    pub fn gap(mut self, v: f32) -> Self {
        let v = v.max(0.0);
        self.gap = Some(v);
        self.row_gap = Some(v);
        self.column_gap = Some(v);
        self
    }
    pub fn row_gap(mut self, v: f32) -> Self {
        self.row_gap = Some(v.max(0.0));
        self
    }
    pub fn column_gap(mut self, v: f32) -> Self {
        self.column_gap = Some(v.max(0.0));
        self
    }
    pub fn align_self(mut self, a: AlignSelf) -> Self {
        self.align_self = Some(a);
        self
    }
    pub fn align_self_center(mut self) -> Self {
        self.align_self = Some(AlignSelf::CENTER);
        self
    }
    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.justify_content = Some(j);
        self
    }
    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.align_items_container = Some(a);
        self
    }
    /// Compose-like content alignment (sets both `align_items` and
    /// `justify_content` in one call).
    pub fn content_alignment(self, alignment: Alignment) -> Self {
        let (ai, jc) = alignment.to_flex();
        self.align_items(ai).justify_content(jc)
    }
    pub fn align_content(mut self, a: AlignContent) -> Self {
        self.align_content = Some(a);
        self
    }
    pub fn clip_rounded(mut self, radius: f32) -> Self {
        self.clip_rounded = Some([radius; 4]);
        self
    }
    pub fn clip_rounded_radii(mut self, radii: [f32; 4]) -> Self {
        self.clip_rounded = Some(radii);
        self
    }
    /// Clip a rectangular region from this element using the given operation.
    /// `left`, `top`, `right`, `bottom` are relative to the element bounds, in dp.
    pub fn clip_rect(mut self, left: f32, top: f32, right: f32, bottom: f32, op: ClipOp) -> Self {
        self.clip_rect = Some(ClipRect {
            left,
            top,
            right,
            bottom,
            op,
        });
        self
    }
    pub fn overflow(mut self, overflow: Overflow) -> Self {
        self.overflow = Some(overflow);
        self
    }
    pub fn z_index(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    /// Sets the render z-index for this view. Higher values are painted on top.
    /// Unlike `z_index` (which only affects hit-testing), this affects visual layering.
    pub fn render_z_index(mut self, z: f32) -> Self {
        self.render_z_index = Some(z);
        self
    }

    /// Prevent pointer/touch from reaching lower layers.
    pub fn input_blocker(mut self) -> Self {
        self.input_blocker = true;
        self
    }

    pub fn hit_passthrough(mut self) -> Self {
        self.hit_passthrough = true;
        self
    }
    pub fn clickable(mut self) -> Self {
        self.click = true;
        if self.indication.is_none() {
            self.indication = crate::locals::local_indication();
        }
        self
    }
    /// Make this element clickable and attach an [`InteractionSource`] for state tracking.
    /// Combines `.clickable()` and `.interaction_source(&source)` in one call.
    pub fn clickable_with_source(mut self, source: &MutableInteractionSource) -> Self {
        self.click = true;
        self.interaction_source = Some(source.source());
        if self.indication.is_none() {
            self.indication = crate::locals::local_indication();
        }
        self
    }
    /// Set state-driven background colors for hover, press, disabled states.
    /// The layout engine automatically selects and animates between these based on interaction.
    pub fn state_colors(mut self, colors: StateColors) -> Self {
        self.state_colors = Some(colors);
        self
    }
    /// Set state-driven elevation values for hover, press, disabled states.
    pub fn state_elevation(mut self, elev: StateElevation) -> Self {
        self.state_elevation = Some(elev);
        self
    }
    /// Mark this component as disabled - it won't respond to pointer events.
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    /// Mark this component as enabled or disabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }
    /// Set explicit focusability for this component.
    /// When `true`, the component can receive keyboard focus even without
    /// explicit click/pointer/dnd handlers. When `false`, focus is suppressed
    /// even for interactive components.
    pub fn focusable(mut self, focusable: bool) -> Self {
        self.focusable = Some(focusable);
        self
    }
    /// Mark this node as a focus group: focus cycles within this group before
    /// moving to siblings outside it.
    pub fn focus_group(mut self) -> Self {
        self.focus_group = true;
        self
    }
    /// Attach an [`InteractionSource`] to this view. The source provides shared
    /// interaction state (hover/press/focus/drag) that supplements the implicit
    /// view-ID-based state. The layout engine auto-wires pointer events
    /// (press/hover), keyboard activation (Space/Enter press parity), focus
    /// transitions (Focus/Unfocus) and DnD drag start/end into the source so it
    /// stays in sync with user interaction.
    ///
    /// Use this when you need programmatic control of interaction state (e.g.,
    /// showing pressed state during an async operation) or to share interaction
    /// state between components.
    pub fn interaction_source(mut self, source: &MutableInteractionSource) -> Self {
        self.interaction_source = Some(source.source());
        self
    }
    /// Convenience: register hover enter/leave callbacks.
    /// Shorthand for setting `on_pointer_enter` and `on_pointer_leave`.
    pub fn hoverable(
        mut self,
        on_enter: impl Fn() + 'static,
        on_leave: impl Fn() + 'static,
    ) -> Self {
        self.on_pointer_enter = Some(Rc::new(move |_| on_enter()));
        self.on_pointer_leave = Some(Rc::new(move |_| on_leave()));
        self
    }
    /// Attach an [`InteractionSource`] to track hover state without explicit callbacks.
    /// The source automatically receives HoverEnter/Leave events from the layout engine's
    /// auto-wiring, so you can use it with `collect_is_hovered()` for custom visuals.
    pub fn hoverable_with_source(mut self, source: &MutableInteractionSource) -> Self {
        self.interaction_source = Some(source.source());
        self
    }
    /// When true, Box passes min-width/min-height constraints to its children
    /// instead of allowing them to shrink below the parent's min constraints.
    pub fn propagate_min_constraints(mut self, propagate: bool) -> Self {
        self.propagate_min = propagate;
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_scroll = Some(Rc::new(f));
        self
    }
    /// Attach a vertical scroll binding to this modifier.
    /// The binding provides callbacks for scroll handling, viewport tracking, etc.
    /// Use `ScrollState::to_binding()` to create one from a scroll state.
    pub fn vertical_scroll(mut self, binding: crate::scroll::ScrollAxisBinding) -> Self {
        self.scroll = Some(crate::scroll::ScrollBinding::Vertical(binding));
        self
    }
    /// Attach a horizontal scroll binding to this modifier.
    pub fn horizontal_scroll(mut self, binding: crate::scroll::ScrollAxisBinding) -> Self {
        self.scroll = Some(crate::scroll::ScrollBinding::Horizontal(binding));
        self
    }
    /// Attach a 2D scroll binding to this modifier.
    pub fn scrollable(mut self, binding: crate::scroll::ScrollBothBinding) -> Self {
        self.scroll = Some(crate::scroll::ScrollBinding::Both(binding));
        self
    }
    /// Attach a nested scroll connection that descendant scrollable containers
    /// will discover during layout. Mirrors Compose's `Modifier.nestedScroll`.
    ///
    /// The connection receives pre/post scroll and pre/post fling callbacks
    /// when a scrollable child dispatches events, enabling coordinated scrolling
    /// patterns like collapsing toolbars and pull-to-refresh.
    pub fn nested_scroll(mut self, conn: crate::nested_scroll::NestedScrollConnection) -> Self {
        self.nested_scroll_connection = Some(conn);
        self
    }
    pub fn on_pointer_down(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_down = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_move(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_move = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_up(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_up = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_cancel(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_cancel = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_enter(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_enter = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_leave(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_leave = Some(Rc::new(f));
        self
    }
    pub fn on_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self.click = true;
        if self.semantics.is_none() {
            self.semantics = Some(crate::Semantics::new(crate::Role::Button));
        }
        self
    }
    pub fn on_double_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_double_click = Some(Rc::new(f));
        self.click = true;
        self
    }
    pub fn on_long_click(mut self, f: impl Fn() + 'static) -> Self {
        self.on_long_click = Some(Rc::new(f));
        self.click = true;
        self
    }
    pub fn clickable_ext(
        mut self,
        enabled: bool,
        on_click_label: Option<String>,
        role: Option<crate::semantics::Role>,
        on_click: impl Fn() + 'static,
    ) -> Self {
        if !enabled {
            let mut s = self.semantics.clone().unwrap_or_else(|| {
                crate::semantics::Semantics::new(role.unwrap_or(crate::semantics::Role::Button))
            });
            s.enabled = false;
            if let Some(r) = role {
                s.role = r;
            }
            if let Some(l) = on_click_label {
                s.label = Some(l);
            }
            return self
                .clickable()
                .enabled(false)
                .default_min_size(48.0, 48.0)
                .semantics(s);
        }
        self = self.clickable().on_click(on_click);
        if role.is_some() || on_click_label.is_some() {
            let mut s = self.semantics.clone().unwrap_or_else(|| {
                crate::semantics::Semantics::new(role.unwrap_or(crate::semantics::Role::Button))
            });
            s.enabled = true;
            if let Some(r) = role {
                s.role = r;
            }
            if let Some(l) = on_click_label {
                s.label = Some(l);
            }
            self = self.semantics(s);
        }
        self.default_min_size(48.0, 48.0)
    }
    pub fn combined_clickable(
        mut self,
        enabled: bool,
        on_click_label: Option<String>,
        role: Option<crate::semantics::Role>,
        on_long_click_label: Option<String>,
        on_click: impl Fn() + 'static,
        on_long_click: Option<impl Fn() + 'static>,
        on_double_click: Option<impl Fn() + 'static>,
    ) -> Self {
        let _ = on_long_click_label;
        if !enabled {
            return self.clickable_ext(false, on_click_label, role, || {});
        }
        self = self.clickable_ext(true, on_click_label, role, on_click);
        if let Some(f) = on_long_click {
            self = self.on_long_click(f);
        }
        if let Some(f) = on_double_click {
            self = self.on_double_click(f);
        }
        self.default_min_size(48.0, 48.0)
    }
    pub fn semantics(mut self, s: crate::Semantics) -> Self {
        self.semantics = Some(s);
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = Some(a);
        self
    }
    /// Render this subtree into an offscreen texture, then composite it
    /// back into the parent with the given group `alpha` (0.0..=1.0).
    /// Allows correct blending when children overlap inside the layer, and
    /// sets up the architecture for future layer effects (shadow, blur, clip).
    pub fn graphics_layer(mut self, alpha: f32) -> Self {
        self.graphics_layer = Some(alpha.clamp(0.0, 1.0));
        self
    }
    /// Drop shadow with the given `blur_radius` (dp) and vertical `offset_y` (dp).
    /// The shadow color defaults to black with alpha 64 (~25%). Combines with
    /// [`Modifier::graphics_layer`] to draw a shadow underneath the layer.
    pub fn shadow(mut self, blur_radius: f32, offset_y: f32) -> Self {
        self.shadow = Some(ShadowSpec {
            blur_radius: blur_radius.max(0.0),
            offset_y,
            color: Color(0, 0, 0, 64),
        });
        self
    }
    /// Drop shadow with a custom color. Alpha 0..=255.
    pub fn shadow_with_color(mut self, blur_radius: f32, offset_y: f32, color: Color) -> Self {
        self.shadow = Some(ShadowSpec {
            blur_radius: blur_radius.max(0.0),
            offset_y,
            color,
        });
        self
    }
    /// Material-style elevation. Auto-scales blur and offset by `level` (dp)
    /// and uses a default shadow color. Level 0 = no shadow. 4 = subtle;
    /// 16 = strong. Requires [`Modifier::graphics_layer`] to take effect.
    pub fn elevation(mut self, level: f32) -> Self {
        if level <= 0.0 {
            self.shadow = None;
            return self;
        }
        self.shadow = Some(ShadowSpec {
            blur_radius: level * 2.0,
            offset_y: level * 0.5,
            color: Color(0, 0, 0, (level * 8.0).clamp(8.0, 80.0) as u8),
        });
        self
    }
    pub fn transform(mut self, t: Transform) -> Self {
        self.transform = Some(t);
        self
    }
    pub fn grid(mut self, columns: usize, row_gap: f32, column_gap: f32) -> Self {
        self.grid = Some(GridConfig {
            columns,
            row_gap,
            column_gap,
        });
        self
    }
    pub fn grid_span(mut self, col_span: u16, row_span: u16) -> Self {
        self.grid_col_span = Some(col_span);
        self.grid_row_span = Some(row_span);
        self
    }
    pub fn absolute(mut self) -> Self {
        self.position_type = Some(PositionType::Absolute);
        self
    }
    pub fn offset(
        mut self,
        left: Option<f32>,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
    ) -> Self {
        self.offset_left = left;
        self.offset_top = top;
        self.offset_right = right;
        self.offset_bottom = bottom;
        self
    }
    pub fn offset_left(mut self, v: f32) -> Self {
        self.offset_left = Some(v);
        self
    }
    pub fn offset_right(mut self, v: f32) -> Self {
        self.offset_right = Some(v);
        self
    }
    pub fn offset_top(mut self, v: f32) -> Self {
        self.offset_top = Some(v);
        self
    }
    pub fn offset_bottom(mut self, v: f32) -> Self {
        self.offset_bottom = Some(v);
        self
    }
    pub fn margin(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }

    pub fn margin_horizontal(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self
    }

    pub fn margin_vertical(mut self, v: f32) -> Self {
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }
    /// Size this node's width to its min or max intrinsic content size.
    pub fn intrinsic_width(mut self, mode: IntrinsicSize) -> Self {
        self.intrinsic_width = Some(mode);
        self
    }
    /// Size this node's height to its min or max intrinsic content size.
    pub fn intrinsic_height(mut self, mode: IntrinsicSize) -> Self {
        self.intrinsic_height = Some(mode);
        self
    }
    pub fn painter(mut self, f: impl Fn(&mut crate::Scene, crate::Rect, f32) + 'static) -> Self {
        self.painter = Some(Rc::new(f));
        self
    }
    pub fn scale(self, s: f32) -> Self {
        self.scale2(s, s)
    }
    pub fn scale2(mut self, sx: f32, sy: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.scale_x *= sx;
        t.scale_y *= sy;
        self.transform = Some(t);
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        let t = self.transform.unwrap_or_else(Transform::identity);
        self.transform = Some(t.combine(&Transform::translate(x, y)));
        self
    }
    pub fn translate_vec2(self, v: Vec2) -> Self {
        self.translate(v.x, v.y)
    }
    pub fn rotate(mut self, radians: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.rotate += radians;
        self.transform = Some(t);
        self
    }
    pub fn transform_origin(mut self, x: f32, y: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.origin_x = x;
        t.origin_y = y;
        self.transform = Some(t);
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        let w = w.max(0.0);
        self.flex_grow = Some(w);
        self.flex_shrink = Some(1.0);
        // dp units; 0 is fine.
        self.flex_basis = Some(0.0);
        self
    }
    /// Marks this view as a repaint boundary candidate.
    ///
    /// The engine may cache its painted output.
    pub fn repaint_boundary(mut self) -> Self {
        self.repaint_boundary = true;
        self
    }
    pub fn on_action(mut self, f: impl Fn(crate::shortcuts::Action) -> bool + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Mark this node as a drag source. Return `Some(payload)` to start dragging.
    pub fn on_drag_start(
        mut self,
        f: impl Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload> + 'static,
    ) -> Self {
        self.on_drag_start = Some(Rc::new(f));
        self
    }

    /// Called when a drag ends (drop accepted or canceled/ignored).
    pub fn on_drag_end(mut self, f: impl Fn(crate::dnd::DragEnd) + 'static) -> Self {
        self.on_drag_end = Some(Rc::new(f));
        self
    }

    /// Called when a drag first enters this target.
    pub fn on_drag_enter(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_enter = Some(Rc::new(f));
        self
    }

    /// Called on every pointer move while a drag is over this target.
    pub fn on_drag_over(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_over = Some(Rc::new(f));
        self
    }

    /// Called when a drag leaves this target.
    pub fn on_drag_leave(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_leave = Some(Rc::new(f));
        self
    }

    /// Called on pointer release while a drag is over this target.
    /// Return `true` to accept the drop.
    pub fn on_drop(mut self, f: impl Fn(crate::dnd::DropEvent) -> bool + 'static) -> Self {
        self.on_drop = Some(Rc::new(f));
        self
    }

    /// Custom drag preview decoration (Compose `drawDragDecoration`).
    ///
    /// Called every frame while this node is the active drag source.
    /// Coordinates are screen px; see [`crate::dnd::DragPreviewCtx`].
    pub fn draw_drag_decoration(
        mut self,
        f: impl Fn(&mut crate::Scene, &crate::dnd::DragPreviewCtx) + 'static,
    ) -> Self {
        self.drag_preview = Some(Rc::new(f));
        self
    }

    /// Same as [`Self::draw_drag_decoration`] but takes an existing [`crate::dnd::DragPreview`] Rc.
    pub fn draw_drag_decoration_rc(mut self, preview: crate::dnd::DragPreview) -> Self {
        self.drag_preview = Some(preview);
        self
    }

    /// Convenience: floating label chip as the drag preview.
    pub fn drag_preview_label(self, label: impl Into<String>, accent: crate::Color) -> Self {
        self.draw_drag_decoration_rc(crate::dnd::drag_preview_label(label, accent))
    }

    /// Convenience: elevated chip preview.
    pub fn drag_preview_chip(self, label: impl Into<String>, accent: crate::Color) -> Self {
        self.draw_drag_decoration_rc(crate::dnd::drag_preview_chip(label, accent))
    }

    /// Set the cursor icon hint for desktop/web runners.
    pub fn cursor(mut self, c: crate::CursorIcon) -> Self {
        self.cursor = Some(c);
        self
    }

    /// Animate size changes smoothly when the content's natural size changes.
    /// Uses the provided `AnimationSpec` for the transition.
    /// The content will be clipped to the animated size during transitions.
    pub fn animate_content_size(mut self, spec: AnimationSpec) -> Self {
        self.animate_content_size = Some(spec);
        self
    }

    /// Attach a `FocusRequester` to this view. The requester will be associated
    /// with the view's focusable element, allowing programmatic focus requests.
    pub fn focus_requester(mut self, fr: crate::runtime::FocusRequester) -> Self {
        self.focus_requester = Some(fr);
        self
    }

    /// Make this composable a focus target (`.focusable(true)`).
    /// Corresponds to Compose's `Modifier.focusTarget()`.
    pub fn focus_target(mut self) -> Self {
        self.focusable = Some(true);
        self
    }

    /// Register a callback that fires when this view gains or loses keyboard focus.
    /// The argument is `true` when the view receives focus, `false` when it loses it.
    pub fn on_focus_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus_changed = Some(Rc::new(f));
        self
    }

    /// Called after layout when this element's position changes.
    /// The callback receives the element's rect in dp (device-independent pixels).
    /// Fires whenever the rect changes, including on the initial layout.
    pub fn on_globally_positioned(mut self, f: impl Fn(crate::Rect) + 'static) -> Self {
        self.on_globally_positioned = Some(Rc::new(f));
        self
    }

    /// Called after layout when this element's size changes.
    /// Provides the new (width, height) in dp.
    pub fn on_size_changed(mut self, f: impl Fn(crate::Vec2) + 'static) -> Self {
        self.on_size_changed = Some(Rc::new(f));
        self
    }

    /// Called when a key event is received while this element is focused.
    /// Return `true` to indicate the event was consumed and should not
    /// propagate further (e.g. to text input handling or shortcut dispatch).
    pub fn on_key_event(mut self, f: impl Fn(crate::input::KeyEvent) -> bool + 'static) -> Self {
        self.on_key_event = Some(Rc::new(f));
        self
    }

    /// Preview variant of `on_key_event`. Called before `on_key_event`;
    /// if the preview handler returns `true`, the event is consumed
    /// and `on_key_event` is NOT called.
    pub fn on_preview_key_event(
        mut self,
        f: impl Fn(crate::input::KeyEvent) -> bool + 'static,
    ) -> Self {
        self.on_preview_key_event = Some(Rc::new(f));
        self
    }

    /// Apply a gaussian blur to this element's rendered content.
    /// `radius_dp` is the uniform blur radius in device-independent pixels.
    /// Larger values produce a stronger blur.
    /// Uses `Rectangle` edge treatment (clip to bounds).
    ///
    /// Requires `graphics_layer` to be enabled (set automatically if not).
    pub fn blur(mut self, radius_dp: f32) -> Self {
        self.blur = Some(BlurStyle {
            radius_x: radius_dp.max(0.0),
            radius_y: radius_dp.max(0.0),
            edge_treatment: BlurredEdgeTreatment::Rectangle,
        });
        self
    }

    /// Apply a gaussian blur with separate horizontal/vertical radii.
    /// `edge_treatment` controls how edge pixels are handled.
    ///
    /// Requires `graphics_layer` to be enabled (set automatically if not).
    pub fn blur_with_edge(
        mut self,
        radius_x: f32,
        radius_y: f32,
        edge_treatment: BlurredEdgeTreatment,
    ) -> Self {
        self.blur = Some(BlurStyle {
            radius_x: radius_x.max(0.0),
            radius_y: radius_y.max(0.0),
            edge_treatment,
        });
        self
    }

    /// Override this element's measured size with a custom callback.
    /// The callback receives `LayoutConstraints` (min/max width/height in dp),
    /// where `max_width`/`max_height` may be `f32::INFINITY` if unbounded.
    /// Returns `(width, height)` for this element.
    ///
    /// Child placement is handled by the parent layout (same as Compose's
    /// `Modifier.size` family).
    pub fn layout(mut self, f: impl Fn(LayoutConstraints) -> (f32, f32) + 'static) -> Self {
        self.layout = Some(Rc::new(f));
        self
    }

    /// Mark this Box as a text input field with the given configuration.
    pub fn text_input(mut self, config: TextInputConfig) -> Self {
        self.text_input = Some(config);
        self
    }

    /// Attach an indication (ripple/highlight) factory for visual feedback.
    /// The factory is paired with an `InteractionSource` (via `.interaction_source(...)`)
    /// to draw press/hover/focus visual feedback.
    pub fn indication(mut self, factory: Rc<dyn IndicationNodeFactory>) -> Self {
        self.indication = Some(factory);
        self
    }
}
