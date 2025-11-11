use crate::anim_ext::AnimatedContent;
use crate::{Box, ViewExt};
use repose_core::*;
use std::any::Any;
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

pub struct NavController {
    stack: RefCell<VecDeque<NavEntry>>,
    pub current: Signal<String>,
    pub transitions: Signal<Option<Transition>>,
}

pub struct NavEntry {
    pub route: String,
    pub args: HashMap<String, String>,
    pub state: Box<dyn Any>,
}

#[derive(Clone)]
pub enum Transition {
    Push { from: String, to: String },
    Pop { from: String, to: String },
    Replace { from: String, to: String },
}

impl NavController {
    pub fn new(initial: impl Into<String>) -> Rc<Self> {
        let route = initial.into();
        Rc::new(Self {
            stack: RefCell::new({
                let mut dq = VecDeque::new();
                dq.push_back(NavEntry {
                    route: route.clone(),
                    args: HashMap::new(),
                    state: Box::new(()),
                });
                dq
            }),
            current: signal(route),
            transitions: signal(None),
        })
    }

    pub fn navigate(&self, route: impl Into<String>) {
        let route = route.into();
        let mut stack = self.stack.borrow_mut();
        let from = self.current.get();

        stack.push_back(NavEntry {
            route: route.clone(),
            args: HashMap::new(),
            state: Box::new(()),
        });

        self.transitions.set(Some(Transition::Push {
            from,
            to: route.clone(),
        }));
        self.current.set(route);
    }

    pub fn replace(&self, route: impl Into<String>) {
        let route = route.into();
        let mut stack = self.stack.borrow_mut();
        let from = self.current.get();

        if let Some(_last) = stack.pop_back() {
            // drop last
        }
        stack.push_back(NavEntry {
            route: route.clone(),
            args: HashMap::new(),
            state: Box::new(()),
        });
        self.transitions.set(Some(Transition::Replace {
            from,
            to: route.clone(),
        }));
        self.current.set(route);
    }

    pub fn pop(&self) -> bool {
        let mut stack = self.stack.borrow_mut();
        if stack.len() > 1 {
            let from = self.current.get();
            stack.pop_back();
            if let Some(entry) = stack.back() {
                self.transitions.set(Some(Transition::Pop {
                    from,
                    to: entry.route.clone(),
                }));
                self.current.set(entry.route.clone());
                return true;
            }
        }
        false
    }

    pub fn take_transition(&self) -> Option<Transition> {
        let t = self.transitions.get();
        self.transitions.set(None);
        t
    }
}

pub fn NavHost(
    controller: Rc<NavController>,
    routes: HashMap<String, Box<dyn Fn() -> View>>,
) -> View {
    let current = controller.current.get();
    let trans = controller.transitions.get();

    if let Some(builder) = routes.get(&current) {
        let page = Box(Modifier::new().fill_max_size()).child((builder)());
        AnimatedContent(current.clone(), trans, page)
    } else {
        // Empty fallback still fills so layouts/scrolls get a definite size
        Box(Modifier::new().fill_max_size())
    }
}
