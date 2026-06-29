//! Tree node definitions.

use repose_core::{Modifier, Rect, ViewId, ViewKind};
use slotmap::new_key_type;
use smallvec::SmallVec;

new_key_type! {
 /// Internal node identifier, stable within a tree's lifetime.
 pub struct NodeId;
}

/// A node in the persistent view tree.
#[derive(Clone)]
pub struct TreeNode {
    /// Internal slotmap key.
    pub id: NodeId,

    /// User-facing stable view ID (for hit testing, focus, etc.).
    pub view_id: ViewId,

    /// The kind of view this node represents.
    pub kind: ViewKind,

    /// Layout and styling modifiers.
    pub modifier: Modifier,

    /// Child node IDs.
    pub children: SmallVec<[NodeId; 4]>,

    /// Parent node ID (None for root).
    pub parent: Option<NodeId>,

    /// Hash of this node's content (kind + modifier + children structure).
    /// Used for change detection.
    pub content_hash: u64,

    /// Hash including all descendants.
    /// If this matches, the entire subtree is unchanged.
    pub subtree_hash: u64,

    /// Cached layout result.
    pub layout_cache: Option<LayoutCache>,

    /// Generation when this node was last updated.
    pub generation: u64,

    /// Key provided by user for stable identity in dynamic lists.
    pub user_key: Option<u64>,

    /// Depth in tree (root = 0).
    pub depth: u32,

    /// If this node is a scope boundary (set by `scope!` macro), this holds the
    /// scope key (e.g., "title", "color_buttons"). `None` for non-scope nodes.
    pub scope_key: Option<String>,
}

impl TreeNode {
    /// Create a new tree node.
    pub fn new(
        id: NodeId,
        view_id: ViewId,
        kind: ViewKind,
        modifier: Modifier,
        generation: u64,
    ) -> Self {
        Self {
            id,
            view_id,
            kind,
            modifier,
            children: SmallVec::new(),
            parent: None,
            content_hash: 0,
            subtree_hash: 0,
            layout_cache: None,
            generation,
            user_key: None,
            depth: 0,
            scope_key: None,
        }
    }

    /// Check if this node's layout cache is still valid.
    pub fn has_valid_layout(&self) -> bool {
        self.layout_cache.is_some()
    }

    /// Invalidate layout cache (called when content changes).
    pub fn invalidate_layout(&mut self) {
        self.layout_cache = None;
    }

    /// Get cached layout if available.
    pub fn cached_rect(&self) -> Option<Rect> {
        self.layout_cache.as_ref().map(|c| c.rect)
    }

    pub fn set_layout(&mut self, rect: Rect) {
        self.layout_cache = Some(LayoutCache {
            rect,
            // constraints and screen_rect are less critical for basic paint
            screen_rect: Rect::default(),
            constraints: LayoutConstraints::default(),
            generation: self.generation,
        });
    }
}

/// Cached layout information for a node.
#[derive(Clone, Debug)]
pub struct LayoutCache {
    /// The computed rectangle in parent coordinates.
    pub rect: Rect,

    /// The computed rectangle in screen coordinates.
    pub screen_rect: Rect,

    /// Size constraints used when computing this layout.
    pub constraints: LayoutConstraints,

    /// Generation when this cache was computed.
    pub generation: u64,
}

/// Constraints used during layout.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}

impl Default for LayoutConstraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        }
    }
}

impl LayoutConstraints {
    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            min_width: width,
            max_width: width,
            min_height: height,
            max_height: height,
        }
    }

    pub fn tight(size: (f32, f32)) -> Self {
        Self::fixed(size.0, size.1)
    }
}

/// Statistics about tree operations.
#[derive(Clone, Debug, Default)]
pub struct TreeStats {
    /// Total nodes in tree.
    pub total_nodes: usize,

    /// Nodes marked dirty this frame.
    pub dirty_nodes: usize,

    /// Nodes that were reconciled (updated).
    pub reconciled_nodes: usize,

    /// Nodes that were skipped (unchanged).
    pub skipped_nodes: usize,

    /// Nodes that were created.
    pub created_nodes: usize,

    /// Nodes that were removed.
    pub removed_nodes: usize,

    /// Layout cache hits.
    pub layout_cache_hits: usize,

    /// Layout cache misses.
    pub layout_cache_misses: usize,
}
