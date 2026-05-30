use crate::scroll::ScrollPhysics;
use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;

pub struct LazyColumnState {
    scroll_offset: Signal<f32>,   // px
    viewport_height: Signal<f32>, // px
    content_height: Signal<f32>,  // px, actual measured height from layout

    physics: RefCell<ScrollPhysics>,
}

impl Default for LazyColumnState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: signal(600.0),
            content_height: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
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

        self.physics.borrow_mut().record_input(consumed);

        delta_px - consumed
    }

    /// Advance inertia one tick; returns true if animating.
    pub fn tick(&self, content_height_px: f32) -> bool {
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
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
    let item_h_px = dp_to_px(item_height_dp).max(1.0);
    let content_height_px = items.len() as f32 * item_h_px;

    // Signals are px (fed by ScrollV)
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    // NOTE: needed for full list traversal
    let actual_content_h_px = if state.content_height.get() > 0.0 {
        state.content_height.get()
    } else {
        content_height_px
    };
    state.tick(actual_content_h_px);

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
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            let leftover_y_px = st.scroll_immediate(d.y, ch);
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
        Rc::new(move |off_px: f32| {
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            st.set_offset(off_px, ch);
        })
    };

    let measured_h_px = {
        let st = state.clone();
        Rc::new(move |h_px: f32| {
            st.content_height.set(h_px);
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
            show_scrollbar: true,
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

/// State for a virtualized scrolling grid.
pub struct LazyGridState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    content_height: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
}

impl Default for LazyGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyGridState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: signal(600.0),
            content_height: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
        }
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        self.scroll_offset.set(off.clamp(0.0, max_off));
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}

/// Virtualized scrolling grid with a fixed number of columns.
///
/// Items are arranged left-to-right, top-to-bottom. Only items visible in the
/// viewport (plus a buffer) are rendered. Each item has the same height (`item_height_dp`).
///
/// Supply row/column gaps via modifier methods on the `modifier` parameter:
/// `.row_gap(v)`, `.column_gap(v)`, or `.gap(v)` to set both.
///
/// # Example
/// ```ignore
/// let state = LazyGridState::new();
/// let state_rc = Rc::new(state);
/// LazyVerticalGrid(
///     3,                          // columns
///     items,                      // Vec<MyItem>
///     120.0,                      // item height in dp
///     state_rc.clone(),
///     Modifier::new().fill_max_size().gap(8.0),
///     |item, index| Card(Modifier::new().fill_max_width(), Text(format!("Item {index}"))),
/// )
/// ```
#[allow(non_snake_case)]
pub fn LazyVerticalGrid<T, F>(
    columns: usize,
    items: Vec<T>,
    item_height_dp: f32,
    state: Rc<LazyGridState>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let columns = columns.max(1);
    let item_h_px = dp_to_px(item_height_dp).max(1.0);
    let total_items = items.len();
    let total_rows = total_items.div_ceil(columns);
    let content_height_px = total_rows as f32 * item_h_px;

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let actual_content_h_px = if state.content_height.get() > 0.0 {
        state.content_height.get()
    } else {
        content_height_px
    };
    state.tick(actual_content_h_px);

    let buffer_rows = 2usize;
    let first_row = ((scroll_offset_px / item_h_px)
        .floor()
        .max(0.0)) as usize;
    let first_row = first_row.saturating_sub(buffer_rows);
    let last_row = (((scroll_offset_px + viewport_height_px) / item_h_px).ceil() as usize + buffer_rows).min(total_rows);

    let first_item = first_row * columns;
    let last_item = (last_row * columns).min(total_items);

    let mut children: Vec<View> = Vec::new();

    // Top spacer
    if first_row > 0 {
        children.push(crate::Box(Modifier::new().size(1.0, first_row as f32 * item_height_dp)));
    }

    // Visible items arranged in a Taffy CSS grid
    if first_item < last_item {
        let visible_items: Vec<View> = (first_item..last_item)
            .map(|i| item_builder(items[i].clone(), i))
            .collect();

        // Extract gap settings from the parent modifier so the inner grid uses them
        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(0.0);
        let grid_mod = Modifier::new()
            .grid(columns, rg, cg)
            .fill_max_width();
        children.push(crate::Column(grid_mod).with_children(visible_items));
    }

    // Bottom spacer
    if last_row < total_rows {
        children.push(crate::Box(Modifier::new().size(1.0, (total_rows - last_row) as f32 * item_height_dp)));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            Vec2 { x: d.x, y: st.scroll_immediate(d.y, ch) }
        })
    };

    let set_viewport = {
        let st = state.clone();
        Rc::new(move |h: f32| st.viewport_height.set(h.max(0.0)))
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> f32 { st.scroll_offset.get() })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |off: f32| {
            let ch = st.content_height.get();
            st.set_offset(off, if ch > 0.0 { ch } else { content_height_px });
        })
    };

    let measured_h = {
        let st = state.clone();
        Rc::new(move |h: f32| {
            st.content_height.set(h);
            st.set_offset(st.scroll_offset.get(), h);
        })
    };

    let content = crate::Column(Modifier::new().fill_max_width()).with_children(children);

    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            set_content_height: Some(Rc::new(move |h| measured_h(h))),
            get_scroll_offset: Some(get_scroll),
            set_scroll_offset: Some(set_scroll),
            show_scrollbar: true,
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

