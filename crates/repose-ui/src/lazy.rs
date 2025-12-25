use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;
use web_time::Instant;

pub struct LazyColumnState {
    scroll_offset: Signal<f32>,   // px
    viewport_height: Signal<f32>, // px

    // physics
    vel_px_s: RefCell<f32>, // px/sec
    last_t: RefCell<Instant>,
    last_input_t: RefCell<Instant>,
    animating: RefCell<bool>,
}

impl Default for LazyColumnState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyColumnState {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            scroll_offset: signal(0.0),
            viewport_height: signal(600.0),
            vel_px_s: RefCell::new(0.0),
            last_t: RefCell::new(now),
            last_input_t: RefCell::new(now),
            animating: RefCell::new(false),
        }
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        self.scroll_offset.set(off.clamp(0.0, max_off));
    }

    /// Consume delta in px. Returns leftover in px (for nested scroll).
    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);

        let consumed = new_offset - before;
        let leftover = delta_px - consumed;

        // estimate velocity (px/sec) from input cadence
        let now = Instant::now();
        let dt = (now - *self.last_input_t.borrow())
            .as_secs_f32()
            .clamp(1.0 / 240.0, 1.0 / 15.0);
        *self.last_input_t.borrow_mut() = now;

        *self.vel_px_s.borrow_mut() = consumed / dt;
        *self.animating.borrow_mut() = self.vel_px_s.borrow().abs() > 10.0;

        leftover
    }

    /// Advance inertia one tick; returns true if animating.
    pub fn tick(&self, content_height_px: f32) -> bool {
        if !*self.animating.borrow() {
            return false;
        }

        let now = Instant::now();
        let dt = (now - *self.last_t.borrow()).as_secs_f32().min(0.1);
        *self.last_t.borrow_mut() = now;

        if dt <= 0.0 {
            return false;
        }

        let vel0 = *self.vel_px_s.borrow();
        if vel0.abs() < 5.0 {
            *self.vel_px_s.borrow_mut() = 0.0;
            *self.animating.borrow_mut() = false;
            return false;
        }

        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let new_off = (before + vel0 * dt).clamp(0.0, max_offset);
        self.scroll_offset.set(new_off);

        // Stop quickly at bounds
        if (new_off - before).abs() < 0.01 && (before <= 0.0 || before >= max_offset) {
            *self.vel_px_s.borrow_mut() = 0.0;
            *self.animating.borrow_mut() = false;
            return false;
        }

        // decay ~0.9 per 60Hz "frame"
        let decay_per_60hz = 0.90f32;
        let decay = decay_per_60hz.powf(dt * 60.0);
        *self.vel_px_s.borrow_mut() = vel0 * decay;

        true
    }
}

/// Virtualized list - only renders visible items
#[allow(non_snake_case)]
pub fn LazyColumn<T, F>(
    items: Vec<T>,
    item_height_dp: f32, // logical dp
    state: Rc<LazyColumnState>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    // Convert once: internal math uses px
    let item_h_px = dp_to_px(item_height_dp);
    let content_height_px = items.len() as f32 * item_h_px;

    // Signals are px (fed by ScrollV)
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    // Advance physics in px
    state.tick(content_height_px);

    // Visible range (px)
    let first_visible = (scroll_offset_px / item_h_px).floor().max(0.0) as usize;
    let last_visible = ((scroll_offset_px + viewport_height_px) / item_h_px).ceil() as usize + 2;

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);

    let mut children = Vec::new();

    // Top spacer (dp; converted by layout)
    if first_with_buffer > 0 {
        children.push(crate::Box(
            Modifier::new().size(1.0, first_with_buffer as f32 * item_height_dp),
        ));
    }

    for i in first_with_buffer..last_visible {
        if let Some(item) = items.get(i) {
            children.push(item_builder(item.clone(), i));
        }
    }

    // Bottom spacer (dp; converted by layout)
    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new().size(1.0, remaining as f32 * item_height_dp),
        ));
    }

    // Scroll callbacks (px)
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: repose_core::Vec2| -> repose_core::Vec2 {
            let leftover_y_px = st.scroll_immediate(d.y, content_height_px);
            repose_core::Vec2 {
                x: d.x,
                y: leftover_y_px,
            }
        })
    };

    let set_viewport = {
        let st = state.clone();
        Rc::new(move |h_px: f32| st.viewport_height.set(h_px.max(0.0)))
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> f32 { st.scroll_offset.get() })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |off_px: f32| st.set_offset(off_px, content_height_px))
    };

    let measured_h_px = {
        let st = state.clone();
        Rc::new(move |h_px: f32| {
            st.set_offset(st.scroll_offset.get(), h_px);
        })
    };

    let content = crate::Column(Modifier::new()).with_children(children);

    repose_core::View::new(
        0,
        repose_core::ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            set_content_height: Some(Rc::new(move |h| measured_h_px(h))),
            get_scroll_offset: Some(get_scroll),
            set_scroll_offset: Some(set_scroll),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

/// List without virtualization (for small lists)
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
