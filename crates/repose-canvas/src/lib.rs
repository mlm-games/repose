#![allow(non_snake_case)]
use std::cell::RefCell;
use std::sync::Arc;

use repose_core::*;
use repose_ui::*;

pub struct DrawScope {
    pub commands: Vec<DrawCommand>,
    pub size: Size,
}

const MAX_TESSELLATION_CACHE_ENTRIES: usize = 128;
const MAX_MESH_MAP_CACHE_ENTRIES: usize = 64;

#[derive(Clone, PartialEq, Eq, Hash)]
enum PathCacheKey {
    Corner(u32),
    Dash(Vec<u32>, u32),
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TessellationCacheKey {
    kind: u8,
    rect: [u32; 8],
    params: [u32; 8],
    path: Option<PathCacheKey>,
    points: Vec<[u32; 2]>,
}

thread_local! {
    static TESSELLATION_CACHE: RefCell<Vec<(TessellationCacheKey, Arc<VectorMeshData>)>> =
        const { RefCell::new(Vec::new()) };
    static MESH_MAP_CACHE:
        RefCell<Vec<((usize, u32, u32, u32), Arc<VectorMeshData>, Arc<VectorMeshData>)>> =
        const { RefCell::new(Vec::new()) };
    static OVERLAY_MAP_CACHE:
        RefCell<Vec<((usize, u32, u32, u32), Arc<[VectorMeshData]>, Arc<[VectorMeshData]>)>> =
        const { RefCell::new(Vec::new()) };
}

fn path_cache_key(effect: Option<&PathEffect>) -> Option<PathCacheKey> {
    match effect {
        Some(PathEffect::Corner { radius }) => Some(PathCacheKey::Corner(radius.to_bits())),
        Some(PathEffect::Dash { intervals, phase }) => Some(PathCacheKey::Dash(
            intervals.iter().map(|value| value.to_bits()).collect(),
            phase.to_bits(),
        )),
        None => None,
    }
}

fn rect_cache_bits(rect: Rect) -> [u32; 4] {
    [
        rect.x.to_bits(),
        rect.y.to_bits(),
        rect.w.to_bits(),
        rect.h.to_bits(),
    ]
}

fn rect_pair_cache_bits(first: Rect, second: Rect) -> [u32; 8] {
    let mut bits = [0; 8];
    bits[..4].copy_from_slice(&rect_cache_bits(first));
    bits[4..].copy_from_slice(&rect_cache_bits(second));
    bits
}

fn stroke_cache_key(
    kind: u8,
    local_rect: Rect,
    canvas_rect: Rect,
    width: Px,
    radius: f32,
    cap: StrokeCap,
    join: StrokeJoin,
    miter: f32,
    path: Option<&PathEffect>,
) -> TessellationCacheKey {
    TessellationCacheKey {
        kind,
        rect: rect_pair_cache_bits(local_rect, canvas_rect),
        params: [
            width.0.to_bits(),
            cap as u32,
            join as u32,
            miter.to_bits(),
            radius.to_bits(),
            0,
            0,
            0,
        ],
        path: path_cache_key(path),
        points: Vec::new(),
    }
}

fn polyline_cache_key(
    points: &[Vec2],
    canvas_rect: Rect,
    width: Px,
    cap: StrokeCap,
    join: StrokeJoin,
    miter: f32,
    path: Option<&PathEffect>,
) -> TessellationCacheKey {
    TessellationCacheKey {
        kind: 2,
        rect: rect_pair_cache_bits(Rect::default(), canvas_rect),
        params: [
            width.0.to_bits(),
            cap as u32,
            join as u32,
            miter.to_bits(),
            0,
            0,
            0,
            0,
        ],
        path: path_cache_key(path),
        points: points
            .iter()
            .map(|point| [point.x.to_bits(), point.y.to_bits()])
            .collect(),
    }
}

fn arc_cache_key(rect: Rect, canvas_rect: Rect, start: f32, sweep: f32) -> TessellationCacheKey {
    TessellationCacheKey {
        kind: 3,
        rect: rect_pair_cache_bits(rect, canvas_rect),
        params: [start.to_bits(), sweep.to_bits(), 0, 0, 0, 0, 0, 0],
        path: None,
        points: Vec::new(),
    }
}

fn cached_tessellation(
    key: TessellationCacheKey,
    build: impl FnOnce() -> Option<VectorMeshData>,
) -> Option<Arc<VectorMeshData>> {
    TESSELLATION_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, mesh)) = cache.iter().find(|(cached, _)| cached == &key) {
            return Some(mesh.clone());
        }
        let mesh = Arc::new(build()?);
        if cache.len() >= MAX_TESSELLATION_CACHE_ENTRIES {
            cache.remove(0);
        }
        cache.push((key, mesh.clone()));
        Some(mesh)
    })
}

fn map_mesh_cached(
    mesh: &Arc<VectorMeshData>,
    dx: f32,
    dy: f32,
    alpha: f32,
) -> Arc<VectorMeshData> {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha == 1.0 && dx == 0.0 && dy == 0.0 {
        return mesh.clone();
    }
    let key = (
        Arc::as_ptr(mesh) as usize,
        dx.to_bits(),
        dy.to_bits(),
        alpha.to_bits(),
    );
    MESH_MAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, _, mapped)) = cache.iter().find(|(cached, _, _)| *cached == key) {
            return mapped.clone();
        }
        let mapped = Arc::new(map_mesh_data(mesh, dx, dy, alpha));
        if cache.len() >= MAX_MESH_MAP_CACHE_ENTRIES {
            cache.remove(0);
        }
        cache.push((key, mesh.clone(), mapped.clone()));
        mapped
    })
}

fn map_overlay_cached(
    meshes: &Arc<[VectorMeshData]>,
    dx: f32,
    dy: f32,
    alpha: f32,
) -> Arc<[VectorMeshData]> {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha == 1.0 && dx == 0.0 && dy == 0.0 {
        return meshes.clone();
    }
    let key = (
        Arc::as_ptr(meshes) as *const () as usize,
        dx.to_bits(),
        dy.to_bits(),
        alpha.to_bits(),
    );
    OVERLAY_MAP_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((_, _, mapped)) = cache.iter().find(|(cached, _, _)| *cached == key) {
            return mapped.clone();
        }
        let mapped: Arc<[VectorMeshData]> = meshes
            .iter()
            .map(|mesh| map_mesh_data(mesh, dx, dy, alpha))
            .collect::<Vec<_>>()
            .into();
        if cache.len() >= MAX_MESH_MAP_CACHE_ENTRIES {
            cache.remove(0);
        }
        cache.push((key, meshes.clone(), mapped.clone()));
        mapped
    })
}

