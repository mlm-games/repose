use crate::{Color, Modifier, Rect};
use std::rc::Rc;

pub type ViewId = u64;

pub type Callback = Rc<dyn Fn()>;

#[derive(Clone, Debug)]
pub enum ViewKind {
    Surface,
    Box,
    Row,
    Column,
    Stack,
    ScrollV,
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
    },
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
}

// impl View {
//     pub fn child(self, children: impl IntoChildren) -> Self {
//         self.with_children(children.into_children())
//     }
// }

// pub trait IntoChildren {
//     fn into_children(self) -> Vec<View>;
// }

// impl IntoChildren for View {
//     fn into_children(self) -> Vec<View> {
//         vec![self]
//     }
// }

// impl<const N: usize> IntoChildren for [View; N] {
//     fn into_children(self) -> Vec<View> {
//         self.into()
//     }
// }

// impl IntoChildren for Vec<View> {
//     fn into_children(self) -> Vec<View> {
//         self
//     }
// }

// // Support tuple nesting like Compose's Column { Text(); Button() }
// macro_rules! impl_into_children_tuple {
//     ($($t:ident),+) => {
//         impl<$($t: IntoChildren),+> IntoChildren for ($($t,)+) {
//             fn into_children(self) -> Vec<View> {
//                 let ($($t,)+) = self;
//                 let mut v = Vec::new();
//                 $(v.extend($t.into_children());)+
//                 v
//             }
//         }
//     };
// }

// impl_into_children_tuple!(A, B);
// impl_into_children_tuple!(A, B, C);
// impl_into_children_tuple!(A, B, C, D);
// impl_into_children_tuple!(A, B, C, D, E);
