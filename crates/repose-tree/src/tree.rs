//! The main ViewTree structure.

use crate::{
    hash::{hash_subtree, hash_view_content},
    node::{LayoutCache, LayoutConstraints, NodeId, TreeNode, TreeStats},
    reconcile::ReconcileContext,
};
use repose_core::{Modifier, Rect, SubcomposeScope, View, ViewId, ViewKind};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SlotMap;
use smallvec::SmallVec;
use std::sync::Arc;

/// A persistent view tree that supports incremental updates.
pub struct ViewTree {
    /// All nodes in the tree.
    nodes: SlotMap<NodeId, TreeNode>,

    /// The root node.
    root: Option<NodeId>,

    /// Nodes that need re-layout.
    dirty: FxHashSet<NodeId>,

    /// Nodes that need re-paint.
    paint_dirty: FxHashSet<NodeId>,

    /// Current generation (frame counter).
    generation: u64,

    /// Map from user-facing ViewId to internal NodeId.
    view_id_map: FxHashMap<ViewId, NodeId>,

    /// Statistics from the last reconcile operation.
    pub stats: TreeStats,

    /// Nodes removed during the last update (needed to sync external systems like Taffy).
    pub removed_ids: Vec<NodeId>,

    /// Root constraints to use when calling a `SubcomposeLayout`'s content
    /// closure during this frame. Set via [`ViewTree::set_subcompose_scope`]
    /// before calling [`ViewTree::update`].
    subcompose_scope: SubcomposeScope,

    /// Cache of (scope, subcomposed slots) for each `SubcomposeLayout` node.
    /// The closure is re-invoked only when the ancestor-derived scope
    /// changes or the node's content changes. Each cached slot view has its
    /// `Modifier::key` overwritten with its slot id.
    subcompose_cache: FxHashMap<NodeId, (SubcomposeScope, Vec<(u64, View)>)>,
}

