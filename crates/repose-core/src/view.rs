use crate::{Brush, Color, Modifier, Rect, TextSpan, Transform};
use std::{rc::Rc, sync::Arc};

/// The constraints that will be passed to a subcomposed child. Values are in
/// device-independent pixels (dp), matching the units used by `Modifier`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubcomposeScope {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl SubcomposeScope {
    /// A scope with no constraints: unbounded in both dimensions. Use this as
    /// a default when the parent constraints are not yet known.
    pub const UNBOUNDED: Self = Self {
        min_width: 0.0,
        max_width: f32::INFINITY,
        min_height: 0.0,
        max_height: f32::INFINITY,
    };

    /// Construct a scope from raw min/max dp values.
    pub fn new(min_width: f32, max_width: f32, min_height: f32, max_height: f32) -> Self {
        Self {
            min_width,
            max_width,
            min_height,
            max_height,
        }
    }
}

/// Scope passed to [`BoxWithConstraints`](crate::prelude::BoxWithConstraints)
/// content. All values are in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxWithConstraintsScope {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl BoxWithConstraintsScope {
    /// `true` if the width is bounded by the parent (i.e. not infinite).
    pub fn has_bounded_width(&self) -> bool {
        self.max_width.is_finite()
    }

    /// `true` if the height is bounded by the parent (i.e. not infinite).
    pub fn has_bounded_height(&self) -> bool {
        self.max_height.is_finite()
    }
}

pub type ViewId = u64;

pub type ImageHandle = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageFit {
    Contain,
    Cover,
    FitWidth,
    FitHeight,
}

pub type Callback = Rc<dyn Fn()>;
pub type ScrollCallback = Rc<dyn Fn(crate::Vec2) -> crate::Vec2>;

#[derive(Clone)]
pub struct OverlayEntry {
    pub id: u64,
    pub view: Box<View>,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum ViewKind {
    Surface,
    Box,
    Row,
    Column,
    Stack,
    ZStack,
    OverlayHost,
    ScrollV {
        on_scroll: Option<ScrollCallback>,
        set_viewport_height: Option<Rc<dyn Fn(f32)>>,
        set_content_height: Option<Rc<dyn Fn(f32)>>,
        get_scroll_offset: Option<Rc<dyn Fn() -> f32>>,
        set_scroll_offset: Option<Rc<dyn Fn(f32)>>,
        show_scrollbar: bool,
        tick_scroll: Option<Rc<dyn Fn()>>,
    },
    ScrollXY {
        on_scroll: Option<ScrollCallback>,
        set_viewport_width: Option<Rc<dyn Fn(f32)>>,
        set_viewport_height: Option<Rc<dyn Fn(f32)>>,
        set_content_width: Option<Rc<dyn Fn(f32)>>,
        set_content_height: Option<Rc<dyn Fn(f32)>>,
        get_scroll_offset_xy: Option<Rc<dyn Fn() -> (f32, f32)>>,
        set_scroll_offset_xy: Option<Rc<dyn Fn(f32, f32)>>,
        show_scrollbar: bool,
        tick_scroll: Option<Rc<dyn Fn()>>,
    },
    Text {
        text: String,
        color: Color,
        font_size: f32,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
        font_family: Option<&'static str>,
        annotations: Option<Arc<[TextSpan]>>,
    },

    Image {
        handle: ImageHandle,
        tint: Color, // multiplicative (WHITE = no tint)
        fit: ImageFit,
    },
    Ellipse {
        rect: Rect,
        color: Color,
    },
    EllipseBorder {
        rect: Rect,
        color: Color,
        width: f32, // screen-space width (px)
    },
    /// A layout whose children are produced by calling `content` with the
    /// current `SubcomposeScope`. The closure is invoked during reconciliation
    /// and returns a list of `(slot_id, view)` pairs. Each slot id is a stable
    /// identity used to reconcile the returned view across frames. This is
    /// the building block for `BoxWithConstraints` and other
    /// constraints-driven layouts.
    ///
    /// Note: any `Modifier::key` set on a returned view is overwritten by its
    /// slot id so the slot's identity is stable across frames.
    SubcomposeLayout {
        content: Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>>,
    },
    /// A collapsible section with a clickable header.
    /// First child is the header content; remaining children shown only when expanded.
    Expander {
        expanded: bool,
        on_toggle: Option<Callback>,
    },
    /// A single row in a tree view with indentation and expand/select support.
    /// First child is rendered as the row label/content.
    TreeRow {
        depth: usize,
        has_children: bool,
        is_expanded: bool,
        is_selected: bool,
        on_toggle: Option<Callback>,
        on_select: Option<Callback>,
    },
}

impl std::fmt::Debug for ViewKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface => f.write_str("Surface"),
            Self::Box => f.write_str("Box"),
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
            Self::Stack => f.write_str("Stack"),
            Self::ZStack => f.write_str("ZStack"),
            Self::OverlayHost => f.write_str("OverlayHost"),
            Self::ScrollV { .. } => f.write_str("ScrollV"),
            Self::ScrollXY { .. } => f.write_str("ScrollXY"),
            Self::Image { .. } => f.write_str("Image"),
            Self::Ellipse { .. } => f.write_str("Ellipse"),
            Self::EllipseBorder { .. } => f.write_str("EllipseBorder"),
            Self::SubcomposeLayout { .. } => f.write_str("SubcomposeLayout"),
            Self::Text { text, .. } => write!(f, "Text({:?})", text),

