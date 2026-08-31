use crate::{
    BaselineShift, Brush, ClipOp, Color, DrawStyle, FontStyle, FontSynthesis, FontWeight, Modifier,
    Rect, TextAlign, TextDecoration, TextDirection, TextSpan, Transform, Vec2,
};
use std::{fmt::Formatter, rc::Rc, sync::Arc};

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
    /// ContentScale.Fit - default in Compose Image
    Contain,
    /// ContentScale.Crop
    Cover,
    /// ContentScale.FillWidth
    FitWidth,
    /// ContentScale.FillHeight
    FitHeight,
    /// ContentScale.FillBounds - stretch, ignore aspect
    FillBounds,
    /// ContentScale.Inside - like Contain but never upscales
    Inside,
    /// ContentScale.None - no scaling, top-left (alignment can offset later)
    None,
}

pub type Callback = Rc<dyn Fn()>;

#[derive(Clone)]
pub struct OverlayEntry {
    pub id: u64,
    pub view: Box<View>,
}

#[derive(Clone)]
#[non_exhaustive]
pub enum ViewKind {
    Box,
    Row,
    Column,
    ZStack,
    OverlayHost,
    Text {
        text: String,
        color: Color,
        font_size: f32,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
        font_family: Option<&'static str>,
        annotations: Option<Arc<[TextSpan]>>,
        text_align: TextAlign,
        font_weight: FontWeight,
        font_style: FontStyle,
        text_decoration: TextDecoration,
        letter_spacing: f32,
        line_height: f32,
        /// URL for clickable link text.
        url: Option<Arc<str>>,
        /// OpenType font variation settings (e.g. "wght 700, opsz 24").
        font_variation_settings: Option<Arc<str>>,
    },

