use crate::ViewExt;
use crate::anim::animate_f32_from;
use crate::scroll::ScrollPhysics;
use repose_core::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub trait ItemHeight<T> {
    fn get(&self, item: &T) -> f32;
}

impl<T> ItemHeight<T> for f32 {
    fn get(&self, _item: &T) -> f32 {
        *self
    }
}

impl<T, F: Fn(&T) -> f32> ItemHeight<T> for F {
    fn get(&self, item: &T) -> f32 {
        (self)(item)
    }
}

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

struct AnimState<T> {
    prev_keys: Vec<u64>,
    exiting: Vec<(u64, usize, T, u64)>,
    item_cache: HashMap<u64, T>,
}

/// Virtualized list - only renders visible items.
///
/// `item_height` may be a uniform `f32` (dp) or a per-item closure. For
/// heterogeneous heights, pass `|item| item.height_dp` to compute each
/// item's height from its data.
///
/// Optionally animates item enter/exit when items are added or removed.
/// Provide `get_key` for stable item identity and `animate_spec` to enable
/// fade-in for new items and fade-out for removed items.
///
/// When `animate_spec` is `None`, behavior is identical to the original
/// (no item animations).
#[allow(non_snake_case)]
pub fn LazyColumn<T, F, K, H>(
    items: Vec<T>,
    item_height: H,
    state: Rc<LazyColumnState>,
    modifier: Modifier,
    get_key: K,
    animate_spec: Option<AnimationSpec>,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
    K: Fn(&T) -> u64 + 'static,
    H: ItemHeight<T>,
{
    let heights_dp: Vec<f32> = items
        .iter()
        .map(|it| item_height.get(it).max(1.0))
        .collect();
    let cumulative_px: Vec<f32> = {
        let mut cum = Vec::with_capacity(heights_dp.len() + 1);
        cum.push(0.0);
        let mut acc = 0.0_f32;
        for h in &heights_dp {
            acc += dp_to_px(*h);
            cum.push(acc);
        }
        cum
    };
    let content_height_px = *cumulative_px.last().unwrap_or(&0.0);

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let first_visible = match cumulative_px.binary_search_by(|p| {
        p.partial_cmp(&scroll_offset_px)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Ok(i) => i,
        Err(i) => i.saturating_sub(1),
    };
    let viewport_end_px = scroll_offset_px + viewport_height_px;
    let last_visible = match cumulative_px.binary_search_by(|p| {
        p.partial_cmp(&viewport_end_px)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        Ok(i) => (i + 1).min(items.len()),
        Err(i) => i.min(items.len()),
    };

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);
    let last_with_buffer = (last_visible + buffer).min(items.len());

    let mut combined_children: Vec<View> = Vec::new();

    if first_with_buffer > 0 {
        let top_spacer_px = cumulative_px[first_with_buffer];
        if top_spacer_px > 0.0 {
            combined_children.push(crate::Box(
                Modifier::new().size(1.0, px_to_dp(top_spacer_px).max(0.0)),
            ));
        }
    }

    let total_slots: usize;
    if let Some(spec) = animate_spec {
        // Stable per-call-site ID for animation key namespacing.
        let inst = remember(|| std::cell::Cell::new(0u64));
        if inst.get() == 0 {
            static CTR: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            inst.set(CTR.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        }
        let aid = inst.get();

        let state_slot: Rc<RefCell<AnimState<T>>> = remember(|| {
            RefCell::new(AnimState {
                prev_keys: Vec::new(),
                exiting: Vec::new(),
                item_cache: HashMap::new(),
            })
        });

        let mut s = state_slot.borrow_mut();

        let curr_keys: Vec<u64> = items.iter().map(&get_key).collect();

        // Find added and removed keys
        let added: Vec<u64> = curr_keys
            .iter()
            .filter(|k| !s.prev_keys.contains(k))
            .copied()
            .collect();
        let removed: Vec<(usize, u64)> = s
            .prev_keys
            .iter()
            .enumerate()
            .filter(|(_, k)| !curr_keys.contains(k))
            .map(|(i, k)| (i, *k))
            .collect();

        let had_prev = !s.prev_keys.is_empty();

        // Add removed items to exiting list
        if had_prev && !removed.is_empty() {
            for (old_idx, key) in &removed {
                if let Some(old_item) = s.item_cache.get(key) {
                    let v = s.exiting.len() as u64;
                    let cloned = old_item.clone();
                    s.exiting.push((*key, *old_idx, cloned, v));
                }
            }
        }

        // Update item cache with current items
        for item in &items {
            s.item_cache.insert(get_key(item), item.clone());
        }

        // Process exiting items - only keep those still fading
        let mut still_exiting: Vec<(u64, usize, T, u64)> = Vec::new();
        for (key, old_idx, old_item, version) in s.exiting.iter() {
            let exit_key = format!("_lz_x:{aid}:{key}:v{version}");
            let alpha = animate_f32_from(exit_key, 1.0, 0.0, spec);
            if alpha > 0.005 {
                still_exiting.push((*key, *old_idx, old_item.clone(), *version));
            }
        }

        // Extend visible range to cover exiting items' positions
        let max_exit_slot = still_exiting
            .iter()
            .map(|(_, i, _, _)| *i)
            .max()
            .unwrap_or(0);
        let vis_end = last_with_buffer.max(max_exit_slot + 1 + buffer);
        total_slots = items.len().max(max_exit_slot + 1);

        // Build combined children: interleave exiting items at their old indices
        // with normal items filling remaining slots in order
        let mut normal_ptr = 0usize;
        for visual_i in first_with_buffer..vis_end {
            let entry = still_exiting.iter().find(|(_, oi, _, _)| *oi == visual_i);
            if let Some((key, old_idx, old_item, version)) = entry {
                let ek = format!("_lz_x:{aid}:{key}:v{version}");
                let alpha = animate_f32_from(ek, 1.0, 0.0, spec);
                let exit_top_px = cumulative_px
                    .get(*old_idx)
                    .copied()
                    .unwrap_or(*old_idx as f32 * 1.0);
                let exit_h_dp = heights_dp.get(*old_idx).copied().unwrap_or(1.0);
                let exit_bottom_px = exit_top_px + dp_to_px(exit_h_dp);
                let in_view = exit_bottom_px > scroll_offset_px
                    && exit_top_px < scroll_offset_px + viewport_height_px;
                if in_view {
                    let exit_view = item_builder(old_item.clone(), *old_idx);
                    combined_children.push(
                        crate::Box(
                            Modifier::new()
                                .fill_max_width()
                                .height(exit_h_dp)
                                .alpha(alpha),
                        )
                        .child(exit_view),
                    );
                }
            } else if let Some(item) = items.get(normal_ptr) {
                let key = get_key(item);
                if had_prev && added.contains(&key) {
                    let enter_key = format!("_lz_n:{aid}:{key}");
                    let alpha = animate_f32_from(enter_key, 0.0, 1.0, spec);
                    combined_children.push(
                        crate::Box(Modifier::new().fill_max_width().alpha(alpha))
                            .child(item_builder(item.clone(), normal_ptr)),
                    );
                } else {
                    combined_children.push(item_builder(item.clone(), normal_ptr));
                }
                normal_ptr += 1;
            }
        }

        s.exiting = still_exiting;
        s.prev_keys = curr_keys;
    } else {
        // No animation: render items normally
        for i in first_with_buffer..last_with_buffer {
            if let Some(item) = items.get(i) {
                let h_dp = item_height.get(item).max(1.0);
                combined_children.push(
                    crate::Box(Modifier::new().fill_max_width().height(h_dp))
                        .child(item_builder(item.clone(), i)),
                );
            }
        }
        total_slots = items.len();
    }

    // Bottom spacer (dp; converted by layout)
    let has_top = first_with_buffer > 0;
    let rendered_items = combined_children.len() - if has_top { 1 } else { 0 };
    if first_with_buffer + rendered_items < total_slots {
        let end_px = cumulative_px
            .get(first_with_buffer + rendered_items)
            .copied()
            .unwrap_or(content_height_px);
        let remaining_px = (content_height_px - end_px).max(0.0);
        if remaining_px > 0.0 {
            combined_children.push(crate::Box(
                Modifier::new().size(1.0, px_to_dp(remaining_px).max(0.0)),
            ));
        }
    }

    let content = crate::View::new(0, ViewKind::Column).with_children(combined_children);

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
            if (st.content_height.get() - h_px).abs() > 0.5 {
                st.content_height.set(h_px);
                st.set_offset(st.scroll_offset.get(), h_px);
            }
        })
    };

    let tick_scroll = {
        let st = state.clone();
        Rc::new(move || {
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            st.tick(ch);
        })
    };

    repose_core::View::new(
        0,
        repose_core::ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            set_content_height: Some(Rc::new(move |h| measured_h_px(h))),
            get_scroll_offset: Some(get_scroll),
            set_scroll_offset: Some(set_scroll),
            show_scrollbar: true,
            tick_scroll: Some(tick_scroll),
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

    let buffer_rows = 2usize;
    let first_row = ((scroll_offset_px / item_h_px).floor().max(0.0)) as usize;
    let first_row = first_row.saturating_sub(buffer_rows);
    let last_row = (((scroll_offset_px + viewport_height_px) / item_h_px).ceil() as usize
        + buffer_rows)
        .min(total_rows);

    let first_item = first_row * columns;
    let last_item = (last_row * columns).min(total_items);

    let mut children: Vec<View> = Vec::new();

    // Top spacer
    if first_row > 0 {
        children.push(crate::Box(
            Modifier::new().size(1.0, first_row as f32 * item_height_dp),
        ));
    }

    // Visible items arranged in a Taffy CSS grid
    if first_item < last_item {
        let visible_items: Vec<View> = (first_item..last_item)
            .map(|i| item_builder(items[i].clone(), i))
            .collect();

        // Extract gap settings from the parent modifier so the inner grid uses them
        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(0.0);
        let grid_mod = Modifier::new().grid(columns, rg, cg).fill_max_width();
        children.push(crate::Column(grid_mod).with_children(visible_items));
    }

    // Bottom spacer
    if last_row < total_rows {
        children.push(crate::Box(
            Modifier::new().size(1.0, (total_rows - last_row) as f32 * item_height_dp),
        ));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            Vec2 {
                x: d.x,
                y: st.scroll_immediate(d.y, ch),
            }
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
            if (st.content_height.get() - h).abs() > 0.5 {
                st.content_height.set(h);
                st.set_offset(st.scroll_offset.get(), h);
            }
        })
    };

    let tick_scroll = {
        let st = state.clone();
        Rc::new(move || {
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            st.tick(ch);
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
            tick_scroll: Some(tick_scroll),
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

/// Virtualized horizontal list - only renders visible items.
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
            Vec2 {
                x: st.scroll_immediate(d.x, cw),
                y: d.y,
            }
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

    let tick_scroll = {
        let st = state.clone();
        Rc::new(move || {
            let cw = st.content_width.get();
            let cw = if cw > 0.0 { cw } else { content_width_px };
            st.tick(cw);
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
            tick_scroll: Some(tick_scroll),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

/// State for a virtualized staggered scrolling grid.
pub struct LazyVerticalStaggeredGridState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    content_height: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
}

impl Default for LazyVerticalStaggeredGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyVerticalStaggeredGridState {
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

/// Pre-computed placement for an item in a staggered grid.
struct StaggeredPlacement {
    /// Which column this item occupies (0..columns).
    col: usize,
    /// Vertical offset from the top of the grid content (px).
    y_px: f32,
    /// Height of this item (px).
    h_px: f32,
}

/// Compute staggered grid placements for all items.
/// Uses the "shortest column" algorithm to balance items across columns.
fn compute_staggered_placements(
    heights_px: &[f32],
    columns: usize,
    gap_px: f32,
) -> Vec<StaggeredPlacement> {
    let mut placements = Vec::with_capacity(heights_px.len());
    let mut col_heights = vec![0.0_f32; columns];
    for (i, h) in heights_px.iter().enumerate() {
        // Find shortest column
        let col = col_heights
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(idx, _)| idx)
            .unwrap_or(i % columns);
        let y = col_heights[col];
        placements.push(StaggeredPlacement {
            col,
            y_px: y,
            h_px: *h,
        });
        col_heights[col] = y + h + gap_px;
    }
    placements
}

/// Virtualized staggered grid (Pinterest-style).
///
/// Items are arranged in a fixed number of columns. Each item can have a
/// different height. Items are placed in the column with the least accumulated
/// height, creating a staggered visual effect.
///
/// Only items visible in the viewport (plus a buffer) are rendered.
///
/// # Example
/// ```ignore
/// let state = Rc::new(LazyVerticalStaggeredGridState::new());
/// LazyVerticalStaggeredGrid(
///     2,                          // columns
///     items,                      // Vec<MyItem>
///     |item: &MyItem| item.height_dp,  // per-item height
///     state,
///     Modifier::new().fill_max_size().gap(8.0),
///     |item: &MyItem, index: usize| { /* ... */ },
/// )
/// ```
#[allow(non_snake_case)]
pub fn LazyVerticalStaggeredGrid<T, F, K>(
    columns: usize,
    items: Vec<T>,
    item_height_dp: K,
    state: Rc<LazyVerticalStaggeredGridState>,
    modifier: Modifier,
    item_builder: F,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
    K: Fn(&T) -> f32 + 'static,
{
    let columns = columns.max(1);
    let gap_dp = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
    let gap_px = dp_to_px(gap_dp);

    // Compute heights in px
    let heights_px: Vec<f32> = items
        .iter()
        .map(|it| dp_to_px(item_height_dp(it).max(1.0)))
        .collect();
    let placements = compute_staggered_placements(&heights_px, columns, gap_px);

    // Compute total content height
    let total_content_height_px = placements
        .iter()
        .map(|p| p.y_px + p.h_px)
        .fold(0.0_f32, f32::max);

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let buffer = 2;
    let mut first_visible = usize::MAX;
    let mut last_visible = 0usize;

    for (i, p) in placements.iter().enumerate() {
        let item_top = p.y_px;
        let item_bot = p.y_px + p.h_px;
        if item_bot > scroll_offset_px && item_top < scroll_offset_px + viewport_height_px {
            if i < first_visible {
                first_visible = i;
            }
            if i > last_visible {
                last_visible = i;
            }
        }
    }

    if first_visible == usize::MAX {
        first_visible = 0;
    }
    if last_visible == 0 && !items.is_empty() {
        last_visible = items.len().saturating_sub(1);
    }

    let first_idx = first_visible.saturating_sub(buffer);
    let last_idx = (last_visible + buffer).min(items.len());

    // Build per-column children with spacers for staggered positioning.
    // Every item in [first_idx, last_idx) gets a placeholder so column heights
    // match total_content_height_px - prevents scroll boundary from jumping.
    let mut col_children: Vec<Vec<View>> = (0..columns).map(|_| Vec::new()).collect();
    for col in 0..columns {
        let mut prev_y = 0.0_f32;
        for (i, p) in placements.iter().enumerate() {
            if p.col != col || i < first_idx || i >= last_idx {
                continue;
            }
            let spacer_y = p.y_px - prev_y;
            if spacer_y > 0.0 {
                col_children[col].push(crate::Box(
                    Modifier::new().size(1.0, px_to_dp(spacer_y).max(0.0)),
                ));
            }
            if let Some(item) = items.get(i) {
                let h_dp = item_height_dp(item).max(1.0);
                let vis_top = p.y_px;
                let vis_bot = vis_top + p.h_px;
                let in_view =
                    vis_bot > scroll_offset_px && vis_top < scroll_offset_px + viewport_height_px;
                if in_view {
                    col_children[col].push(
                        crate::Box(Modifier::new().fill_max_width().height(h_dp))
                            .child(item_builder(item.clone(), i)),
                    );
                } else {
                    // Placeholder to maintain column height
                    col_children[col].push(crate::Box(Modifier::new().size(1.0, h_dp)));
                }
            }
            prev_y = p.y_px + p.h_px;
        }
        let remaining = total_content_height_px - prev_y;
        if remaining > 0.0 {
            col_children[col].push(crate::Box(
                Modifier::new().size(1.0, px_to_dp(remaining).max(0.0)),
            ));
        }
    }

    let col_views: Vec<View> = col_children
        .into_iter()
        .map(|children| {
            crate::Column(Modifier::new().flex_grow(1.0).flex_basis(0.0)).with_children(children)
        })
        .collect();

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let ch = st.content_height.get().max(st.viewport_height.get());
            Vec2 {
                x: d.x,
                y: st.scroll_immediate(d.y, ch),
            }
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
            let ch = st.content_height.get().max(st.viewport_height.get());
            st.set_offset(off, ch);
        })
    };

    let measured_h = {
        let st = state.clone();
        Rc::new(move |h: f32| {
            if (st.content_height.get() - h).abs() > 0.5 {
                st.content_height.set(h);
                st.set_offset(st.scroll_offset.get(), h);
            }
        })
    };

    let tick_scroll = {
        let st = state.clone();
        Rc::new(move || {
            let ch = st.content_height.get().max(st.viewport_height.get());
            st.tick(ch);
        })
    };

    let content = crate::Row(Modifier::new().fill_max_width().gap(gap_dp)).with_children(col_views);

    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            set_content_height: Some(Rc::new(move |h| measured_h(h))),
            get_scroll_offset: Some(get_scroll),
            set_scroll_offset: Some(set_scroll),
            show_scrollbar: true,
            tick_scroll: Some(tick_scroll),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

fn builder(_item: i32, _idx: usize) -> View {
    crate::Box(Modifier::new().size(10.0, 10.0))
}

#[test]
fn test_item_height_uniform_f32() {
    let f: f32 = 50.0;
    let item = 7;
    assert_eq!(f.get(&item), 50.0);
}

#[test]
fn test_item_height_per_item_closure() {
    let items: Vec<i32> = vec![1, 2, 3, 4, 5];
    let h = |i: &i32| 30.0 + (*i as f32) * 10.0;
    let sum_dp: f32 = items.iter().map(|i| h.get(i)).sum();
    let expected_sum: f32 = items.iter().map(|i| 30.0 + (*i as f32) * 10.0).sum();
    assert!((sum_dp - expected_sum).abs() < 0.001);
}

#[test]
fn test_lazy_column_uniform_height_compiles() {
    let state = Rc::new(LazyColumnState::new());
    let v = LazyColumn(
        vec![1, 2, 3],
        48.0_f32,
        state,
        Modifier::new().size(200.0, 400.0),
        |it: &i32| *it as u64,
        None,
        builder,
    );
    let _ = v;
}

#[test]
fn test_lazy_column_heterogeneous_heights_compiles() {
    let state = Rc::new(LazyColumnState::new());
    let v = LazyColumn(
        vec![1, 2, 3, 4, 5],
        |it: &i32| 30.0 + (*it as f32) * 12.0,
        state,
        Modifier::new().size(200.0, 400.0),
        |it: &i32| *it as u64,
        None,
        builder,
    );
    let _ = v;
}
