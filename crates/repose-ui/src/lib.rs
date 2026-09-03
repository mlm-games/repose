#![allow(non_snake_case)]
//! # Views, Modifiers, and Layout
//!
//! Repose UI is built around three core ideas:
//!
//! - `View`: an immutable description of a UI node (cheap to rebuild every frame).
//! - `Modifier`: layout, styling, and interaction hints attached to a `View`.
//! - Incremental layout + paint via a persistent engine:
//!   composition produces a new `View` tree each frame; `LayoutEngine`
//!   reconciles it into a persistent `ViewTree` (`repose-tree`) and runs
//!   incremental Taffy layout + paint (with scopes, dirty sets, and paint caches).
//!
//! ## Views
//!
//! A `View` is a lightweight value that describes *what* to show, not *how* it is
//! rendered. It is cheap to create; you rebuild the description each frame
//! (Compose-style). Identity and layout state live in the persistent tree, not
//! in the `View` values themselves.
//!
//! ```rust,ignore
//! use repose_core::*;
//! use repose_ui::*;
//!
//! fn Counter(count: i32, on_inc: impl Fn() + 'static) -> View {
//!     Column(Modifier::new().padding(16.0)).child((
//!         Text(format!("Count = {count}")),
//!         Button("Increment".into_children(), on_inc),
//!     ))
//! }
//! ```
//!
//! Internally, a `View` has:
//!
//! - `id: ViewId` - assigned during composition / layout.
//! - `kind: ViewKind` - which widget it is (Text, Button, etc.).
//! - `modifier: Modifier` - layout/styling/interaction metadata.
//! - `children: Vec<View>` - owned child views.
//!
//! Views are *pure data*: they do not hold state or platform handles.
//! State lives in signals / `remember_*`; platform integration is in
//! `repose-platform` / `repose-app`.
//!
//! ## Modifiers
//!
//! `Modifier` describes *how* a view participates in layout and hit-testing:
//!
//! - Size: `size`, `width`, `height`, `min_*`, `max_*`, `fill_max_*`
//! - Box model: `padding`, `padding_values`, margins
//! - Visuals: `background`, `border`, `clip_rounded`, `alpha`, `transform`, layers
//! - Flex / grid: `flex_*`, `align_*`, `justify_*`, `grid`, `grid_span`
//! - Positioning: `absolute()`, `offset(..)`
//! - Scroll: `vertical_scroll` / `horizontal_scroll` / `scrollable`, `nested_scroll_connection`
//! - Interaction: `clickable()`, pointer callbacks, `semantics`
//! - Custom paint: `painter` (used by `repose-canvas`)
//! - Incremental helpers: `key`, `repaint_boundary`, `scope!` (core)
//!
//! Modifiers are mapped to Taffy `Style` inside `LayoutEngine`. Values are in
//! density-independent pixels (dp) and converted to physical px via `Density`.
//!
//! ## Layout + paint
//!
//! Public entry:
//!
//! ```rust,ignore
//! pub fn layout_and_paint(
//!     root: &View,
//!     size_px: (u32, u32),
//!     textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
//!     interactions: &Interactions,
//!     focused: Option<u64>,
//! ) -> (Scene, Vec<HitRegion>, Vec<SemNode>);
//! ```
//!
//! This is a thin thread-local wrapper around `LayoutEngine::layout_frame`, which:
//!
//! 1. Reconciles `root` into the persistent `ViewTree` (stable `NodeId`s, content +
//!    subtree hashes, dirty set, generation GC).
//! 2. Syncs dual Taffy trees (root + per-`scope!` `ScopeLayoutTree`s).
//! 3. Computes layout (measure callbacks, constraint equality skip for scopes).
//! 4. Walks the tree to emit `SceneNode`s, `HitRegion`s, and `SemNode`s, with
//!    paint-cache hits on `repaint_boundary` / scopes, culling, nested scroll, etc.
//!
//! Prefer `scope!`, stable keys, and `repaint_boundary` on expensive subtrees so
//! the incremental engine can skip work.