/// Fill-or-stroke style for canvas shapes, mirroring Compose's `DrawStyle`.
/// `width` carries px magnitudes; dash intervals/phase are px as well.
#[derive(Clone, Debug)]
pub enum ShapeStyle {
    Fill,
    Stroke {
        width: Px,
        cap: StrokeCap,
        join: StrokeJoin,
        miter: f32,
        path_effect: Option<PathEffect>,
    },
}

impl ShapeStyle {
    pub fn stroke(width: Px) -> Self {
        Self::Stroke {
            width,
            cap: StrokeCap::Butt,
            join: StrokeJoin::Miter,
            miter: 4.0,
            path_effect: None,
        }
    }
}

/// Paint-space draw commands. `Rect`/`Vec2` compounds carry px magnitudes
/// (like Compose `Offset`/`Size`/`Rect`); scalar lengths use [`Px`].
#[derive(Clone)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        fill: Brush,
        radius: Px,
        style: ShapeStyle,
    },
    Ellipse {
        center: Vec2,
        rx: f32,
        ry: f32,
        fill: Brush,
        style: ShapeStyle,
    },
    /// Stroked polyline through `points` (px, canvas-local).
    LinePath {
        points: Vec<Vec2>,
        brush: Brush,
        width: Px,
        cap: StrokeCap,
        join: StrokeJoin,
        miter: f32,
        path_effect: Option<PathEffect>,
    },
    /// Stroked arc inscribed in `rect`. Angles are radians, clockwise
    /// positive (y-down), starting at 3 o'clock like the `Arc` scene node.
    Arc {
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        use_center: bool,
        brush: Brush,
        width: Px,
        cap: StrokeCap,
    },
    Text {
        text: String,
        pos: Vec2,
        color: Color,
        size: Px,
        /// `None` = default UI font (Open Sans). `Some("monospace")` resolves
        /// via the font-awl `monospace` feature (JetBrains Mono when bundled,
        /// system monospace otherwise). `&'static str` to match `ViewKind::Text`.
        font_family: Option<&'static str>,
    },
    /// Pre-tessellated vector mesh (fill or stroke) in mesh-local space.
    /// `transform` is a 2x3 affine mapping local -> world pixels and is applied
    /// in the vertex shader.
    VectorMesh {
        mesh: Arc<VectorMeshData>,
        transform: [f32; 6],
        paint: PaintDesc,
        clip: Option<u32>,
        blend: BlendMode,
    },
    /// Canvas-local overlay meshes translated to the canvas origin and emitted
    /// without the world transform.
    VectorOverlay { meshes: Arc<[VectorMeshData]> },
    /// Begin a stencil clip from an arbitrary tessellated mask.
    PushVectorClip {
        mesh: Arc<VectorMeshData>,
        op: ClipOp,
    },
    /// End a stencil clip opened by `PushVectorClip`.
    PopVectorClip,
    /// Push a world transform onto the stack (image/vector subtree).
    PushTransform { transform: Transform },
    /// Pop a transform pushed by `PushTransform`.
    PopTransform,
    /// A positioned image. `rect` is in image-local space. The transform stack
    /// maps it to the canvas. `handle` was uploaded via `RenderContext`.
    Image {
        rect: Rect,
        handle: ImageHandle,
        tint: Color,
        fit: ImageFit,
        filter: ImageFilter,
        source_rect: Option<ImageSourceRect>,
    },
}

impl DrawScope {
    pub fn draw_image(&mut self, rect: Rect, handle: ImageHandle, tint: Color, fit: ImageFit) {
        self.draw_image_filtered(rect, handle, None, tint, fit, ImageFilter::Linear);
    }

    pub fn draw_image_subrect(
        &mut self,
        rect: Rect,
        handle: ImageHandle,
        source_rect: ImageSourceRect,
        tint: Color,
        fit: ImageFit,
    ) {
        self.draw_image_filtered(
            rect,
            handle,
            Some(source_rect),
            tint,
            fit,
            ImageFilter::Linear,
        );
    }

    pub fn draw_image_filtered(
        &mut self,
        rect: Rect,
        handle: ImageHandle,
        source_rect: Option<ImageSourceRect>,
        tint: Color,
        fit: ImageFit,
        filter: ImageFilter,
    ) {
        self.commands.push(DrawCommand::Image {
            rect,
            handle,
            tint,
            fit,
            filter,
            source_rect,
        });
    }

