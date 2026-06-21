#![allow(non_snake_case)]
//! # Views, Modifiers, and Layout
//!
//! Repose UI is built around three core ideas:
//!
//! - `View`: an immutable description of a UI node.
//! - `Modifier`: layout, styling, and interaction hints attached to a `View`.
//! - Layout + paint: a separate pass (`layout_and_paint`) that turns the
//!   `View` tree into a `Scene` + hit regions using the Taffy layout engine.
//!
//! ## Views
//!
//! A `View` is a lightweight value that describes *what* to show, not *how* it is
//! rendered. It is cheap to create and you are expected to rebuild the entire
//! view tree on each frame:
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
//! - `id: ViewId` - assigned during composition/layout.
//! - `kind: ViewKind` - which widget it is (Text, Button, ScrollV, etc.).
//! - `modifier: Modifier` - layout/styling/interaction metadata.
//! - `children: Vec<View>` - owned child views.
//!
//! Views are *pure data*: they do not hold state or references into platform
//! APIs. State lives in signals / `remember_*` and platform integration happens
//! in the runner (`repose-platform`).
//!
//! ## Modifiers
//!
//! `Modifier` describes *how* a view participates in layout and hit‑testing:
//!
//! - Size hints: `size`, `width`, `height`, `min_size`, `max_size`,
//!   `fill_max_size`, `fill_max_width`, `fill_max_height`.
//! - Box model: `padding`, `padding_values`.
//! - Visuals: `background`, `background_brush`, `border`, `clip_rounded`, `alpha`, `transform`.
//! - Flex / grid: `flex_grow`, `flex_shrink`, `flex_basis`, `align_self`,
//!   `justify_content`, `align_items`, `grid`, `grid_span`.
//! - Positioning: `absolute()`, `offset(..)` for overlay / Stack / FABs.
//! - Interaction: `clickable()`, pointer callbacks, `on_scroll`, `semantics`.
//! - Custom paint: `painter` (used by `repose-canvas`).
//!
//! Example:
//!
//! ```rust
//! use repose_core::*;
//! use repose_ui::*;
//!
//! fn CardExample() -> View {
//!     Surface(
//!         Modifier::new()
//!             .padding(16.0)
//!             .background(theme().surface)
//!             .border(1.0, theme().outline, 8.0)
//!             .clip_rounded(8.0),
//!         Text("Hello, Repose!"),
//!     )
//! }
//! ```
//!
//! Modifiers are merged into a Taffy `Style` inside `layout_and_paint`. Most
//! values are specified in density‑independent pixels (dp) and converted to
//! physical pixels (`px`) using the current `Density` local.
//!
//! ## Layout
//!
//! Layout is a pure function:
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
//! It:
//!
//! 1. Clones the root `View` and assigns stable `ViewId`s.
//! 2. Builds a parallel Taffy tree and computes layout for the given window size.
//! 3. Walks the tree to:
//!    - Emit `SceneNode`s for visuals (rects, text, images, scrollbars, etc.).
//!    - Build `HitRegion`s for input routing (clicks, pointer events, scroll).
//!    - Build `SemNode`s for accessibility / semantics.
//!
//! `Row`, `Column`, `Stack`, `Grid`, `ScrollV` and `ScrollXY` are all special
//! `ViewKind`s that map into Taffy styles and additional paint/hit logic.
//!
//! Because layout + paint are separate from the platform runner, you can reuse
//! the same UI code on desktop, Android, and other platforms.

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
    LazyColumn, LazyColumnState, LazyGridState, LazyRow, LazyRowState, LazyVerticalGrid,
    LazyVerticalStaggeredGrid, LazyVerticalStaggeredGridState, SimpleList,
};
pub use subcompose::{
    BoxWithConstraints, SubcomposeLayout, box_with_constraints_with_key, subcompose_hash_key,
    subcompose_layout_with_slots, subcompose_with_key, subcompose_with_key_slots,
};
pub mod navigation;
pub mod overlay;
pub mod pager;
pub mod scroll;
pub mod windowing;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use taffy::style::FlexDirection;

