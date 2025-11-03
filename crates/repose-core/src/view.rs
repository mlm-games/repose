use crate::{Color, Modifier, Rect, Transform};
use std::rc::Rc;

pub type ViewId = u64;

pub type Callback = Rc<dyn Fn()>;
pub type ScrollCallback = Rc<dyn Fn(f32) -> f32>;

#[derive(Clone)]
pub enum ViewKind {
    Surface,
    Box,
    Row,
    Column,
    Stack,
    ScrollV {
        on_scroll: Option<ScrollCallback>,
        set_viewport_height: Option<Rc<dyn Fn(f32)>>,
        get_scroll_offset: Option<Rc<dyn Fn() -> f32>>,
    },
    Text {
        text: String,
        color: Color,
        font_size: f32,
    },
    Button {
        text: String,
        on_click: Option<Callback>,
    },
    TextField {
        state_key: ViewId,
        hint: String,
        on_change: Option<Rc<dyn Fn(String)>>,
        on_submit: Option<Rc<dyn Fn(String)>>,
    },
    Checkbox {
        checked: bool,
        label: String,
        on_change: Option<Rc<dyn Fn(bool)>>,
    },
    RadioButton {
        selected: bool,
        label: String,
        on_select: Option<Callback>,
    },
    Switch {
        checked: bool,
        label: String,
        on_change: Option<Rc<dyn Fn(bool)>>,
    },
    Slider {
        value: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        label: String,
        on_change: Option<CallbackF32>,
    },
    RangeSlider {
        start: f32,
        end: f32,
        min: f32,
        max: f32,
        step: Option<f32>,
        label: String,
        on_change: Option<CallbackRange>,
    },
    ProgressBar {
        value: f32,
        min: f32,
        max: f32,
        label: String,
        circular: bool,
    },
}

impl std::fmt::Debug for ViewKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewKind::Checkbox { checked, label, .. } => f
                .debug_struct("Checkbox")
                .field("checked", checked)
                .field("label", label)
                .finish(),
            ViewKind::RadioButton {
                selected, label, ..
            } => f
                .debug_struct("RadioButton")
                .field("selected", selected)
                .field("label", label)
                .finish(),
            ViewKind::Switch { checked, label, .. } => f
                .debug_struct("Switch")
                .field("checked", checked)
                .field("label", label)
                .finish(),
            ViewKind::Surface => write!(f, "Surface"),
            ViewKind::Box => write!(f, "Box"),
            ViewKind::Row => write!(f, "Row"),
            ViewKind::Column => write!(f, "Column"),
            ViewKind::Stack => write!(f, "Stack"),
            ViewKind::ScrollV { .. } => write!(f, "ScrollV"),
            ViewKind::Text {
                text,
                color,
                font_size,
            } => f
                .debug_struct("Text")
                .field("text", text)
                .field("color", color)
                .field("font_size", font_size)
                .finish(),
            ViewKind::Button { text, .. } => f
                .debug_struct("Button")
                .field("text", text)
                .field("on_click", &"<callback>")
                .finish(),
            ViewKind::TextField {
                state_key,
                hint,
                on_change,
                on_submit,
            } => f
                .debug_struct("TextField")
                .field("state_key", state_key)
                .field("hint", hint)
                .finish(),
            ViewKind::Slider {
                value,
                min,
                max,
                step,
                label,
                ..
            } => f
                .debug_struct("Slider")
                .field("value", value)
                .field("min", min)
                .field("max", max)
                .field("step", step)
                .field("label", label)
                .finish(),
            ViewKind::RangeSlider {
                start,
                end,
                min,
                max,
                step,
                label,
                ..
            } => f
                .debug_struct("RangeSlider")
                .field("start", start)
                .field("end", end)
                .field("min", min)
                .field("max", max)
                .field("step", step)
                .field("label", label)
                .finish(),
            ViewKind::ProgressBar {
                value,
                min,
                max,
                label,
                circular,
            } => f
                .debug_struct("ProgressBar")
                .field("value", value)
                .field("min", min)
                .field("max", max)
                .field("label", label)
                .field("circular", circular)
                .finish(),
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
        color: Color,
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
        text: String,
        color: Color,
        size: f32,
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
}

pub type CallbackF32 = Rc<dyn Fn(f32)>;
pub type CallbackRange = Rc<dyn Fn(f32, f32)>;
