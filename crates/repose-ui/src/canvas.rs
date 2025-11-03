use repose_core::*;

pub struct DrawScope {
    commands: Vec<DrawCommand>,
    size: Size,
}

pub enum DrawCommand {
    DrawRect {
        rect: Rect,
        paint: Paint,
    },
    DrawCircle {
        center: Vec2,
        radius: f32,
        paint: Paint,
    },
    DrawLine {
        start: Vec2,
        end: Vec2,
        paint: Paint,
    },
    DrawPath {
        path: Path,
        paint: Paint,
    },
    DrawText {
        text: String,
        pos: Vec2,
        paint: Paint,
    },
    ClipRect {
        rect: Rect,
    },
    Transform {
        transform: Transform,
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
    pub fn draw_rect(&mut self, rect: Rect, paint: Paint) {
        self.commands.push(DrawCommand::DrawRect { rect, paint });
    }

    pub fn draw_circle(&mut self, center: Vec2, radius: f32, paint: Paint) {
        self.commands.push(DrawCommand::DrawCircle {
            center,
            radius,
            paint,
        });
    }

    pub fn draw_line(&mut self, start: Vec2, end: Vec2, paint: Paint) {
        self.commands
            .push(DrawCommand::DrawLine { start, end, paint });
    }
}

pub fn Canvas(modifier: Modifier, on_draw: impl Fn(&mut DrawScope) + 'static) -> View {
    // Converts DrawScope commands to SceneNodes
    let mut scope = DrawScope {
        commands: Vec::new(),
        size: Size {
            width: 100.0,
            height: 100.0,
        },
    };

    on_draw(&mut scope);

    /// Use to transform to scene nodes
    View::new(0, ViewKind::Box).modifier(modifier)
}