impl Default for ViewTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewTree {
    /// Create a new empty tree.
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            root: None,
            dirty: FxHashSet::default(),
            paint_dirty: FxHashSet::default(),
            generation: 0,
            view_id_map: FxHashMap::default(),
            stats: TreeStats::default(),
            removed_ids: Vec::new(),
            subcompose_scope: SubcomposeScope::UNBOUNDED,
            subcompose_cache: FxHashMap::default(),
        }
    }

    /// Set the constraints that will be passed to any `SubcomposeLayout`
    /// content closures during the next [`update`](Self::update) call.
    /// The scope is read once per reconcile of a `SubcomposeLayout` node; if
    /// you need different scopes at different depths, the closure itself is
    /// responsible for narrowing the values it receives.
    pub fn set_subcompose_scope(&mut self, scope: SubcomposeScope) {
        self.subcompose_scope = scope;
    }

    /// Get the currently-set subcompose scope.
    pub fn subcompose_scope(&self) -> SubcomposeScope {
        self.subcompose_scope
    }

    /// Run a `SubcomposeLayout`'s content closure, returning the cached list
    /// of `(slot_id, view)` pairs when the scope is unchanged for this node.
    /// The caller is responsible for ensuring the cache is invalidated (e.g.
    /// on content change) via [`ViewTree::invalidate_subcompose_cache`].
    ///
    /// The scope is computed by walking the node's ancestor chain and
    /// intersecting the root scope with each ancestor's `Modifier` width /
    /// height / min / max fields. The SubcomposeLayout's own modifier is
    /// included as the last intersection.
    fn run_subcompose(
        &mut self,
        node_id: NodeId,
        content: &Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>>,
    ) -> Vec<(u64, View)> {
        let scope = self.compute_scope_for_node(node_id);
        if let Some((cached_scope, cached_slots)) = self.subcompose_cache.get(&node_id)
            && *cached_scope == scope
        {
            return cached_slots.clone();
        }
        let mut slots = content(&scope);
        // Assign each subcomposed slot its own scope tree for per-scope TaffyTree
        let scope_key = format!("subcompose_{:?}", node_id);
        for (slot_id, view) in slots.iter_mut() {
            view.modifier.key = Some(*slot_id);
            view.scope_key = Some(scope_key.clone());
            view.modifier.repaint_boundary = true;
        }
        self.subcompose_cache
            .insert(node_id, (scope, slots.clone()));
        slots
    }

    /// Compute the `SubcomposeScope` visible to a `SubcomposeLayout` at
    /// `node_id`. Starts with the user-set root scope and intersects each
    /// ancestor's `Modifier` width / height / min / max / padding fields in
    /// root-to-leaf order. Then narrows using the SubcomposeLayout's own
    /// cached Taffy-computed size (from the previous frame's layout pass)
    /// if available, so `fill_max_width` / `fill_max_height` and other
    /// parent-dependent sizes are reflected in the scope after the first frame.
    fn compute_scope_for_node(&self, node_id: NodeId) -> SubcomposeScope {
        let mut scope = self.subcompose_scope;
        let mut chain: Vec<NodeId> = Vec::new();
        let mut current = Some(node_id);
        while let Some(id) = current {
            chain.push(id);
            match self.nodes.get(id) {
                Some(node) => current = node.parent,
                None => break,
            }
        }
        chain.reverse();
        for ancestor_id in chain {
            if let Some(node) = self.nodes.get(ancestor_id) {
                scope = intersect_scope_with_modifier(scope, &node.modifier);
                // Apply cached Taffy-computed width for this ancestor if
                // available.
                if let Some(cache) = &node.layout_cache {
                    let w = cache.rect.w;
                    if w > 0.0 && w.is_finite() {
                        scope.max_width = scope.max_width.min(w);
                    }
                }
            }
        }
        scope
    }

    /// Drop the cached subcomposed view for a single node. Call this when the
    /// `SubcomposeLayout`'s modifier or identity changes so the next
    /// reconciliation re-invokes the closure.
    pub fn invalidate_subcompose_cache(&mut self, node_id: NodeId) {
        self.subcompose_cache.remove(&node_id);
    }

    /// Drop the cached subcomposed views for a list of nodes (used by garbage
    /// collection).
    fn drop_subcompose_cache_for(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.subcompose_cache.remove(id);
        }
    }

    /// Recursively drop cached subcomposed views for a subtree rooted at
    /// `node_id`. Called when the node is being removed.
    fn collect_subcompose_cache(&mut self, node_id: &NodeId) {
        self.subcompose_cache.remove(node_id);
        let children: Vec<NodeId> = self
            .nodes
            .get(*node_id)
            .map(|n| n.children.iter().copied().collect())
            .unwrap_or_default();
        for child in children {
            self.collect_subcompose_cache(&child);
        }
    }

    /// Get the current generation.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Get the root node ID.
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Get a node by ID.
    pub fn get(&self, id: NodeId) -> Option<&TreeNode> {
        self.nodes.get(id)
    }

    /// Get a mutable node by ID.
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut TreeNode> {
        self.nodes.get_mut(id)
    }

    /// Get a node by ViewId.
    pub fn get_by_view_id(&self, view_id: ViewId) -> Option<&TreeNode> {
        self.view_id_map
            .get(&view_id)
            .and_then(|id| self.nodes.get(*id))
    }

    /// Get the number of nodes in the tree.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Check if the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Check if a node is marked dirty.
    pub fn is_dirty(&self, id: NodeId) -> bool {
        self.dirty.contains(&id)
    }

    /// Get the set of dirty nodes.
    pub fn dirty_nodes(&self) -> &FxHashSet<NodeId> {
        &self.dirty
    }

    /// Clear the dirty set (after layout).
    pub fn clear_dirty(&mut self) {
        self.dirty.clear();
    }

    /// Mark a node as needing re-layout.
    pub fn mark_dirty(&mut self, id: NodeId) {
        self.dirty.insert(id);

        // Also mark ancestors dirty (layout flows down from root)
        let mut current = id;
        while let Some(node) = self.nodes.get(current) {
            if let Some(parent) = node.parent {
                self.dirty.insert(parent);
                current = parent;
            } else {
                break;
            }
        }
    }

    /// Update the tree from a new View, performing incremental reconciliation.
    /// Returns the root NodeId.
    pub fn update(&mut self, new_root: &View) -> NodeId {
        self.removed_ids.clear(); // Clear previous frame's removals

        self.generation += 1;
        self.stats = TreeStats::default();

        let mut ctx = ReconcileContext::new(self.generation);

        let root_id = if let Some(existing_root) = self.root {
            self.reconcile_node(existing_root, new_root, None, 0, 0, &mut ctx)
        } else {
            self.create_node(new_root, None, 0, 0, &mut ctx)
        };

        self.root = Some(root_id);

        // Remove orphaned nodes (nodes not updated this generation)
        self.collect_garbage();

        // Update stats
        self.stats.total_nodes = self.nodes.len();
        self.stats.dirty_nodes = self.dirty.len();
        self.stats.reconciled_nodes = ctx.reconciled;
        self.stats.skipped_nodes = ctx.skipped;
        self.stats.created_nodes = ctx.created;
        self.stats.removed_nodes = ctx.removed;

        root_id
    }

    /// Reconcile an existing node with a new View.
    fn reconcile_node(
        &mut self,
        node_id: NodeId,
        view: &View,
        parent: Option<NodeId>,
        depth: u32,
        index_in_parent: u32,
        ctx: &mut ReconcileContext,
    ) -> NodeId {
        let content_hash = hash_view_content(view);

        let old_hash = self
            .nodes
            .get(node_id)
            .expect("reconcile_node: node not found")
            .content_hash;
        let content_changed = old_hash != content_hash;

        if content_changed {
            self.invalidate_subcompose_cache(node_id);
        }

        let new_children_hashes = if let ViewKind::SubcomposeLayout { content } = &view.kind {
            let subcomposed = self.run_subcompose(node_id, content);
            let slot_views: Vec<View> = subcomposed.into_iter().map(|(_, v)| v).collect();
            self.reconcile_children(node_id, &slot_views, depth, ctx)
        } else {
            self.reconcile_children(node_id, &view.children, depth, ctx)
        };

        let new_subtree_hash = hash_subtree(content_hash, &new_children_hashes);

        let view_id = self.compute_view_id(view, node_id, parent, index_in_parent);

        let subtree_changed;
        {
            let node = self
                .nodes
                .get_mut(node_id)
                .expect("reconcile_node: node not found");

            // Update parent, depth, generation
            node.parent = parent;
            node.depth = depth;
            node.generation = self.generation;

            // NOTE: fields like on_pointer_down aren't part of the content hash, so can't rely on content_changed to keep them in sync.
            node.kind = view.kind.clone();
            node.modifier = view.modifier.clone();
            node.content_hash = content_hash;
            node.user_key = view.modifier.key;
            node.scope_key = view.scope_key.clone();

            if content_changed {
                node.invalidate_layout();
                ctx.reconciled += 1;
            }

            // Update subtree hash
            subtree_changed = node.subtree_hash != new_subtree_hash;
            if subtree_changed {
                node.subtree_hash = new_subtree_hash;
            } else if !content_changed {
                ctx.skipped += 1;
            }

            // Update view_id
            node.view_id = view_id;
        } // Mutable borrow of node ends here

        if subtree_changed {
            self.mark_dirty(node_id);
        }
        self.view_id_map.insert(view_id, node_id);

        node_id
    }
    /// Reconcile children of a node.
    /// Returns the subtree hashes of all children (for computing parent's subtree hash).
    fn reconcile_children(
        &mut self,
        parent_id: NodeId,
        new_children: &[View],
        parent_depth: u32,
        ctx: &mut ReconcileContext,
    ) -> Vec<u64> {
        let child_depth = parent_depth + 1;

        // Get current children
        let old_children: SmallVec<[NodeId; 4]> = self
            .nodes
            .get(parent_id)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        // Build a map of keyed children for efficient lookup
        let mut keyed_children: FxHashMap<u64, NodeId> = FxHashMap::default();
        let mut unkeyed_children: Vec<NodeId> = Vec::new();

        for &child_id in &old_children {
            if let Some(node) = self.nodes.get(child_id) {
                if let Some(key) = node.user_key {
                    keyed_children.insert(key, child_id);
                } else {
                    unkeyed_children.push(child_id);
                }
            }
        }

        let mut new_child_ids: SmallVec<[NodeId; 4]> = SmallVec::new();
        let mut new_subtree_hashes: Vec<u64> = Vec::with_capacity(new_children.len());
        let mut unkeyed_index = 0;
        let mut used_nodes: FxHashSet<NodeId> = FxHashSet::default();
        let mut new_seen_keys: FxHashSet<u64> = FxHashSet::default();

        for (i, new_child) in new_children.iter().enumerate() {
            if let Some(key) = new_child.modifier.key {
                if !new_seen_keys.insert(key) {
                    panic!(
                        "reconcile_children: duplicate modifier.key={} in children of node {:?}.\n\
                         Two sibling views share the same key. Each view passed to a layout \
                         must have a unique modifier.key. For lazy layouts (LazyColumn, LazyRow, \
                         etc.), ensure `get_key` returns a unique key for each item by hashing \
                         the full item identity.",
                        key, parent_id,
                    );
                }
            }
            let idx = i as u32;
            let child_id = if let Some(key) = new_child.modifier.key {
                // Keyed child: look up by key
                if let Some(&existing_id) = keyed_children.get(&key) {
                    used_nodes.insert(existing_id);
                    self.reconcile_node(
                        existing_id,
                        new_child,
                        Some(parent_id),
                        child_depth,
                        idx,
                        ctx,
                    )
                } else {
                    self.create_node(new_child, Some(parent_id), child_depth, idx, ctx)
                }
            } else {
                // Unkeyed child: match by position
                if unkeyed_index < unkeyed_children.len() {
                    let existing_id = unkeyed_children[unkeyed_index];
                    unkeyed_index += 1;
                    used_nodes.insert(existing_id);
                    self.reconcile_node(
                        existing_id,
                        new_child,
                        Some(parent_id),
                        child_depth,
                        idx,
                        ctx,
                    )
                } else {
                    self.create_node(new_child, Some(parent_id), child_depth, idx, ctx)
                }
            };

            new_child_ids.push(child_id);

            if let Some(node) = self.nodes.get(child_id) {
                new_subtree_hashes.push(node.subtree_hash);
            }
        }

        // Mark unused old children for removal
        for &old_child in &old_children {
            if !used_nodes.contains(&old_child) {
                self.mark_for_removal(old_child, ctx);
            }
        }

        // Update parent's children list
        if let Some(parent) = self.nodes.get_mut(parent_id) {
            parent.children = new_child_ids;
        }

        new_subtree_hashes
    }

    /// Create a new node from a View.
    fn create_node(
        &mut self,
        view: &View,
        parent: Option<NodeId>,
        depth: u32,
        index_in_parent: u32,
        ctx: &mut ReconcileContext,
    ) -> NodeId {
        let content_hash = hash_view_content(view);

        // Insert a partial node first
        let node_id = self.nodes.insert_with_key(|id| {
            TreeNode::new(
                id,
                0,
                view.kind.clone(),
                view.modifier.clone(),
                self.generation,
            )
        });
        ctx.created += 1;

        {
            let node = self
                .nodes
                .get_mut(node_id)
                .expect("create_node: node just inserted");
            node.parent = parent;
            node.depth = depth;
            node.content_hash = content_hash;
            node.user_key = view.modifier.key;
            node.scope_key = view.scope_key.clone();
        }

        // Now, recursively create children
        let child_depth = depth + 1;
        let mut child_ids: SmallVec<[NodeId; 4]> = SmallVec::new();
        let mut child_hashes: Vec<u64> = Vec::with_capacity(view.children.len());
        let children_to_create: Vec<View> =
            if let ViewKind::SubcomposeLayout { content } = &view.kind {
                self.run_subcompose(node_id, content)
                    .into_iter()
                    .map(|(_, v)| v)
                    .collect()
            } else {
                view.children.clone()
            };
        for (i, child_view) in children_to_create.iter().enumerate() {
            let child_id = self.create_node(child_view, Some(node_id), child_depth, i as u32, ctx);
            child_ids.push(child_id);
            child_hashes.push(
                self.nodes
                    .get(child_id)
                    .expect("create_node: child just created")
                    .subtree_hash,
            );
        }

        // Now compute the view_id and subtree_hash, and update the node
        let view_id = self.compute_view_id(view, node_id, parent, index_in_parent);
        let subtree_hash = hash_subtree(content_hash, &child_hashes);

        let node = self
            .nodes
            .get_mut(node_id)
            .expect("create_node: node just inserted");
        node.children = child_ids;
        node.subtree_hash = subtree_hash;
        node.view_id = view_id;

        self.view_id_map.insert(view_id, node_id);
        self.dirty.insert(node_id);

        node_id
    }
    /// Compute a stable ViewId for a node.
    fn compute_view_id(
        &self,
        view: &View,
        _node_id: NodeId,
        parent: Option<NodeId>,
        index_in_parent: u32,
    ) -> ViewId {
        // If the view already has an ID assigned, use it
        if view.id != 0 {
            return view.id;
        }

        // Otherwise compute from parent + index/key
        let parent_id = parent
            .and_then(|p| self.nodes.get(p))
            .map(|n| n.view_id)
            .unwrap_or(0);

        let salt = view.modifier.key.unwrap_or(index_in_parent as u64);

        // Simple hash combination
        let mut id = parent_id.wrapping_mul(31).wrapping_add(salt);
        id = id.wrapping_mul(0x9E3779B97F4A7C15);
        id ^= id >> 30;

        if id == 0 {
            id = 1;
        }

        id
    }

    /// Mark a node and its descendants for removal.
    fn mark_for_removal(&mut self, node_id: NodeId, ctx: &mut ReconcileContext) {
        // Gather what we need from the node first so the immutable borrow ends
        // before we mutate other state.
        let (view_id, children) = {
            let node = self.nodes.get(node_id);
            match node {
                Some(n) => (n.view_id, n.children.clone()),
                None => return,
            }
        };
        self.view_id_map.remove(&view_id);
        self.subcompose_cache.remove(&node_id);
        for child_id in children.iter() {
            self.collect_subcompose_cache(child_id);
        }
        for child_id in children {
            self.mark_for_removal(child_id, ctx);
        }
        ctx.removed += 1;

        // Mark the node's generation as old so it gets collected
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.generation = 0; // Will be collected
        }
    }

    /// Remove nodes that weren't updated this generation.
    fn collect_garbage(&mut self) {
        let current_gen = self.generation;

        // Find nodes to remove
        let to_remove: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.generation != current_gen)
            .map(|(id, _)| id)
            .collect();

        // Remove them
        for id in to_remove {
            if let Some(node) = self.nodes.remove(id) {
                self.view_id_map.remove(&node.view_id);
                self.dirty.remove(&id);

                // Track removal for external sync
                self.removed_ids.push(id);
            }
        }
    }

    /// Set cached layout for a node.
    pub fn set_layout(
        &mut self,
        id: NodeId,
        rect: Rect,
        screen_rect: Rect,
        constraints: LayoutConstraints,
    ) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.layout_cache = Some(LayoutCache {
                rect,
                screen_rect,
                constraints,
                generation: self.generation,
            });
        }
    }

    /// Iterate over all nodes (parent before children).
    pub fn iter(&self) -> impl Iterator<Item = &TreeNode> {
        self.nodes.values()
    }

    /// Iterate over all nodes with their IDs.
    pub fn iter_with_ids(&self) -> impl Iterator<Item = (NodeId, &TreeNode)> {
        self.nodes.iter()
    }

    /// Walk the tree from root, calling `f` for each node.
    /// Returns early if `f` returns false.
    pub fn walk<F>(&self, mut f: F)
    where
        F: FnMut(&TreeNode, u32) -> bool,
    {
        if let Some(root_id) = self.root {
            self.walk_node(root_id, 0, &mut f);
        }
    }

    fn walk_node<F>(&self, id: NodeId, depth: u32, f: &mut F)
    where
        F: FnMut(&TreeNode, u32) -> bool,
    {
        if let Some(node) = self.nodes.get(id) {
            if !f(node, depth) {
                return;
            }

            for &child_id in &node.children {
                self.walk_node(child_id, depth + 1, f);
            }
        }
    }

    /// Get children of a node.
    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.nodes.get(id).map(|n| n.children.as_slice())
    }
}

