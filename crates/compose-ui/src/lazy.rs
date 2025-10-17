use compose_core::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct LazyColumnState {
    pub scroll_offset: f32,
    pub viewport_height: f32,
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            viewport_height: 600.0,
        }
    }

    pub fn scroll(&mut self, delta: f32, content_height: f32) {
        self.scroll_offset = (self.scroll_offset + delta)
            .max(0.0)
            .min((content_height - self.viewport_height).max(0.0));
    }
}

/// Virtualized list - only renders visible items
#[allow(non_snake_case)]
pub fn LazyColumn<T, F>(
    items: Vec<T>,
    item_height: f32,
    state: Rc<RefCell<LazyColumnState>>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let state_ref = state.borrow();
    let scroll_offset = state_ref.scroll_offset;
    let viewport_height = state_ref.viewport_height;
    drop(state_ref);

    // Calculate visible range
    let first_visible = (scroll_offset / item_height).floor() as usize;
    let last_visible = ((scroll_offset + viewport_height) / item_height).ceil() as usize;

    let first_visible = first_visible.min(items.len());
    let last_visible = last_visible.min(items.len());

    // Build only visible items
    let mut children = Vec::new();

    // Top spacer
    if first_visible > 0 {
        children.push(crate::Box(
            Modifier::new().size(1.0, first_visible as f32 * item_height),
        ));
    }

    // Visible items
    for i in first_visible..last_visible {
        if let Some(item) = items.get(i) {
            children.push(item_builder(item.clone(), i));
        }
    }

    // Bottom spacer
    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new().size(1.0, remaining as f32 * item_height),
        ));
    }

    // Scroll callbacks
    let content_height = items.len() as f32 * item_height;
    let on_scroll = {
        let state = state.clone();
        Rc::new(move |dy: f32| {
            state.borrow_mut().scroll(dy, content_height);
        })
    };
    let set_viewport = {
        let state = state.clone();
        Rc::new(move |h: f32| {
            state.borrow_mut().viewport_height = h;
        })
    };

    // Wrap content in a scroll container; apply modifier to the viewport
    let content = crate::Column(Modifier::new()).with_children(children);
    compose_core::View::new(
        0,
        compose_core::ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

/// Simple list without virtualization (for small lists)
#[allow(non_snake_case)]
pub fn SimpleList<T: Clone + 'static>(
    items: Vec<T>,
    modifier: Modifier,
    item_builder: Rc<dyn Fn(T, usize) -> View>,
) -> View {
    let children: Vec<View> = items
        .into_iter()
        .enumerate()
        .map(|(i, item)| item_builder(item, i))
        .collect();

    crate::Column(modifier).with_children(children)
}