pub mod adaptive;
pub mod anim;
pub mod anim_ext;
pub mod color_picker;
pub mod gestures;
pub mod layout;
pub use layout::IntrinsicSizeMode;
pub mod lazy;
pub mod selection;
pub mod subcompose;
pub use lazy::{
    LazyColumn, LazyHorizontalGrid, LazyRow, LazyVerticalGrid, LazyVerticalStaggeredGrid,
    SimpleList,
};
pub mod lazy_states;
pub use lazy_states::{
    ItemHeight, LazyColumnConfig, LazyColumnState, LazyGridConfig, LazyGridState, LazyRowConfig,
    LazyRowState, LazyVerticalStaggeredGridConfig, LazyVerticalStaggeredGridState,
};
pub use subcompose::{
    BoxWithConstraints, SubcomposeLayout, box_with_constraints_with_key, subcompose_hash_key,
    subcompose_layout_with_slots, subcompose_with_key, subcompose_with_key_slots,
};
pub mod overlay;
pub mod pager;
pub mod scroll;
pub mod window_v2;
pub mod windowing;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;

pub mod textfield;
use repose_core::locals;
pub use selection::{SelectableText, SelectableTextExt};
pub use textfield::{
    BasicSecureTextField, BasicTextField, KeyboardOptions, TextFieldConfig, TextFieldState,
};

thread_local! {
    static LAYOUT_ENGINE: RefCell<layout::LayoutEngine> =
        RefCell::new(layout::LayoutEngine::new());
}

#[derive(Default)]
pub struct Interactions {
    pub hover: Option<u64>,
    pub hover_ancestors: std::collections::HashSet<u64>,
    pub pressed: HashSet<u64>,
}

pub fn Box(modifier: Modifier) -> View {
    View::new(0, ViewKind::Box).modifier(modifier)
}

pub fn Row(modifier: Modifier) -> View {
    View::new(0, ViewKind::Row).modifier(modifier)
}

pub fn Column(modifier: Modifier) -> View {
    View::new(0, ViewKind::Column).modifier(modifier)
}

/// A horizontally-oriented flow layout that wraps children to new rows when
/// they exceed the available width. Equivalent to `Row` with `flex_wrap(Wrap)`.
pub fn FlowRow(modifier: Modifier) -> View {
    Row(modifier.flex_wrap(FlexWrap::Wrap))
}

/// Flipped container (identical to `Column`).
/// Deprecated: use `Column` directly.
#[deprecated = "Use Column instead (identical behavior)"]
pub fn Stack(modifier: Modifier) -> View {
    Column(modifier)
}

/// A vertically-oriented flow layout that wraps children to new columns when
/// they exceed the available height. Equivalent to `Column` with `flex_wrap(Wrap)`.
pub fn FlowColumn(modifier: Modifier) -> View {
    Column(modifier.flex_wrap(FlexWrap::Wrap))
}

/// Centers children both axes inside this Box.
/// (Compose `Box(contentAlignment = Alignment.Center)`.)
pub fn Center(modifier: Modifier) -> View {
    Box(modifier.content_alignment(Alignment::Center))
}

pub fn ZStack(modifier: Modifier) -> View {
    View::new(0, ViewKind::ZStack).modifier(modifier)
}

pub fn OverlayHost(modifier: Modifier) -> View {
    View::new(0, ViewKind::OverlayHost).modifier(modifier)
}

#[deprecated = "Use Modifier::vertical_scroll instead"]
pub fn Scroll(modifier: Modifier) -> View {
    View::new(0, ViewKind::Box).modifier(modifier.vertical_scroll(ScrollAxisBinding {
        show_scrollbar: true,
        ..Default::default()
    }))
}

pub fn Text(text: impl Into<String>) -> View {
    View::new(
        0,
        ViewKind::Text {
            text: text.into(),
            color: locals::content_color(),
            font_size: locals::text_size().unwrap_or(16.0), // dp (converted to px in layout/paint)
            soft_wrap: true,
            max_lines: None,
            overflow: TextOverflow::Clip,
            font_family: Some("sans-serif"),
            annotations: None,
            text_align: TextAlign::Start,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            text_decoration: TextDecoration::default(),
            letter_spacing: 0.0,
            line_height: 0.0,
            url: None,
            font_variation_settings: None,
        },
    )
}

