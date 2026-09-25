use crate::units::{Dp, Px, Sp};
use crate::{
    BaselineShift, Brush, ClipOp, Color, DrawStyle, FontStyle, FontSynthesis, FontWeight, Modifier,
    Rect, TextAlign, TextDecoration, TextDirection, TextSpan, Transform, Vec2,
};
use std::{fmt::Formatter, sync::Arc};

/// The constraints that will be passed to a subcomposed child. Values are in
/// [`Dp`], matching the units used by `Modifier`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SubcomposeScope {
    pub min_width: Dp,
    pub max_width: Dp,
    pub min_height: Dp,
    pub max_height: Dp,
}

impl SubcomposeScope {
    /// A scope with no finite upper bounds. Values are density-independent
    /// pixels (`Dp`).
    pub const UNBOUNDED: Self = Self {
        min_width: Dp(0.0),
        max_width: Dp(f32::INFINITY),
        min_height: Dp(0.0),
        max_height: Dp(f32::INFINITY),
    };

    /// Construct a scope from raw min/max [`Dp`] values. Non-finite minimums
    /// and negative values are coerced to zero, a NaN maximum becomes
    /// unbounded, and inverted bounds collapse to the maximum.
    pub fn new(min_width: Dp, max_width: Dp, min_height: Dp, max_height: Dp) -> Self {
        fn minimum(value: f32) -> f32 {
            if value.is_nan() || value < 0.0 || !value.is_finite() {
                0.0
            } else {
                value
            }
        }
        fn maximum(value: f32) -> f32 {
            if value.is_nan() || value < 0.0 {
                if value.is_nan() { f32::INFINITY } else { 0.0 }
            } else {
                value
            }
        }
        let mut scope = Self {
            min_width: Dp(minimum(min_width.0)),
            max_width: Dp(maximum(max_width.0)),
            min_height: Dp(minimum(min_height.0)),
            max_height: Dp(maximum(max_height.0)),
        };
        if scope.min_width.0 > scope.max_width.0 {
            scope.min_width = scope.max_width;
        }
        if scope.min_height.0 > scope.max_height.0 {
            scope.min_height = scope.max_height;
        }
        scope
    }

    pub fn has_bounded_width(&self) -> bool {
        self.max_width.0.is_finite()
    }

    pub fn has_bounded_height(&self) -> bool {
        self.max_height.0.is_finite()
    }
}

/// Scope passed to [`BoxWithConstraints`](crate::prelude::BoxWithConstraints)
/// content. All values are in [`Dp`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxWithConstraintsScope {
    pub min_width: Dp,
    pub max_width: Dp,
    pub min_height: Dp,
    pub max_height: Dp,
}

impl BoxWithConstraintsScope {
    /// `true` if the width is bounded by the parent (i.e. not infinite).
    pub fn has_bounded_width(&self) -> bool {
        self.max_width.0.is_finite()
    }

    /// `true` if the height is bounded by the parent (i.e. not infinite).
    pub fn has_bounded_height(&self) -> bool {
        self.max_height.0.is_finite()
    }
}

pub type ViewId = u64;

pub type ImageHandle = u64;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageFilter {
    #[default]
    Linear,
    Nearest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageSourceRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl ImageSourceRect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

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
        font_size: Sp,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
        font_family: Option<&'static str>,
        annotations: Option<Arc<[TextSpan]>>,
        text_align: TextAlign,
        font_weight: FontWeight,
        font_style: FontStyle,
        text_decoration: TextDecoration,
        letter_spacing: Sp,
        line_height: Sp,
        /// URL for clickable link text.
        url: Option<Arc<str>>,
        /// OpenType font variation settings (e.g. "wght 700, opsz 24").
        font_variation_settings: Option<Arc<str>>,
        /// Fill, outline, or both (faux-bold). Default `Fill`.
        draw_style: DrawStyle,
    },

    Image {
        handle: ImageHandle,
        tint: Color,
        fit: ImageFit,
        filter: ImageFilter,
        source_rect: Option<ImageSourceRect>,
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
}

impl std::fmt::Debug for ViewKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Box => f.write_str("Box"),
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
            Self::ZStack => f.write_str("ZStack"),
            Self::OverlayHost => f.write_str("OverlayHost"),

            Self::Image {
                handle,
                tint,
                fit,
                filter,
                source_rect,
            } => f
                .debug_struct("Image")
                .field("handle", handle)
                .field("tint", tint)
                .field("fit", fit)
                .field("filter", filter)
                .field("source_rect", source_rect)
                .finish(),
            Self::SubcomposeLayout { .. } => f.write_str("SubcomposeLayout"),
            Self::Text { text, .. } => write!(f, "Text({:?})", text),
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

