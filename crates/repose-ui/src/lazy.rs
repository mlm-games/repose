use crate::ViewExt;
use crate::anim::animate_f32_from;
use crate::lazy_states::*;
use repose_core::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn scope_key(state_id: usize, key: u64) -> String {
    format!("lazy_{:x}_{}", state_id, key)
}

fn scope_item(mut view: View, key: u64, state_id: usize) -> View {
    view.scope_key = Some(scope_key(state_id, key));
    view.modifier.key = Some(key);
    view.modifier.repaint_boundary = true;
    view
}

fn scope_item_static(mut view: View, key: u64, state_id: usize) -> View {
    view.scope_key = Some(scope_key(state_id, key));
    view.modifier.key = Some(key);
    view
}

struct AnimState<T> {
    prev_keys: Vec<u64>,
    exiting: Vec<(u64, usize, T, u64, f32)>,
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
    get_key: K,
    item_builder: F,
    config: LazyColumnConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
    K: Fn(&T) -> u64 + 'static,
    H: ItemHeight<T>,
{
    let LazyColumnConfig {
        modifier,
        state,
        animate_spec,
        content_padding,
        reverse_layout,
        user_scroll_enabled,
    } = config;

    let mut items = items;
    if reverse_layout {
        items.reverse();
    }

    {
        let mut seen = std::collections::HashSet::new();
        for item in &items {
            let key = get_key(item);
            if !seen.insert(key) {
                panic!("Duplicate key {key} detected in LazyColumn. Keys must be unique.");
            }
        }
    }

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
    let padding_top_px = dp_to_px(content_padding.top);
    let padding_bottom_px = dp_to_px(content_padding.bottom);
    let content_height_px =
        *cumulative_px.last().unwrap_or(&0.0) + padding_top_px + padding_bottom_px;

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let padded_visible_start = scroll_offset_px - padding_top_px;
    let first_visible = if padded_visible_start <= 0.0 {
        0
    } else {
        match cumulative_px.binary_search_by(|p| {
            p.partial_cmp(&padded_visible_start)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    };
    let padded_visible_end = (scroll_offset_px + viewport_height_px) - padding_top_px;
    let last_visible = if padded_visible_end <= 0.0 {
        0
    } else {
        match cumulative_px.binary_search_by(|p| {
            p.partial_cmp(&padded_visible_end)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            Ok(i) => (i + 1).min(items.len()),
            Err(i) => i.min(items.len()),
        }
    };

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);
    let last_with_buffer = (last_visible + buffer).min(items.len());

    let mut combined_children: Vec<View> = Vec::new();

    let top_padding_dp = px_to_dp(padding_top_px).max(0.0);
    if top_padding_dp > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new().fill_max_width().height(top_padding_dp),
        ));
    }

    if first_with_buffer > 0 {
        let top_spacer_px = cumulative_px[first_with_buffer];
        if top_spacer_px > 0.0 {
            combined_children.push(crate::Box(
                Modifier::new()
                    .fill_max_width()
                    .height(px_to_dp(top_spacer_px).max(0.0)),
            ));
        }
    }

