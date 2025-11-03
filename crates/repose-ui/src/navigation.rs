use repose_core::*;
use std::collections::VecDeque;
use std::rc::Rc;

pub struct NavController {
    stack: RefCell<VecDeque<NavEntry>>,
    current: Signal<String>,
    transitions: Signal<Option<Transition>>,
}

pub struct NavEntry {
    route: String,
    args: HashMap<String, String>,
    state: Box<dyn Any>,
}

pub enum Transition {
    Push { from: String, to: String },
    Pop { from: String, to: String },
    Replace { from: String, to: String },
}

impl NavController {
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
}

pub fn NavHost(
    controller: Rc<NavController>,
    routes: HashMap<String, Box<dyn Fn() -> View>>,
) -> View {
    let current = controller.current.get();

    if let Some(builder) = routes.get(&current) {
        AnimatedContent(current.clone(), controller.transitions.get(), builder())
    } else {
        Box(Modifier::new()) // Empty fallback
    }
}
