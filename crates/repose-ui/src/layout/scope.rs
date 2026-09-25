#![allow(non_snake_case)]

use repose_core::*;
use repose_tree::NodeId;
use rustc_hash::{FxHashMap, FxHashSet};

use super::*;

impl LayoutEngine {
    pub(crate) fn build_scope_maps(&mut self) {
        self.scope_root_map.clear();
        self.node_to_scope.clear();
        self.scope_root_ids.clear();

        let mut scope_roots: Vec<(u32, NodeId, String)> = Vec::new();
        for (id, node) in self.tree.iter_with_ids() {
            if let Some(ref key) = node.scope_key {
                scope_roots.push((node.depth, id, key.clone()));
            }
        }
        scope_roots.sort_by(|a, b| b.0.cmp(&a.0));

        for (_, node_id, key) in &scope_roots {
            self.scope_root_map.insert(*node_id, key.clone());
            self.scope_root_ids
                .entry(key.clone())
                .or_default()
                .push(*node_id);
            self.mark_scope_subtree(*node_id, key);
        }

        let active_keys: FxHashSet<String> = self.scope_root_ids.keys().cloned().collect();
        self.scope_trees.retain(|key, _| active_keys.contains(key));
        for key in active_keys {
            self.scope_trees
                .entry(key)
                .or_insert_with(ScopeLayoutTree::new);
        }
        self.scope_maps_valid = true;
    }

    pub(crate) fn mark_scope_subtree(&mut self, root_id: NodeId, key: &str) {
        let mut stack = vec![root_id];
        while let Some(id) = stack.pop() {
            // Don't cross into nested scope boundaries (they handle their own marking).
            if id != root_id && self.scope_root_map.contains_key(&id) {
                continue;
            }
            self.node_to_scope.insert(id, key.to_string());
            self.ensure_view_id(id);
            if let Some(node) = self.tree.get(id) {
                stack.extend(node.children.iter().copied());
            }
        }
    }

    pub(crate) fn sync_scope_trees(&mut self, font_px: &dyn Fn(Sp) -> f32) {
        let dirty_nodes: FxHashSet<NodeId> = self.tree.dirty_nodes().iter().copied().collect();
        let mut dirty_by_scope: FxHashMap<String, Vec<NodeId>> = FxHashMap::default();
        for node_id in &dirty_nodes {
            if let Some(key) = self.node_to_scope.get(node_id) {
                dirty_by_scope
                    .entry(key.clone())
                    .or_default()
                    .push(*node_id);
            }
        }

        let scope_keys: Vec<String> = self.scope_trees.keys().cloned().collect();
        for key in scope_keys {
            let mut updated_nodes = FxHashSet::default();
            let mut changed = false;
            let stale: Vec<NodeId> = self
                .scope_trees
                .get(&key)
                .map(|st| {
                    st.taffy_map
                        .keys()
                        .copied()
                        .filter(|node_id| self.tree.get(*node_id).is_none())
                        .collect()
                })
                .unwrap_or_default();
            if let Some(st) = self.scope_trees.get_mut(&key) {
                for node_id in stale {
                    if let Some(tid) = st.taffy_map.remove(&node_id) {
                        let _ = st.taffy.remove(tid);
                        st.reverse_map.remove(&tid);
                    }
                    st.text_cache.remove(&node_id);
                    self.paint_cache.remove(&node_id);
                    self.view_ids.remove(&node_id);
                    changed = true;
                }
            }

            if let Some(nodes) = dirty_by_scope.get(&key) {
                for &node_id in nodes {
                    self.update_scope_taffy_node(&key, node_id, font_px, &mut updated_nodes);
                    changed = true;
                }
            }

            if !changed && let Some(root_ids) = self.scope_root_ids.get(&key) {
                for &root_id in root_ids {
                    let mut cur = self.tree.get(root_id).and_then(|node| node.parent);
                    let mut ancestor_changed = false;
                    while let Some(pid) = cur {
                        if dirty_nodes.contains(&pid)
                            && let Some(parent) = self.tree.get(pid)
                            && (parent.modifier.transform.is_some()
                                || parent.modifier.alpha.is_some()
                                || parent.modifier.graphics_layer.is_some())
                        {
                            ancestor_changed = true;
                            break;
                        }
                        cur = self.tree.get(pid).and_then(|node| node.parent);
                    }
                    if ancestor_changed {
                        changed = true;
                        break;
                    }
                }
            }

            if let Some(root_ids) = self.scope_root_ids.get(&key).cloned() {
                for root_id in root_ids {
                    let exists = self
                        .scope_trees
                        .get(&key)
                        .is_some_and(|st| st.taffy_map.contains_key(&root_id));
                    if !exists {
                        self.update_scope_taffy_node(&key, root_id, font_px, &mut updated_nodes);
                        changed = true;
                    }
                }
            }

            if changed && let Some(st) = self.scope_trees.get_mut(&key) {
                st.valid = false;
            }
        }
    }

