#![allow(non_snake_case)]
use std::sync::Arc;

use repose_core::*;
use repose_ui::*;

pub struct DrawScope {
    pub commands: Vec<DrawCommand>,
    pub size: Size,
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
    /// Screen-space overlays drawn in final device pixels, unaffected by the
    /// world transform.
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
    },
}

impl DrawScope {
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
    /// the rect's local space: `(0,0)` is the rect top-left, so
    /// `LinearGradient::vertical` spans the rect height.
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
    pub fn draw_vector_mesh(
        &mut self,
        mesh: Arc<VectorMeshData>,
        transform: [f32; 6],
        paint: PaintDesc,
    ) {
        self.commands.push(DrawCommand::VectorMesh {
            mesh,
            transform,
            paint,
            clip: None,
            blend: BlendMode::Alpha,
        });
    }

    /// Draw a screen-space overlay mesh in final device pixels.
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

fn translate_mesh_data(m: &VectorMeshData, dx: f32, dy: f32) -> VectorMeshData {
    let vertices: Arc<[VectorVertex]> = m
        .vertices
        .iter()
        .map(|v| VectorVertex {
            pos: [v.pos[0] + dx, v.pos[1] + dy],
            ..*v
        })
        .collect();
    VectorMeshData {
        vertices,
        indices: m.indices.clone(),
    }
}

fn brush_to_paint(brush: &Brush) -> PaintDesc {
    match brush {
        Brush::Solid(_) => PaintDesc::Solid,
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => PaintDesc::Linear {
            start: *start,
            end: *end,
            start_color: *start_color,
            end_color: *end_color,
        },
        Brush::Radial {
            center,
            radius,
            start_color,
            end_color,
        } => PaintDesc::Radial {
            center: *center,
            radius: *radius,
            start_color: *start_color,
            end_color: *end_color,
        },
        Brush::Sweep {
            center,
            start_color,
            end_color,
        } => PaintDesc::Sweep {
            center: *center,
            start_color: *start_color,
            end_color: *end_color,
        },
        _ => PaintDesc::Solid,
    }
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
    if buffers.indices.is_empty() {
        return None;
    }
    let vertices: Arc<[VectorVertex]> = buffers
        .indices
        .iter()
        .map(|&i| {
            let v = &buffers.vertices[i as usize];
            VectorVertex {
                pos: [v.x, v.y],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
            }
        })
        .collect();
    let indices: Arc<[u32]> = (0..vertices.len() as u32).collect();
    Some(VectorMeshData { vertices, indices })
}

fn apply_canvas_path_effect(path: &lyon_path::Path, effect: &PathEffect) -> lyon_path::Path {
    use lyon_path::PathEvent;
    use lyon_path::iterator::PathIterator;
    match effect {
        PathEffect::Corner { .. } => path.clone(),
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
    if buffers.indices.is_empty() {
        return None;
    }
    let vertices: Arc<[VectorVertex]> = buffers
        .indices
        .iter()
        .map(|&i| {
            let v = &buffers.vertices[i as usize];
            VectorVertex {
                pos: [v.x, v.y],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
            }
        })
        .collect();
    let indices: Arc<[u32]> = (0..vertices.len() as u32).collect();
    Some(VectorMeshData { vertices, indices })
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
    builder.close();
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
    if buffers.indices.is_empty() {
        return None;
    }
    let vertices: Arc<[VectorVertex]> = buffers
        .indices
        .iter()
        .map(|&i| {
            let v = &buffers.vertices[i as usize];
            VectorVertex {
                pos: [v.x, v.y],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
            }
        })
        .collect();
    let indices: Arc<[u32]> = (0..vertices.len() as u32).collect();
    Some(VectorMeshData { vertices, indices })
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
    if buffers.indices.is_empty() {
        return None;
    }
    let vertices: Arc<[VectorVertex]> = buffers
        .indices
        .iter()
        .map(|&i| {
            let v = &buffers.vertices[i as usize];
            VectorVertex {
                pos: [v.x, v.y],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
            }
        })
        .collect();
    let indices: Arc<[u32]> = (0..vertices.len() as u32).collect();
    Some(VectorMeshData { vertices, indices })
}

pub fn Canvas(modifier: Modifier, on_draw: impl Fn(&mut DrawScope) + 'static) -> View {
    let painter = move |scene: &mut Scene, rect: Rect, _alpha: f32| {
        let mut scope = DrawScope {
            commands: Vec::new(),
            size: Size {
                width: rect.w.max(0.0),
                height: rect.h.max(0.0),
            },
        };
        on_draw(&mut scope);

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
                    let r = to_global(*r);
                    match style {
                        ShapeStyle::Fill => {
                            scene.nodes.push(SceneNode::Rect {
                                rect: r,
                                brush: *fill,
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
                            if let Some(mesh) = tessellate_rounded_rect_stroke(
                                r,
                                *radius,
                                rect,
                                *width,
                                *cap,
                                *join,
                                *miter,
                                path_effect.as_ref(),
                            ) {
                                scene.nodes.push(SceneNode::VectorMesh {
                                    mesh: Arc::new(mesh),
                                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                    paint: brush_to_paint(fill),
                                    clip: None,
                                    blend: BlendMode::Alpha,
                                });
                            } else {
                                scene.nodes.push(SceneNode::Border {
                                    rect: r,
                                    brush: *fill,
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
                    let r = Rect {
                        x: center.x - *rx,
                        y: center.y - *ry,
                        w: 2.0 * *rx,
                        h: 2.0 * *ry,
                    };
                    let r = to_global(r);
                    match style {
                        ShapeStyle::Fill => {
                            scene.nodes.push(SceneNode::Ellipse {
                                rect: r,
                                brush: *fill,
                            });
                        }
                        ShapeStyle::Stroke {
                            width,
                            cap,
                            join,
                            miter,
                            path_effect,
                        } => {
                            if let Some(mesh) = tessellate_ellipse_stroke(
                                r,
                                rect,
                                *width,
                                *cap,
                                *join,
                                *miter,
                                path_effect.as_ref(),
                            ) {
                                scene.nodes.push(SceneNode::VectorMesh {
                                    mesh: Arc::new(mesh),
                                    transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                                    paint: brush_to_paint(fill),
                                    clip: None,
                                    blend: BlendMode::Alpha,
                                });
                            } else {
                                scene.nodes.push(SceneNode::EllipseBorder {
                                    rect: r,
                                    brush: *fill,
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
                    let mesh = tessellate_polyline(
                        points,
                        rect,
                        *width,
                        *cap,
                        *join,
                        *miter,
                        path_effect.as_ref(),
                    );
                    let Some(mesh) = mesh else { continue };
                    scene.nodes.push(SceneNode::VectorMesh {
                        mesh: Arc::new(mesh),
                        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                        paint: brush_to_paint(brush),
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
                    if *use_center {
                        let mesh = tessellate_arc_wedge(*r, rect, *start_angle, *sweep_angle);
                        let Some(mesh) = mesh else { continue };
                        scene.nodes.push(SceneNode::VectorMesh {
                            mesh: Arc::new(mesh),
                            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                            paint: brush_to_paint(brush),
                            clip: None,
                            blend: BlendMode::Alpha,
                        });
                    } else {
                        scene.nodes.push(SceneNode::Arc {
                            rect: to_global(*r),
                            start_angle: *start_angle,
                            sweep_angle: *sweep_angle,
                            stroke_width: *width,
                            brush: *brush,
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
                        color: *color,
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
                        mesh: mesh.clone(),
                        transform: [
                            transform[0],
                            transform[1],
                            transform[2],
                            transform[3],
                            transform[4] + rect.x,
                            transform[5] + rect.y,
                        ],
                        paint: *paint,
                        clip: *clip,
                        blend: *blend,
                    });
                }
                DrawCommand::VectorOverlay { meshes } => {
                    let translated: Vec<VectorMeshData> = meshes
                        .iter()
                        .map(|m| translate_mesh_data(m, rect.x, rect.y))
                        .collect();
                    scene.nodes.push(SceneNode::VectorOverlay {
                        meshes: translated.into(),
                    });
                }
                DrawCommand::PushVectorClip { mesh, op } => {
                    scene.nodes.push(SceneNode::PushVectorClip {
                        mesh: Arc::new(translate_mesh_data(mesh, rect.x, rect.y)),
                        op: *op,
                    });
                }
                DrawCommand::PopVectorClip => {
                    scene.nodes.push(SceneNode::PopVectorClip);
                }
                DrawCommand::PushTransform { transform } => {
                    let mut transform = *transform;

                    // Canvas local -> window global.
                    transform.translate_x += rect.x;
                    transform.translate_y += rect.y;

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
                } => {
                    scene.nodes.push(SceneNode::Image {
                        rect: repose_core::Rect {
                            x: r.x + rect.x,
                            y: r.y + rect.y,
                            w: r.w,
                            h: r.h,
                        },
                        handle: *handle,
                        tint: *tint,
                        fit: *fit,
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