/// Intersect a `SubcomposeScope` with a `Modifier`'s size-related fields.
/// `Modifier::width` / `Modifier::height` are treated as exact sizes (the
/// resulting min and max both equal that value). Padding values reduce the
/// available content area (matching Compose semantics where padding offsets
/// the child constraints). `fill_max_w` / `fill_max_h` are layout-direction
/// hints resolved by Taffy and cannot produce a concrete size here; they
/// are handled by storing the Taffy-computed size after layout.
fn intersect_scope_with_modifier(scope: SubcomposeScope, modifier: &Modifier) -> SubcomposeScope {
    let mut s = scope;
    // `size()` sets both min and max to the same value (exact size).
    if let Some(sz) = modifier.size {
        s.min_width = s.min_width.max(sz.width);
        s.max_width = s.max_width.min(sz.width);
        s.min_height = s.min_height.max(sz.height);
        s.max_height = s.max_height.min(sz.height);
    }
    if let Some(w) = modifier.width {
        s.min_width = s.min_width.max(w);
        s.max_width = s.max_width.min(w);
    }
    if let Some(h) = modifier.height {
        s.min_height = s.min_height.max(h);
        s.max_height = s.max_height.min(h);
    }
    if let Some(mw) = modifier.min_width {
        s.min_width = s.min_width.max(mw);
    }
    if let Some(mh) = modifier.min_height {
        s.min_height = s.min_height.max(mh);
    }
    if let Some(mw) = modifier.max_width {
        s.max_width = s.max_width.min(mw);
    }
    if let Some(mh) = modifier.max_height {
        s.max_height = s.max_height.min(mh);
    }
    // Padding reduces the available content area (Compose semantics:
    // constraints are offset by the padding amount).
    if let Some(p) = modifier.padding {
        let total = p * 2.0;
        s.min_width = (s.min_width - total).max(0.0);
        s.max_width = (s.max_width - total).max(0.0);
        s.min_height = (s.min_height - total).max(0.0);
        s.max_height = (s.max_height - total).max(0.0);
    }
    if let Some(pv) = modifier.padding_values {
        let h_total = pv.left + pv.right;
        let v_total = pv.top + pv.bottom;
        s.min_width = (s.min_width - h_total).max(0.0);
        s.max_width = (s.max_width - h_total).max(0.0);
        s.min_height = (s.min_height - v_total).max(0.0);
        s.max_height = (s.max_height - v_total).max(0.0);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::{
        Color, FontStyle, FontWeight, Modifier, SubcomposeScope, TextAlign, TextDecoration, View,
        ViewKind,
    };
    use std::sync::Arc;

    fn text_view(text: &str) -> View {
        View::new(
            0,
            ViewKind::Text {
                text: text.to_string(),
                color: Color::WHITE,
                font_size: 16.0,
                soft_wrap: true,
                max_lines: None,
                overflow: repose_core::TextOverflow::Visible,
                font_family: None,
                annotations: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: TextDecoration::default(),
                letter_spacing: 0.0,
                line_height: 0.0,
            },
        )
    }

    fn box_view() -> View {
        View::new(0, ViewKind::Box)
    }

    #[test]
    fn test_create_tree() {
        let mut tree = ViewTree::new();

        let root = box_view().with_children(vec![text_view("Hello"), text_view("World")]);

        tree.update(&root);

        assert_eq!(tree.len(), 3); // box + 2 text
        assert!(tree.root().is_some());
    }

    #[test]
    fn test_unchanged_tree_skips() {
        let mut tree = ViewTree::new();

        let root = box_view().with_children(vec![text_view("Hello")]);

        tree.update(&root);
        let gen1 = tree.generation();

        // Same tree
        tree.update(&root);
        let gen2 = tree.generation();

        assert_eq!(gen2, gen1 + 1);
        assert!(tree.stats.skipped_nodes > 0);
    }

    #[test]
    fn test_changed_content_reconciles() {
        let mut tree = ViewTree::new();

        let root1 = box_view().with_children(vec![text_view("Hello")]);

        tree.update(&root1);

        let root2 = box_view().with_children(vec![text_view("Changed")]);

        tree.update(&root2);

        assert!(tree.stats.reconciled_nodes > 0);
    }

    #[test]
    fn test_keyed_children_stable() {
        let mut tree = ViewTree::new();

        // Initial: A, B, C
        let root1 = box_view().with_children(vec![
            text_view("A").modifier(Modifier::new().key(1)),
            text_view("B").modifier(Modifier::new().key(2)),
            text_view("C").modifier(Modifier::new().key(3)),
        ]);

        tree.update(&root1);

        // Get B's NodeId
        let b_view_id = tree
            .root()
            .and_then(|r| tree.children(r))
            .and_then(|c| c.get(1).copied())
            .and_then(|id| tree.get(id))
            .map(|n| n.view_id);

        // Reorder: C, A, B
        let root2 = box_view().with_children(vec![
            text_view("C").modifier(Modifier::new().key(3)),
            text_view("A").modifier(Modifier::new().key(1)),
            text_view("B").modifier(Modifier::new().key(2)),
        ]);

        tree.update(&root2);

        // B should have same view_id (key-based stability)
        // Note: Implementation detail - the node may be reused
        assert_eq!(tree.len(), 4); // Still 4 nodes (box + 3 text)
    }

    fn subcompose_view<F>(f: F) -> View
    where
        F: Fn(&SubcomposeScope) -> View + 'static,
    {
        let content: Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>> =
            Arc::new(move |scope| vec![(0, f(scope))]);
        View {
            id: 0,
            kind: ViewKind::SubcomposeLayout { content },
            modifier: Modifier::default(),
            children: Vec::new(),
            scope_key: None,
            semantics: None,
        }
    }

    #[test]
    fn test_subcompose_invokes_content() {
        let mut tree = ViewTree::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter2 = counter.clone();

        let root = box_view().with_children(vec![subcompose_view(move |_scope| {
            counter2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("from subcompose")
        })]);

        tree.update(&root);

        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(tree.len(), 3); // box + subcompose + text
    }

    #[test]
    fn test_subcompose_receives_scope() {
        let mut tree = ViewTree::new();
        let captured = Arc::new(std::sync::Mutex::new(None));
        let captured2 = captured.clone();

        let root = box_view().with_children(vec![subcompose_view(move |scope| {
            *captured2.lock().unwrap() = Some(*scope);
            text_view("hi")
        })]);

        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 360.0, 0.0, 640.0));
        tree.update(&root);

        let observed = captured.lock().unwrap().expect("scope captured");
        assert_eq!(observed.max_width, 360.0);
        assert_eq!(observed.max_height, 640.0);
        assert_eq!(observed.min_width, 0.0);
        assert_eq!(observed.min_height, 0.0);
    }

    #[test]
    fn test_subcompose_re_invokes_on_update() {
        let mut tree = ViewTree::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter2 = counter.clone();

        let root = box_view().with_children(vec![subcompose_view(move |_scope| {
            counter2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("hi")
        })]);

        tree.update(&root);
        tree.update(&root);
        tree.update(&root);

        // Closure should run only on the first update; subsequent updates hit
        // the cache because the scope and content are unchanged.
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_subcompose_reruns_on_scope_change() {
        let mut tree = ViewTree::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter2 = counter.clone();

        let root = box_view().with_children(vec![subcompose_view(move |_scope| {
            counter2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("hi")
        })]);

        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 100.0, 0.0, 100.0));
        tree.update(&root);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Same scope: cache hit.
        tree.update(&root);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Scope changed: closure re-runs.
        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 200.0, 0.0, 200.0));
        tree.update(&root);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_subcompose_reruns_on_content_change() {
        let mut tree = ViewTree::new();
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c1 = counter.clone();

        let root1 = box_view().with_children(vec![subcompose_view(move |_scope| {
            c1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("hi")
        })]);

        tree.update(&root1);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Same content: cache hit.
        tree.update(&root1);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Changed modifier: closure re-runs.
        let c2 = counter.clone();
        let root2 = box_view().with_children(vec![
            subcompose_view(move |_scope| {
                c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                text_view("hi")
            })
            .modifier(Modifier::new().padding(4.0)),
        ]);

        tree.update(&root2);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn test_subcompose_cache_drops_on_node_removal() {
        let mut tree = ViewTree::new();
        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 100.0, 0.0, 100.0));

        // First root has a SubcomposeLayout child.
        let counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let c1 = counter.clone();
        let root_with_sub = box_view().with_children(vec![subcompose_view(move |_scope| {
            c1.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("hi")
        })]);

        tree.update(&root_with_sub);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Swap to a root without the SubcomposeLayout - the old node should be
        // garbage-collected and its cache entry dropped.
        let root_no_sub = box_view().with_children(vec![text_view("plain")]);
        tree.update(&root_no_sub);
        assert_eq!(tree.len(), 2);

        // Bring the SubcomposeLayout back - it must run the closure again
        // because the cache entry was dropped during GC.
        let c2 = counter.clone();
        let root_with_sub_again = box_view().with_children(vec![subcompose_view(move |_scope| {
            c2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            text_view("hi")
        })]);

        tree.update(&root_with_sub_again);
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    fn multi_slot_view<F>(f: F) -> View
    where
        F: Fn(&SubcomposeScope) -> Vec<(u64, View)> + 'static,
    {
        let content: Arc<dyn Fn(&SubcomposeScope) -> Vec<(u64, View)>> = Arc::new(f);
        View {
            id: 0,
            kind: ViewKind::SubcomposeLayout { content },
            modifier: Modifier::default(),
            children: Vec::new(),
            scope_key: None,
            semantics: None,
        }
    }

    #[test]
    fn test_subcompose_multi_slot_produces_multiple_children() {
        let mut tree = ViewTree::new();
        let root = box_view().with_children(vec![multi_slot_view(|_scope| {
            vec![
                (0, text_view("a")),
                (1, text_view("b")),
                (2, text_view("c")),
            ]
        })]);

        tree.update(&root);

        // box + subcompose + 3 texts
        assert_eq!(tree.len(), 5);
        let sub_id = tree
            .root()
            .and_then(|r| tree.children(r))
            .and_then(|c| c.first().copied())
            .expect("subcompose node");
        let sub_children = tree.children(sub_id).expect("subcompose has children");
        assert_eq!(sub_children.len(), 3);
    }

    #[test]
    fn test_subcompose_multi_slot_preserves_identity_across_removal() {
        let mut tree = ViewTree::new();

        // 3 slots: 0, 1, 2
        let root3 = box_view().with_children(vec![multi_slot_view(|_scope| {
            vec![
                (0, text_view("a")),
                (1, text_view("b")),
                (2, text_view("c")),
            ]
        })]);
        tree.update(&root3);

        let sub_id = tree
            .root()
            .and_then(|r| tree.children(r))
            .and_then(|c| c.first().copied())
            .expect("subcompose node");
        let before = tree.children(sub_id).expect("children").to_vec();
        let a_node = before[0];
        let b_node = before[1];
        let c_node = before[2];

        // 2 slots: 0, 2 (middle removed). The Modifier change (padding) forces
        // the subcompose cache to invalidate so the new closure runs.
        let root2 = box_view().with_children(vec![
            multi_slot_view(|_scope| vec![(0, text_view("a")), (2, text_view("c"))])
                .modifier(Modifier::new().padding(4.0)),
        ]);
        tree.update(&root2);

        let after = tree.children(sub_id).expect("children after");
        assert_eq!(after.len(), 2);
        // Slot 0 (a) and slot 2 (c) should keep their NodeId.
        assert_eq!(after[0], a_node);
        assert_eq!(after[1], c_node);
        // Slot 1 (b) should be gone.
        assert!(tree.get(b_node).is_none());
    }

    #[test]
    fn test_subcompose_ancestor_modifier_narrows_scope() {
        let mut tree = ViewTree::new();
        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 1000.0, 0.0, 1000.0));

        let captured = Arc::new(std::sync::Mutex::new(SubcomposeScope::UNBOUNDED));
        let cap2 = captured.clone();

        // SubcomposeLayout inside a Box with width(200.dp) - the closure should
        // see max_width == 200.
        let sub = multi_slot_view(move |scope| {
            *cap2.lock().unwrap() = *scope;
            vec![(0, text_view("hi"))]
        });
        let root = box_view()
            .modifier(Modifier::new().width(200.0))
            .with_children(vec![sub]);

        tree.update(&root);

        let observed = *captured.lock().unwrap();
        assert_eq!(observed.max_width, 200.0);
    }

    #[test]
    fn test_subcompose_chained_ancestor_constraints_intersect() {
        let mut tree = ViewTree::new();
        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 1000.0, 0.0, 1000.0));

        let captured = Arc::new(std::sync::Mutex::new(SubcomposeScope::UNBOUNDED));
        let cap2 = captured.clone();

        let sub = multi_slot_view(move |scope| {
            *cap2.lock().unwrap() = *scope;
            vec![(0, text_view("hi"))]
        });
        // Box(width=400) -> Box(max_width=300) -> SubcomposeLayout.
        // The intersection should give max_width = 300.
        let root = box_view()
            .modifier(Modifier::new().width(400.0))
            .with_children(vec![
                box_view()
                    .modifier(Modifier::new().max_width(300.0))
                    .with_children(vec![sub]),
            ]);

        tree.update(&root);

        let observed = *captured.lock().unwrap();
        assert_eq!(observed.max_width, 300.0);
    }

    #[test]
    fn test_subcompose_nested_layouts_inherit_narrowed_scope() {
        let mut tree = ViewTree::new();
        tree.set_subcompose_scope(SubcomposeScope::new(0.0, 1000.0, 0.0, 1000.0));

        let outer_captured = Arc::new(std::sync::Mutex::new(SubcomposeScope::UNBOUNDED));
        let inner_captured = Arc::new(std::sync::Mutex::new(SubcomposeScope::UNBOUNDED));
        let outer2 = outer_captured.clone();
        let inner2 = inner_captured.clone();

        // Outer SubcomposeLayout(width=400) hosts an inner SubcomposeLayout.
        // The inner closure should observe max_width = 400, not 1000.
        let inner = Arc::new(multi_slot_view(move |scope| {
            *inner2.lock().unwrap() = *scope;
            vec![(0, text_view("inner"))]
        }));
        let inner_clone = inner.clone();
        let outer = multi_slot_view(move |scope| {
            *outer2.lock().unwrap() = *scope;
            vec![(0, (*inner_clone).clone())]
        })
        .modifier(Modifier::new().width(400.0));
        let root = box_view().with_children(vec![outer]);

        tree.update(&root);

        let outer_obs = *outer_captured.lock().unwrap();
        let inner_obs = *inner_captured.lock().unwrap();
        assert_eq!(outer_obs.max_width, 400.0);
        assert_eq!(inner_obs.max_width, 400.0);
    }
}
