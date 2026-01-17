//! # repose-tree
//!
//! Persistent view tree with incremental reconciliation.
//!
//! Instead of rebuilding the entire view tree every frame, this module
//! maintains a persistent tree structure that:
//!
//! 1. **Stable Identity**: Nodes have stable IDs across frames
//! 2. **Change Detection**: Content hashing identifies what changed
//! 3. **Incremental Updates**: Only changed subtrees are processed
//! 4. **Layout Caching**: Unchanged nodes keep their cached layout
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │ ViewTree │
//! │ ┌─────────────────────────────────────────────────────┐│
//! │ │ nodes: SlotMap<NodeId, TreeNode> ││
//! │ │ root: NodeId ││
//! │ │ dirty: FxHashSet<NodeId> ││
//! │ │ generation: u64 ││
//! │ └─────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────┘
//! │
//! ▼
//! ┌─────────────────────────────────────────────────────────┐
//! │ TreeNode │
//! │ ┌─────────────────────────────────────────────────────┐│
//! │ │ id: NodeId ││
//! │ │ view_id: ViewId (stable user-facing ID) ││
//! │ │ kind: ViewKind ││
//! │ │ modifier: Modifier ││
//! │ │ children: SmallVec<[NodeId; 4]> ││
//! │ │ parent: Option<NodeId> ││
//! │ │ content_hash: u64 ││
//! │ │ layout_cache: Option<LayoutCache> ││
//! │ │ generation: u64 ││
//! │ └─────────────────────────────────────────────────────┘│
//! └─────────────────────────────────────────────────────────┘
//! ```

mod hash;
mod node;
mod reconcile;
mod tree;

pub use hash::*;
pub use node::*;
pub use reconcile::*;
pub use tree::*;

#[cfg(test)]
mod tests;
