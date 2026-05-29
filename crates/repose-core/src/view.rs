use crate::{Brush, Color, Modifier, Rect, Transform, Vec2};
use std::{rc::Rc, sync::Arc};

pub type ViewId = u64;

pub type ImageHandle = u64;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
pub enum ViewKind {
    Surface,
    Box,
    Row,
    Column,
    Stack,
    OverlayHost,
    ScrollV {
        on_scroll: Option<ScrollCallback>,
        set_viewport_height: Option<Rc<dyn Fn(f32)>>,
        set_content_height: Option<Rc<dyn Fn(f32)>>,
        get_scroll_offset: Option<Rc<dyn Fn() -> f32>>,
        set_scroll_offset: Option<Rc<dyn Fn(f32)>>,
        show_scrollbar: bool,
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
    },
    Text {
        text: String,
        color: Color,
        font_size: f32,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
        font_family: Option<&'static str>,
    },
    Button {
        on_click: Option<Callback>,
    },
    TextField {
        state_key: ViewId,
        hint: String,
        multiline: bool,
        on_change: Option<Rc<dyn Fn(String)>>,
        on_submit: Option<Rc<dyn Fn(String)>>,
    },
    Checkbox {
        checked: bool,
        on_change: Option<Rc<dyn Fn(bool)>>,
    },
    RadioButton {
        selected: bool,
        on_select: Option<Callback>,
    },
    Switch {
        checked: bool,
        on_change: Option<Rc<dyn Fn(bool)>>,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        on_change: Option<CallbackF32>,
    },
    RangeSlider {
        start: f32,
        end: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        on_change: Option<CallbackRange>,
    },
    ProgressBar {
        value: f32,
        min: f32,
        max: f32,
        circular: bool,
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
}

impl std::fmt::Debug for ViewKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Surface => f.write_str("Surface"),
            Self::Box => f.write_str("Box"),
            Self::Row => f.write_str("Row"),
            Self::Column => f.write_str("Column"),
            Self::Stack => f.write_str("Stack"),
            Self::OverlayHost => f.write_str("OverlayHost"),
            Self::ScrollV { .. } => f.write_str("ScrollV"),
            Self::ScrollXY { .. } => f.write_str("ScrollXY"),
            Self::Button { .. } => f.write_str("Button"),
            Self::Image { .. } => f.write_str("Image"),
            Self::Ellipse { .. } => f.write_str("Ellipse"),
            Self::EllipseBorder { .. } => f.write_str("EllipseBorder"),
            Self::Text { text, .. } => write!(f, "Text({:?})", text),
            Self::TextField { hint, .. } => write!(f, "TextField({:?})", hint),
            Self::Checkbox { checked, .. } => write!(f, "Checkbox({})", checked),
            Self::RadioButton { selected, .. } => write!(f, "Radio({})", selected),
            Self::Switch { checked, .. } => write!(f, "Switch({})", checked),
            Self::Slider { value, .. } => write!(f, "Slider({})", value),
            Self::RangeSlider { start, end, .. } => write!(f, "Range({}..{})", start, end),
            Self::ProgressBar { value, .. } => write!(f, "Progress({})", value),
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
}

pub type CallbackF32 = Rc<dyn Fn(f32)>;
pub type CallbackRange = Rc<dyn Fn(f32, f32)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOverflow {
    Visible,
    Clip,
    Ellipsis,
}
