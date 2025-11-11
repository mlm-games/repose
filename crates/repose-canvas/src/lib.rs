#![allow(non_snake_case)]
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
}

pub struct Paint {
    pub color: Color,
    pub stroke_width: Option<f32>,
    pub style: PaintStyle,
}

pub enum PaintStyle {
    Fill,
    Stroke,
}

pub struct Path {
    segments: Vec<PathSegment>,
}

pub enum PathSegment {
    MoveTo(Vec2),
    LineTo(Vec2),
    QuadTo(Vec2, Vec2),
    CubicTo(Vec2, Vec2, Vec2),
    Close,
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
}

pub fn Canvas(modifier: Modifier, on_draw: impl Fn(&mut DrawScope) + 'static) -> View {
    // Record commands upfront; they are replayed during paint for the node's rect
    let mut scope = DrawScope {
        commands: Vec::new(),
        size: Size {
            width: 100.0,
            height: 100.0,
        },
    };
    on_draw(&mut scope);

    let painter_cmds = scope.commands.clone();
    let painter = move |scene: &mut Scene, rect: Rect| {
        // local->global helper
        let to_global = |r: Rect| Rect {
            x: rect.x + r.x,
            y: rect.y + r.y,
            w: r.w,
            h: r.h,
        };
        for cmd in &painter_cmds {
            match cmd {
                DrawCommand::Rect {
                    rect: r,
                    color,
                    radius,
                    stroke,
                } => {
                    scene.nodes.push(SceneNode::Rect {
                        rect: to_global(*r),
                        color: *color,
                        radius: *radius,
                    });
                    if let Some((w, c)) = stroke {
                        scene.nodes.push(SceneNode::Border {
                            rect: to_global(*r),
                            color: *c,
                            width: *w,
                            radius: *radius,
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
                        color: *color,
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
                        text: text.clone(),
                        color: *color,
                        size: *size,
                    });
                }
            }
        }
    };

    Box(modifier
        .painter(painter)
        .size(scope.size.width, scope.size.height))
}
