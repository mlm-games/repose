use std::rc::Rc;

use taffy::{AlignContent, AlignItems, AlignSelf, FlexDirection, FlexWrap, JustifyContent};

use crate::{Brush, Color, PointerEvent, Size, Transform, Vec2};

#[derive(Clone, Debug)]
pub struct Border {
    pub width: f32,
    pub color: Color,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaddingValues {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
pub struct GridConfig {
    pub columns: usize,
    pub row_gap: f32,
    pub column_gap: f32,
}

#[derive(Clone, Copy, Debug)]
pub enum PositionType {
    Relative,
    Absolute,
}

#[derive(Clone, Default)]
pub struct Modifier {
    pub size: Option<Size>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub fill_max: bool,
    pub fill_max_w: bool,
    pub fill_max_h: bool,
    pub padding: Option<f32>,
    pub padding_values: Option<PaddingValues>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub background: Option<Brush>,
    pub border: Option<Border>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<f32>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_dir: Option<FlexDirection>,
    pub align_self: Option<AlignSelf>,
    pub justify_content: Option<JustifyContent>,
    pub align_items_container: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub clip_rounded: Option<f32>,
    /// Works for hit-testing only, draw order is not changed.
    pub z_index: f32,
    pub click: bool,
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    pub on_pointer_down: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(PointerEvent)>>,
    pub semantics: Option<crate::Semantics>,
    pub alpha: Option<f32>,
    pub transform: Option<Transform>,
    pub grid: Option<GridConfig>,
    pub grid_col_span: Option<u16>,
    pub grid_row_span: Option<u16>,
    pub position_type: Option<PositionType>,
    pub offset_left: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_top: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub painter: Option<Rc<dyn Fn(&mut crate::Scene, crate::Rect)>>,
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modifier")
            .field("size", &self.size)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("fill_max", &self.fill_max)
            .field("fill_max_w", &self.fill_max_w)
            .field("fill_max_h", &self.fill_max_h)
            .field("padding", &self.padding)
            .field("padding_values", &self.padding_values)
            .field("min_width", &self.min_width)
            .field("min_height", &self.min_height)
            .field("max_width", &self.max_width)
            .field("max_height", &self.max_height)
            .field("background", &self.background)
            .field("border", &self.border)
            .field("flex_grow", &self.flex_grow)
            .field("flex_shrink", &self.flex_shrink)
            .field("flex_basis", &self.flex_basis)
            .field("align_self", &self.align_self)
            .field("justify_content", &self.justify_content)
            .field("align_items_container", &self.align_items_container)
            .field("align_content", &self.align_content)
            .field("clip_rounded", &self.clip_rounded)
            .field("z_index", &self.z_index)
            .field("click", &self.click)
            .field("on_scroll", &self.on_scroll.as_ref().map(|_| "..."))
            .field(
                "on_pointer_down",
                &self.on_pointer_down.as_ref().map(|_| "..."),
            )
            .field(
                "on_pointer_move",
                &self.on_pointer_move.as_ref().map(|_| "..."),
            )
            .field("on_pointer_up", &self.on_pointer_up.as_ref().map(|_| "..."))
            .field(
                "on_pointer_enter",
                &self.on_pointer_enter.as_ref().map(|_| "..."),
            )
            .field(
                "on_pointer_leave",
                &self.on_pointer_leave.as_ref().map(|_| "..."),
            )
            .field("semantics", &self.semantics)
            .field("alpha", &self.alpha)
            .field("transform", &self.transform)
            .field("grid", &self.grid)
            .field("grid_col_span", &self.grid_col_span)
            .field("grid_row_span", &self.grid_row_span)
            .field("position_type", &self.position_type)
            .field("offset_left", &self.offset_left)
            .field("offset_right", &self.offset_right)
            .field("offset_top", &self.offset_top)
            .field("offset_bottom", &self.offset_bottom)
            .field("aspect_ratio", &self.aspect_ratio)
            .field("painter", &self.painter.as_ref().map(|_| "..."))
            .finish()
    }
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some(Size {
            width: w,
            height: h,
        });
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
    pub fn fill_max_size(mut self) -> Self {
        self.fill_max = true;
        self
    }
    pub fn fill_max_width(mut self) -> Self {
        self.fill_max_w = true;
        self
    }
    pub fn fill_max_height(mut self) -> Self {
        self.fill_max_h = true;
        self
    }
    pub fn padding(mut self, v: f32) -> Self {
        self.padding = Some(v);
        self
    }
    pub fn padding_values(mut self, padding: PaddingValues) -> Self {
        self.padding_values = Some(padding);
        self
    }
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_width = Some(w);
        self.min_height = Some(h);
        self
    }
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_width = Some(w);
        self.max_height = Some(h);
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w);
        self
    }
    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = Some(h);
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }
    /// Set a solid color background.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }
    /// Set a brush (solid, gradient, etc.) background.
    pub fn background_brush(mut self, brush: Brush) -> Self {
        self.background = Some(brush);
        self
    }
    pub fn border(mut self, width: f32, color: Color, radius: f32) -> Self {
        self.border = Some(Border {
            width,
            color,
            radius,
        });
        self
    }
    pub fn flex_grow(mut self, v: f32) -> Self {
        self.flex_grow = Some(v);
        self
    }
    pub fn flex_shrink(mut self, v: f32) -> Self {
        self.flex_shrink = Some(v);
        self
    }
    pub fn flex_basis(mut self, v: f32) -> Self {
        self.flex_basis = Some(v);
        self
    }
    pub fn flex_wrap(mut self, w: FlexWrap) -> Self {
        self.flex_wrap = Some(w);
        self
    }
    pub fn flex_dir(mut self, d: FlexDirection) -> Self {
        self.flex_dir = Some(d);
        self
    }
    pub fn align_self(mut self, a: AlignSelf) -> Self {
        self.align_self = Some(a);
        self
    }
    pub fn align_self_center(mut self) -> Self {
        self.align_self = Some(AlignSelf::Center);
        self
    }
    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.justify_content = Some(j);
        self
    }
    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.align_items_container = Some(a);
        self
    }
    pub fn align_content(mut self, a: AlignContent) -> Self {
        self.align_content = Some(a);
        self
    }
    pub fn clip_rounded(mut self, radius: f32) -> Self {
        self.clip_rounded = Some(radius);
        self
    }
    pub fn z_index(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }
    pub fn clickable(mut self) -> Self {
        self.click = true;
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_scroll = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_down(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_down = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_move(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_move = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_up(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_up = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_enter(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_enter = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_leave(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_leave = Some(Rc::new(f));
        self
    }
    pub fn semantics(mut self, s: crate::Semantics) -> Self {
        self.semantics = Some(s);
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = Some(a);
        self
    }
    pub fn transform(mut self, t: Transform) -> Self {
        self.transform = Some(t);
        self
    }
    pub fn grid(mut self, columns: usize, row_gap: f32, column_gap: f32) -> Self {
        self.grid = Some(GridConfig {
            columns,
            row_gap,
            column_gap,
        });
        self
    }
    pub fn grid_span(mut self, col_span: u16, row_span: u16) -> Self {
        self.grid_col_span = Some(col_span);
        self.grid_row_span = Some(row_span);
        self
    }
    pub fn absolute(mut self) -> Self {
        self.position_type = Some(PositionType::Absolute);
        self
    }
    pub fn offset(
        mut self,
        left: Option<f32>,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
    ) -> Self {
        self.offset_left = left;
        self.offset_top = top;
        self.offset_right = right;
        self.offset_bottom = bottom;
        self
    }
    pub fn offset_left(mut self, v: f32) -> Self {
        self.offset_left = Some(v);
        self
    }
    pub fn offset_right(mut self, v: f32) -> Self {
        self.offset_right = Some(v);
        self
    }
    pub fn offset_top(mut self, v: f32) -> Self {
        self.offset_top = Some(v);
        self
    }
    pub fn offset_bottom(mut self, v: f32) -> Self {
        self.offset_bottom = Some(v);
        self
    }

    pub fn margin(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }

    pub fn margin_horizontal(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self
    }

    pub fn margin_vertical(mut self, v: f32) -> Self {
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }
    pub fn painter(mut self, f: impl Fn(&mut crate::Scene, crate::Rect) + 'static) -> Self {
        self.painter = Some(Rc::new(f));
        self
    }
    pub fn scale(self, s: f32) -> Self {
        self.scale2(s, s)
    }
    pub fn scale2(mut self, sx: f32, sy: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.scale_x *= sx;
        t.scale_y *= sy;
        self.transform = Some(t);
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        let t = self.transform.unwrap_or_else(Transform::identity);
        self.transform = Some(t.combine(&Transform::translate(x, y)));
        self
    }
    pub fn rotate(mut self, radians: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.rotate += radians;
        self.transform = Some(t);
        self
    }
}
