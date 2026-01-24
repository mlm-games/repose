use crate::{Brush, Color, Modifier, Rect, Transform};
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
    },
    ScrollXY {
        on_scroll: Option<ScrollCallback>,
        set_viewport_width: Option<Rc<dyn Fn(f32)>>,
        set_viewport_height: Option<Rc<dyn Fn(f32)>>,
        set_content_width: Option<Rc<dyn Fn(f32)>>,
        set_content_height: Option<Rc<dyn Fn(f32)>>,
        get_scroll_offset_xy: Option<Rc<dyn Fn() -> (f32, f32)>>,
        set_scroll_offset_xy: Option<Rc<dyn Fn(f32, f32)>>,
    },
    Text {
        text: String,
        color: Color,
        font_size: f32,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
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
            ViewKind::Checkbox { checked, .. } => f
                .debug_struct("Checkbox")
                .field("checked", checked)
                .finish(),
            ViewKind::RadioButton { selected, .. } => f
                .debug_struct("RadioButton")
                .field("selected", selected)
                .finish(),
            ViewKind::Switch { checked, .. } => {
                f.debug_struct("Switch").field("checked", checked).finish()
            }
            ViewKind::Slider { value, .. } => {
                f.debug_struct("Slider").field("value", value).finish()
            }
            ViewKind::RangeSlider { start, end, .. } => f
                .debug_struct("RangeSlider")
                .field("start", start)
                .field("end", end)
                .finish(),
            ViewKind::ProgressBar { value, .. } => {
                f.debug_struct("ProgressBar").field("value", value).finish()
            }
            ViewKind::ScrollV { .. } => f.debug_struct("ScrollV").finish(),
            ViewKind::ScrollXY { .. } => f.debug_struct("ScrollXY").finish(),
            ViewKind::Text { text, .. } => f.debug_struct("Text").field("text", text).finish(),
            ViewKind::Button { .. } => f.debug_struct("Button").finish(),
            ViewKind::TextField { hint, .. } => {
                f.debug_struct("TextField").field("hint", hint).finish()
            }
            ViewKind::Surface => f.debug_struct("Surface").finish(),
            ViewKind::Box => f.debug_struct("Box").finish(),
            ViewKind::Row => f.debug_struct("Row").finish(),
            ViewKind::Column => f.debug_struct("Column").finish(),
            ViewKind::Stack => f.debug_struct("Stack").finish(),
            ViewKind::OverlayHost => f.debug_struct("OverlayHost").finish(),
            ViewKind::Image { .. } => f.debug_struct("Image").finish(),
            ViewKind::Ellipse { .. } => f.debug_struct("Ellipse").finish(),
            ViewKind::EllipseBorder { .. } => f.debug_struct("EllipseBorder").finish(),
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
}

pub type CallbackF32 = Rc<dyn Fn(f32)>;
pub type CallbackRange = Rc<dyn Fn(f32, f32)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOverflow {
    Visible,
    Clip,
    Ellipsis,
}
