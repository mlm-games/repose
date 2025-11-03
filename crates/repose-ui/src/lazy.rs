use crate::{Box, ViewExt};
use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::Stack;

pub struct LazyColumnState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    // Internal non-reactive state
    vel: RefCell<f32>,
    last_t: RefCell<Instant>,
    animating: RefCell<bool>,
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: signal(600.0),
            vel: RefCell::new(0.0),
            last_t: RefCell::new(Instant::now()),
            animating: RefCell::new(false),
        }
    }

    pub fn scroll_immediate(&self, delta: f32, content_height: f32) -> f32 {
        let current = self.scroll_offset.get();
        let viewport = self.viewport_height.get();

        let max_offset = (content_height - viewport).max(0.0);
        let new_offset = (current + delta).clamp(0.0, max_offset);

        // THIS IS THE KEY: Setting the signal triggers recomposition!
        self.scroll_offset.set(new_offset);

        let consumed = new_offset - current;
        let leftover = delta - consumed;

        *self.vel.borrow_mut() = consumed * 5.0;
        *self.animating.borrow_mut() = true;

        leftover
    }

    pub fn tick(&self, content_height: f32) -> bool {
        if !*self.animating.borrow() {
            return false;
        }

        let now = Instant::now();
        let dt = (now - *self.last_t.borrow()).as_secs_f32().min(0.1);
        *self.last_t.borrow_mut() = now;

        if dt <= 0.0 {
            return false;
        }

        let vel = *self.vel.borrow();
        if vel.abs() < 0.5 {
            *self.vel.borrow_mut() = 0.0;
            *self.animating.borrow_mut() = false;
            return false;
        }

        // Update position
        let current = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height - viewport).max(0.0);

        let new_offset = (current + vel * dt * 60.0).clamp(0.0, max_offset);

        // Signal update triggers recomposition!
        self.scroll_offset.set(new_offset);

        // Apply friction
        *self.vel.borrow_mut() *= 0.95;

        true
    }
}

/// Virtualized list - only renders visible items
#[allow(non_snake_case)]
pub fn LazyColumn<T, F>(
    items: Vec<T>,
    item_height: f32,
    state: Rc<LazyColumnState>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let content_height = items.len() as f32 * item_height;

    // Get current values from signals - this subscribes to changes!
    let scroll_offset = state.scroll_offset.get();
    let viewport_height = state.viewport_height.get();

    // Tick physics
    state.tick(content_height);

    // Calculate visible range
    let first_visible = (scroll_offset / item_height).floor().max(0.0) as usize;
    let last_visible = ((scroll_offset + viewport_height) / item_height).ceil() as usize + 2;

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);

    let mut children = Vec::new();

    // Top spacer = baseline start. After visual offset (-scroll_offset) this becomes -remainder.
    if first_with_buffer > 0 {
        children.push(crate::Box(
            Modifier::new().size(1.0, first_with_buffer as f32 * item_height),
        ));
    }

    for i in first_with_buffer..last_visible {
        if let Some(item) = items.get(i) {
            children.push(item_builder(item.clone(), i));
        }
    }

    // Optional: bottom spacer (not strictly required for visual; harmless)
    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new().size(1.0, remaining as f32 * item_height),
        ));
    }

    // Scroll callbacks
    let state_clone = state.clone();
    let on_scroll =
        Rc::new(move |dy: f32| -> f32 { state_clone.scroll_immediate(dy, content_height) });

    let state_clone = state.clone();
    let set_viewport = Rc::new(move |h: f32| {
        state_clone.viewport_height.set(h);
    });

    let state_clone = state.clone();
    let get_scroll = Rc::new(move || -> f32 { state_clone.scroll_offset.get() });

    // Content inside scroll viewport (clip and translation happen in layout_and_paint)
    let content = crate::Column(Modifier::new()).with_children(children);

    repose_core::View::new(
        0,
        repose_core::ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            get_scroll_offset: Some(get_scroll), // NEW
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