    pub fn draw_rect(&mut self, rect: Rect, color: Color, radius: Px) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill: Brush::Solid(color),
            radius,
            style: ShapeStyle::Fill,
        });
    }
    /// Brush-filled rect (solid, linear, radial, sweep). Mirrors Compose
    /// `DrawScope.drawRect(brush, ...)`. Gradient endpoints are expressed in
    /// the rect's local space: `(0,0)` is the rect top-left and endpoints are
    /// pixel offsets.
    pub fn draw_rect_brush(&mut self, rect: Rect, brush: Brush, radius: Px) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill: brush,
            radius,
            style: ShapeStyle::Fill,
        });
    }
    pub fn draw_rect_stroke(&mut self, rect: Rect, color: Color, radius: Px, width: Px) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill: Brush::Solid(color),
            radius,
            style: ShapeStyle::stroke(width),
        });
    }
    /// Brush outline. Cap/join/miter/path_effect ride `style`. Gradient
    /// endpoints use the same rect-local space as [`draw_rect_brush`](Self::draw_rect_brush).
    pub fn draw_rect_stroke_brush(&mut self, rect: Rect, brush: Brush, radius: Px, width: Px) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill: brush,
            radius,
            style: ShapeStyle::stroke(width),
        });
    }
    /// Full-style rect (Compose `style: DrawStyle = Fill` equivalent).
    pub fn draw_rect_style(&mut self, rect: Rect, brush: Brush, radius: Px, style: ShapeStyle) {
        self.commands.push(DrawCommand::Rect {
            rect,
            fill: brush,
            radius,
            style,
        });
    }
    pub fn draw_ellipse(&mut self, center: Vec2, rx: f32, ry: f32, color: Color) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            fill: Brush::Solid(color),
            style: ShapeStyle::Fill,
        });
    }
    /// Brush-filled ellipse (Compose `drawOval/drawCircle(brush, ...)`).
    /// Gradient endpoints are ellipse-local: `(0,0)` is the bounding-box
    /// top-left.
    pub fn draw_ellipse_brush(&mut self, center: Vec2, rx: f32, ry: f32, brush: Brush) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            fill: brush,
            style: ShapeStyle::Fill,
        });
    }
    pub fn draw_ellipse_stroke(&mut self, center: Vec2, rx: f32, ry: f32, color: Color, width: Px) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            fill: Brush::Solid(color),
            style: ShapeStyle::stroke(width),
        });
    }
    /// Brush ellipse outline.
    pub fn draw_ellipse_stroke_brush(
        &mut self,
        center: Vec2,
        rx: f32,
        ry: f32,
        brush: Brush,
        width: Px,
    ) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            fill: brush,
            style: ShapeStyle::stroke(width),
        });
    }
    pub fn draw_circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.draw_ellipse(center, radius, radius, color);
    }
    pub fn draw_circle_stroke(&mut self, center: Vec2, radius: f32, color: Color, width: Px) {
        self.draw_ellipse_stroke(center, radius, radius, color, width);
    }
    /// Brush-filled circle.
    pub fn draw_circle_brush(&mut self, center: Vec2, radius: f32, brush: Brush) {
        self.draw_ellipse_brush(center, radius, radius, brush);
    }
    /// Stroked polyline (Compose `drawLine` for >2 points / `drawPath` stroke).
    /// Needs 2+ points. Cap/join/miter mirror `Stroke` defaults. Gradient
    /// endpoints are canvas-local, matching the `points` space.
    pub fn draw_line_path(
        &mut self,
        points: impl Into<Vec<Vec2>>,
        brush: Brush,
        width: Px,
        cap: StrokeCap,
        join: StrokeJoin,
    ) {
        self.commands.push(DrawCommand::LinePath {
            points: points.into(),
            brush,
            width,
            cap,
            join,
            miter: 4.0,
            path_effect: None,
        });
    }
    /// Single segment. Mirrors Compose `drawLine(brush, start, end, ...)`.
    pub fn draw_line(&mut self, start: Vec2, end: Vec2, color: Color, width: Px, cap: StrokeCap) {
        self.commands.push(DrawCommand::LinePath {
            points: vec![start, end],
            brush: Brush::Solid(color),
            width,
            cap,
            join: StrokeJoin::Miter,
            miter: 4.0,
            path_effect: None,
        });
    }
    /// Brush line with explicit joins (multi-segment).
    pub fn draw_line_brush(
        &mut self,
        start: Vec2,
        end: Vec2,
        brush: Brush,
        width: Px,
        cap: StrokeCap,
    ) {
        self.commands.push(DrawCommand::LinePath {
            points: vec![start, end],
            brush,
            width,
            cap,
            join: StrokeJoin::Miter,
            miter: 4.0,
            path_effect: None,
        });
    }
    /// Stroked arc (Compose `drawArc(brush, startAngle, sweepAngle, ...)`).
    /// Angles are radians, clockwise positive, from 3 o'clock.
    /// `use_center = true` emits a pie wedge; false emits the open arc.
    /// Gradient endpoints are arc-local: `(0,0)` is the bounding-`rect`
    /// top-left.
    pub fn draw_arc(
        &mut self,
        rect: Rect,
        start_angle: f32,
        sweep_angle: f32,
        use_center: bool,
        brush: Brush,
        width: Px,
        cap: StrokeCap,
    ) {
        self.commands.push(DrawCommand::Arc {
            rect,
            start_angle,
            sweep_angle,
            use_center,
            brush,
            width,
            cap,
        });
    }
    pub fn draw_text(&mut self, text: impl Into<String>, pos: Vec2, color: Color, size: Px) {
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            pos,
            color,
            size,
            font_family: None,
        });
    }

    /// Like [`draw_text`](Self::draw_text) but with an explicit font family.
    /// Pass `"monospace"` for code (JetBrains Mono via font-awl `monospace`
    /// feature, else system monospace fallback).
    pub fn draw_text_with_family(
        &mut self,
        text: impl Into<String>,
        pos: Vec2,
        color: Color,
        size: Px,
        font_family: Option<&'static str>,
    ) {
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            pos,
            color,
            size,
            font_family,
        });
    }

    /// Draw a pre-tessellated vector mesh. `transform` maps mesh-local
    /// coordinates to world pixels as a 2x3 affine `[m00, m01, m10, m11, tx,
    /// ty]` (a 2x2 row-major linear part, then translation; identity is
    /// `[1.0, 0.0, 0.0, 1.0, 0.0, 0.0]`). `out = M * local + t`.
    /// `blend` selects the compositing mode; non-`Alpha` modes need a GPU
    /// backend with blend support (the wgpu renderer implements all of
    /// [`BlendMode`](repose_core::BlendMode)).
    pub fn draw_vector_mesh(
        &mut self,
        mesh: Arc<VectorMeshData>,
        transform: [f32; 6],
        paint: PaintDesc,
    ) {
        self.draw_vector_mesh_blended(mesh, transform, paint, BlendMode::Alpha)
    }

    /// [`draw_vector_mesh`](Self::draw_vector_mesh) with an explicit blend
    /// mode.
    pub fn draw_vector_mesh_blended(
        &mut self,
        mesh: Arc<VectorMeshData>,
        transform: [f32; 6],
        paint: PaintDesc,
        blend: BlendMode,
    ) {
        self.commands.push(DrawCommand::VectorMesh {
            mesh,
            transform,
            paint,
            clip: None,
            blend,
        });
    }

    /// Draw canvas-local overlay meshes translated to the canvas origin without
    /// applying the world transform.
    pub fn draw_vector_overlay(&mut self, meshes: Arc<[VectorMeshData]>) {
        self.commands.push(DrawCommand::VectorOverlay { meshes });
    }

    pub fn push_vector_clip(&mut self, mesh: Arc<VectorMeshData>) {
        self.commands.push(DrawCommand::PushVectorClip {
            mesh,
            op: ClipOp::Intersect,
        });
    }

    /// Push a vector clip with an explicit operator (e.g. `Difference` to
    /// cut the mask out instead of masking to it).
    pub fn push_vector_clip_op(&mut self, mesh: Arc<VectorMeshData>, op: ClipOp) {
        self.commands.push(DrawCommand::PushVectorClip { mesh, op });
    }

    /// Pop the most recent vector clip.
    pub fn pop_vector_clip(&mut self) {
        self.commands.push(DrawCommand::PopVectorClip);
    }

    /// Push a 2D transform for subsequent draws (rotation, mirroring,
    /// extra translation). Composes with the canvas offset and any outer
    /// scene-graph transforms; the GPU backend rotates in-shader
    /// (`fwd_mat`), so rotated rects stay crisp — no AABB fallback needed.
    /// Balance with [`pop_transform`](Self::pop_transform).
    pub fn push_transform(&mut self, transform: Transform) {
        self.commands.push(DrawCommand::PushTransform { transform });
    }

    /// Pop the most recent [`push_transform`](Self::push_transform).
    pub fn pop_transform(&mut self) {
        self.commands.push(DrawCommand::PopTransform);
    }

    /// Draw a rect rotated `rotation` radians about `pivot` (canvas-local
    /// px, y-down positive-clockwise like the rest of the canvas API).
    /// Exact on canvas and GPU alike: the rotation rides the transform
    /// stack instead of baking an axis-aligned bounding box.
    pub fn draw_rect_rotated(
        &mut self,
        rect: Rect,
        color: Color,
        radius: Px,
        rotation: f32,
        pivot: Vec2,
    ) {
        if !rotation.is_finite() || rotation.abs() < 1e-7 {
            self.draw_rect(rect, color, radius);
            return;
        }
        let mut spin = Transform::identity();
        spin.rotate = rotation;
        let mut t = Transform::translate(pivot.x, pivot.y)
            .combine(&spin)
            .combine(&Transform::translate(-pivot.x, -pivot.y));
        // Pivot is baked into the rows above, and zero the origin so pivot-aware
        // consumers never apply it twice (same shape the paint bake emits).
        t.origin_x = 0.0;
        t.origin_y = 0.0;
        self.push_transform(t);
        self.draw_rect(rect, color, radius);
        self.pop_transform();
    }
}