    let total_slots: usize;
    if let Some(spec) = animate_spec {
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

        if had_prev && !removed.is_empty() {
            for (old_idx, key) in &removed {
                if let Some(old_item) = s.item_cache.get(key) {
                    let v = s.exiting.len() as u64;
                    let h_dp = item_height.get(old_item).max(1.0);
                    let cloned = old_item.clone();
                    s.exiting.push((*key, *old_idx, cloned, v, h_dp));
                }
            }
        }

        for item in &items {
            s.item_cache.insert(get_key(item), item.clone());
        }

        let mut still_exiting: Vec<(u64, usize, T, u64, f32)> = Vec::new();
        for (key, old_idx, old_item, version, h_dp) in s.exiting.iter() {
            let exit_key = format!("_lz_x:{aid}:{key}:v{version}");
            let alpha = animate_f32_from(exit_key, 1.0, 0.0, spec);
            if alpha > 0.005 {
                still_exiting.push((*key, *old_idx, old_item.clone(), *version, *h_dp));
            }
        }

        let max_exit_slot = still_exiting
            .iter()
            .map(|(_, i, _, _, _)| *i)
            .max()
            .unwrap_or(0);
        let vis_end = last_with_buffer.max(max_exit_slot + 1 + buffer);
        total_slots = items.len().max(max_exit_slot + 1);

        let state_id = Rc::as_ptr(&state) as usize;
        let mut normal_ptr = 0usize;
        for visual_i in first_with_buffer..vis_end {
            let entry = still_exiting
                .iter()
                .find(|(_, oi, _, _, _)| *oi == visual_i);
            if let Some((key, old_idx, old_item, version, exit_h_dp)) = entry {
                let ek = format!("_lz_x:{aid}:{key}:v{version}");
                let alpha = animate_f32_from(ek, 1.0, 0.0, spec);
                let exit_top_px = cumulative_px
                    .get(*old_idx)
                    .copied()
                    .unwrap_or(*old_idx as f32 * 1.0);
                let exit_bottom_px = exit_top_px + dp_to_px(*exit_h_dp);
                let in_view =
                    exit_bottom_px > padded_visible_start && exit_top_px < padded_visible_end;
                if in_view {
                    let exit_view = item_builder(old_item.clone(), *old_idx);
                    combined_children.push(scope_item(
                        crate::Box(
                            Modifier::new()
                                .fill_max_width()
                                .height(*exit_h_dp)
                                .alpha(alpha),
                        )
                        .child(exit_view),
                        *key,
                        state_id,
                    ));
                }
            } else if let Some(item) = items.get(normal_ptr) {
                let key = get_key(item);
                let h_dp = item_height.get(item).max(1.0);
                if had_prev && added.contains(&key) {
                    let enter_key = format!("_lz_n:{aid}:{key}");
                    let alpha = animate_f32_from(enter_key, 0.0, 1.0, spec);
                    combined_children.push(scope_item(
                        crate::Box(Modifier::new().fill_max_width().height(h_dp).alpha(alpha))
                            .child(item_builder(item.clone(), normal_ptr)),
                        key,
                        state_id,
                    ));
                } else {
                    combined_children.push(scope_item(
                        crate::Box(Modifier::new().fill_max_width().height(h_dp))
                            .child(item_builder(item.clone(), normal_ptr)),
                        key,
                        state_id,
                    ));
                }
                normal_ptr += 1;
            }
        }

        s.exiting = still_exiting;
        s.prev_keys = curr_keys;
    } else {
        let state_id = Rc::as_ptr(&state) as usize;
        for i in first_with_buffer..last_with_buffer {
            if let Some(item) = items.get(i) {
                let h_dp = item_height.get(item).max(1.0);
                let key = get_key(item);
                combined_children.push(scope_item_static(
                    crate::Box(Modifier::new().fill_max_width().height(h_dp))
                        .child(item_builder(item.clone(), i)),
                    key,
                    state_id,
                ));
            }
        }
        total_slots = items.len();
    }

    let rendered_items = combined_children.len()
        - (if first_with_buffer > 0 { 1 } else { 0 })
        - (if top_padding_dp > 0.0 { 1 } else { 0 });
    if first_with_buffer + rendered_items < total_slots {
        let end_px = cumulative_px
            .get(first_with_buffer + rendered_items)
            .copied()
            .unwrap_or(content_height_px - padding_top_px - padding_bottom_px);
        let remaining_px =
            (content_height_px - padding_top_px - padding_bottom_px - end_px).max(0.0);
        if remaining_px > 0.0 {
            combined_children.push(crate::Box(
                Modifier::new()
                    .fill_max_width()
                    .height(px_to_dp(remaining_px).max(0.0)),
            ));
        }
    }

