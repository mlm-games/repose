#![allow(non_snake_case)]


use repose_core::*;
use repose_tree::NodeId;
use rustc_hash::FxHashMap;


use super::*;

impl LayoutEngine {
    pub(crate) fn build_scope_maps(&mut self) {
        self.scope_root_map.clear();
        self.node_to_scope.clear();

        // Collect scope boundary nodes, sorted deepest-first so that inner
        // scopes overwrite outer scope markings in `node_to_scope`.
        let mut scope_roots: Vec<(NodeId, String)> = Vec::new();
        for (id, node) in self.tree.iter_with_ids() {
            if let Some(ref key) = node.scope_key {
                scope_roots.push((id, key.clone()));
            }
        }
        scope_roots.sort_by(|a, b| {
            // Deeper scopes (higher depth) first -> their subtree marking
            // wins over shallower enclosing scopes.
            let depth_a = self.tree.get(a.0).map(|n| n.depth).unwrap_or(0);
            let depth_b = self.tree.get(b.0).map(|n| n.depth).unwrap_or(0);
            depth_b.cmp(&depth_a)
        });

        for (node_id, key) in &scope_roots {
            self.scope_root_map.insert(*node_id, key.clone());
            self.mark_scope_subtree(*node_id, key);
        }

        // Create scope trees for new keys, clean up stale ones
        let active_keys: Vec<String> = self.scope_root_map.values().cloned().collect();
        self.scope_trees.retain(|k, _| active_keys.contains(k));
        for key in &active_keys {
            self.scope_trees
                .entry(key.clone())
                .or_insert_with(|| ScopeLayoutTree::new(key.clone()));
        }
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

    pub(crate) fn sync_scope_trees(&mut self, font_px: &dyn Fn(f32) -> f32) {
        let removed_ids: Vec<NodeId> = self.tree.removed_ids.iter().copied().collect();
        let dirty_nodes: Vec<NodeId> = self.tree.dirty_nodes().iter().copied().collect();
        let scope_keys: Vec<String> = self.scope_trees.keys().cloned().collect();
        let node_to_scope: FxHashMap<NodeId, String> = self
            .node_to_scope
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();
        let scope_root_map: FxHashMap<NodeId, String> = self
            .scope_root_map
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        for key in &scope_keys {
            let mut changed = false;

            // Removals from scope tree
            for &node_id in &removed_ids {
                if node_to_scope.get(&node_id).map(|k| k.as_str()) != Some(key) {
                    continue;
                }
                if let Some(st) = self.scope_trees.get_mut(key) {
                    if let Some(tid) = st.taffy_map.remove(&node_id) {
                        let _ = st.taffy.remove(tid);
                        st.reverse_map.remove(&tid);
                        st.text_cache.remove(&node_id);
                        changed = true;
                    }
                }
            }

            // Dirty scope-internal nodes
            for &node_id in &dirty_nodes {
                if node_to_scope.get(&node_id).map(|k| k.as_str()) != Some(key) {
                    continue;
                }
                self.update_scope_taffy_node(key, node_id, font_px);
                changed = true;
            }

            // Ensure scope root exists
            let root_ids: Vec<NodeId> = scope_root_map
                .iter()
                .filter(|(_, k)| k.as_str() == key)
                .map(|(id, _)| *id)
                .collect();
            for &root_id in &root_ids {
                let exists = self
                    .scope_trees
                    .get(key)
                    .map(|st| st.taffy_map.contains_key(&root_id))
                    .unwrap_or(false);
                if !exists {
                    self.update_scope_taffy_node(key, root_id, font_px);
                    changed = true;
                }
            }

            // Invalidate scope tree layout when content changed
            if changed {
                if let Some(st) = self.scope_trees.get_mut(key) {
                    st.valid = false;
                }
            }
        }
    }

    pub(crate) fn update_scope_taffy_node(
        &mut self,
        scope_key: &str,
        node_id: NodeId,
        font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::NodeId {
        let _ = self.ensure_view_id(node_id);

        // Extract node data before borrowing scope_trees to avoid borrow conflicts
        let node = self.tree.get(node_id).unwrap();
        let style = self.style_from_node(node, font_px);
        let ctx = self.context_from_node(node);
        let children = node.children.clone();
        let is_zstack = matches!(node.kind, ViewKind::ZStack);
        let scroll_axis = node.modifier.scroll.as_ref().map(|s| s.axis());
        drop(node);

        let child_tids: Vec<taffy::NodeId> = children
            .iter()
            .map(|&c| {
                if c != node_id && self.scope_root_map.contains_key(&c) {
                    self.scope_leaf_marker(scope_key, c, font_px)
                } else {
                    self.update_scope_taffy_node(scope_key, c, font_px)
                }
            })
            .collect();

        let is_root = self.scope_root_map.contains_key(&node_id);
        let st = self.scope_trees.get_mut(scope_key).unwrap();
        if let Some(&t_id) = st.taffy_map.get(&node_id) {
            let _ = st.taffy.set_style(t_id, style);
            let _ = st.taffy.set_node_context(t_id, Some(ctx));
            let _ = st.taffy.set_children(t_id, &child_tids);
            if is_root {
                st.root_taffy_id = Some(t_id);
            }
            drop(st);
            let st = self.scope_trees.get_mut(scope_key).unwrap();
            Self::make_children_absolute_on(is_zstack, &child_tids, &mut st.taffy);
            if let Some(axis) = scroll_axis {
                Self::apply_scroll_content_styles(axis, &child_tids, &mut st.taffy);
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
            drop(st);
            let st = self.scope_trees.get_mut(scope_key).unwrap();
            Self::make_children_absolute_on(is_zstack, &child_tids, &mut st.taffy);
            if let Some(axis) = scroll_axis {
                Self::apply_scroll_content_styles(axis, &child_tids, &mut st.taffy);
            }
            t_id
        }
    }

    fn scope_leaf_marker(
        &mut self,
        scope_key: &str,
        node_id: NodeId,
        font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::NodeId {
        let _ = self.ensure_view_id(node_id);
        let node = self.tree.get(node_id).unwrap();
        let style = self.style_from_node(node, font_px);
        let ctx = self.context_from_node(node);
        drop(node);
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