    pub(crate) fn update_scope_taffy_node(
        &mut self,
        scope_key: &str,
        node_id: NodeId,
        font_px: &dyn Fn(Sp) -> f32,
        updated_nodes: &mut FxHashSet<NodeId>,
    ) -> taffy::NodeId {
        if updated_nodes.contains(&node_id)
            && let Some(&taffy_id) = self
                .scope_trees
                .get(scope_key)
                .and_then(|st| st.taffy_map.get(&node_id))
        {
            return taffy_id;
        }
        updated_nodes.insert(node_id);
        let _ = self.ensure_view_id(node_id);

        // Extract node data before borrowing scope_trees to avoid borrow conflicts
        let node = self.tree.get(node_id).unwrap();
        let style = self.style_from_node(node, font_px);
        let ctx = self.context_from_node(node);
        let children = node.children.clone();
        let is_zstack = matches!(node.kind, ViewKind::ZStack);
        let scroll_axis = node.modifier.scroll.as_ref().map(|s| s.axis());
        let viewport_main_is_definite = scroll_axis
            .map(|a| LayoutEngine::viewport_main_is_definite(&node.modifier, a))
            .unwrap_or(false);
        let _ = node;

        let child_tids: Vec<taffy::NodeId> = children
            .iter()
            .map(|&c| {
                if c != node_id && self.scope_root_map.contains_key(&c) {
                    self.scope_leaf_marker(scope_key, c, font_px)
                } else {
                    self.update_scope_taffy_node(scope_key, c, font_px, updated_nodes)
                }
            })
            .collect();

        let is_root = self.scope_root_map.contains_key(&node_id);
        let st = self.scope_trees.get_mut(scope_key).unwrap();
        if let Some(&t_id) = st.taffy_map.get(&node_id) {
            let _ = st.taffy.set_style(t_id, style);
            let _ = st.taffy.set_node_context(t_id, Some(ctx));
            let _ = st.taffy.mark_dirty(t_id);
            let _ = st.taffy.set_children(t_id, &child_tids);
            if is_root {
                st.root_taffy_id = Some(t_id);
            }
            let _ = st;
            let st = self.scope_trees.get_mut(scope_key).unwrap();
            Self::make_children_absolute_on(is_zstack, &child_tids, &mut st.taffy);
            if let Some(axis) = scroll_axis {
                Self::apply_scroll_content_styles(
                    axis,
                    viewport_main_is_definite,
                    &child_tids,
                    &mut st.taffy,
                );
            }
            t_id
        } else {
            let t_id = if child_tids.is_empty() {
                st.taffy.new_leaf_with_context(style, ctx).unwrap()
            } else {
                let t = st.taffy.new_with_children(style, &child_tids).unwrap();
                let _ = st.taffy.set_node_context(t, Some(ctx));
                t
            };
            st.taffy_map.insert(node_id, t_id);
            st.reverse_map.insert(t_id, node_id);
            if is_root {
                st.root_taffy_id = Some(t_id);
            }
            let _ = st;
            let st = self.scope_trees.get_mut(scope_key).unwrap();
            Self::make_children_absolute_on(is_zstack, &child_tids, &mut st.taffy);
            if let Some(axis) = scroll_axis {
                Self::apply_scroll_content_styles(
                    axis,
                    viewport_main_is_definite,
                    &child_tids,
                    &mut st.taffy,
                );
            }
            t_id
        }
    }

    fn scope_leaf_marker(
        &mut self,
        scope_key: &str,
        node_id: NodeId,
        font_px: &dyn Fn(Sp) -> f32,
    ) -> taffy::NodeId {
        let _ = self.ensure_view_id(node_id);
        let node = self.tree.get(node_id).unwrap();
        let style = self.style_from_node(node, font_px);
        let ctx = self.context_from_node(node);
        let _ = node;
        let st = self.scope_trees.get_mut(scope_key).unwrap();
        if let Some(&t_id) = st.taffy_map.get(&node_id) {
            let _ = st.taffy.set_style(t_id, style);
            let _ = st.taffy.set_node_context(t_id, Some(ctx));
            let _ = st.taffy.set_children(t_id, &[]);
            t_id
        } else {
            let t_id = st.taffy.new_leaf_with_context(style, ctx).unwrap();
            st.taffy_map.insert(node_id, t_id);
            st.reverse_map.insert(t_id, node_id);
            t_id
        }
    }
}