    let bottom_padding_dp = px_to_dp(padding_bottom_px).max(0.0);
    if bottom_padding_dp > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new().fill_max_width().height(bottom_padding_dp),
        ));
    }

    let content = crate::View::new(0, ViewKind::Column).with_children(combined_children);

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: repose_core::Vec2| -> repose_core::Vec2 {
            let d = run_pre_scroll(&st.parent_connection, d);
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            let leftover_y_px = st.scroll_immediate(d.y, ch);
            let result = repose_core::Vec2 {
                x: d.x,
                y: leftover_y_px,
            };
            run_post_scroll(&st.parent_connection, result)
        })
    };

    let set_viewport = {
        let st = state.clone();
        Rc::new(move |h_px: f32| {
            let h = h_px.max(0.0);
            if (st.viewport_height.get() - h).abs() > 0.5 {
                st.viewport_height.set(h);
            }
        })
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

    let on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>> = if user_scroll_enabled {
        Some(on_scroll)
    } else {
        None
    };
    let tick_scroll: Option<Rc<dyn Fn()>> = if user_scroll_enabled {
        Some(tick_scroll)
    } else {
        None
    };

    let set_nested_scroll_parent = {
        let st = state.clone();
        Rc::new(move |conn| st.set_nested_scroll_parent(conn))
    };
    repose_core::View::new(
        0,
        repose_core::ViewKind::Box,
    )
    .modifier(modifier.vertical_scroll(ScrollAxisBinding {
        on_scroll,
        set_viewport_main: Some(set_viewport),
        set_content_main: Some(Rc::new(move |h| measured_h_px(h))),
        get_offset_main: Some(get_scroll),
        set_offset_main: Some(set_scroll),
        show_scrollbar: true,
        tick: tick_scroll,
        set_nested_scroll_parent: Some(set_nested_scroll_parent),
    }))
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
    item_builder: F,
    config: LazyGridConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let LazyGridConfig {
        modifier,
        state,
        content_padding,
        reverse_layout,
        user_scroll_enabled,
    } = config;

    let columns = columns.max(1);
    let item_h_px = dp_to_px(item_height_dp).max(1.0);
    let total_items = items.len();

    let mut row_indices: Vec<usize> = (0..total_items.div_ceil(columns)).collect();
    if reverse_layout {
        row_indices.reverse();
    }

    let total_rows = row_indices.len();
    let content_height_px = total_rows as f32 * item_h_px
        + dp_to_px(content_padding.top)
        + dp_to_px(content_padding.bottom);

    let padding_top_px = dp_to_px(content_padding.top);
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let padded_offset = scroll_offset_px - padding_top_px;
    let buffer_rows = 2usize;
    let first_row = ((padded_offset / item_h_px).floor().max(0.0)) as usize;
    let first_row = first_row.saturating_sub(buffer_rows);
    let last_row = (((padded_offset + viewport_height_px) / item_h_px).ceil() as usize
        + buffer_rows)
        .min(total_rows);

    let first_item = if reverse_layout {
        let row_idx = if first_row < row_indices.len() {
            row_indices[first_row]
        } else {
            0
        };
        row_idx * columns
    } else {
        first_row * columns
    };
    let last_item = if reverse_layout {
        let row_idx = if last_row > 0 && last_row <= row_indices.len() {
            row_indices[last_row - 1]
        } else {
            0
        };
        ((row_idx + 1) * columns).min(total_items)
    } else {
        (last_row * columns).min(total_items)
    };

    let mut children: Vec<View> = Vec::new();

    let top_padding_dp = px_to_dp(padding_top_px).max(0.0);
    if top_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_width().height(top_padding_dp),
        ));
    }

    if first_row > 0 {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(first_row as f32 * item_height_dp),
        ));
    }

    if first_item < last_item {
        let state_id = Rc::as_ptr(&state) as usize;
        let visible_items: Vec<View> = (first_item..last_item)
            .map(|i| scope_item(item_builder(items[i].clone(), i), i as u64, state_id))
            .collect();

        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(0.0);
        let grid_mod = Modifier::new().grid(columns, rg, cg).fill_max_width();
        children.push(crate::Column(grid_mod).with_children(visible_items));
    }

    if last_row < total_rows {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height((total_rows - last_row) as f32 * item_height_dp),
        ));
    }

    let bottom_padding_dp = px_to_dp(content_padding.bottom).max(0.0);
    if bottom_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_width().height(bottom_padding_dp),
        ));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let d = run_pre_scroll(&st.parent_connection, d);
            let ch = st.content_height.get();
            let ch = if ch > 0.0 { ch } else { content_height_px };
            let result = Vec2 {
                x: d.x,
                y: st.scroll_immediate(d.y, ch),
            };
            run_post_scroll(&st.parent_connection, result)
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

    let on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>> = if user_scroll_enabled {
        Some(on_scroll)
    } else {
        None
    };
    let tick_scroll: Option<Rc<dyn Fn()>> = if user_scroll_enabled {
        Some(tick_scroll)
    } else {
        None
    };

    let content = crate::Column(Modifier::new().fill_max_width()).with_children(children);

    let set_nested_scroll_parent = {
        let st = state.clone();
        Rc::new(move |conn| st.set_nested_scroll_parent(conn))
    };
    View::new(
        0,
        ViewKind::Box,
    )
    .modifier(modifier.vertical_scroll(ScrollAxisBinding {
        on_scroll,
        set_viewport_main: Some(set_viewport),
        set_content_main: Some(Rc::new(move |h| measured_h(h))),
        get_offset_main: Some(get_scroll),
        set_offset_main: Some(set_scroll),
        show_scrollbar: true,
        tick: tick_scroll,
        set_nested_scroll_parent: Some(set_nested_scroll_parent),
    }))
    .with_children(vec![content])
}