/// Create a text view with rich text spans (AnnotatedString).
///
/// Each span can override color and font_size for a range of text.
pub fn AnnotatedText(annotated: AnnotatedString) -> View {
    let annotations: Option<std::sync::Arc<[TextSpan]>> = if annotated.spans.is_empty() {
        None
    } else {
        Some(annotated.spans.clone())
    };
    View::new(
        0,
        ViewKind::Text {
            text: annotated.text,
            color: locals::content_color(),
            font_size: locals::text_size().unwrap_or(16.0),
            soft_wrap: true,
            max_lines: None,
            overflow: TextOverflow::Clip,
            font_family: Some("sans-serif"),
            annotations,
            text_align: TextAlign::Start,
            font_weight: FontWeight::NORMAL,
            font_style: FontStyle::Normal,
            text_decoration: TextDecoration::default(),
            letter_spacing: 0.0,
            line_height: 0.0,
            url: None,
            font_variation_settings: None,
        },
    )
}

pub fn Spacer() -> View {
    Box(Modifier::new().flex_grow(1.0))
}

pub fn Space(modifier: Modifier) -> View {
    Box(modifier)
}

pub fn Grid(
    columns: usize,
    modifier: Modifier,
    children: Vec<View>,
    row_gap: f32,
    column_gap: f32,
) -> View {
    Column(modifier.grid(columns, row_gap, column_gap)).with_children(children)
}

pub fn Expander(modifier: Modifier, expanded: bool, on_toggle: impl Fn() + 'static) -> View {
    View::new(
        0,
        ViewKind::Expander {
            expanded,
            on_toggle: Some(Rc::new(on_toggle)),
        },
    )
    .modifier(modifier)
}

/// A single row in a tree view.
///
/// Renders with indentation based on `depth`, an expand/collapse arrow if
/// `has_children` is true, and a highlight background if `is_selected`.
/// The first child is the row's label/content.
pub fn TreeRow(
    modifier: Modifier,
    depth: usize,
    has_children: bool,
    is_expanded: bool,
    is_selected: bool,
    on_toggle: impl Fn() + 'static,
    on_select: impl Fn() + 'static,
) -> View {
    View::new(
        0,
        ViewKind::TreeRow {
            depth,
            has_children,
            is_expanded,
            is_selected,
            on_toggle: Some(Rc::new(on_toggle)),
            on_select: Some(Rc::new(on_select)),
        },
    )
    .modifier(modifier)
}

static DRAGVALUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A drag-to-change numeric value field (like egui's `DragValue`).
///
/// Click and drag left/right to change the value. Displays the current value
/// as centered text in a bordered box.
pub fn DragValue(
    value: f32,
    range: (f32, f32),
    speed: f32,
    on_change: impl Fn(f32) + 'static,
) -> View {
    let id = DRAGVALUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let drag_start_x = remember_mutable_with_key(format!("dv_dsx_{}", id), || 0.0f32);
    let drag_start_val = remember_mutable_with_key(format!("dv_dsv_{}", id), || 0.0f32);
    let is_dragging = remember_mutable_with_key(format!("dv_drg_{}", id), || false);

    let oc = Rc::new(on_change);
    let min = range.0;
    let max = range.1;
    let cur = value;

    let th = locals::theme();

    Box(Modifier::new()
        .min_width(48.0)
        .height(28.0)
        .background(th.surface_container)
        .border(1.0, th.outline, 4.0)
        .clip_rounded(4.0)
        .padding_values(PaddingValues {
            left: 4.0,
            right: 4.0,
            top: 0.0,
            bottom: 0.0,
        })
        .on_pointer_down({
            let dsx = drag_start_x.clone();
            let dsv = drag_start_val.clone();
            let drg = is_dragging.clone();
            move |pe: PointerEvent| {
                drg.set(true);
                dsx.set(pe.position_in_window().x);
                dsv.set(cur);
            }
        })
        .on_pointer_move({
            let dsx = drag_start_x.clone();
            let dsv = drag_start_val.clone();
            let drg = is_dragging.clone();
            let oc = oc.clone();
            move |pe: PointerEvent| {
                if !drg.with(|v| *v) {
                    return;
                }
                let start_x = dsx.with(|v| *v);
                let start_val = dsv.with(|v| *v);
                let new_val =
                    (start_val + (pe.position_in_window().x - start_x) * speed).clamp(min, max);
                (oc)(new_val);
            }
        })
        .on_pointer_up({
            let drg = is_dragging.clone();
            move |_pe: PointerEvent| {
                drg.set(false);
            }
        })
        .cursor(CursorIcon::EwResize))
    .child(
        Text(format_value(value))
            .size(13.0)
            .color(th.on_surface)
            .single_line()
            .overflow_ellipsize(),
    )
}

fn format_value(v: f32) -> String {
    if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else if (v * 10.0 - (v * 10.0).round()).abs() < 1e-6 {
        format!("{:.1}", v)
    } else {
        format!("{:.2}", v)
    }
}

pub fn Image(modifier: Modifier, handle: ImageHandle) -> View {
    View::new(
        0,
        ViewKind::Image {
            handle,
            tint: Color::WHITE,
            fit: ImageFit::Contain,
        },
    )
    .modifier(modifier)
}

/// Embedded GPU callback (like `egui::PaintCallback`).
pub fn Embedded(modifier: Modifier, payload: PaintCallbackPayload) -> View {
    let mut m = modifier.paint_callback(payload);
    let has_size = m.size.is_some()
        || m.width.is_some()
        || m.height.is_some()
        || m.fill_max.is_some()
        || m.fill_max_w.is_some()
        || m.fill_max_h.is_some();
    if !has_size {
        m = m.size(100.0, 100.0);
    }
    Box(m)
}

pub trait ImageExt {
    fn image_tint(self, c: Color) -> View;
    fn image_fit(self, fit: ImageFit) -> View;
}
impl ImageExt for View {
    fn image_tint(mut self, c: Color) -> View {
        if let ViewKind::Image { tint, .. } = &mut self.kind {
            *tint = c;
        }
        self
    }
    fn image_fit(mut self, fit: ImageFit) -> View {
        if let ViewKind::Image { fit: f, .. } = &mut self.kind {
            *f = fit;
        }
        self
    }
}

/// Extension trait for child building
pub trait ViewExt: Sized {
    fn child(self, children: impl IntoChildren) -> Self;
}

impl ViewExt for View {
    fn child(mut self, children: impl IntoChildren) -> Self {
        self.children.extend(children.into_children());
        self
    }
}

pub trait IntoChildren {
    fn into_children(self) -> Vec<View>;
}

impl IntoChildren for View {
    fn into_children(self) -> Vec<View> {
        vec![self]
    }
}

impl IntoChildren for Vec<View> {
    fn into_children(self) -> Vec<View> {
        self
    }
}

impl<const N: usize> IntoChildren for [View; N] {
    fn into_children(self) -> Vec<View> {
        self.into()
    }
}

// Tuple implementations
macro_rules! impl_into_children_tuple {
    ($($idx:tt $t:ident),+) => {
        impl<$($t: IntoChildren),+> IntoChildren for ($($t,)+) {
            fn into_children(self) -> Vec<View> {
                let mut v = Vec::new();
                $(v.extend(self.$idx.into_children());)+
                v
            }
        }
    };
}

impl_into_children_tuple!(0 A);
impl_into_children_tuple!(0 A, 1 B);
impl_into_children_tuple!(0 A, 1 B, 2 C);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G);
impl_into_children_tuple!(0 A, 1 B, 2 C, 3 D, 4 E, 5 F, 6 G, 7 H);

/// Reconcile `root` into the thread-local `LayoutEngine` and run incremental
/// layout + paint for this frame.
pub fn layout_and_paint(
    root: &View,
    size_px_u32: (u32, u32),
    textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
    interactions: &Interactions,
    focused: Option<u64>,
) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
    LAYOUT_ENGINE.with(|engine| {
        engine
            .borrow_mut()
            .layout_frame(root, size_px_u32, textfield_states, interactions, focused)
    })
}

/// Return the [`LayoutStats`] from the most recent `layout_and_paint` call on
/// this thread. Used by the inspector / HUD to report real layout+paint timing
/// and cache counters instead of a hardcoded estimate.
pub fn last_layout_stats() -> layout::LayoutStats {
    LAYOUT_ENGINE.with(|engine| engine.borrow().stats.clone())
}