fn alpha_color(color: Color, alpha: f32) -> Color {
    if alpha == 1.0 {
        return color;
    }
    Color(
        color.0,
        color.1,
        color.2,
        (color.3 as f32 * alpha.clamp(0.0, 1.0)) as u8,
    )
}

fn alpha_brush(brush: Brush, alpha: f32) -> Brush {
    if alpha == 1.0 {
        return brush;
    }
    match brush {
        Brush::Solid(color) => Brush::Solid(alpha_color(color, alpha)),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start,
            end,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        Brush::LinearNormalized {
            start,
            end,
            start_color,
            end_color,
        } => Brush::LinearNormalized {
            start,
            end,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => Brush::Radial {
            center,
            radius,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => Brush::Sweep {
            center,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        _ => brush,
    }
}

fn alpha_paint(paint: PaintDesc, alpha: f32) -> PaintDesc {
    if alpha == 1.0 {
        return paint;
    }
    match paint {
        PaintDesc::Solid => PaintDesc::Solid,
        PaintDesc::Linear {
            start,
            end,
            start_color,
            end_color,
        } => PaintDesc::Linear {
            start,
            end,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        PaintDesc::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => PaintDesc::Radial {
            center,
            radius,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        PaintDesc::Sweep {
            center,
            start_color,
            end_color,
        } => PaintDesc::Sweep {
            center,
            start_color: alpha_color(start_color, alpha),
            end_color: alpha_color(end_color, alpha),
        },
        _ => paint,
    }
}

fn map_mesh_data(m: &VectorMeshData, dx: f32, dy: f32, alpha: f32) -> VectorMeshData {
    let alpha = alpha.clamp(0.0, 1.0);
    let vertices: Arc<[VectorVertex]> = m
        .vertices
        .iter()
        .map(|v| VectorVertex {
            pos: [v.pos[0] + dx, v.pos[1] + dy],
            color: if alpha == 1.0 {
                v.color
            } else {
                [
                    v.color[0] * alpha,
                    v.color[1] * alpha,
                    v.color[2] * alpha,
                    v.color[3] * alpha,
                ]
            },
            uv: v.uv,
        })
        .collect();
    VectorMeshData {
        vertices,
        indices: m.indices.clone(),
    }
}

fn mapped_mesh(m: &Arc<VectorMeshData>, dx: f32, dy: f32, alpha: f32) -> Arc<VectorMeshData> {
    map_mesh_cached(m, dx, dy, alpha)
}

fn offset_brush(brush: Brush, origin: Vec2) -> Brush {
    match brush {
        Brush::Solid(color) => Brush::Solid(color),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start: Vec2 {
                x: start.x + origin.x,
                y: start.y + origin.y,
            },
            end: Vec2 {
                x: end.x + origin.x,
                y: end.y + origin.y,
            },
            start_color,
            end_color,
        },
        Brush::LinearNormalized {
            start,
            end,
            start_color,
            end_color,
        } => Brush::LinearNormalized {
            start: Vec2 {
                x: start.x + origin.x,
                y: start.y + origin.y,
            },
            end: Vec2 {
                x: end.x + origin.x,
                y: end.y + origin.y,
            },
            start_color,
            end_color,
        },
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => Brush::Radial {
            center: Vec2 {
                x: center.x + origin.x,
                y: center.y + origin.y,
            },
            radius,
            start_color,
            end_color,
        },
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => Brush::Sweep {
            center: Vec2 {
                x: center.x + origin.x,
                y: center.y + origin.y,
            },
            start_color,
            end_color,
        },
        _ => brush,
    }
}

fn resolve_normalized_brush(brush: Brush, size: Vec2) -> Brush {
    match brush {
        Brush::LinearNormalized {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start: Vec2 {
                x: start.x * size.x,
                y: start.y * size.y,
            },
            end: Vec2 {
                x: end.x * size.x,
                y: end.y * size.y,
            },
            start_color,
            end_color,
        },
        brush => brush,
    }
}

fn brush_to_paint(brush: &Brush, origin: Vec2, size: Vec2) -> PaintDesc {
    match offset_brush(resolve_normalized_brush(*brush, size), origin) {
        Brush::Solid(_) => PaintDesc::Solid,
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => PaintDesc::Linear {
            start,
            end,
            start_color,
            end_color,
        },
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => PaintDesc::Radial {
            center,
            radius,
            start_color,
            end_color,
        },
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => PaintDesc::Sweep {
            center,
            start_color,
            end_color,
        },
        _ => PaintDesc::Solid,
    }
}

fn brush_to_mesh_paint(brush: &Brush, origin: Vec2, size: Vec2, alpha: f32) -> PaintDesc {
    if let Brush::Solid(color) = brush {
        let color = alpha_color(*color, alpha);
        return PaintDesc::Linear {
            start: Vec2::ZERO,
            end: Vec2::ZERO,
            start_color: color,
            end_color: color,
        };
    }
    brush_to_paint(&alpha_brush(*brush, alpha), origin, size)
}

fn generated_vertex_color() -> [f32; 4] {
    [1.0; 4]
}

fn mesh_from_buffers(
    buffers: lyon_tessellation::VertexBuffers<lyon_path::math::Point, u16>,
    vertex_color: [f32; 4],
) -> Option<VectorMeshData> {
    if buffers.indices.is_empty() {
        return None;
    }
    let vertices: Arc<[VectorVertex]> = buffers
        .vertices
        .iter()
        .map(|vertex| VectorVertex {
            pos: [vertex.x, vertex.y],
            color: vertex_color,
            uv: [0.0, 0.0],
        })
        .collect();
    let indices: Arc<[u32]> = buffers.indices.iter().map(|&index| index as u32).collect();
    Some(VectorMeshData { vertices, indices })
}

fn tessellate_polyline(
    points: &[Vec2],
    canvas_rect: Rect,
    width: Px,
    cap: StrokeCap,
    join: StrokeJoin,
    miter: f32,
    path_effect: Option<&PathEffect>,
) -> Option<VectorMeshData> {
    use lyon_path::Path;
    use lyon_path::math::Point;
    use lyon_tessellation::{
        LineCap, LineJoin, StrokeOptions, StrokeTessellator, VertexBuffers,
        geometry_builder::simple_builder,
    };

    if points.len() < 2 {
        return None;
    }

    let mut builder = Path::builder();
    let pt = |p: &Vec2| Point::new(canvas_rect.x + p.x, canvas_rect.y + p.y);
    builder.begin(pt(&points[0]));
    for p in &points[1..] {
        builder.line_to(pt(p));
    }
    builder.end(false);
    let mut path = builder.build();
    if let Some(effect) = path_effect {
        path = apply_canvas_path_effect(&path, effect);
    }

    let lyon_cap = match cap {
        StrokeCap::Butt => LineCap::Butt,
        StrokeCap::Round => LineCap::Round,
        StrokeCap::Square => LineCap::Square,
    };
    let lyon_join = match join {
        StrokeJoin::Miter => LineJoin::Miter,
        StrokeJoin::Round => LineJoin::Round,
        StrokeJoin::Bevel => LineJoin::Bevel,
    };
    let mut tess = StrokeTessellator::new();
    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
    tess.tessellate_path(
        &path,
        &StrokeOptions::default()
            .with_tolerance(0.25)
            .with_line_width(width.0.max(0.0))
            .with_line_cap(lyon_cap)
            .with_line_join(lyon_join)
            .with_miter_limit(miter),
        &mut simple_builder(&mut buffers),
    )
    .ok()?;
    mesh_from_buffers(buffers, generated_vertex_color())
}

fn apply_canvas_corner_effect(path: &lyon_path::Path, radius: f32) -> lyon_path::Path {
    use lyon_path::PathEvent;
    use lyon_path::math::{Point, Vector};

    if !radius.is_finite() || radius <= 0.0 {
        return path.clone();
    }
    if path
        .iter()
        .any(|event| matches!(event, PathEvent::Quadratic { .. } | PathEvent::Cubic { .. }))
    {
        return path.clone();
    }
    struct CornerContour {
        points: Vec<Point>,
        closed: bool,
    }

    let events: Vec<PathEvent> = path.iter().collect();
    let mut contours: Vec<CornerContour> = Vec::new();
    let mut current: Vec<Point> = Vec::new();
    let mut contour_start = Point::new(0.0, 0.0);
    for event in events {
        match event {
            PathEvent::Begin { at } => {
                if !current.is_empty() {
                    contours.push(CornerContour {
                        points: std::mem::take(&mut current),
                        closed: false,
                    });
                }
                contour_start = at;
                current.push(at);
            }
            PathEvent::Line { to, .. } => current.push(to),
            PathEvent::End { close, .. } => {
                if !current.is_empty() {
                    if close
                        && current
                            .last()
                            .is_some_and(|last| (*last - contour_start).square_length() > 1e-6)
                    {
                        current.push(contour_start);
                    }
                    contours.push(CornerContour {
                        points: std::mem::take(&mut current),
                        closed: close,
                    });
                }
            }
            _ => {}
        }
    }
    if !current.is_empty() {
        contours.push(CornerContour {
            points: current,
            closed: false,
        });
    }

    let mut builder = lyon_path::Path::builder();
    for contour in contours {
        let points = contour.points;
        let closed = contour.closed;
        if points.len() < 2 {
            continue;
        }
        if points.len() == 2 {
            builder.begin(points[0]);
            builder.line_to(points[1]);
            builder.end(false);
            continue;
        }
        let n = points.len();
        let mut output: Vec<(Point, Option<Point>)> = Vec::new();
        for i in 0..n {
            let current_point = points[i];
            if !closed && (i == 0 || i == n - 1) {
                output.push((current_point, None));
                continue;
            }
            if closed && i == n - 1 {
                break;
            }
            let previous = if i == 0 { points[n - 2] } else { points[i - 1] };
            let next = if i + 1 == n { points[1] } else { points[i + 1] };
            let incoming = Vector::new(current_point.x - previous.x, current_point.y - previous.y);
            let outgoing = Vector::new(next.x - current_point.x, next.y - current_point.y);
            let incoming_len = incoming.length();
            let outgoing_len = outgoing.length();
            if incoming_len <= 1e-6 || outgoing_len <= 1e-6 {
                output.push((current_point, None));
                continue;
            }
            let u1 = incoming / incoming_len;
            let u2 = outgoing / outgoing_len;
            let angle = (u1.x * u2.x + u1.y * u2.y).clamp(-1.0, 1.0).acos();
            let half_angle = angle * 0.5;
            if angle >= std::f32::consts::PI - 1e-3 || half_angle <= 1e-3 {
                output.push((current_point, None));
                continue;
            }
            let inset = (radius / half_angle.tan())
                .min(incoming_len * 0.49)
                .min(outgoing_len * 0.49)
                .max(0.0);
            output.push((
                Point::new(
                    current_point.x - u1.x * inset,
                    current_point.y - u1.y * inset,
                ),
                Some(current_point),
            ));
            output.push((
                Point::new(
                    current_point.x + u2.x * inset,
                    current_point.y + u2.y * inset,
                ),
                None,
            ));
        }
        if output.is_empty() {
            continue;
        }
        builder.begin(output[0].0);
        for (point, control) in output.iter().skip(1) {
            if let Some(control) = control {
                builder.quadratic_bezier_to(*control, *point);
            } else {
                builder.line_to(*point);
            }
        }
        if closed {
            builder.close();
        } else {
            builder.end(false);
        }
    }
    builder.build()
}

fn apply_canvas_path_effect(path: &lyon_path::Path, effect: &PathEffect) -> lyon_path::Path {
    use lyon_path::PathEvent;
    use lyon_path::iterator::PathIterator;
    match effect {
        PathEffect::Corner { radius } => apply_canvas_corner_effect(path, *radius),
        PathEffect::Dash { intervals, phase } => {
            if intervals.len() < 2 || intervals.len() % 2 != 0 {
                return path.clone();
            }
            if intervals.iter().sum::<f32>() <= 0.0 {
                return path.clone();
            }
            let events: Vec<PathEvent> = path.iter().flattened(0.25).collect();
            let dash_len: f32 = intervals.iter().sum();
            let mut phase = phase % dash_len;
            if phase < 0.0 {
                phase += dash_len;
            }
            let mut idx = 0usize;
            let mut acc = 0.0f32;
            let mut dash_dist = 0.0f32;
            let mut emitting = true;
            for (i, &len) in intervals.iter().enumerate() {
                if phase < acc + len {
                    idx = i;
                    dash_dist = phase - acc;
                    emitting = i % 2 == 0;
                    break;
                }
                acc += len;
            }
            let mut builder = lyon_path::Path::builder();
            let mut in_subpath = false;
            for ev in events {
                match ev {
                    PathEvent::Begin { .. } => {}
                    PathEvent::Line { from, to } => {
                        let seg = to - from;
                        let seg_len = seg.length();
                        if seg_len < 0.0001 {
                            continue;
                        }
                        let dir = seg / seg_len;
                        let mut remaining = seg_len;
                        let mut cur = from;
                        while remaining > 0.0 {
                            if intervals[idx] <= 0.0 {
                                idx = (idx + 1) % intervals.len();
                                emitting = !emitting;
                                dash_dist = 0.0;
                                continue;
                            }
                            let avail = intervals[idx] - dash_dist;
                            let take = avail.min(remaining);
                            if take > 0.0 {
                                let next = lyon_path::math::Point::new(
                                    cur.x + dir.x * take,
                                    cur.y + dir.y * take,
                                );
                                if emitting {
                                    if !in_subpath {
                                        builder.begin(cur);
                                        in_subpath = true;
                                    }
                                    builder.line_to(next);
                                } else if in_subpath {
                                    builder.end(false);
                                    in_subpath = false;
                                }
                                cur = next;
                            }
                            remaining -= take;
                            dash_dist += take;
                            if dash_dist >= intervals[idx] {
                                dash_dist = 0.0;
                                idx = (idx + 1) % intervals.len();
                                emitting = !emitting;
                            }
                        }
                    }
                    PathEvent::End { close, .. } if in_subpath => {
                        if close {
                            builder.close();
                        } else {
                            builder.end(false);
                        }
                        in_subpath = false;
                    }
                    _ => {}
                }
            }
            if in_subpath {
                builder.end(false);
            }
            builder.build()
        }
    }
}

pub use repose_core::{PaintCallbackInfo, PaintCallbackPayload};

/// Stroked rounded-rect ring for styles the `Border` scene node cannot
/// express: non-butt caps are meaningless on closed rings, so this covers
/// round/bevel joins, custom miters, and dash path effects. Butt joins with
/// no path effect return `None` so the caller keeps the cheap SDF border.
fn tessellate_rounded_rect_stroke(
    rect: Rect,
    radius: Px,
    canvas_rect: Rect,
    width: Px,
    _cap: StrokeCap,
    join: StrokeJoin,
    miter: f32,
    path_effect: Option<&PathEffect>,
) -> Option<VectorMeshData> {
    let needs_mesh = !matches!(join, StrokeJoin::Miter) || miter != 4.0 || path_effect.is_some();
    if !needs_mesh {
        return None;
    }
    use lyon_path::math::Box2D;
    use lyon_path::math::Point;
    use lyon_tessellation::{
        LineCap, LineJoin, StrokeOptions, StrokeTessellator, VertexBuffers,
        geometry_builder::simple_builder,
    };
    let x0 = canvas_rect.x + rect.x;
    let y0 = canvas_rect.y + rect.y;
    let x1 = x0 + rect.w.max(0.0);
    let y1 = y0 + rect.h.max(0.0);
    let r = radius.0.clamp(0.0, (rect.w.min(rect.h) * 0.5).max(0.0));
    let mut builder = lyon_path::Path::builder();
    builder.add_rounded_rectangle(
        &Box2D {
            min: Point::new(x0, y0),
            max: Point::new(x1, y1),
        },
        &lyon_path::builder::BorderRadii {
            top_left: r,
            top_right: r,
            bottom_left: r,
            bottom_right: r,
        },
        lyon_path::Winding::Positive,
    );
    let mut path = builder.build();
    if let Some(effect) = path_effect {
        path = apply_canvas_path_effect(&path, effect);
    }
    let lyon_join = match join {
        StrokeJoin::Miter => LineJoin::Miter,
        StrokeJoin::Round => LineJoin::Round,
        StrokeJoin::Bevel => LineJoin::Bevel,
    };
    let mut tess = StrokeTessellator::new();
    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
    tess.tessellate_path(
        &path,
        &StrokeOptions::default()
            .with_tolerance(0.25)
            .with_line_width(width.0.max(0.0))
            .with_line_cap(LineCap::Butt)
            .with_line_join(lyon_join)
            .with_miter_limit(miter),
        &mut simple_builder(&mut buffers),
    )
    .ok()?;
    mesh_from_buffers(buffers, generated_vertex_color())
}

/// Stroked ellipse ring for styles `EllipseBorder` cannot express (same
/// rule as [`tessellate_rounded_rect_stroke`]: butt/miter/default stays SDF).
fn tessellate_ellipse_stroke(
    rect: Rect,
    canvas_rect: Rect,
    width: Px,
    _cap: StrokeCap,
    join: StrokeJoin,
    miter: f32,
    path_effect: Option<&PathEffect>,
) -> Option<VectorMeshData> {
    let needs_mesh = !matches!(join, StrokeJoin::Miter) || miter != 4.0 || path_effect.is_some();
    if !needs_mesh {
        return None;
    }
    use lyon_path::math::{Angle, Point, Vector};
    use lyon_tessellation::{
        LineCap, LineJoin, StrokeOptions, StrokeTessellator, VertexBuffers,
        geometry_builder::simple_builder,
    };
    let cx = canvas_rect.x + rect.x + rect.w * 0.5;
    let cy = canvas_rect.y + rect.y + rect.h * 0.5;
    let rx = (rect.w * 0.5).max(0.0);
    let ry = (rect.h * 0.5).max(0.0);
    if rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let mut builder = lyon_path::Path::builder();
    builder.add_ellipse(
        Point::new(cx, cy),
        Vector::new(rx, ry),
        Angle::radians(0.0),
        lyon_path::Winding::Positive,
    );
    let mut path = builder.build();
    if let Some(effect) = path_effect {
        path = apply_canvas_path_effect(&path, effect);
    }
    let lyon_join = match join {
        StrokeJoin::Miter => LineJoin::Miter,
        StrokeJoin::Round => LineJoin::Round,
        StrokeJoin::Bevel => LineJoin::Bevel,
    };
    let mut tess = StrokeTessellator::new();
    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
    tess.tessellate_path(
        &path,
        &StrokeOptions::default()
            .with_tolerance(0.25)
            .with_line_width(width.0.max(0.0))
            .with_line_cap(LineCap::Butt)
            .with_line_join(lyon_join)
            .with_miter_limit(miter),
        &mut simple_builder(&mut buffers),
    )
    .ok()?;
    mesh_from_buffers(buffers, generated_vertex_color())
}

fn tessellate_arc_wedge(
    rect: Rect,
    canvas_rect: Rect,
    start: f32,
    sweep: f32,
) -> Option<VectorMeshData> {
    use lyon_path::math::Point;
    use lyon_tessellation::{
        FillOptions, FillTessellator, VertexBuffers, geometry_builder::simple_builder,
    };
    let cx = canvas_rect.x + rect.x + rect.w * 0.5;
    let cy = canvas_rect.y + rect.y + rect.h * 0.5;
    let rx = (rect.w * 0.5).max(0.0);
    let ry = (rect.h * 0.5).max(0.0);
    if !sweep.is_finite() || sweep.abs() < 1e-6 || rx <= 0.0 || ry <= 0.0 {
        return None;
    }
    let segs = ((sweep.abs() / std::f32::consts::TAU * 96.0).ceil() as usize).clamp(2, 128);
    let mut builder = lyon_path::Path::builder();
    builder.begin(Point::new(cx, cy));
    for i in 0..=segs {
        let a = start + sweep * (i as f32 / segs as f32);
        builder.line_to(Point::new(cx + rx * a.cos(), cy + ry * a.sin()));
    }
    builder.close();
    let path = builder.build();
    let mut tess = FillTessellator::new();
    let mut buffers: VertexBuffers<Point, u16> = VertexBuffers::new();
    tess.tessellate_path(
        &path,
        &FillOptions::default().with_tolerance(0.25),
        &mut simple_builder(&mut buffers),
    )
    .ok()?;
    mesh_from_buffers(buffers, generated_vertex_color())
}

pub fn Canvas(modifier: Modifier, on_draw: impl Fn(&mut DrawScope) + 'static) -> View {
    let painter = move |scene: &mut Scene, rect: Rect, alpha: f32| {
        let mut scope = DrawScope {
            commands: Vec::with_capacity(16),
            size: Size {
                width: rect.w.max(0.0),
                height: rect.h.max(0.0),
            },
        };
        on_draw(&mut scope);
        let brush_size = Vec2 {
            x: scope.size.width,
            y: scope.size.height,
        };

        let to_global = |r: Rect| Rect {
            x: rect.x + r.x,
            y: rect.y + r.y,
            w: r.w,
            h: r.h,
        };
        for cmd in &scope.commands {
            match cmd {
                DrawCommand::Rect {
                    rect: r,
                    fill,
                    radius,
                    style,
                } => {
                    let local_r = *r;
                    let raw_fill = *fill;
                    let fill = alpha_brush(raw_fill, alpha);
                    let r = to_global(local_r);
                    match style {
                        ShapeStyle::Fill => {
                            scene.nodes.push(SceneNode::Rect {
                                rect: r,
                                brush: fill,
                                radius: [*radius; 4],
                            });
                        }
                        ShapeStyle::Stroke {
                            width,
                            cap,
                            join,
                            miter,
                            path_effect,
                        } => {
                            let cache_key = stroke_cache_key(
                                0,
                                local_r,
                                rect,
                                *width,
                                radius.0,
                                *cap,
                                *join,
                                *miter,
                                path_effect.as_ref(),
                            );
                            if let Some(mesh) = cached_tessellation(cache_key, || {
                                tessellate_rounded_rect_stroke(
                                    local_r,
                                    *radius,
                                    rect,
                                    *width,
                                    *cap,
                                    *join,
                                    *miter,
                                    path_effect.as_ref(),
                                )
                            }) {
                                scene.nodes.push(SceneNode::VectorMesh {
                                    mesh,
                                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                    paint: brush_to_mesh_paint(
                                        &raw_fill,
                                        Vec2 {
                                            x: rect.x + local_r.x,
                                            y: rect.y + local_r.y,
                                        },
                                        Vec2 {
                                            x: local_r.w,
                                            y: local_r.h,
                                        },
                                        alpha,
                                    ),
                                    clip: None,
                                    blend: BlendMode::Alpha,
                                });
                            } else {
                                scene.nodes.push(SceneNode::Border {
                                    rect: r,
                                    brush: fill,
                                    width: *width,
                                    radius: [*radius; 4],
                                });
                            }
                        }
                    }
                }
                DrawCommand::Ellipse {
                    center,
                    rx,
                    ry,
                    fill,
                    style,
                } => {
                    let local_r = Rect {
                        x: center.x - *rx,
                        y: center.y - *ry,
                        w: 2.0 * *rx,
                        h: 2.0 * *ry,
                    };
                    let raw_fill = *fill;
                    let fill = alpha_brush(raw_fill, alpha);
                    let r = to_global(local_r);
                    match style {
                        ShapeStyle::Fill => {
                            scene.nodes.push(SceneNode::Ellipse {
                                rect: r,
                                brush: fill,
                            });
                        }
                        ShapeStyle::Stroke {
                            width,
                            cap,
                            join,
                            miter,
                            path_effect,
                        } => {
                            let cache_key = stroke_cache_key(
                                1,
                                local_r,
                                rect,
                                *width,
                                0.0,
                                *cap,
                                *join,
                                *miter,
                                path_effect.as_ref(),
                            );
                            if let Some(mesh) = cached_tessellation(cache_key, || {
                                tessellate_ellipse_stroke(
                                    local_r,
                                    rect,
                                    *width,
                                    *cap,
                                    *join,
                                    *miter,
                                    path_effect.as_ref(),
                                )
                            }) {
                                scene.nodes.push(SceneNode::VectorMesh {
                                    mesh,
                                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                    paint: brush_to_mesh_paint(
                                        &raw_fill,
                                        Vec2 {
                                            x: rect.x + local_r.x,
                                            y: rect.y + local_r.y,
                                        },
                                        Vec2 {
                                            x: local_r.w,
                                            y: local_r.h,
                                        },
                                        alpha,
                                    ),
                                    clip: None,
                                    blend: BlendMode::Alpha,
                                });
                            } else {
                                scene.nodes.push(SceneNode::EllipseBorder {
                                    rect: r,
                                    brush: fill,
                                    width: *width,
                                });
                            }
                        }
                    }
                }
                DrawCommand::LinePath {
                    points,
                    brush,
                    width,
                    cap,
                    join,
                    miter,
                    path_effect,
                } => {
                    if points.len() < 2 {
                        continue;
                    }
                    let raw_brush = *brush;
                    let cache_key = polyline_cache_key(
                        points,
                        rect,
                        *width,
                        *cap,
                        *join,
                        *miter,
                        path_effect.as_ref(),
                    );
                    let mesh = cached_tessellation(cache_key, || {
                        tessellate_polyline(
                            points,
                            rect,
                            *width,
                            *cap,
                            *join,
                            *miter,
                            path_effect.as_ref(),
                        )
                    });
                    let Some(mesh) = mesh else { continue };
                    scene.nodes.push(SceneNode::VectorMesh {
                        mesh,
                        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        paint: brush_to_mesh_paint(
                            &raw_brush,
                            Vec2 {
                                x: rect.x,
                                y: rect.y,
                            },
                            brush_size,
                            alpha,
                        ),
                        clip: None,
                        blend: BlendMode::Alpha,
                    });
                }
                DrawCommand::Arc {
                    rect: r,
                    start_angle,
                    sweep_angle,
                    use_center,
                    brush,
                    width,
                    cap,
                } => {
                    let local_r = *r;
                    let raw_brush = *brush;
                    let fill = alpha_brush(raw_brush, alpha);
                    if *use_center {
                        let cache_key = arc_cache_key(local_r, rect, *start_angle, *sweep_angle);
                        let mesh = cached_tessellation(cache_key, || {
                            tessellate_arc_wedge(local_r, rect, *start_angle, *sweep_angle)
                        });
                        let Some(mesh) = mesh else { continue };
                        scene.nodes.push(SceneNode::VectorMesh {
                            mesh,
                            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            paint: brush_to_mesh_paint(
                                &raw_brush,
                                Vec2 {
                                    x: rect.x + local_r.x,
                                    y: rect.y + local_r.y,
                                },
                                Vec2 {
                                    x: local_r.w,
                                    y: local_r.h,
                                },
                                alpha,
                            ),
                            clip: None,
                            blend: BlendMode::Alpha,
                        });
                    } else {
                        scene.nodes.push(SceneNode::Arc {
                            rect: to_global(local_r),
                            start_angle: *start_angle,
                            sweep_angle: *sweep_angle,
                            stroke_width: *width,
                            brush: fill,
                            cap: *cap,
                        });
                    }
                }
                DrawCommand::Text {
                    text,
                    pos,
                    color,
                    size,
                    font_family,
                } => {
                    scene.nodes.push(SceneNode::Text {
                        rect: Rect {
                            x: rect.x + pos.x,
                            y: rect.y + pos.y,
                            w: 0.0,
                            h: size.0,
                        },
                        text: Arc::<str>::from(text.clone()),
                        color: alpha_color(*color, alpha),
                        size: *size,
                        font_family: *font_family,
                        text_align: TextAlign::Unspecified,
                        font_weight: FontWeight::NORMAL,
                        font_style: FontStyle::Normal,
                        text_decoration: TextDecoration::default(),
                        letter_spacing: Px::ZERO,
                        line_height: Px::ZERO,
                        extra_style: Default::default(),
                        url: None,
                        font_variation_settings: None,
                    });
                }
                DrawCommand::VectorMesh {
                    mesh,
                    transform,
                    paint,
                    clip,
                    blend,
                } => {
                    scene.nodes.push(SceneNode::VectorMesh {
                        mesh: mapped_mesh(mesh, 0.0, 0.0, alpha),
                        transform: [
                            transform[0],
                            transform[1],
                            transform[2],
                            transform[3],
                            transform[4] + rect.x,
                            transform[5] + rect.y,
                        ],
                        paint: alpha_paint(*paint, alpha),
                        clip: *clip,
                        blend: *blend,
                    });
                }
                DrawCommand::VectorOverlay { meshes } => {
                    let mapped = map_overlay_cached(meshes, rect.x, rect.y, alpha);
                    scene
                        .nodes
                        .push(SceneNode::VectorOverlay { meshes: mapped });
                }
                DrawCommand::PushVectorClip { mesh, op } => {
                    scene.nodes.push(SceneNode::PushVectorClip {
                        mesh: mapped_mesh(mesh, rect.x, rect.y, alpha),
                        op: *op,
                    });
                }
                DrawCommand::PopVectorClip => {
                    scene.nodes.push(SceneNode::PopVectorClip);
                }
                DrawCommand::PushTransform { transform } => {
                    let mut transform = *transform;
                    let origin = Vec2 {
                        x: rect.x,
                        y: rect.y,
                    };
                    let linear = transform.linear();
                    transform.translate_x +=
                        origin.x - (linear[0] * origin.x + linear[1] * origin.y);
                    transform.translate_y +=
                        origin.y - (linear[2] * origin.x + linear[3] * origin.y);
                    transform.perspective[2] -=
                        transform.perspective[0] * origin.x + transform.perspective[1] * origin.y;
                    transform.origin_x = 0.0;
                    transform.origin_y = 0.0;

                    scene.nodes.push(SceneNode::PushTransform { transform });
                }
                DrawCommand::PopTransform => {
                    scene.nodes.push(SceneNode::PopTransform);
                }
                DrawCommand::Image {
                    rect: r,
                    handle,
                    tint,
                    fit,
                    filter,
                    source_rect,
                } => {
                    scene.nodes.push(SceneNode::Image {
                        rect: repose_core::Rect {
                            x: r.x + rect.x,
                            y: r.y + rect.y,
                            w: r.w,
                            h: r.h,
                        },
                        handle: *handle,
                        tint: alpha_color(*tint, alpha),
                        fit: *fit,
                        filter: *filter,
                        source_rect: *source_rect,
                    });
                }
            }
        }
    };

    let mut m = modifier.painter(painter);
    let has_size = m.size.is_some()
        || m.width.is_some()
        || m.height.is_some()
        || m.fill_max.is_some()
        || m.fill_max_w.is_some()
        || m.fill_max_h.is_some();
    if !has_size {
        m = m.size(Dp(100.0), Dp(100.0));
    }

    Box(m)
}

/// Low-level `Embedded` - prefers `repose_render_wgpu::Callback::new` for payload.
/// Idiomatic `repose` (signal snapshot): `let payload = { let a=*angle.get(); Callback::new(MyTriangle{angle:a}) }; Embedded(modifier,payload)`.
/// `Callback::embedded_view(modifier, cb)` hides `Arc<dyn Any>`. `Canvas` is for 2D `DrawScope`, `Embedded` for raw `wgpu`.
pub fn Embedded(modifier: Modifier, payload: PaintCallbackPayload) -> View {
    let mut m = modifier.paint_callback(payload);
    let has_size = m.size.is_some()
        || m.width.is_some()
        || m.height.is_some()
        || m.fill_max.is_some()
        || m.fill_max_w.is_some()
        || m.fill_max_h.is_some();
    if !has_size {
        m = m.size(Dp(100.0), Dp(100.0));
    }
    Box(m)
}

/// Alias for `Embedded` (for egui-like naming)
pub fn PaintCallbackView(modifier: Modifier, payload: PaintCallbackPayload) -> View {
    Embedded(modifier, payload)
}
