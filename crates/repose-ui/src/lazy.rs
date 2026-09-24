use crate::ViewExt;
use crate::anim::animate_f32_from;
use crate::lazy_states::*;
use repose_core::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn item_scope_key(state_id: usize, key: u64) -> String {
    format!("lazy_{:x}_item_{}", state_id, key)
}

fn exit_scope_key(state_id: usize, key: u64, version: u64) -> String {
    format!("lazy_{:x}_exit_{}_{}", state_id, key, version)
}

fn scoped_item_with_key<F>(full_key: String, item_key: u64, revision: u64, build: F) -> View
where
    F: FnOnce() -> View,
{
    if !repose_core::scope_cache::should_run(&full_key, revision) {
        let mut scheduler = repose_core::runtime::Scheduler::new();
        return repose_core::scope_cache::get_cached(&full_key, &mut scheduler);
    }
    repose_core::scope_cache::clear_scope_deps(&full_key);
    let previous = repose_core::runtime::COMPOSER.with(|composer| composer.borrow().cursor);
    let mut view = repose_core::scope_cache::with_scope_key(&full_key, build);
    let next = repose_core::runtime::COMPOSER.with(|composer| composer.borrow().cursor);
    view.scope_key = Some(full_key.clone());
    view.modifier.key = Some(item_key);
    view.modifier.repaint_boundary = true;
    let slot_delta = next - previous;
    repose_core::scope_cache::set_cache(&full_key, revision, view.clone(), slot_delta);
    view
}

fn scoped_item<F>(key: u64, state_id: usize, revision: u64, build: F) -> View
where
    F: FnOnce() -> View,
{
    scoped_item_with_key(item_scope_key(state_id, key), key, revision, build)
}

struct ExitingItem<T> {
    key: u64,
    item: T,
    data_index: usize,
    version: u64,
    top_px: f32,
    height_px: f32,
}