#[derive(Clone, Copy, Debug)]
pub struct PaintCallbackInfo {
    /// Viewport in physical pixels where the callback should paint.
    pub viewport: Rect,
    /// Clip rect in physical pixels (intersection of callback rect and current clip).
    pub clip_rect: Rect,
    /// Pixels per point (DPI scale).
    pub pixels_per_point: f32,
    /// Screen size in physical pixels.
    pub screen_size_px: [u32; 2],
}

pub type PaintCallbackPayload = Arc<dyn std::any::Any + Send + Sync>;

/// Paint-space scene graph. `Rect`/`Vec2` compounds carry physical pixels
/// (like Compose `Offset`/`Size`/`Rect`); scalar lengths use [`Px`] so the
/// dp→px boundary is explicit instead of unitless `f32`.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum SceneNode {
    Rect {
        rect: Rect,
        brush: Brush,
        radius: [Px; 4],
    },
    Border {
        rect: Rect,
        brush: Brush,
        width: Px,
        radius: [Px; 4],
    },
    Text {
        rect: Rect,
        text: Arc<str>,
        color: Color,
        size: Px,
        font_family: Option<&'static str>,
        text_align: TextAlign,
        font_weight: FontWeight,
        font_style: FontStyle,
        text_decoration: TextDecoration,
        letter_spacing: Px,
        line_height: Px,
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
        brush: Brush,
        width: Px,
    },
    PushClip {
        rect: Rect,
        radius: [Px; 4],
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
        filter: ImageFilter,
        source_rect: Option<ImageSourceRect>,
    },
    /// Tinted A8 coverage mask: samples `handle` (registered with
    /// `register_coverage_a8`) as coverage and composites `color` with
    /// source-over. Lets hosts rasterize geometry once (e.g. cached glyph
    /// coverage tiles) and re-composite it per frame with a new color or
    /// position without re-uploading. `rect` positions the tile's top-left;
    /// its size is the registered tile size.
    Coverage {
        rect: Rect,
        handle: ImageHandle,
        color: Color,
    },
    /// Shadow behind a rounded rect, typically driven by `StateElevation`.
    /// The `elevation` field controls offset and alpha.
    Shadow {
        rect: Rect,
        radius: [Px; 4],
        elevation: Px,
        color: Color,
    },
    /// Mark the start of a graphics layer: the contained subtree is rendered
    /// into an offscreen texture and then composited back into the parent.
    /// `alpha` is the group-compositing alpha applied at composite time.
    /// `blur_radius_x` / `blur_radius_y` are the gaussian blur radii in [`Px`]
    /// applied to the layer before compositing (zero = no blur on that axis).
    /// `rectangle_edge` true = clamp edge pixels (Rectangle). False = transparent out-of-bounds (Unbounded).
    BeginLayer {
        rect: Rect,
        layer_id: u32,
        alpha: f32,
        blur_radius_x: Px,
        blur_radius_y: Px,
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
        blur_px: Px,
        offset_px: (Px, Px),
        color: Color,
    },
    /// Arc stroke
    Arc {
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        stroke_width: Px,
        brush: Brush,
        cap: StrokeCap,
    },
    /// Pre-tessellated vector mesh (fill or stroke geometry produced by the
    /// host, e.g. lyon tessellation). Vertices live in the mesh's own local
    /// space; `transform` is a 2x3 affine that maps local -> world pixels as
    /// `[m00, m01, m10, m11, tx, ty]` (`out = M * local + t`; identity is
    /// `[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]`) and is applied in the vertex shader.
    /// The current scene `PushTransform` stack is folded in on top of
    /// `transform`.
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
    /// `op` selects intersection (keep content inside the mask, the common
    /// case) or difference (cut the mask out, e.g. ASS `\iclip` drawings).
    /// Difference is exact for a lone mask and for a mask nested inside
    /// intersect clips; a normal clip nested inside a difference mask is
    /// best-effort (see the renderer docs).
    PushVectorClip {
        mesh: Arc<VectorMeshData>,
        op: ClipOp,
    },
    /// End a vector clip opened by `PushVectorClip`.
    PopVectorClip,
    /// Custom GPU paint callback (check `egui::PaintCallback`).
    Callback {
        rect: Rect,
        payload: PaintCallbackPayload,
    },
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

/// How a `VectorMesh` is painted. Mirrors the [`Brush`] variants;
/// gradient endpoints live in the mesh's local space.
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
    /// Radial gradient centered at `center` (mesh-local) with `radius`.
    Radial {
        center: Vec2,
        radius: f32,
        start_color: Color,
        end_color: Color,
    },
    /// Angular sweep around `center` (mesh-local), clockwise from 3 o'clock.
    Sweep {
        center: Vec2,
        start_color: Color,
        end_color: Color,
    },
}