/// State for a horizontal lazy list (`LazyRow`).
///
/// Tracks scroll offset, viewport width, content width, and inertial physics.
pub struct LazyRowState {
    scroll_offset: Signal<f32>,
    viewport_width: Signal<f32>,
    content_width: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
}

impl Default for LazyRowState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyRowState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_width: signal(600.0),
            content_width: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
        }
    }

    pub fn set_offset(&self, off: f32, content_width: f32) {
        let vw = self.viewport_width.get();
        let max_off = (content_width - vw).max(0.0);
        self.scroll_offset.set(off.clamp(0.0, max_off));
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_width_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick(&self, content_width_px: f32) -> bool {
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}

/// Virtualized horizontal list — only renders visible items.
///
/// Items are arranged left-to-right. Only items within the viewport
/// (plus a buffer) are rendered. Each item has the same width (`item_width_dp`).
///
/// # Example
/// ```ignore
/// let state = Rc::new(LazyRowState::new());
/// LazyRow(
///     items,                  // Vec<MyItem>
///     100.0,                  // item width in dp
///     state,
///     Modifier::new().fill_max_width().height(120.0),
///     |item, index| Text(format!("Item {index}")),
/// )
/// ```
#[allow(non_snake_case)]
pub fn LazyRow<T, F>(
    items: Vec<T>,
    item_width_dp: f32,
    state: Rc<LazyRowState>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let item_w_px = dp_to_px(item_width_dp).max(1.0);
    let content_width_px = items.len() as f32 * item_w_px;

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_width_px = state.viewport_width.get();

    let actual_content_w_px = if state.content_width.get() > 0.0 {
        state.content_width.get()
    } else {
        content_width_px
    };
    state.tick(actual_content_w_px);

    let first_visible = (scroll_offset_px / item_w_px).floor().max(0.0) as usize;
    let last_visible = ((scroll_offset_px + viewport_width_px) / item_w_px).ceil() as usize + 2;

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);

    let mut children = Vec::new();

    if first_with_buffer > 0 {
        children.push(crate::Box(
            Modifier::new().size(first_with_buffer as f32 * item_width_dp, 1.0),
        ));
    }

    for i in first_with_buffer..last_visible {
        if let Some(item) = items.get(i) {
            children.push(item_builder(item.clone(), i));
        }
    }

    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new().size(remaining as f32 * item_width_dp, 1.0),
        ));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let cw = st.content_width.get();
            let cw = if cw > 0.0 { cw } else { content_width_px };
            Vec2 { x: st.scroll_immediate(d.x, cw), y: d.y }
        })
    };

    let set_viewport_w = {
        let st = state.clone();
        Rc::new(move |w: f32| st.viewport_width.set(w.max(0.0)))
    };

    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            st.content_width.set(w);
            st.set_offset(st.scroll_offset.get(), w);
        })
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> (f32, f32) { (st.scroll_offset.get(), 0.0) })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |x: f32, _y: f32| {
            let cw = st.content_width.get();
            st.set_offset(x, if cw > 0.0 { cw } else { content_width_px });
        })
    };

    let content = crate::Row(Modifier::new().flex_shrink(0.0)).with_children(children);

    View::new(
        0,
        ViewKind::ScrollXY {
            on_scroll: Some(on_scroll),
            set_viewport_width: Some(set_viewport_w),
            set_viewport_height: None,
            set_content_width: Some(set_content_w),
            set_content_height: None,
            get_scroll_offset_xy: Some(get_scroll),
            set_scroll_offset_xy: Some(set_scroll),
            show_scrollbar: true,
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}
