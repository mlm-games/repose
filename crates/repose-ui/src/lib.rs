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
//! ```rust
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
//! ```rust
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

use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::{cell::RefCell, cmp::Ordering};

use repose_core::*;
use taffy::style::FlexDirection;
use taffy::{Overflow, Point};

pub mod textfield;
use crate::textfield::{TF_FONT_DP, TF_PADDING_X_DP, byte_to_char_index, measure_text};
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
    let mut v = View::new(0, ViewKind::Surface).modifier(modifier);
    v.children = vec![child];
    v
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

pub fn Button(content: impl IntoChildren, on_click: impl Fn() + 'static) -> View {
    View::new(
        0,
        ViewKind::Button {
            on_click: Some(Rc::new(on_click)),
        },
    )
    .with_children(content.into_children())
    .semantics(Semantics {
        role: Role::Button,
        label: None, // optional: we could derive from first Text child later
        focused: false,
        enabled: true,
    })
}

pub fn Slider(
    value: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::Slider {
            value,
            min: range.0,
            max: range.1,
            step,
            on_change: Some(Rc::new(on_change)),
        },
    )
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn RangeSlider(
    start: f32,
    end: f32,
    range: (f32, f32),
    step: Option<f32>,
    on_change: impl Fn(f32, f32) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::RangeSlider {
            start,
            end,
            min: range.0,
            max: range.1,
            step,
            on_change: Some(Rc::new(on_change)),
        },
    )
    .semantics(Semantics {
        role: Role::Slider,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn LinearProgress(value: Option<f32>) -> View {
    View::new(
        0,
        ViewKind::ProgressBar {
            value: value.unwrap_or(0.0),
            min: 0.0,
            max: 1.0,
            circular: false,
        },
    )
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

pub fn ProgressBar(value: f32, range: (f32, f32)) -> View {
    View::new(
        0,
        ViewKind::ProgressBar {
            value,
            min: range.0,
            max: range.1,
            circular: false,
        },
    )
    .semantics(Semantics {
        role: Role::ProgressBar,
        label: None,
        focused: false,
        enabled: true,
    })
}

/// A circular progress indicator (spinner).
///
/// If `value` is `None`, renders an indeterminate spinner (filled circle).
/// If `value` is `Some(f)`, renders a determinate circular progress with
/// the value clamped to `0.0..=1.0`.
pub fn CircularProgress(value: Option<f32>) -> View {
    View::new(
        0,
        ViewKind::ProgressBar {
            value: value.unwrap_or(0.0),
            min: 0.0,
            max: 1.0,
            circular: true,
        },
    )
    .modifier(Modifier::new().width(48.0).height(48.0))
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
        ViewKind::Column | ViewKind::Surface | ViewKind::ScrollV { .. } => {
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