    Image {
        handle: ImageHandle,
        tint: Color, // multiplicative (WHITE = no tint)
        fit: ImageFit,
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
    /// First child is the header content. Remaining children shown only when expanded.
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
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Box => f.write_str("Box"),
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
            Self::ZStack => f.write_str("ZStack"),
            Self::OverlayHost => f.write_str("OverlayHost"),

            Self::Image { .. } => f.write_str("Image"),
            Self::SubcomposeLayout { .. } => f.write_str("SubcomposeLayout"),
            Self::Text { text, .. } => write!(f, "Text({:?})", text),

            Self::Expander { expanded, .. } => {
                if *expanded {
                    write!(f, "Expander(expanded)")
                } else {
                    write!(f, "Expander(collapsed)")
                }
            }
            Self::TreeRow {
                depth,
                has_children,
                is_expanded,
                is_selected,
                ..
            } => {
                write!(
                    f,
                    "TreeRow(depth={}, children={}, expanded={}, selected={})",
                    depth, has_children, is_expanded, is_selected
                )
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
    /// Set by `scope!` macro to mark this as a scope boundary node.
    /// Carries the scope key (e.g., "title", "color_buttons") for per-scope
    /// TaffyTree isolation.
    pub scope_key: Option<String>,
}

impl View {
    pub fn new(id: ViewId, kind: ViewKind) -> Self {
        View {
            id,
            kind,
            modifier: Modifier::default(),
            children: vec![],
            semantics: None,
            scope_key: None,
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
    pub fn children(mut self, kids: impl Into<Vec<View>>) -> Self {
        self.children = kids.into();
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

/// Rarely-tweaked text style properties bundled for ergonomic Default.
#[derive(Clone, Debug, PartialEq)]
pub struct TextExtraStyle {
    pub text_direction: TextDirection,
    pub font_synthesis: FontSynthesis,
    pub baseline_shift: BaselineShift,
    pub draw_style: DrawStyle,
}

impl Default for TextExtraStyle {
    fn default() -> Self {
        Self {
            text_direction: TextDirection::Ltr,
            font_synthesis: FontSynthesis::Unspecified,
            baseline_shift: BaselineShift::Unspecified,
            draw_style: DrawStyle::Fill,
        }
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SceneNode {
    Rect {
        rect: Rect,
        brush: Brush,
        radius: [f32; 4],
    },
    Border {
        rect: Rect,
        color: Color,
        width: f32,
        radius: [f32; 4],
    },
    Text {
        rect: Rect,
        text: Arc<str>,
        color: Color,
        size: f32,
        font_family: Option<&'static str>,
        text_align: TextAlign,
        font_weight: FontWeight,
        font_style: FontStyle,
        text_decoration: TextDecoration,
        letter_spacing: f32,
        line_height: f32,
        /// Rarely-tweaked style properties, bundled for ergonomic Default.
        extra_style: TextExtraStyle,
        /// URL for clickable link text.
        url: Option<Arc<str>>,
        /// OpenType font variation settings (e.g. "wght 700, opsz 24").
        font_variation_settings: Option<Arc<str>>,
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
        radius: [f32; 4],
        op: ClipOp,
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
        radius: [f32; 4],
        elevation: f32,
        color: Color,
    },
    /// Mark the start of a graphics layer: the contained subtree is rendered
    /// into an offscreen texture and then composited back into the parent.
    /// `alpha` is the group-compositing alpha applied at composite time.
    /// `blur_radius_x` / `blur_radius_y` are the gaussian blur radii in pixels
    /// applied to the layer before compositing (0.0 = no blur on that axis).
    /// `rectangle_edge` true = clamp edge pixels (Rectangle). False = transparent out-of-bounds (Unbounded).
    BeginLayer {
        rect: Rect,
        layer_id: u32,
        alpha: f32,
        blur_radius_x: f32,
        blur_radius_y: f32,
        rectangle_edge: bool,
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
    /// Arc stroke
    Arc {
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        stroke_width: f32,
        color: Color,
        cap: StrokeCap,
    },
    /// Pre-tessellated vector mesh (fill or stroke geometry produced by the
    /// host, e.g. lyon tessellation). Vertices live in the mesh's own local
    /// space; `transform` is a 2x3 affine that maps local -> world pixels and
    /// is applied in the vertex shader. The current scene `PushTransform`
    /// stack is folded in on top of `transform`.
    VectorMesh {
        mesh: Arc<VectorMeshData>,
        transform: [f32; 6],
        paint: PaintDesc,
        /// Reserved for explicit clip assignment. Clipping is otherwise
        /// structural via `PushVectorClip`/`PopVectorClip`.
        clip: Option<u32>,
        blend: BlendMode,
    },
    /// Screen-space overlays (handles, rubber bands, playhead). Each mesh
    /// is positioned in final device pixels and ignores the world
    /// PushTransform stack. Emit outside the viewport's world transform.
    VectorOverlay {
        meshes: Arc<[VectorMeshData]>,
    },
    /// Start a vector clip: the mesh is rendered into the stencil buffer
    /// (increment) and subsequent content is masked to it. Mirrors the
    /// rect-based `PushClip` but for arbitrary tessellated masks.
    PushVectorClip {
        mesh: Arc<VectorMeshData>,
    },
    /// End a vector clip opened by `PushVectorClip`.
    PopVectorClip,
}

/// Shared vertex/index buffers for a tessellated vector mesh.
#[derive(Clone, Debug, Default)]
pub struct VectorMeshData {
    pub vertices: Arc<[VectorVertex]>,
    pub indices: Arc<[u32]>,
}

/// Pre-tessellated vertex: local position, premultiplied-linear color, and a
/// free-form uv channel (unused for solid fills, reserved for texture/gradient
/// sampling).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct VectorVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}

/// How a `VectorMesh` is painted.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum PaintDesc {
    /// Use per-vertex color.
    Solid,
    /// Two-stop linear gradient in the mesh's local space.
    Linear {
        start: Vec2,
        end: Vec2,
        start_color: Color,
        end_color: Color,
    },
}

/// Blend mode for a `VectorMesh`. Only `Alpha` (premultiplied alpha) is wired
/// into the renderer today. The remaining variants are reserved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(Default)]
pub enum BlendMode {
    /// Standard premultiplied alpha blending.
    #[default]
    Alpha,
    /// Additive (screen-space light). Not yet implemented.
    Add,
    /// Multiply. Not yet implemented.
    Multiply,
    /// Overlay. Not yet implemented.
    Overlay,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TextOverflow {
    Visible,
    Clip,
    Ellipsis,
}

/// Controls how line segments are joined in a stroked path.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum StrokeJoin {
    #[default]
    /// Sharp corner joins.
    Miter,
    /// Semi-circular joins.
    Round,
    /// Beveled (flat) joins.
    Bevel,
}

/// Controls how the endpoints of a stroked arc are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum StrokeCap {
    #[default]
    /// Flat ends at the exact arc endpoint. No extension.
    Butt,
    /// Semicircle with diameter equal to the stroke width, centered at the
    /// arc endpoint.
    Round,
    /// Flat-ended rectangle extending half the stroke width beyond the arc
    /// endpoint.
    Square,
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
            scope_key: None,
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
            scope_key: None,
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
