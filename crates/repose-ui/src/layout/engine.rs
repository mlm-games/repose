#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use repose_core::*;
use repose_tree::{LayoutConstraints, NodeId, ViewTree};
use rustc_hash::{FxHashMap, FxHasher};
use taffy::TaffyTree;
use taffy::prelude::*;

use crate::Interactions;
use crate::textfield::TextFieldState;

use super::*;
impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
    pub fn layout_frame(
        &mut self,
        root: &View,
        size_px: (u32, u32),
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
    ) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
        let start = web_time::Instant::now();
        repose_text::begin_frame();
        self.stats = LayoutStats::default();

        // 0a. Reset per-frame state
        self.focus_group_stack.clear();

        // 0b. Check global invalidation
        let locals_stamp = Self::locals_stamp();
        let locals_changed = self.last_locals_stamp != Some(locals_stamp);
        if locals_changed {
            self.layout_valid = false;
            self.paint_cache.clear();
            self.text_cache.clear();
        }

        let density_scale = locals::effective_density_scale();
        let mut max_w_dp = size_px.0 as f32 / density_scale;
        let mut max_h_dp = size_px.1 as f32 / density_scale;
        if !max_w_dp.is_finite() || max_w_dp < 10.0 {
            max_w_dp = 1280.0;
        }
        if !max_h_dp.is_finite() || max_h_dp < 10.0 {
            max_h_dp = 800.0;
        }
        self.tree
            .set_subcompose_scope(repose_core::SubcomposeScope::new(
                0.0, max_w_dp, 0.0, max_h_dp,
            ));
        let root_node_id = self.tree.update(root);
        self.stats.tree = self.tree.stats.clone();

        // 1a. Build scope maps from TreeNode.scope_key (set by scope! macro)
        self.build_scope_maps();

        // 2. Determine layout need
        let size_changed = self.last_size_px != Some(size_px);
        // 2a. Publish the current window size class as a default local so that
        let class = locals::calculate_window_size_class(
            (max_w_dp * density_scale) as u32,
            (max_h_dp * density_scale) as u32,
            density_scale,
        );
        if class != locals::window_size_class() {
            locals::set_window_size_class_default(class);
        }
        locals::set_window_container_size(max_w_dp, max_h_dp);
        let has_tree_mutation =
            !self.tree.dirty_nodes().is_empty() || !self.tree.removed_ids.is_empty();
        let mut need_layout =
            size_changed || !self.layout_valid || has_tree_mutation || locals_changed;

        // Must force a style refresh for all nodes to keep physical sizes in sync with the new scale.
        if locals_changed {
            let all_ids: Vec<NodeId> = self.tree.iter_with_ids().map(|(id, _)| id).collect();
            for id in all_ids {
                self.tree.mark_dirty(id);
            }
            for st in self.scope_trees.values_mut() {
                st.valid = false;
            }
            need_layout = true;
        }

        // NOTE: Needed to ensure that text is always re-measured with the new available width
        if size_changed {
            // Root tree text cache
            for &node_id in self.text_cache.keys() {
                if let Some(&taffy_id) = self.taffy_map.get(&node_id) {
                    let _ = self.taffy.mark_dirty(taffy_id);
                }
            }
            self.text_cache.clear();
            // Scope tree text caches
            for st in self.scope_trees.values_mut() {
                for &node_id in st.text_cache.keys() {
                    if let Some(&tid) = st.taffy_map.get(&node_id) {
                        let _ = st.taffy.mark_dirty(tid);
                    }
                }
                st.text_cache.clear();
            }
        }
        if locals_changed {
            for st in self.scope_trees.values_mut() {
                st.text_cache.clear();
            }
        }

        if has_tree_mutation {
            let dirty: Vec<NodeId> = self.tree.dirty_nodes().iter().copied().collect();
            for nid in dirty {
                if self.text_cache.contains_key(&nid) {
                    if let Some(&tid) = self.taffy_map.get(&nid) {
                        let _ = self.taffy.mark_dirty(tid);
                    }
                    self.text_cache.remove(&nid);
                }
                for st in self.scope_trees.values_mut() {
                    if st.text_cache.contains_key(&nid) {
                        if let Some(&tid) = st.taffy_map.get(&nid) {
                            let _ = st.taffy.mark_dirty(tid);
                        }
                        st.text_cache.remove(&nid);
                    }
                }
                if let Some(&tid) = self.taffy_map.get(&nid) {
                    let _ = self.taffy.mark_dirty(tid);
                }
                for st in self.scope_trees.values_mut() {
                    if let Some(&tid) = st.taffy_map.get(&nid) {
                        let _ = st.taffy.mark_dirty(tid);
                    }
                }
                self.paint_cache.remove(&nid);
            }
        }

        // Helpers
        let px = |dp_val: f32| dp_to_px(dp_val);
        let font_px = |dp_font: f32| dp_to_px(dp_font) * locals::text_scale().0;

        // 3. Sync Taffy
        // 3a. Sync scope-internal TaffyTrees first
        self.sync_scope_trees(&font_px);
        // 3b. Sync root TaffyTree (non-scope nodes + scope root markers)
        self.sync_taffy_tree(root_node_id, &font_px);

        // 4. Compute Layout
        let taffy_root = self.taffy_map.get(&root_node_id).copied();
        if let Some(taffy_root) = taffy_root {
            if need_layout {
                if let Ok(mut style) = self.taffy.style(taffy_root).cloned() {
                    style.size.width = length(size_px.0 as f32);
                    style.size.height = length(size_px.1 as f32);
                    if let Err(e) = self.taffy.set_style(taffy_root, style) {
                        log::error!("taffy set_style failed for root: {e:?}");
                    }
                } else {
                    log::error!("taffy root style missing for {:?}", taffy_root);
                }

                let available = taffy::geometry::Size {
                    width: AvailableSpace::Definite(size_px.0 as f32),
                    height: AvailableSpace::Definite(size_px.1 as f32),
                };

                Self::run_measure_pass(
                    &mut self.taffy,
                    taffy_root,
                    available,
                    &self.tree,
                    &mut self.text_cache,
                    &self.reverse_map,
                    &self.scope_root_map,
                    &self.node_to_scope,
                    &mut self.scope_trees,
                    &font_px,
                    &px,
                );

                // 4a. Store Taffy-computed sizes for non-scope + scope-root nodes
                for (&node_id, &taffy_id) in &self.taffy_map {
                    if let Ok(layout) = self.taffy.layout(taffy_id) {
                        let dp_w = layout.size.width / density_scale;
                        let dp_h = layout.size.height / density_scale;
                        let rect = repose_core::Rect {
                            x: 0.0,
                            y: 0.0,
                            w: dp_w,
                            h: dp_h,
                        };
                        self.tree
                            .set_layout(node_id, rect, rect, LayoutConstraints::default());
                    }
                }

                self.last_locals_stamp = Some(locals_stamp);

                self.layout_valid = true;
                self.last_size_px = Some(size_px);
                self.stats.layout_misses += 1;
            } else {
                self.stats.layout_hits += 1;
            }
        }
        self.stats.layout_time_ms = (web_time::Instant::now() - start).as_secs_f32() * 1000.0;

        // 4.5. Advance scroll physics (pre-paint, so paint only reads offset)
        self.walk_tick(root_node_id);

        // 5. Paint
        let t_paint = web_time::Instant::now();
        self.focus_interaction_sources.clear();
        let (scene, hits, sems) = self.paint(
            root_node_id,
            textfield_states,
            interactions,
            focused,
            &font_px,
        );
        self.stats.paint_time_ms = (web_time::Instant::now() - t_paint).as_secs_f32() * 1000.0;

        // Fire focus change callbacks.
        if self.prev_focused != focused {
            if let Some(old_id) = self.prev_focused {
                if let Some(cb) = self.focus_callbacks.get(&old_id) {
                    (cb)(false);
                }
                if let Some(src) = self.focus_interaction_sources.get(&old_id) {
                    src.to_mutable().emit(Interaction::Unfocus);
                }
            }
            if let Some(new_id) = focused {
                if let Some(cb) = self.focus_callbacks.get(&new_id) {
                    (cb)(true);
                }
                if let Some(src) = self.focus_interaction_sources.get(&new_id) {
                    src.to_mutable().emit(Interaction::Focus);
                }
            }
            self.prev_focused = focused;
        }

        // Clean up callbacks for removed nodes
        for &node_id in &self.tree.removed_ids {
            if let Some(&vid) = self.view_ids.get(&node_id) {
                self.focus_callbacks.remove(&vid);
                self.focus_interaction_sources.remove(&vid);
            }
        }

        self.tree.clear_dirty();
        (scene, hits, sems)
    }

    pub fn intrinsic_size(&mut self, view: &View, mode: IntrinsicSizeMode) -> (f32, f32) {
        let px_closure = |dp_val: f32| dp_to_px(dp_val);
        let font_px_closure = |dp_font: f32| dp_to_px(dp_font) * locals::text_scale().0;

        let mut temp_taffy = taffy::TaffyTree::new();
        let root_tid = self.build_taffy_subtree(view, &mut temp_taffy, &font_px_closure);

        let avail = match mode {
            IntrinsicSizeMode::MinContent => taffy::geometry::Size {
                width: taffy::style::AvailableSpace::MinContent,
                height: taffy::style::AvailableSpace::MinContent,
            },
            IntrinsicSizeMode::MaxContent => taffy::geometry::Size {
                width: taffy::style::AvailableSpace::MaxContent,
                height: taffy::style::AvailableSpace::MaxContent,
            },
        };

        let mut text_cache: FxHashMap<NodeId, TextLayout> = FxHashMap::default();
        let reverse_map: FxHashMap<taffy::NodeId, NodeId> = FxHashMap::default();

        Self::run_measure_pass(
            &mut temp_taffy,
            root_tid,
            avail,
            &self.tree,
            &mut text_cache,
            &reverse_map,
            &self.scope_root_map,
            &self.node_to_scope,
            &mut self.scope_trees,
            &font_px_closure,
            &px_closure,
        );

        let layout = temp_taffy.layout(root_tid).ok();
        match layout {
            Some(l) => (l.size.width, l.size.height),
            None => (0.0, 0.0),
        }
    }

    pub fn new() -> Self {
        Self {
            tree: ViewTree::new(),
            taffy: TaffyTree::new(),
            taffy_map: FxHashMap::default(),
            reverse_map: FxHashMap::default(),
            scope_trees: HashMap::new(),
            scope_root_map: FxHashMap::default(),
            node_to_scope: FxHashMap::default(),
            text_cache: FxHashMap::default(),
            last_size_px: None,
            layout_valid: false,
            paint_cache: FxHashMap::default(),
            stats: LayoutStats::default(),
            last_locals_stamp: None,
            view_ids: FxHashMap::default(),
            next_view_id: 1,
            layer_id_counter: 0,
            prev_focused: None,
            focus_callbacks: FxHashMap::default(),
            focus_interaction_sources: FxHashMap::default(),
            prev_observed_rects: FxHashMap::default(),
            focus_group_stack: Vec::new(),
        }
    }

    pub(crate) fn layout_for_node(&self, node_id: NodeId) -> taffy::prelude::Layout {
        fn fallback() -> taffy::prelude::Layout {
            log::error!("layout_for_node: missing taffy layout, returning zero rect");
            taffy::prelude::Layout {
                size: taffy::geometry::Size {
                    width: 0.0,
                    height: 0.0,
                },
                location: taffy::geometry::Point { x: 0.0, y: 0.0 },
                ..Default::default()
            }
        }
        if self.scope_root_map.contains_key(&node_id) {
            if let Some(&tid) = self.taffy_map.get(&node_id) {
                if let Ok(l) = self.taffy.layout(tid) {
                    return *l;
                }
                log::error!(
                    "layout_for_node: taffy layout missing for scope root {:?}",
                    node_id
                );
                return fallback();
            }
            if let Some(parent_id) = self.tree.get(node_id).and_then(|n| n.parent)
                && let Some(outer_key) = self.node_to_scope.get(&parent_id)
                && let Some(st) = self.scope_trees.get(outer_key)
                && let Some(&tid) = st.taffy_map.get(&node_id)
            {
                if let Ok(l) = st.taffy.layout(tid) {
                    return *l;
                }
            }
            if let Some(key) = self.node_to_scope.get(&node_id)
                && let Some(st) = self.scope_trees.get(key)
                && let Some(&tid) = st.taffy_map.get(&node_id)
            {
                if let Ok(l) = st.taffy.layout(tid) {
                    return *l;
                }
            }
            return fallback();
        }
        if let Some(key) = self.node_to_scope.get(&node_id)
            && let Some(st) = self.scope_trees.get(key)
        {
            if let Some(&tid) = st.taffy_map.get(&node_id) {
                if let Ok(l) = st.taffy.layout(tid) {
                    return *l;
                }
                log::error!(
                    "layout_for_node: scope taffy layout missing for {:?}",
                    node_id
                );
                return fallback();
            }
            log::error!("layout_for_node: scope taffy_map missing for {:?}", node_id);
            return fallback();
        }
        if let Some(&tid) = self.taffy_map.get(&node_id) {
            if let Ok(l) = self.taffy.layout(tid) {
                return *l;
            }
            log::error!("layout_for_node: taffy layout missing for {:?}", node_id);
        } else {
            log::error!("layout_for_node: taffy_map missing for {:?}", node_id);
        }
        fallback()
    }

    pub(crate) fn ensure_view_id(&mut self, node_id: NodeId) -> u64 {
        if let Some(&id) = self.view_ids.get(&node_id) {
            return id;
        }
        let id = self.next_view_id;
        self.next_view_id += 1;
        self.view_ids.insert(node_id, id);
        id
    }

    pub(crate) fn locals_stamp() -> u64 {
        let mut h = FxHasher::default();

        // These affect layout measurement and/or flex direction decisions.
        locals::density().scale.to_bits().hash(&mut h);
        locals::ui_scale().0.to_bits().hash(&mut h);
        locals::text_scale().0.to_bits().hash(&mut h);

        let dir_u8 = match locals::text_direction() {
            locals::TextDirection::Ltr => 0u8,
            locals::TextDirection::Rtl => 1u8,
        };
        dir_u8.hash(&mut h);
        // For font fallback generation (ex: emoji loaded dynamically)
        repose_text::font_generation().hash(&mut h);

        h.finish()
    }
}