            Self::Expander { expanded, .. } => {
                if *expanded { write!(f, "Expander(expanded)") } else { write!(f, "Expander(collapsed)") }
            }
            Self::TreeRow { depth, has_children, is_expanded, is_selected, .. } => {
                write!(f, "TreeRow(depth={}, children={}, expanded={}, selected={})",
                    depth, has_children, is_expanded, is_selected)
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct View {
    pub id: ViewId,
    pub kind: ViewKind,
    pub modifier: Modifier,
    pub children: Vec<View>,
    pub semantics: Option<crate::semantics::Semantics>,
}

impl View {
    pub fn new(id: ViewId, kind: ViewKind) -> Self {
        View {
            id,
            kind,
            modifier: Modifier::default(),
            children: vec![],
            semantics: None,
        }
    }
    pub fn modifier(mut self, m: Modifier) -> Self {
        self.modifier = m;
        self
    }
    /// Mark this view as disabled - ignores pointer events.
    pub fn disabled(mut self) -> Self {
        self.modifier.disabled = true;
        self
    }
    pub fn with_children(mut self, kids: Vec<View>) -> Self {
        self.children = kids;
        self
    }
    pub fn semantics(mut self, s: crate::semantics::Semantics) -> Self {
        self.semantics = Some(s);
        self
    }
}

/// Renderable scene
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub clear_color: Color,
    pub nodes: Vec<SceneNode>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SceneNode {
    Rect {
        rect: Rect,
        brush: Brush,
        radius: f32,
    },
    Border {
        rect: Rect,
        color: Color,
        width: f32,
        radius: f32,
    },
    Text {
        rect: Rect,
        text: Arc<str>,
        color: Color,
        size: f32,
        font_family: Option<&'static str>,
    },
    Ellipse {
        rect: Rect,
        brush: Brush,
    },
    EllipseBorder {
        rect: Rect,
        color: Color,
        width: f32, // screen-space width (px)
    },
    PushClip {
        rect: Rect,
        radius: f32,
    },
    PopClip,
    PushTransform {
        transform: Transform,
    },
    PopTransform,
    Image {
        rect: Rect,
        handle: ImageHandle,
        tint: Color,
        fit: ImageFit,
    },
    /// Shadow behind a rounded rect, typically driven by `StateElevation`.
    /// The `elevation` field controls offset and alpha.
    Shadow {
        rect: Rect,
        radius: f32,
        elevation: f32,
        color: Color,
    },
    /// Mark the start of a graphics layer: the contained subtree is rendered
    /// into an offscreen texture and then composited back into the parent.
    /// `alpha` is the group-compositing alpha applied at composite time.
    BeginLayer {
        rect: Rect,
        layer_id: u32,
        alpha: f32,
    },
    /// Closes the graphics layer opened by the matching `BeginLayer`.
    EndLayer {
        layer_id: u32,
    },
    /// Draws a blurred drop shadow underneath a previously-rendered layer.
    /// Emitted between `EndLayer` and the layer's `CompositeLayer`. The
    /// quad samples the layer's texture with a 3x3 Gaussian blur and an
    /// optional vertical offset.
    CompositeShadow {
        layer_id: u32,
        blur_px: f32,
        offset_px: (f32, f32),
        color: Color,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextOverflow {
    Visible,
    Clip,
    Ellipsis,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subcompose_scope_unbounded_has_infinite_max() {
        let s = SubcomposeScope::UNBOUNDED;
        assert!(!s.max_width.is_finite());
        assert!(!s.max_height.is_finite());
        assert_eq!(s.min_width, 0.0);
        assert_eq!(s.min_height, 0.0);
    }

    #[test]
    fn subcompose_scope_new_round_trips() {
        let s = SubcomposeScope::new(10.0, 200.0, 20.0, 300.0);
        assert_eq!(s.min_width, 10.0);
        assert_eq!(s.max_width, 200.0);
        assert_eq!(s.min_height, 20.0);
        assert_eq!(s.max_height, 300.0);
    }

    #[test]
    fn box_with_constraints_scope_bounded_predicates() {
        let bounded = BoxWithConstraintsScope {
            min_width: 0.0,
            max_width: 360.0,
            min_height: 0.0,
            max_height: 640.0,
        };
        assert!(bounded.has_bounded_width());
        assert!(bounded.has_bounded_height());

        let unbounded = BoxWithConstraintsScope {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        assert!(!unbounded.has_bounded_width());
        assert!(!unbounded.has_bounded_height());
    }

    #[test]
    fn view_kind_subcompose_layout_holds_closure() {
        let v: View = View {
            id: 0,
            kind: ViewKind::SubcomposeLayout {
                content: std::sync::Arc::new(|scope| {
                    let _ = scope.max_width;
                    vec![(0, View::new(0, ViewKind::Box))]
                }),
            },
            modifier: Modifier::default(),
            children: vec![],
            semantics: None,
        };
        match &v.kind {
            ViewKind::SubcomposeLayout { .. } => {}
            _ => panic!("expected SubcomposeLayout"),
        }
    }

    #[test]
    fn view_kind_subcompose_layout_supports_multiple_slots() {
        let v: View = View {
            id: 0,
            kind: ViewKind::SubcomposeLayout {
                content: std::sync::Arc::new(|_scope| {
                    vec![
                        (1, View::new(0, ViewKind::Box)),
                        (2, View::new(0, ViewKind::Box)),
                        (3, View::new(0, ViewKind::Box)),
                    ]
                }),
            },
            modifier: Modifier::default(),
            children: vec![],
            semantics: None,
        };
        if let ViewKind::SubcomposeLayout { content } = &v.kind {
            let slots = content(&SubcomposeScope::UNBOUNDED);
            assert_eq!(slots.len(), 3);
            assert_eq!(slots[0].0, 1);
            assert_eq!(slots[1].0, 2);
            assert_eq!(slots[2].0, 3);
        } else {
            panic!("expected SubcomposeLayout");
        }
    }
}
