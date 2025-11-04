use std::rc::Rc;

use taffy::AlignSelf;

use crate::{Color, PointerEvent, Size, Transform, Vec2};

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

#[derive(Clone, Default)]
pub struct Modifier {
    pub padding: Option<f32>,
    pub padding_values: Option<PaddingValues>,
    pub size: Option<Size>,
    pub fill_max: bool,
    pub fill_max_w: bool,
    pub fill_max_h: bool,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub background: Option<Color>,
    pub border: Option<Border>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<f32>,
    pub align_self: Option<AlignSelf>,
    pub aspect_ratio: Option<f32>, // width/height
    pub position_type: Option<PositionType>,
    pub offset_left: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_top: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub grid: Option<GridConfig>,
    pub grid_col_span: Option<u16>,
    pub grid_row_span: Option<u16>,
    pub click: bool,
    pub semantics_label: Option<String>,
    pub z_index: f32,
    pub clip_rounded: Option<f32>,
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,

    // Pointer callbacks
    pub on_pointer_down: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(PointerEvent)>>,

    pub alpha: Option<f32>,
    pub transform: Option<Transform>,

    pub painter: Option<Rc<dyn Fn(&mut crate::Scene, crate::Rect)>>,
}

#[derive(Clone, Copy, Debug)]
pub enum PositionType {
    Relative,
    Absolute,
}

#[derive(Clone, Copy, Debug)]
pub struct GridConfig {
    pub columns: usize, // e.g. 3 (auto 1fr tracks)
    pub row_gap: f32,
    pub column_gap: f32,
}

impl Default for Border {
    fn default() -> Self {
        Border {
            width: 1.0,
            color: Color::WHITE,
            radius: 0.0,
        }
    }
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Modifier")
            .field("padding", &self.padding)
            .field("padding_values", &self.padding_values)
            .field("size", &self.size)
            .field("fill_max", &self.fill_max)
            .field("background", &self.background)
            .field("border", &self.border)
            .field("click", &self.click)
            .field("semantics_label", &self.semantics_label)
            .field("z_index", &self.z_index)
            .field("clip_rounded", &self.clip_rounded)
            .field("alpha", &self.alpha)
            .finish()
    }
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn padding(mut self, px: f32) -> Self {
        self.padding = Some(px);
        self
    }
    pub fn padding_values(mut self, pv: PaddingValues) -> Self {
        self.padding_values = Some(pv);
        self
    }
    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some(Size {
            width: w,
            height: h,
        });
        self
    }
    pub fn fill_max_size(mut self) -> Self {
        self.fill_max = true;
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
    pub fn fill_max_width(mut self) -> Self {
        self.fill_max_w = true;
        self
    }
    pub fn fill_max_height(mut self) -> Self {
        self.fill_max_h = true;
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
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
    pub fn clickable(mut self) -> Self {
        self.click = true;
        self
    }
    pub fn semantics(mut self, label: impl Into<String>) -> Self {
        self.semantics_label = Some(label.into());
        self
    }
    pub fn z_index(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }
    pub fn clip_rounded(mut self, r: f32) -> Self {
        self.clip_rounded = Some(r);
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
    pub fn flex_grow(mut self, g: f32) -> Self {
        self.flex_grow = Some(g);
        self
    }
    pub fn flex_shrink(mut self, s: f32) -> Self {
        self.flex_shrink = Some(s);
        self
    }
    pub fn flex_basis(mut self, px: f32) -> Self {
        self.flex_basis = Some(px);
        self
    }
    pub fn align_self_baseline(mut self) -> Self {
        self.align_self = Some(taffy::style::AlignSelf::Baseline);
        self
    }
    pub fn align_self_center(mut self) -> Self {
        self.align_self = Some(taffy::style::AlignSelf::Center);
        self
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio.max(0.0));
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
    pub fn grid(mut self, columns: usize, row_gap: f32, column_gap: f32) -> Self {
        self.grid = Some(GridConfig {
            columns: columns.max(1),
            row_gap,
            column_gap,
        });
        self
    }
    pub fn grid_span(mut self, col_span: u16, row_span: u16) -> Self {
        self.grid_col_span = Some(col_span.max(1));
        self.grid_row_span = Some(row_span.max(1));
        self
    }

    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = Some(a.clamp(0.0, 1.0));
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        let t = self.transform.unwrap_or_else(Transform::identity);
        self.transform = Some(t.combine(&Transform::translate(x, y)));
        self
    }
    pub fn scale(mut self, s: f32) -> Self {
        self.scale2(s, s)
    }
    pub fn scale2(mut self, sx: f32, sy: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.scale_x *= sx;
        t.scale_y *= sy;
        self.transform = Some(t);
        self
    }
    pub fn rotate(mut self, radians: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.rotate += radians;
        self.transform = Some(t);
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_scroll = Some(Rc::new(f));
        self
    }

    pub fn painter(mut self, f: impl Fn(&mut crate::Scene, crate::Rect) + 'static) -> Self {
        self.painter = Some(Rc::new(f));
        self
    }
}
