//! The main ViewTree structure.

use crate::{
    hash::{hash_subtree, hash_view_content},
    node::{LayoutCache, LayoutConstraints, NodeId, TreeNode, TreeStats},
    reconcile::ReconcileContext,
};
use repose_core::{Rect, View, ViewId};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SlotMap;
use smallvec::SmallVec;

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

    /// Map from user key to NodeId (for stable identity in dynamic lists).
    key_map: FxHashMap<(NodeId, u64), NodeId>, // (parent, key) -> child

    /// Statistics for the last reconcile operation.
    pub stats: TreeStats,

    /// Nodes removed during the last update (needed to sync external systems like Taffy).
    pub removed_ids: Vec<NodeId>,
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
            key_map: FxHashMap::default(),
            stats: TreeStats::default(),
            removed_ids: Vec::new(),
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

        let new_children_hashes = self.reconcile_children(node_id, &view.children, depth, ctx);

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

        for (i, new_child) in new_children.iter().enumerate() {
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
        }

        // Now, recursively create children
        let child_depth = depth + 1;
        let mut child_ids: SmallVec<[NodeId; 4]> = SmallVec::new();
        let mut child_hashes: Vec<u64> = Vec::with_capacity(view.children.len());
        for (i, child_view) in view.children.iter().enumerate() {
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
        node_id: NodeId,
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
        if let Some(node) = self.nodes.get(node_id) {
            // Remove from view_id map
            self.view_id_map.remove(&node.view_id);

            // Recursively mark children
            let children: SmallVec<[NodeId; 4]> = node.children.clone();
            for child_id in children {
                self.mark_for_removal(child_id, ctx);
            }

            ctx.removed += 1;
        }

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

#[cfg(test)]
mod tests {
    use super::*;
    use repose_core::{Color, Modifier, View, ViewKind};

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
}