/// Virtualized horizontally-scrolling grid with a fixed number of rows.
///
/// Items are arranged top-to-bottom, left-to-right. Only items visible in the
/// viewport (plus a buffer) are rendered. Each item has the same width (`item_width_dp`).
///
/// # Example
/// ```ignore
/// let state = Rc::new(LazyGridState::new());
/// LazyHorizontalGrid(
///     3,                          // rows
///     items,                      // Vec<MyItem>
///     100.0,                      // item width in dp
///     state,
///     Modifier::new().fill_max_size().gap(8.0),
///     |item, index| Card(Modifier::new().fill_max_height(), Text(format!("Item {index}"))),
/// )
/// ```
#[allow(non_snake_case)]
pub fn LazyHorizontalGrid<T, F>(
    rows: usize,
    items: Vec<T>,
    item_width_dp: f32,
    item_builder: F,
    config: LazyGridConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let LazyGridConfig {
        modifier,
        state,
        content_padding,
        reverse_layout,
        user_scroll_enabled,
    } = config;

    let rows = rows.max(1);
    let item_w_px = dp_to_px(item_width_dp).max(1.0);
    let total_items = items.len();

    let mut col_indices: Vec<usize> = (0..total_items.div_ceil(rows)).collect();
    if reverse_layout {
        col_indices.reverse();
    }

    let total_cols = col_indices.len();
    let content_width_px = total_cols as f32 * item_w_px
        + dp_to_px(content_padding.left)
        + dp_to_px(content_padding.right);

    let padding_left_px = dp_to_px(content_padding.left);
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_width_px = state.viewport_width.get();

    let padded_offset = scroll_offset_px - padding_left_px;
    let buffer_cols = 2usize;
    let first_col = ((padded_offset / item_w_px).floor().max(0.0)) as usize;
    let first_col = first_col.saturating_sub(buffer_cols);
    let last_col = (((padded_offset + viewport_width_px) / item_w_px).ceil() as usize
        + buffer_cols)
        .min(total_cols);

    let first_item = if reverse_layout {
        let col_idx = if first_col < col_indices.len() {
            col_indices[first_col]
        } else {
            0
        };
        col_idx * rows
    } else {
        first_col * rows
    };
    let last_item = if reverse_layout {
        let col_idx = if last_col > 0 && last_col <= col_indices.len() {
            col_indices[last_col - 1]
        } else {
            0
        };
        ((col_idx + 1) * rows).min(total_items)
    } else {
        (last_col * rows).min(total_items)
    };

    let mut children: Vec<View> = Vec::new();

    let left_padding_dp = px_to_dp(padding_left_px).max(0.0);
    if left_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(left_padding_dp),
        ));
    }

    if first_col > 0 {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(first_col as f32 * item_width_dp),
        ));
    }

    if first_item < last_item {
        let visible_count = last_item - first_item;
        let total_chunks = visible_count.div_ceil(rows);
        let state_id = Rc::as_ptr(&state) as usize;
        let chunked: Vec<Vec<View>> = (0..total_chunks)
            .map(|ci| {
                let start = first_item + ci * rows;
                let end = (start + rows).min(last_item);
                (start..end)
                    .map(|i| scope_item(item_builder(items[i].clone(), i), i as u64, state_id))
                    .collect()
            })
            .collect();
        let col_mod = Modifier::new().fill_max_height().width(item_width_dp);
        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
        let cols: Vec<View> = chunked
            .into_iter()
            .map(|col_items| {
                let items: Vec<View> = col_items
                    .into_iter()
                    .map(|item| {
                        crate::Box(Modifier::new().flex_grow(1.0).flex_basis(0.0)).child(item)
                    })
                    .collect();
                crate::Column(col_mod.clone().align_items(AlignItems::STRETCH).row_gap(rg))
                    .with_children(items)
            })
            .collect();
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(0.0);
        children
            .push(crate::Row(Modifier::new().column_gap(cg).fill_max_height()).with_children(cols));
    }

    if last_col < total_cols {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width((total_cols - last_col) as f32 * item_width_dp),
        ));
    }

    let right_padding_dp = px_to_dp(content_padding.right).max(0.0);
    if right_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(right_padding_dp),
        ));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let d = run_pre_scroll(&st.parent_connection, d);
            let cw = st.content_width.get();
            let cw = if cw > 0.0 { cw } else { content_width_px };
            let result = Vec2 {
                x: st.scroll_immediate_x(d.x, cw),
                y: d.y,
            };
            run_post_scroll(&st.parent_connection, result)
        })
    };

    let set_viewport_w = {
        let st = state.clone();
        Rc::new(move |w_px: f32| {
            let w = w_px.max(0.0);
            if (st.viewport_width.get() - w).abs() > 0.5 {
                st.viewport_width.set(w);
            }
        })
    };

    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            if (st.content_width.get() - w).abs() > 0.5 {
                st.content_width.set(w);
                st.set_offset_x(st.scroll_offset.get(), w);
            }
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
            st.set_offset_x(x, if cw > 0.0 { cw } else { content_width_px });
        })
    };

    let tick_scroll = {
        let st = state.clone();
        Rc::new(move || {
            let cw = st.content_width.get();
            let cw = if cw > 0.0 { cw } else { content_width_px };
            st.tick_x(cw);
        })
    };

    let on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>> = if user_scroll_enabled {
        Some(on_scroll)
    } else {
        None
    };
    let tick_scroll: Option<Rc<dyn Fn()>> = if user_scroll_enabled {
        Some(tick_scroll)
    } else {
        None
    };

    let content =
        crate::Row(Modifier::new().flex_shrink(0.0).fill_max_height()).with_children(children);

    let set_nested_scroll_parent = {
        let st = state.clone();
        Rc::new(move |conn| st.set_nested_scroll_parent(conn))
    };
    View::new(
        0,
        ViewKind::Box,
    )
    .modifier(modifier.scrollable(ScrollBothBinding {
        on_scroll,
        set_viewport_width: Some(set_viewport_w),
        set_viewport_height: None,
        set_content_width: Some(set_content_w),
        set_content_height: None,
        get_offset_xy: Some(get_scroll),
        set_offset_xy: Some(set_scroll),
        show_scrollbar: true,
        tick: tick_scroll,
        set_nested_scroll_parent: Some(set_nested_scroll_parent),
    }))
    .with_children(vec![content])
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
    item_builder: F,
    config: LazyRowConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
{
    let LazyRowConfig {
        modifier,
        state,
        content_padding,
        reverse_layout,
        user_scroll_enabled,
    } = config;

    let mut items = items;
    if reverse_layout {
        items.reverse();
    }

    let padding_left_px = dp_to_px(content_padding.left);
    let padding_right_px = dp_to_px(content_padding.right);
    let item_w_px = dp_to_px(item_width_dp).max(1.0);
    let content_width_px = items.len() as f32 * item_w_px + padding_left_px + padding_right_px;

    let scroll_offset_px = state.scroll_offset.get();
    let viewport_width_px = state.viewport_width.get();

    let padded_offset = scroll_offset_px - padding_left_px;
    let first_visible = if padded_offset <= 0.0 {
        0
    } else {
        (padded_offset / item_w_px).floor().max(0.0) as usize
    };
    let last_visible = ((padded_offset + viewport_width_px) / item_w_px).ceil() as usize + 2;

    let buffer = 2usize;
    let first_with_buffer = first_visible.saturating_sub(buffer);

    let mut children = Vec::new();

    let left_padding_dp = px_to_dp(padding_left_px).max(0.0);
    if left_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(left_padding_dp),
        ));
    }

    if first_with_buffer > 0 {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(first_with_buffer as f32 * item_width_dp),
        ));
    }

    let state_id = Rc::as_ptr(&state) as usize;
    for i in first_with_buffer..last_visible {
        if let Some(item) = items.get(i) {
            children.push(scope_item(
                item_builder(item.clone(), i),
                i as u64,
                state_id,
            ));
        }
    }

    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(remaining as f32 * item_width_dp),
        ));
    }

    let right_padding_dp = px_to_dp(padding_right_px).max(0.0);
    if right_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(right_padding_dp),
        ));
    }

    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let d = run_pre_scroll(&st.parent_connection, d);
            let cw = st.content_width.get();
            let cw = if cw > 0.0 { cw } else { content_width_px };
            let result = Vec2 {
                x: st.scroll_immediate(d.x, cw),
                y: d.y,
            };
            run_post_scroll(&st.parent_connection, result)
        })
    };

    let set_viewport_w = {
        let st = state.clone();
        Rc::new(move |w_px: f32| {
            let w = w_px.max(0.0);
            if (st.viewport_width.get() - w).abs() > 0.5 {
                st.viewport_width.set(w);
            }
        })
    };

    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            if (st.content_width.get() - w).abs() > 0.5 {
                st.content_width.set(w);
                st.set_offset(st.scroll_offset.get(), w);
            }
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

    let on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>> = if user_scroll_enabled {
        Some(on_scroll)
    } else {
        None
    };
    let tick_scroll: Option<Rc<dyn Fn()>> = if user_scroll_enabled {
        Some(tick_scroll)
    } else {
        None
    };

    let content =
        crate::Row(Modifier::new().flex_shrink(0.0).fill_max_height()).with_children(children);

    let set_nested_scroll_parent = {
        let st = state.clone();
        Rc::new(move |conn| st.set_nested_scroll_parent(conn))
    };
    View::new(
        0,
        ViewKind::Box,
    )
    .modifier(modifier.scrollable(ScrollBothBinding {
        on_scroll,
        set_viewport_width: Some(set_viewport_w),
        set_viewport_height: None,
        set_content_width: Some(set_content_w),
        set_content_height: None,
        get_offset_xy: Some(get_scroll),
        set_offset_xy: Some(set_scroll),
        show_scrollbar: true,
        tick: tick_scroll,
        set_nested_scroll_parent: Some(set_nested_scroll_parent),
    }))
    .with_children(vec![content])
}