pub mod textfield;
use repose_core::locals;
pub use selection::SelectableText;
pub use textfield::{
    KeyboardOptions, TextArea, TextAreaEx, TextField, TextFieldEx, TextFieldState,
};

thread_local! {
    static LAYOUT_ENGINE: RefCell<layout::LayoutEngine> =
        RefCell::new(layout::LayoutEngine::new());
}

#[derive(Default)]
pub struct Interactions {
    pub hover: Option<u64>,
    pub pressed: HashSet<u64>,
}

pub fn Surface(modifier: Modifier, child: View) -> View {
    Column(modifier).child(child)
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

/// A vertically-oriented flow layout that wraps children to new columns when
/// they exceed the available height. Equivalent to `Column` with `flex_wrap(Wrap)`.
pub fn FlowColumn(modifier: Modifier) -> View {
    Column(modifier.flex_wrap(FlexWrap::Wrap))
}

/// Align self-center shorthand.
pub fn Center(modifier: Modifier) -> View {
    Box(modifier.align_self(AlignSelf::Center))
}

pub fn Stack(modifier: Modifier) -> View {
    View::new(0, ViewKind::Stack).modifier(modifier)
}

pub fn ZStack(modifier: Modifier) -> View {
    View::new(0, ViewKind::ZStack).modifier(modifier)
}

pub fn OverlayHost(modifier: Modifier) -> View {
    View::new(0, ViewKind::OverlayHost).modifier(modifier)
}

#[deprecated = "Use ScollArea instead"]
pub fn Scroll(modifier: Modifier) -> View {
    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll: None,
            set_viewport_height: None,
            set_content_height: None,
            get_scroll_offset: None,
            set_scroll_offset: None,
            show_scrollbar: true,
            tick_scroll: None,
        },
    )
    .modifier(modifier)
}

pub fn Text(text: impl Into<String>) -> View {
    View::new(
        0,
        ViewKind::Text {
            text: text.into(),
            color: locals::content_color(),
            font_size: 16.0, // dp (converted to px in layout/paint)
            soft_wrap: true,
            max_lines: None,
            overflow: TextOverflow::Visible,
            font_family: None,
            annotations: None,
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
            font_size: 16.0,
            soft_wrap: true,
            max_lines: None,
            overflow: TextOverflow::Visible,
            font_family: None,
            annotations,
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
    let drag_start_x = remember_state_with_key(format!("dv_dsx_{}", id), || 0.0f32);
    let drag_start_val = remember_state_with_key(format!("dv_dsv_{}", id), || 0.0f32);
    let is_dragging = remember_state_with_key(format!("dv_drg_{}", id), || false);

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
                *drg.borrow_mut() = true;
                *dsx.borrow_mut() = pe.position.x;
                *dsv.borrow_mut() = cur;
            }
        })
        .on_pointer_move({
            let dsx = drag_start_x.clone();
            let dsv = drag_start_val.clone();
            let drg = is_dragging.clone();
            let oc = oc.clone();
            move |pe: PointerEvent| {
                if !*drg.borrow() {
                    return;
                }
                let dx = pe.position.x - *dsx.borrow();
                let new_val = (*dsv.borrow() + dx * speed).clamp(min, max);
                (oc)(new_val);
            }
        })
        .on_pointer_up({
            let drg = is_dragging.clone();
            move |_pe: PointerEvent| {
                *drg.borrow_mut() = false;
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

fn flex_dir_for(kind: &ViewKind) -> Option<FlexDirection> {
    match kind {
        ViewKind::Row => {
            if repose_core::locals::text_direction() == repose_core::locals::TextDirection::Rtl {
                Some(FlexDirection::RowReverse)
            } else {
                Some(FlexDirection::Row)
            }
        }
        ViewKind::Column | ViewKind::ScrollV { .. } => {
            Some(FlexDirection::Column)
        }
        _ => None,
    }
}

/// Extension trait for child building
pub trait ViewExt: Sized {
    fn child(self, children: impl IntoChildren) -> Self;
}

impl ViewExt for View {
    fn child(self, children: impl IntoChildren) -> Self {
        self.with_children(children.into_children())
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

/// Layout and paint with TextField state injection (Taffy 0.9 API)
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
}