pub use layout::LayoutStats;

/// Method styling
pub trait TextStyle {
    fn color(self, c: Color) -> View;
    fn size(self, px: f32) -> View;
    fn max_lines(self, n: usize) -> View;
    fn single_line(self) -> View;
    fn overflow_ellipsize(self) -> View;
    fn overflow_clip(self) -> View;
    fn overflow_visible(self) -> View;
    fn font_family(self, family: &'static str) -> View;
    fn text_align(self, align: TextAlign) -> View;
    fn font_weight(self, weight: FontWeight) -> View;
    fn font_style(self, style: FontStyle) -> View;
    fn text_decoration(self, decoration: TextDecoration) -> View;
    fn letter_spacing(self, spacing: f32) -> View;
    fn line_height(self, height: f32) -> View;
    fn url(self, url: impl Into<std::sync::Arc<str>>) -> View;
    fn font_variation_settings(self, settings: &str) -> View;
}
impl TextStyle for View {
    fn color(mut self, c: Color) -> View {
        if let ViewKind::Text {
            color: text_color, ..
        } = &mut self.kind
        {
            *text_color = c;
        }
        self
    }
    fn size(mut self, dp_font: f32) -> View {
        if let ViewKind::Text {
            font_size: text_size_dp,
            ..
        } = &mut self.kind
        {
            *text_size_dp = dp_font;
        }
        self
    }
    fn max_lines(mut self, n: usize) -> View {
        if let ViewKind::Text {
            max_lines,
            soft_wrap,
            ..
        } = &mut self.kind
        {
            *max_lines = Some(n);
            *soft_wrap = true;
        }
        self
    }
    fn single_line(mut self) -> View {
        if let ViewKind::Text {
            soft_wrap,
            max_lines,
            ..
        } = &mut self.kind
        {
            *soft_wrap = false;
            *max_lines = Some(1);
        }
        self
    }
    fn overflow_ellipsize(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Ellipsis;
        }
        self
    }
    fn overflow_clip(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Clip;
        }
        self
    }
    fn overflow_visible(mut self) -> View {
        if let ViewKind::Text { overflow, .. } = &mut self.kind {
            *overflow = TextOverflow::Visible;
        }
        self
    }
    fn font_family(mut self, family: &'static str) -> View {
        if let ViewKind::Text {
            font_family: ff, ..
        } = &mut self.kind
        {
            *ff = Some(family);
        }
        self
    }
    fn text_align(mut self, align: TextAlign) -> View {
        if let ViewKind::Text { text_align, .. } = &mut self.kind {
            *text_align = align;
        }
        self
    }
    fn font_weight(mut self, weight: FontWeight) -> View {
        if let ViewKind::Text { font_weight, .. } = &mut self.kind {
            *font_weight = weight;
        }
        self
    }
    fn font_style(mut self, style: FontStyle) -> View {
        if let ViewKind::Text { font_style, .. } = &mut self.kind {
            *font_style = style;
        }
        self
    }
    fn text_decoration(mut self, decoration: TextDecoration) -> View {
        if let ViewKind::Text {
            text_decoration, ..
        } = &mut self.kind
        {
            *text_decoration = decoration;
        }
        self
    }
    fn letter_spacing(mut self, spacing: f32) -> View {
        if let ViewKind::Text { letter_spacing, .. } = &mut self.kind {
            *letter_spacing = spacing;
        }
        self
    }
    fn line_height(mut self, height: f32) -> View {
        if let ViewKind::Text { line_height, .. } = &mut self.kind {
            *line_height = height;
        }
        self
    }
    fn url(mut self, url: impl Into<std::sync::Arc<str>>) -> View {
        if let ViewKind::Text {
            url: u,
            text_decoration,
            ..
        } = &mut self.kind
        {
            *u = Some(url.into());
            if !text_decoration.underline && !text_decoration.strikethrough {
                *text_decoration = TextDecoration::UNDERLINE;
            }
        }
        self
    }
    fn font_variation_settings(mut self, settings: &str) -> View {
        if let ViewKind::Text {
            font_variation_settings,
            ..
        } = &mut self.kind
        {
            *font_variation_settings = Some(settings.into());
        }
        self
    }
}