struct StaggeredPlacement {
    col: usize,
    y_px: f32,
    h_px: f32,
}

fn compute_staggered_placements(
    heights_px: &[f32],
    columns: usize,
    gap_px: f32,
) -> Vec<StaggeredPlacement> {
    let mut placements = Vec::with_capacity(heights_px.len());
    let mut col_heights = vec![0.0_f32; columns];
    for (i, h) in heights_px.iter().enumerate() {
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
    item_builder: F,
    config: LazyVerticalStaggeredGridConfig,
) -> View
where
    T: Clone + 'static,
    F: Fn(T, usize) -> View + 'static,
    K: Fn(&T) -> f32 + 'static,
{
    let LazyVerticalStaggeredGridConfig {
        modifier,
        state,
        content_padding,
        reverse_layout,
        user_scroll_enabled,
    } = config;

    let columns = columns.max(1);
    let gap_dp = modifier.row_gap.or(modifier.gap).unwrap_or(0.0);
    let gap_px = dp_to_px(gap_dp);

    let mut items = items;
    if reverse_layout {
        items.reverse();
    }

    let heights_px: Vec<f32> = items
        .iter()
        .map(|it| dp_to_px(item_height_dp(it).max(1.0)))
        .collect();
    let placements = compute_staggered_placements(&heights_px, columns, gap_px);

    let total_content_height_px = placements
        .iter()
        .map(|p| p.y_px + p.h_px)
        .fold(0.0_f32, f32::max)
        + dp_to_px(content_padding.top)
        + dp_to_px(content_padding.bottom);

    let padding_top_px = dp_to_px(content_padding.top);
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let buffer = 2;
    let mut first_visible = usize::MAX;
    let mut last_visible = 0usize;

    for (i, p) in placements.iter().enumerate() {
        let item_top = p.y_px + padding_top_px;
        let item_bot = p.y_px + p.h_px + padding_top_px;
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

    let state_id = Rc::as_ptr(&state) as usize;
    let mut col_children: Vec<Vec<View>> = (0..columns).map(|_| Vec::new()).collect();

    let top_padding_dp = px_to_dp(padding_top_px).max(0.0);
    if top_padding_dp > 0.0 {
        for col in 0..columns {
            col_children[col].push(crate::Box(
                Modifier::new().fill_max_width().height(top_padding_dp),
            ));
        }
    }

    for col in 0..columns {
        let mut prev_y = padding_top_px;
        for (i, p) in placements.iter().enumerate() {
            if p.col != col || i < first_idx || i >= last_idx {
                continue;
            }
            let spacer_y = (p.y_px + padding_top_px) - prev_y;
            if spacer_y > 0.0 {
                col_children[col].push(crate::Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(px_to_dp(spacer_y).max(0.0)),
                ));
            }
            if let Some(item) = items.get(i) {
                let h_dp = item_height_dp(item).max(1.0);
                let vis_top = p.y_px + padding_top_px;
                let vis_bot = vis_top + p.h_px;
                let in_view =
                    vis_bot > scroll_offset_px && vis_top < scroll_offset_px + viewport_height_px;
                if in_view {
                    col_children[col].push(scope_item(
                        crate::Box(Modifier::new().fill_max_width().height(h_dp))
                            .child(item_builder(item.clone(), i)),
                        i as u64,
                        state_id,
                    ));
                } else {
                    col_children[col]
                        .push(crate::Box(Modifier::new().fill_max_width().height(h_dp)));
                }
            }
            prev_y = p.y_px + p.h_px + padding_top_px;
        }
        let remaining = total_content_height_px - prev_y;
        if remaining > 0.0 {
            col_children[col].push(crate::Box(
                Modifier::new()
                    .fill_max_width()
                    .height(px_to_dp(remaining).max(0.0)),
            ));
        }
    }

    let bottom_padding_dp = px_to_dp(content_padding.bottom).max(0.0);
    if bottom_padding_dp > 0.0 {
        for col in 0..columns {
            col_children[col].push(crate::Box(
                Modifier::new().fill_max_width().height(bottom_padding_dp),
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
            let d = run_pre_scroll(&st.parent_connection, d);
            let ch = st.content_height.get().max(st.viewport_height.get());
            let result = Vec2 {
                x: d.x,
                y: st.scroll_immediate(d.y, ch),
            };
            run_post_scroll(&st.parent_connection, result)
        })
    };

    let set_viewport = {
        let st = state.clone();
        Rc::new(move |h_px: f32| {
            let h = h_px.max(0.0);
            if (st.viewport_height.get() - h).abs() > 0.5 {
                st.viewport_height.set(h);
            }
        })
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> f32 { st.scroll_offset.get() })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |off: f32| {
            let ch = st.content_height.get().max(0.0);
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

    let on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>> = if user_scroll_enabled {
        Some(on_scroll)
    } else {
        None
    };
    let tick_scroll: Option<Rc<dyn Fn()>> = if user_scroll_enabled {
        Some(tick_scroll)
    } else {
        None
    };

    let content = crate::Row(Modifier::new().fill_max_width().gap(gap_dp)).with_children(col_views);

    let set_nested_scroll_parent = {
        let st = state.clone();
        Rc::new(move |conn| st.set_nested_scroll_parent(conn))
    };
    View::new(
        0,
        ViewKind::Box,
    )
    .modifier(modifier.vertical_scroll(ScrollAxisBinding {
        on_scroll,
        set_viewport_main: Some(set_viewport),
        set_content_main: Some(Rc::new(move |h| measured_h(h))),
        get_offset_main: Some(get_scroll),
        set_offset_main: Some(set_scroll),
        show_scrollbar: true,
        tick: tick_scroll,
        set_nested_scroll_parent: Some(set_nested_scroll_parent),
    }))
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
    let v = LazyColumn(
        vec![1, 2, 3],
        48.0_f32,
        |it: &i32| *it as u64,
        builder,
        LazyColumnConfig {
            modifier: Modifier::new().size(200.0, 400.0),
            ..Default::default()
        },
    );
    let _ = v;
}

#[test]
fn test_lazy_column_heterogeneous_heights_compiles() {
    let v = LazyColumn(
        vec![1, 2, 3, 4, 5],
        |it: &i32| 30.0 + (*it as f32) * 12.0,
        |it: &i32| *it as u64,
        builder,
        LazyColumnConfig::default(),
    );
    let _ = v;
}
