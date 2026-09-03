#![allow(non_snake_case)]
use std::sync::Arc;

use repose_core::*;
use repose_ui::*;

pub struct DrawScope {
    pub commands: Vec<DrawCommand>,
    pub size: Size,
}

#[derive(Clone)]
pub enum DrawCommand {
    Rect {
        rect: Rect,
        color: Color,
        radius: f32,
        stroke: Option<(f32, Color)>,
    },
    Ellipse {
        center: Vec2,
        rx: f32,
        ry: f32,
        color: Color,
        stroke: Option<(f32, Color)>,
    },
    Text {
        text: String,
        pos: Vec2,
        color: Color,
        size: f32,
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
    PushVectorClip { mesh: Arc<VectorMeshData> },
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
    pub fn draw_rect(&mut self, rect: Rect, color: Color, radius: f32) {
        self.commands.push(DrawCommand::Rect {
            rect,
            color,
            radius,
            stroke: None,
        });
    }
    pub fn draw_rect_stroke(&mut self, rect: Rect, color: Color, radius: f32, width: f32) {
        self.commands.push(DrawCommand::Rect {
            rect,
            color,
            radius,
            stroke: Some((width, color)),
        });
    }
    pub fn draw_ellipse(&mut self, center: Vec2, rx: f32, ry: f32, color: Color) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            color,
            stroke: None,
        });
    }
    pub fn draw_ellipse_stroke(
        &mut self,
        center: Vec2,
        rx: f32,
        ry: f32,
        color: Color,
        width: f32,
    ) {
        self.commands.push(DrawCommand::Ellipse {
            center,
            rx: rx.max(0.0),
            ry: ry.max(0.0),
            color,
            stroke: Some((width.max(0.0), color)),
        });
    }
    pub fn draw_circle(&mut self, center: Vec2, radius: f32, color: Color) {
        self.draw_ellipse(center, radius, radius, color);
    }
    pub fn draw_circle_stroke(&mut self, center: Vec2, radius: f32, color: Color, width: f32) {
        self.draw_ellipse_stroke(center, radius, radius, color, width);
    }
    pub fn draw_text(&mut self, text: impl Into<String>, pos: Vec2, color: Color, size: f32) {
        self.commands.push(DrawCommand::Text {
            text: text.into(),
            pos,
            color,
            size,
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
        self.commands.push(DrawCommand::PushVectorClip { mesh });
    }

    /// Pop the most recent vector clip.
    pub fn pop_vector_clip(&mut self) {
        self.commands.push(DrawCommand::PopVectorClip);
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

pub use repose_core::{PaintCallbackInfo, PaintCallbackPayload};

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
                    color,
                    radius,
                    stroke,
                } => {
                    scene.nodes.push(SceneNode::Rect {
                        rect: to_global(*r),
                        brush: Brush::Solid(*color),
                        radius: [*radius; 4],
                    });
                    if let Some((w, c)) = stroke {
                        scene.nodes.push(SceneNode::Border {
                            rect: to_global(*r),
                            color: *c,
                            width: *w,
                            radius: [*radius; 4],
                        });
                    }
                }
                DrawCommand::Ellipse {
                    center,
                    rx,
                    ry,
                    color,
                    stroke,
                } => {
                    let r = Rect {
                        x: center.x - *rx,
                        y: center.y - *ry,
                        w: 2.0 * *rx,
                        h: 2.0 * *ry,
                    };
                    scene.nodes.push(SceneNode::Ellipse {
                        rect: to_global(r),
                        brush: Brush::Solid(*color),
                    });
                    if let Some((w, c)) = stroke {
                        scene.nodes.push(SceneNode::EllipseBorder {
                            rect: to_global(r),
                            color: *c,
                            width: *w,
                        });
                    }
                }
                DrawCommand::Text {
                    text,
                    pos,
                    color,
                    size,
                } => {
                    scene.nodes.push(SceneNode::Text {
                        rect: Rect {
                            x: rect.x + pos.x,
                            y: rect.y + pos.y,
                            w: 0.0,
                            h: *size,
                        },
                        text: Arc::<str>::from(text.clone()),
                        color: *color,
                        size: *size,
                        font_family: None,
                        text_align: TextAlign::Unspecified,
                        font_weight: FontWeight::NORMAL,
                        font_style: FontStyle::Normal,
                        text_decoration: TextDecoration::default(),
                        letter_spacing: 0.0,
                        line_height: 0.0,
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
                DrawCommand::PushVectorClip { mesh } => {
                    scene.nodes.push(SceneNode::PushVectorClip {
                        mesh: Arc::new(translate_mesh_data(mesh, rect.x, rect.y)),
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
                        rect: *r,
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
        m = m.size(100.0, 100.0);
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
        m = m.size(100.0, 100.0);
    }
    Box(m)
}

/// Alias for `Embedded` (for egui-like naming)
pub fn PaintCallbackView(modifier: Modifier, payload: PaintCallbackPayload) -> View {
    Embedded(modifier, payload)
}
