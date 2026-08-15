#![allow(non_snake_case)]

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use repose_core::*;
use repose_tree::{NodeId, TreeStats, ViewTree};
use rustc_hash::FxHashMap;
use taffy::TaffyTree;

pub(crate) struct ScopeLayoutTree {
    #[allow(dead_code)]
    pub(crate) key: String,
    pub(crate) taffy: TaffyTree<NodeContext>,
    pub(crate) taffy_map: FxHashMap<NodeId, taffy::NodeId>,
    pub(crate) reverse_map: FxHashMap<taffy::NodeId, NodeId>,
    pub(crate) root_taffy_id: Option<taffy::NodeId>,
    pub(crate) last_constraints:
        Option<(taffy::Size<Option<f32>>, taffy::Size<taffy::AvailableSpace>)>,
    pub(crate) cached_size: Option<taffy::Size<f32>>,
    pub(crate) text_cache: FxHashMap<NodeId, TextLayout>,
    pub(crate) valid: bool,
}

impl ScopeLayoutTree {
    pub(crate) fn new(key: String) -> Self {
        Self {
            key,
            taffy: TaffyTree::new(),
            taffy_map: FxHashMap::default(),
            reverse_map: FxHashMap::default(),
            root_taffy_id: None,
            last_constraints: None,
            cached_size: None,
            text_cache: FxHashMap::default(),
            valid: false,
        }
    }
}

pub struct LayoutEngine {
    /// Persistent view tree.
    pub(crate) tree: ViewTree,

    /// Root Taffy layout tree (inter-scope layout + non-scope nodes).
    pub(crate) taffy: TaffyTree<NodeContext>,

    /// Map from ViewTree NodeId to root Taffy NodeId.
    pub(crate) taffy_map: FxHashMap<NodeId, taffy::NodeId>,

    /// Reverse map: root Taffy NodeId to ViewTree NodeId.
    pub(crate) reverse_map: FxHashMap<taffy::NodeId, NodeId>,

    /// Per-scope TaffyTrees for scope! macro isolation.
    pub(crate) scope_trees: HashMap<String, ScopeLayoutTree>,

    /// ViewTree NodeId -> scope key for scope boundary root nodes.
    pub(crate) scope_root_map: FxHashMap<NodeId, String>,

    /// ViewTree NodeId -> scope key for ALL nodes belonging to a scope.
    pub(crate) node_to_scope: FxHashMap<NodeId, String>,

    /// Cached text layouts for non-scope nodes (persists across frames).
    pub(crate) text_cache: FxHashMap<NodeId, TextLayout>,

    /// Last window size used for layout.
    pub(crate) last_size_px: Option<(u32, u32)>,

    /// Whether root Taffy has a valid computed layout for `last_size_px`.
    pub(crate) layout_valid: bool,

    /// Repaint-boundary cache (SceneNodes + hits + semantics).
    pub(crate) paint_cache: FxHashMap<NodeId, PaintCacheEntry>,

    /// Statistics from the last frame.
    pub stats: LayoutStats,

    /// Tracks the previously focused view ID to detect focus changes.
    pub(crate) prev_focused: Option<u64>,

    /// Callbacks registered via `on_focus_changed` modifier, keyed by view ID.
    pub(crate) focus_callbacks: FxHashMap<u64, Rc<dyn Fn(bool)>>,

    /// Last "locals" stamp used for layout decisions (density/text scale/dir).
    pub(crate) last_locals_stamp: Option<u64>,

    /// Stable, unique ViewId per ViewTree NodeId.
    pub(crate) view_ids: FxHashMap<NodeId, u64>,
    pub(crate) next_view_id: u64,

    /// Monotonic counter for graphics layer ids, assigned during paint.
    pub(crate) layer_id_counter: u32,

    /// Previous absolute rects for `on_globally_positioned` / `on_size_changed` callbacks.
    pub(crate) prev_observed_rects: FxHashMap<u64, repose_core::Rect>,

    /// Stack of active focus group IDs. When non-empty, newly created hit regions
    /// get `focus_group_id` set to the top of this stack. A focus group is entered
    /// when a node with `modifier.focus_group == true` is traversed.
    pub(crate) focus_group_stack: Vec<u64>,

    /// InteractionSources that should receive Focus/Unfocus for the current paint tree,
    /// keyed by view ID.
    pub(crate) focus_interaction_sources: FxHashMap<u64, InteractionSource>,
}

/// Statistics about layout performance.
#[derive(Clone, Debug, Default)]
pub struct LayoutStats {
    /// Stats from tree reconciliation.
    pub tree: TreeStats,

    /// Taffy nodes created this frame.
    pub taffy_created: usize,

    /// Taffy nodes reused this frame.
    pub taffy_reused: usize,

    /// Layout cache hits.
    pub layout_hits: usize,

    /// Layout cache misses.
    pub layout_misses: usize,

    /// Paint cache hits (repaint boundaries).
    pub paint_cache_hits: usize,

    /// Paint cache misses (repaint boundaries).
    pub paint_cache_misses: usize,

    /// Nodes skipped due to clip/viewport culling.
    pub paint_culled: usize,

    /// Total time for layout+paint (ms).
    pub layout_time_ms: f32,

    /// Total time spent in the paint pass only (ms).
    pub paint_time_ms: f32,
}

#[derive(Clone)]
pub(crate) struct PaintCacheEntry {
    pub(crate) subtree_hash: u64,
    pub(crate) stamp: u64,
    pub(crate) rect: repose_core::Rect,
    pub(crate) parent_offset_px: (f32, f32),
    pub(crate) sem_parent: Option<u64>,
    pub(crate) alpha_q: u8,
    pub(crate) nodes: Arc<Vec<SceneNode>>,
    pub(crate) hits: Arc<Vec<HitRegion>>,
    pub(crate) sems: Arc<Vec<SemNode>>,
}

/// Selects the intrinsic sizing mode for [`LayoutEngine::intrinsic_size`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntrinsicSizeMode {
    /// Smallest size at which the view's content does not overflow.
    MinContent,
    /// Largest size the view's content would take if unconstrained.
    MaxContent,
}

/// Context stored with each Taffy node.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum NodeContext {
    Text {
        text: String,
        color: Color,
        font_dp: f32,
        soft_wrap: bool,
        max_lines: Option<usize>,
        overflow: TextOverflow,
        font_family: Option<&'static str>,
        annotations: Option<Arc<[TextSpan]>>,
        text_align: TextAlign,
        font_weight: FontWeight,
        font_style: FontStyle,
        text_decoration: TextDecoration,
        letter_spacing: f32,
        line_height: f32,
        font_variation_settings: Option<Arc<str>>,
    },
    Container,
    ScrollContainer,
    TextInput {
        multiline: bool,
    },
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct TextLayout {
    pub(crate) lines: Vec<String>,
    /// Byte ranges into the original text for each line (used for annotation splitting).
    pub(crate) line_ranges: Vec<(usize, usize)>,
    pub(crate) size_px: f32,
    pub(crate) line_h_px: f32,
    /// Pre-measured width per line.
    pub(crate) line_widths: Vec<f32>,
}
