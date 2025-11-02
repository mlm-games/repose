use crate::{Box, ViewExt};
use compose_core::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::Stack;

pub struct LazyColumnState {
    pub scroll_offset: f32,
    pub viewport_height: f32,
    // kinetic scrolling related
    vel: f32,
    last_t: Instant,
    animating: bool,
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            scroll_offset: 0.0,
            viewport_height: 600.0,
            vel: 0.0,
            last_t: Instant::now(),
            animating: false,
        }
    }
    // Immediate scroll (for wheel routing, no overscroll propagation)
    fn clamp_to_bounds(&self, content_height: f32) -> (f32, f32) {
        let max_off = (content_height - self.viewport_height).max(0.0);
        (0.0, max_off)
    }
    pub fn scroll_immediate(&mut self, delta: f32, content_height: f32) -> f32 {
        let (min_off, max_off) = self.clamp_to_bounds(content_height);
        let old = self.scroll_offset;
        let unclamped = old + delta;
        let clamped = unclamped.clamp(min_off, max_off);
        self.scroll_offset = clamped;
        let consumed = clamped - old;
        let leftover = delta - consumed;
        // Add a bit of kinetic continuation
        self.vel += consumed;
        self.animating = self.vel.abs() > 0.1;
        leftover
    }

    // Per-frame integration with friction and edge spring
    pub fn tick(&mut self, content_height: f32) -> bool {
        let now = Instant::now();
        let dt = (now - self.last_t).as_secs_f32();
        self.last_t = now;
        if dt <= 0.0 {
            return false;
        }

        let (min_off, max_off) = self.clamp_to_bounds(content_height);
        if !self.animating && (self.scroll_offset >= min_off && self.scroll_offset <= max_off) {
            return false;
        }
        // Integrate velocity
        let friction = 8.0; // larger -> faster decay
        self.scroll_offset += self.vel * dt;
        self.vel *= (-friction * dt).exp(); // exp decay

        // Edge spring (elastic bounce)
        let k = 200.0; // spring stiffness
        let c = 20.0; // damping
        if self.scroll_offset < min_off {
            let x = self.scroll_offset - min_off; // negative
            // x'' + (c)*x' + k*x = 0 -> Euler step on velocity
            self.vel += (-k * x - c * self.vel) * dt;
        } else if self.scroll_offset > max_off {
            let x = self.scroll_offset - max_off; // positive
            self.vel += (-k * x - c * self.vel) * dt;
        }
        // Stop if settled
        let settled = self.vel.abs() < 0.02
            && self.scroll_offset >= min_off - 0.5
            && self.scroll_offset <= max_off + 0.5;
        self.animating = !settled;
        if settled {
            self.scroll_offset = self.scroll_offset.clamp(min_off, max_off);
        }
        true
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
    let content_height = items.len() as f32 * item_height;

    // Advance physics once per frame
    {
        let mut st = state.borrow_mut();
        let _ = st.tick(content_height);
    }

    let (scroll_offset, viewport_height) = {
        let st = state.borrow();
        (st.scroll_offset, st.viewport_height)
    };

    // Visible window (with a small render buffer for smoothness)
    let buffer = 2usize;
    let first_visible = (scroll_offset / item_height).floor().max(0.0) as usize;
    let last_visible = ((scroll_offset + viewport_height) / item_height).ceil() as usize;

    let first_visible = first_visible.min(items.len());
    let last_visible = last_visible.saturating_add(buffer).min(items.len());
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
    let on_scroll = {
        let state = state.clone();
        Rc::new(move |dy: f32| -> f32 { state.borrow_mut().scroll_immediate(dy, content_height) })
    };
    let set_viewport = {
        let state = state.clone();
        Rc::new(move |h: f32| {
            state.borrow_mut().viewport_height = h;
        })
    };
    let get_scroll = {
        let state = state.clone();
        Rc::new(move || -> f32 { state.borrow().scroll_offset })
    };

    // Content inside scroll viewport (clip and translation happen in layout_and_paint)
    let content = crate::Column(Modifier::new()).with_children(children);

    compose_core::View::new(
        0,
        compose_core::ViewKind::ScrollV {
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