/// Blend mode for a `VectorMesh`, following the CSS `mix-blend-mode`
/// vocabulary used by SVG/Lottie-style artwork. All variants are wired into
/// the renderer: separable modes use fixed-function blending, the rest
/// (marked below) isolate the mesh into a graphics layer and composite it
/// with a custom shader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
#[derive(Default)]
pub enum BlendMode {
    /// Standard premultiplied alpha blending (`normal`).
    #[default]
    Alpha,
    /// `screen`: `1 - (1 - S) * (1 - D)`. Fixed-function.
    Screen,
    /// `overlay`: `Multiply` for dark backdrops, `Screen` for light ones.
    Overlay,
    /// `darken`: per-channel `min(S, D)`. Fixed-function.
    Darken,
    /// `lighten`: per-channel `max(S, D)`. Fixed-function.
    Lighten,
    /// `color-dodge`: brightens the backdrop toward the source color.
    ColorDodge,
    /// `color-burn`: darkens the backdrop toward the source color.
    ColorBurn,
    /// `hard-light`: `Overlay` with source and backdrop swapped.
    HardLight,
    /// `soft-light`: subtle `Overlay` variant.
    SoftLight,
    /// `difference`: per-channel `|S - D|`.
    /// Needs the backdrop (fixed-function subtract only covers `S - D`),
    /// so it composites via an isolated layer.
    Difference,
    /// `exclusion`: like `difference` with lower contrast.
    Exclusion,
    /// `hue`: source hue with backdrop saturation and luminosity.
    Hue,
    /// `saturation`: source saturation with backdrop hue and luminosity.
    Saturation,
    /// `color`: source hue and saturation with backdrop luminosity.
    Color,
    /// `luminosity`: source luminosity with backdrop hue and saturation.
    Luminosity,
    /// Additive (screen-space light). Fixed-function.
    Add,
    /// `multiply`: per-channel `S * D`. Fixed-function.
    Multiply,
}

impl BlendMode {
    /// Modes implemented with fixed-function hardware blending on the mesh
    /// pipeline. Everything else isolates the mesh into a graphics layer and
    /// composites it with the backdrop-blend shader.
    pub fn needs_isolation(self) -> bool {
        !matches!(
            self,
            BlendMode::Alpha
                | BlendMode::Add
                | BlendMode::Multiply
                | BlendMode::Screen
                | BlendMode::Darken
                | BlendMode::Lighten
        )
    }

    /// Discriminant consumed by `blend_layer.wgsl` (`mode` flat varying).
    pub fn shader_mode(self) -> u32 {
        match self {
            BlendMode::Alpha => 0,
            BlendMode::Add => 1,
            BlendMode::Multiply => 2,
            BlendMode::Screen => 3,
            BlendMode::Overlay => 4,
            BlendMode::Darken => 5,
            BlendMode::Lighten => 6,
            BlendMode::ColorDodge => 7,
            BlendMode::ColorBurn => 8,
            BlendMode::HardLight => 9,
            BlendMode::SoftLight => 10,
            BlendMode::Difference => 11,
            BlendMode::Exclusion => 12,
            BlendMode::Hue => 13,
            BlendMode::Saturation => 14,
            BlendMode::Color => 15,
            BlendMode::Luminosity => 16,
        }
    }
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
        assert!(!s.max_width.0.is_finite());
        assert!(!s.max_height.0.is_finite());
        assert_eq!(s.min_width, Dp(0.0));
        assert_eq!(s.min_height, Dp(0.0));
    }

    #[test]
    fn subcompose_scope_new_round_trips() {
        let s = SubcomposeScope::new(Dp(10.0), Dp(200.0), Dp(20.0), Dp(300.0));
        assert_eq!(s.min_width, Dp(10.0));
        assert_eq!(s.max_width, Dp(200.0));
        assert_eq!(s.min_height, Dp(20.0));
        assert_eq!(s.max_height, Dp(300.0));
    }

    #[test]
    fn box_with_constraints_scope_bounded_predicates() {
        let bounded = BoxWithConstraintsScope {
            min_width: Dp(0.0),
            max_width: Dp(360.0),
            min_height: Dp(0.0),
            max_height: Dp(640.0),
        };
        assert!(bounded.has_bounded_width());
        assert!(bounded.has_bounded_height());

        let unbounded = BoxWithConstraintsScope {
            min_width: Dp(0.0),
            max_width: Dp(f32::INFINITY),
            min_height: Dp(0.0),
            max_height: Dp(f32::INFINITY),
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