struct AnimState<T> {
    prev_keys: Vec<u64>,
    item_cache: HashMap<u64, (T, usize, f32, f32)>,
    exiting: Vec<ExitingItem<T>>,
    next_exit_version: u64,
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
    let src_len = items.len();
    if reverse_layout {
        items.reverse();
    }
    let to_data_idx = |visual_i: usize| {
        if reverse_layout {
            src_len.saturating_sub(1).saturating_sub(visual_i)
        } else {
            visual_i
        }
    };

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
            acc += Dp(*h).to_px().0;
            cum.push(acc);
        }
        cum
    };
    let padding_top_px = content_padding.top.to_px().0;
    let padding_bottom_px = content_padding.bottom.to_px().0;
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
    let mut exit_views: Vec<View> = Vec::new();
    let mut exit_extent_px = 0.0_f32;
    let state_id = Rc::as_ptr(&state) as usize;
    let current_keys: Vec<u64> = items.iter().map(&get_key).collect();
    let current_key_set: std::collections::HashSet<u64> = current_keys.iter().copied().collect();
    let current_geometry: Vec<(f32, f32)> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            (
                padding_top_px + cumulative_px[index],
                Dp(item_height.get(item).max(1.0)).to_px().0,
            )
        })
        .collect();
    let mut entering = std::collections::HashSet::new();
    let animation_id = state_id as u64;
    let animation_spec = animate_spec;

    let animation_slot = animation_spec.map(|_| {
        remember(|| {
            RefCell::new(AnimState::<T> {
                prev_keys: Vec::new(),
                item_cache: HashMap::new(),
                exiting: Vec::new(),
                next_exit_version: 1,
            })
        })
    });
    if let (Some(state_slot), Some(spec)) = (&animation_slot, animation_spec) {
        let mut animation = state_slot.borrow_mut();
        let had_prev = !animation.prev_keys.is_empty();
        for (index, item) in items.iter().enumerate() {
            animation.item_cache.insert(
                get_key(item),
                (
                    item.clone(),
                    to_data_idx(index),
                    current_geometry[index].0,
                    current_geometry[index].1,
                ),
            );
        }
        if had_prev {
            entering.extend(
                current_keys
                    .iter()
                    .filter(|key| !animation.prev_keys.contains(key))
                    .copied(),
            );
            let previous_keys = animation.prev_keys.clone();
            for key in &previous_keys {
                if current_key_set.contains(key) {
                    continue;
                }
                let Some((item, data_index, top_px, height_px)) =
                    animation.item_cache.get(key).cloned()
                else {
                    continue;
                };
                let version = animation.next_exit_version;
                animation.next_exit_version = animation.next_exit_version.wrapping_add(1);
                animation.exiting.push(ExitingItem {
                    key: *key,
                    item,
                    data_index,
                    version,
                    top_px,
                    height_px,
                });
            }
        }

        let mut still_exiting = Vec::new();
        for exit in animation.exiting.drain(..) {
            if current_key_set.contains(&exit.key) {
                continue;
            }
            let alpha = animate_f32_from(
                format!("_lz_x:{animation_id}:{}:v{}", exit.key, exit.version),
                1.0,
                0.0,
                spec,
            );
            if alpha <= 0.005 {
                continue;
            }
            exit_extent_px = exit_extent_px.max(exit.top_px + exit.height_px);
            let visible = exit.top_px + exit.height_px > scroll_offset_px
                && exit.top_px < scroll_offset_px + viewport_height_px;
            if visible {
                let revision = state.cache_revision_for_with(
                    exit.key,
                    &exit.item as *const T as usize,
                    exit.height_px,
                    alpha.to_bits() as u64,
                );
                let full_key = exit_scope_key(state_id, exit.key, exit.version);
                let top = Px(exit.top_px).to_dp().0.max(0.0);
                let height = Px(exit.height_px).to_dp().0.max(0.0);
                let exiting_item = &exit.item;
                let data_index = exit.data_index;
                let item_builder_ref = &item_builder;
                exit_views.push(scoped_item_with_key(
                    full_key,
                    exit.key,
                    revision,
                    move || {
                        crate::Box(
                            Modifier::new()
                                .absolute()
                                .offset(Some(Dp::ZERO), Some(Dp(top)), None, None)
                                .fill_max_width()
                                .height(Dp(height))
                                .alpha(alpha),
                        )
                        .child(item_builder_ref(exiting_item.clone(), data_index))
                    },
                ));
            }
            still_exiting.push(exit);
        }
        animation.exiting = still_exiting;
        let active_exit_keys: std::collections::HashSet<u64> =
            animation.exiting.iter().map(|exit| exit.key).collect();
        animation
            .item_cache
            .retain(|key, _| current_key_set.contains(key) || active_exit_keys.contains(key));
        animation.prev_keys = current_keys;
    }

    let top_padding_dp = Px(padding_top_px).to_dp().0.max(0.0);
    if top_padding_dp > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new().fill_max_width().height(Dp(top_padding_dp)),
        ));
    }
    if first_with_buffer > 0 {
        let top_spacer_px = cumulative_px[first_with_buffer];
        if top_spacer_px > 0.0 {
            combined_children.push(crate::Box(
                Modifier::new()
                    .fill_max_width()
                    .height(Dp(Px(top_spacer_px).to_dp().0.max(0.0))),
            ));
        }
    }
    for visual_index in first_with_buffer..last_with_buffer {
        let Some(item) = items.get(visual_index) else {
            continue;
        };
        let key = get_key(item);
        let height_dp = item_height.get(item).max(1.0);
        let data_index = to_data_idx(visual_index);
        let is_entering = entering.contains(&key);
        let alpha = if is_entering {
            animation_spec.map_or(1.0, |spec| {
                animate_f32_from(format!("_lz_n:{animation_id}:{key}"), 0.0, 1.0, spec)
            })
        } else {
            1.0
        };
        let variation = is_entering.then_some(alpha.to_bits() as u64).unwrap_or(0);
        let revision = state.cache_revision_for_with(
            key,
            item as *const T as usize,
            Dp(height_dp).to_px().0,
            variation,
        );
        let item_builder_ref = &item_builder;
        if is_entering {
            combined_children.push(scoped_item(key, state_id, revision, move || {
                crate::Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(Dp(height_dp))
                        .alpha(alpha),
                )
                .child(item_builder_ref(item.clone(), data_index))
            }));
        } else {
            combined_children.push(scoped_item(key, state_id, revision, move || {
                crate::Box(Modifier::new().fill_max_width().height(Dp(height_dp)))
                    .child(item_builder_ref(item.clone(), data_index))
            }));
        }
    }
    let bottom_start_px = cumulative_px
        .get(last_with_buffer)
        .copied()
        .unwrap_or_else(|| cumulative_px.last().copied().unwrap_or(0.0));
    let remaining_px = (cumulative_px.last().copied().unwrap_or(0.0) - bottom_start_px).max(0.0);
    if remaining_px > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(Dp(Px(remaining_px).to_dp().0.max(0.0))),
        ));
    }
    let bottom_padding_dp = Px(padding_bottom_px).to_dp().0.max(0.0);
    if bottom_padding_dp > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(Dp(bottom_padding_dp)),
        ));
    }
    let extra_exit_px = (exit_extent_px - content_height_px).max(0.0);
    if extra_exit_px > 0.0 {
        combined_children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(Dp(Px(extra_exit_px).to_dp().0.max(0.0))),
        ));
    }

    let content = crate::View::new(0, ViewKind::Column).with_children(combined_children);
    let content = if exit_views.is_empty() {
        content
    } else {
        crate::View::new(0, ViewKind::ZStack)
            .modifier(Modifier::new().fill_max_width())
            .child(content)
            .with_children(exit_views)
    };

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
                repose_core::request_frame();
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
                repose_core::request_frame();
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
    repose_core::View::new(0, repose_core::ViewKind::Box)
        .modifier(modifier.vertical_scroll(ScrollAxisBinding {
            on_scroll,
            set_viewport_main: Some(set_viewport),
            set_content_main: Some(Rc::new(move |h| measured_h_px(h))),
            get_offset_main: Some(get_scroll),
            set_offset_main: Some(set_scroll),
            show_scrollbar: user_scroll_enabled,
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
    let item_h_px = Dp(item_height_dp).to_px().0.max(1.0);
    let total_items = items.len();

    let rg_dp = modifier.row_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
    let row_gap_px = rg_dp.to_px().0.max(0.0);
    let stride_px = item_h_px + row_gap_px;

    let total_rows = total_items.div_ceil(columns);
    let content_height_px = if total_rows == 0 {
        content_padding.top.to_px().0 + content_padding.bottom.to_px().0
    } else {
        total_rows as f32 * item_h_px
            + (total_rows.saturating_sub(1)) as f32 * row_gap_px
            + content_padding.top.to_px().0
            + content_padding.bottom.to_px().0
    };

    let padding_top_px = content_padding.top.to_px().0;
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let padded_offset = scroll_offset_px - padding_top_px;
    let buffer_rows = 2usize;
    let first_row = ((padded_offset / stride_px).floor().max(0.0)) as usize;
    let first_row = first_row.saturating_sub(buffer_rows).min(total_rows);
    let last_row = (((padded_offset + viewport_height_px) / stride_px).ceil() as usize
        + buffer_rows)
        .min(total_rows);

    let first_item = (first_row * columns).min(total_items);
    let last_item = (last_row * columns).min(total_items);

    let mut children: Vec<View> = Vec::new();

    let top_padding_dp = Px(padding_top_px).to_dp().0.max(0.0);
    if top_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_width().height(Dp(top_padding_dp)),
        ));
    }

    if first_row > 0 {
        let top_px = (first_row as f32 * stride_px - row_gap_px).max(0.0);
        children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(Dp(Px(top_px).to_dp().0.max(0.0))),
        ));
    }

    if first_item < last_item {
        let state_id = Rc::as_ptr(&state) as usize;
        let n = items.len();
        let visible_items: Vec<View> = (first_item..last_item)
            .map(|visual_i| {
                let data_i = if reverse_layout {
                    n.saturating_sub(1).saturating_sub(visual_i)
                } else {
                    visual_i
                };
                let item = &items[data_i];
                let revision =
                    state.cache_revision_for(data_i as u64, item as *const T as usize, item_h_px);
                scoped_item(data_i as u64, state_id, revision, || {
                    item_builder(items[data_i].clone(), data_i)
                })
            })
            .collect();

        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
        let grid_mod = Modifier::new().grid(columns, rg, cg).fill_max_width();
        children.push(crate::Column(grid_mod).with_children(visible_items));
    }

    if last_row < total_rows {
        let skipped = (total_rows - last_row) as f32;
        let bottom_px = (skipped * stride_px - row_gap_px).max(0.0);
        children.push(crate::Box(
            Modifier::new()
                .fill_max_width()
                .height(Dp(Px(bottom_px).to_dp().0.max(0.0))),
        ));
    }

    // `content_padding.bottom` is already Dp (the old code double-converted).
    let bottom_padding_dp = content_padding.bottom;
    if bottom_padding_dp.0 > 0.0 {
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
        Rc::new(move |h: f32| {
            let h = h.max(0.0);
            if (st.viewport_height.get() - h).abs() > 0.5 {
                st.viewport_height.set(h);
                repose_core::request_frame();
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
                repose_core::request_frame();
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
    View::new(0, ViewKind::Box)
        .modifier(modifier.vertical_scroll(ScrollAxisBinding {
            on_scroll,
            set_viewport_main: Some(set_viewport),
            set_content_main: Some(Rc::new(move |h| measured_h(h))),
            get_offset_main: Some(get_scroll),
            set_offset_main: Some(set_scroll),
            show_scrollbar: user_scroll_enabled,
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
    let item_w_px = Dp(item_width_dp).to_px().0.max(1.0);
    let total_items = items.len();

    let cg_dp = modifier.column_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
    let col_gap_px = cg_dp.to_px().0.max(0.0);
    let stride_px = item_w_px + col_gap_px;

    let total_cols = total_items.div_ceil(rows);
    let content_width_px = if total_cols == 0 {
        content_padding.left.to_px().0 + content_padding.right.to_px().0
    } else {
        total_cols as f32 * item_w_px
            + (total_cols.saturating_sub(1)) as f32 * col_gap_px
            + content_padding.left.to_px().0
            + content_padding.right.to_px().0
    };

    let padding_left_px = content_padding.left.to_px().0;
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_width_px = state.viewport_width.get();

    let padded_offset = scroll_offset_px - padding_left_px;
    let buffer_cols = 2usize;
    let first_col = ((padded_offset / stride_px).floor().max(0.0)) as usize;
    let first_col = first_col.saturating_sub(buffer_cols).min(total_cols);
    let last_col = (((padded_offset + viewport_width_px) / stride_px).ceil() as usize
        + buffer_cols)
        .min(total_cols);

    let first_item = (first_col * rows).min(total_items);
    let last_item = (last_col * rows).min(total_items);

    let mut children: Vec<View> = Vec::new();

    let left_padding_dp = Px(padding_left_px).to_dp().0.max(0.0);
    if left_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(Dp(left_padding_dp)),
        ));
    }

    if first_col > 0 {
        let left_px = (first_col as f32 * stride_px - col_gap_px).max(0.0);
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(Dp(Px(left_px).to_dp().0.max(0.0))),
        ));
    }

    if first_item < last_item {
        let visible_count = last_item - first_item;
        let total_chunks = visible_count.div_ceil(rows);
        let state_id = Rc::as_ptr(&state) as usize;
        let state = &state;
        let n = items.len();
        let chunked: Vec<Vec<View>> = (0..total_chunks)
            .map(|ci| {
                let start = first_item + ci * rows;
                let end = (start + rows).min(last_item);
                (start..end)
                    .map(|visual_i| {
                        let data_i = if reverse_layout {
                            n.saturating_sub(1).saturating_sub(visual_i)
                        } else {
                            visual_i
                        };
                        let item = &items[data_i];
                        let revision = state.cache_revision_for(
                            data_i as u64,
                            item as *const T as usize,
                            item_w_px,
                        );
                        scoped_item(data_i as u64, state_id, revision, || {
                            item_builder(items[data_i].clone(), data_i)
                        })
                    })
                    .collect()
            })
            .collect();
        let col_mod = Modifier::new().fill_max_height().width(Dp(item_width_dp));
        let rg = modifier.row_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
        let cols: Vec<View> = chunked
            .into_iter()
            .map(|col_items| {
                let items: Vec<View> = col_items
                    .into_iter()
                    .map(|item| {
                        crate::Box(Modifier::new().flex_grow(1.0).flex_basis(Dp::ZERO)).child(item)
                    })
                    .collect();
                crate::Column(col_mod.clone().align_items(AlignItems::STRETCH).row_gap(rg))
                    .with_children(items)
            })
            .collect();
        let cg = modifier.column_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
        children
            .push(crate::Row(Modifier::new().column_gap(cg).fill_max_height()).with_children(cols));
    }

    if last_col < total_cols {
        let skipped = (total_cols - last_col) as f32;
        let right_px = (skipped * stride_px - col_gap_px).max(0.0);
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(Dp(Px(right_px).to_dp().0.max(0.0))),
        ));
    }

    // `content_padding.right` is already Dp (the old code double-converted).
    let right_padding_dp = content_padding.right;
    if right_padding_dp.0 > 0.0 {
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
                repose_core::request_frame();
            }
        })
    };

    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            if (st.content_width.get() - w).abs() > 0.5 {
                st.content_width.set(w);
                st.set_offset_x(st.scroll_offset.get(), w);
                repose_core::request_frame();
            }
        })
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> f32 { st.scroll_offset.get() })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |x: f32| {
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
    View::new(0, ViewKind::Box)
        .modifier(modifier.horizontal_scroll(ScrollAxisBinding {
            on_scroll,
            set_viewport_main: Some(set_viewport_w),
            set_content_main: Some(set_content_w),
            get_offset_main: Some(get_scroll),
            set_offset_main: Some(set_scroll),
            show_scrollbar: user_scroll_enabled,
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

    let items = items;
    let src_len = items.len();
    let to_data_idx = |visual_i: usize| {
        if reverse_layout {
            src_len.saturating_sub(1).saturating_sub(visual_i)
        } else {
            visual_i
        }
    };

    let padding_left_px = content_padding.left.to_px().0;
    let padding_right_px = content_padding.right.to_px().0;
    let item_w_px = Dp(item_width_dp).to_px().0.max(1.0);
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

    let left_padding_dp = Px(padding_left_px).to_dp().0.max(0.0);
    if left_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new().fill_max_height().width(Dp(left_padding_dp)),
        ));
    }

    if first_with_buffer > 0 {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(Dp(first_with_buffer as f32 * item_width_dp)),
        ));
    }

    let state_id = Rc::as_ptr(&state) as usize;
    for i in first_with_buffer..last_visible {
        if i >= src_len {
            continue;
        }
        let data_i = to_data_idx(i);
        if let Some(item) = items.get(data_i) {
            let revision =
                state.cache_revision_for(data_i as u64, item as *const T as usize, item_w_px);
            children.push(scoped_item(data_i as u64, state_id, revision, || {
                item_builder(item.clone(), data_i)
            }));
        }
    }

    if last_visible < items.len() {
        let remaining = items.len() - last_visible;
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(Dp(remaining as f32 * item_width_dp)),
        ));
    }

    let right_padding_dp = Px(padding_right_px).to_dp().0.max(0.0);
    if right_padding_dp > 0.0 {
        children.push(crate::Box(
            Modifier::new()
                .fill_max_height()
                .width(Dp(right_padding_dp)),
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
                repose_core::request_frame();
            }
        })
    };

    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            if (st.content_width.get() - w).abs() > 0.5 {
                st.content_width.set(w);
                st.set_offset(st.scroll_offset.get(), w);
                repose_core::request_frame();
            }
        })
    };

    let get_scroll = {
        let st = state.clone();
        Rc::new(move || -> f32 { st.scroll_offset.get() })
    };

    let set_scroll = {
        let st = state.clone();
        Rc::new(move |x: f32| {
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
    View::new(0, ViewKind::Box)
        .modifier(modifier.horizontal_scroll(ScrollAxisBinding {
            on_scroll,
            set_viewport_main: Some(set_viewport_w),
            set_content_main: Some(set_content_w),
            get_offset_main: Some(get_scroll),
            set_offset_main: Some(set_scroll),
            show_scrollbar: user_scroll_enabled,
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
    let gap_dp = modifier.row_gap.or(modifier.gap).unwrap_or(Dp::ZERO);
    let gap_px = gap_dp.to_px().0;

    let mut items = items;
    let src_len = items.len();
    if reverse_layout {
        items.reverse();
    }
    let to_data_idx = |visual_i: usize| {
        if reverse_layout {
            src_len.saturating_sub(1).saturating_sub(visual_i)
        } else {
            visual_i
        }
    };

    let heights_px: Vec<f32> = items
        .iter()
        .map(|it| Dp(item_height_dp(it).max(1.0)).to_px().0)
        .collect();
    let placements = compute_staggered_placements(&heights_px, columns, gap_px);

    let total_content_height_px = placements
        .iter()
        .map(|p| p.y_px + p.h_px)
        .fold(0.0_f32, f32::max)
        + content_padding.top.to_px().0
        + content_padding.bottom.to_px().0;

    let padding_top_px = content_padding.top.to_px().0;
    let scroll_offset_px = state.scroll_offset.get();
    let viewport_height_px = state.viewport_height.get();

    let buffer = 2;
    let mut first_visible = usize::MAX;
    let mut last_visible = 0usize;
    let mut any_visible = false;

    for (i, p) in placements.iter().enumerate() {
        let item_top = p.y_px + padding_top_px;
        let item_bot = p.y_px + p.h_px + padding_top_px;
        if item_bot > scroll_offset_px && item_top < scroll_offset_px + viewport_height_px {
            any_visible = true;
            if i < first_visible {
                first_visible = i;
            }
            if i > last_visible {
                last_visible = i;
            }
        }
    }

    let (first_idx, last_idx) = if !any_visible || items.is_empty() {
        (0, 0)
    } else {
        (
            first_visible.saturating_sub(buffer),
            (last_visible + buffer).min(items.len()),
        )
    };

    let state_id = Rc::as_ptr(&state) as usize;
    let mut col_children: Vec<Vec<View>> = (0..columns).map(|_| Vec::new()).collect();

    let top_padding_dp = Px(padding_top_px).to_dp().0.max(0.0);
    if top_padding_dp > 0.0 {
        for col_child in col_children.iter_mut() {
            col_child.push(crate::Box(
                Modifier::new().fill_max_width().height(Dp(top_padding_dp)),
            ));
        }
    }

    for (col, col_child) in col_children.iter_mut().enumerate() {
        let mut prev_y = padding_top_px;
        for (i, p) in placements.iter().enumerate() {
            if p.col != col || i < first_idx || i >= last_idx {
                continue;
            }
            let spacer_y = (p.y_px + padding_top_px) - prev_y;
            if spacer_y > 0.0 {
                col_child.push(crate::Box(
                    Modifier::new()
                        .fill_max_width()
                        .height(Dp(Px(spacer_y).to_dp().0.max(0.0))),
                ));
            }
            if let Some(item) = items.get(i) {
                let h_dp = item_height_dp(item).max(1.0);
                let vis_top = p.y_px + padding_top_px;
                let vis_bot = vis_top + p.h_px;
                let in_view =
                    vis_bot > scroll_offset_px && vis_top < scroll_offset_px + viewport_height_px;
                let data_i = to_data_idx(i);
                if in_view {
                    let revision =
                        state.cache_revision_for(data_i as u64, item as *const T as usize, p.h_px);
                    col_child.push(scoped_item(data_i as u64, state_id, revision, || {
                        crate::Box(Modifier::new().fill_max_width().height(Dp(h_dp)))
                            .child(item_builder(item.clone(), data_i))
                    }));
                } else {
                    col_child.push(crate::Box(
                        Modifier::new().fill_max_width().height(Dp(h_dp)),
                    ));
                }
            }
            prev_y = p.y_px + p.h_px + padding_top_px;
        }
        let pad_bottom_px = content_padding.bottom.to_px().0;
        let remaining = total_content_height_px - pad_bottom_px - prev_y;
        if remaining > 0.0 {
            col_child.push(crate::Box(
                Modifier::new()
                    .fill_max_width()
                    .height(Dp(Px(remaining).to_dp().0.max(0.0))),
            ));
        }
    }

    // `content_padding.bottom` is already Dp (the old code double-converted).
    let bottom_padding_dp = content_padding.bottom;
    if bottom_padding_dp.0 > 0.0 {
        for col_child in col_children.iter_mut() {
            col_child.push(crate::Box(
                Modifier::new().fill_max_width().height(bottom_padding_dp),
            ));
        }
    }

    let col_views: Vec<View> = col_children
        .into_iter()
        .map(|children| {
            crate::Column(Modifier::new().flex_grow(1.0).flex_basis(Dp::ZERO))
                .with_children(children)
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
                repose_core::request_frame();
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
                repose_core::request_frame();
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
    View::new(0, ViewKind::Box)
        .modifier(modifier.vertical_scroll(ScrollAxisBinding {
            on_scroll,
            set_viewport_main: Some(set_viewport),
            set_content_main: Some(Rc::new(move |h| measured_h(h))),
            get_offset_main: Some(get_scroll),
            set_offset_main: Some(set_scroll),
            show_scrollbar: user_scroll_enabled,
            tick: tick_scroll,
            set_nested_scroll_parent: Some(set_nested_scroll_parent),
        }))
        .with_children(vec![content])
}

#[allow(dead_code)] // test helper
fn builder(_item: i32, _idx: usize) -> View {
    crate::Box(Modifier::new().size(Dp(10.0), Dp(10.0)))
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
            modifier: Modifier::new().size(Dp(200.0), Dp(400.0)),
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
