#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use repose_core::*;
use repose_tree::{LayoutConstraints, NodeId, TreeNode, TreeStats, ViewTree};
use rustc_hash::{FxHashMap, FxHasher};
use taffy::TaffyTree;
use taffy::prelude::*;
use taffy::style::FlexDirection;
use taffy::style::Overflow;

use crate::Interactions;
use crate::anim::{animate_color, animate_f32};
use crate::textfield::{TF_FONT_DP, TextFieldState, TextMeasureConfig, measure_text};

fn open_url(url: &str) {
    let _ = webbrowser::open(url);
}

fn push_focus_ring(scene: &mut Scene, rect: repose_core::Rect, radius_dp: f32) {
    scene.nodes.push(SceneNode::Border {
        rect,
        color: locals::theme().focus,
        width: dp_to_px(2.0),
        radius: [dp_to_px(radius_dp); 4],
    });
}

fn focus_radius(modifier: &Modifier) -> f32 {
    modifier.clip_rounded.map(|r| r[0]).unwrap_or(6.0)
}

/// Associate a `FocusRequester` (if present on the modifier) with the view.
fn set_focus_requester(modifier: &Modifier, view_id: u64) {
    if let Some(ref fr) = modifier.focus_requester {
        FocusManager::set_requester_target(fr, view_id);
    }
}

/// Per-scope layout state for the `scope!` macro.
/// Each scope gets its own TaffyTree so that cached scopes can skip layout
/// computation entirely (Compose-style constraint-equality skip).
struct ScopeLayoutTree {
    key: String,
    taffy: TaffyTree<NodeContext>,
    taffy_map: FxHashMap<NodeId, taffy::NodeId>,
    reverse_map: FxHashMap<taffy::NodeId, NodeId>,
    root_taffy_id: Option<taffy::NodeId>,
    last_constraints: Option<(taffy::Size<Option<f32>>, taffy::Size<taffy::AvailableSpace>)>,
    cached_size: Option<taffy::Size<f32>>,
    text_cache: FxHashMap<NodeId, TextLayout>,
    valid: bool,
}

impl ScopeLayoutTree {
    fn new(key: String) -> Self {
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

/// The incremental layout engine.
pub struct LayoutEngine {
    /// Persistent view tree.
    tree: ViewTree,

    /// Root Taffy layout tree (inter-scope layout + non-scope nodes).
    taffy: TaffyTree<NodeContext>,

    /// Map from ViewTree NodeId to root Taffy NodeId.
    taffy_map: FxHashMap<NodeId, taffy::NodeId>,

    /// Reverse map: root Taffy NodeId to ViewTree NodeId.
    reverse_map: FxHashMap<taffy::NodeId, NodeId>,

    /// Per-scope TaffyTrees for scope! macro isolation.
    scope_trees: HashMap<String, ScopeLayoutTree>,

    /// ViewTree NodeId → scope key for scope boundary root nodes.
    scope_root_map: FxHashMap<NodeId, String>,

    /// ViewTree NodeId → scope key for ALL nodes belonging to a scope.
    node_to_scope: FxHashMap<NodeId, String>,

    /// Cached text layouts for non-scope nodes (persists across frames).
    text_cache: FxHashMap<NodeId, TextLayout>,

    /// Last window size used for layout.
    last_size_px: Option<(u32, u32)>,

    /// Whether root Taffy has a valid computed layout for `last_size_px`.
    layout_valid: bool,

    /// Repaint-boundary cache (SceneNodes + hits + semantics).
    paint_cache: FxHashMap<NodeId, PaintCacheEntry>,

    /// Statistics from the last frame.
    pub stats: LayoutStats,

    /// Tracks the previously focused view ID to detect focus changes.
    prev_focused: Option<u64>,

    /// Callbacks registered via `on_focus_changed` modifier, keyed by view ID.
    focus_callbacks: FxHashMap<u64, Rc<dyn Fn(bool)>>,

    /// Last "locals" stamp used for layout decisions (density/text scale/dir).
    last_locals_stamp: Option<u64>,

    /// Stable, unique ViewId per ViewTree NodeId.
    view_ids: FxHashMap<NodeId, u64>,
    next_view_id: u64,

    /// Monotonic counter for graphics layer ids, assigned during paint.
    layer_id_counter: u32,

    /// Previous absolute rects for `on_globally_positioned` / `on_size_changed` callbacks.
    prev_observed_rects: FxHashMap<u64, repose_core::Rect>,

    /// Stack of active focus group IDs. When non-empty, newly created hit regions
    /// get `focus_group_id` set to the top of this stack. A focus group is entered
    /// when a node with `modifier.focus_group == true` is traversed.
    focus_group_stack: Vec<u64>,
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
}

#[derive(Clone)]
struct PaintCacheEntry {
    subtree_hash: u64,
    stamp: u64,
    rect: repose_core::Rect,
    parent_offset_px: (f32, f32),
    sem_parent: Option<u64>,
    alpha_q: u8,
    nodes: Arc<Vec<SceneNode>>,
    hits: Arc<Vec<HitRegion>>,
    sems: Arc<Vec<SemNode>>,
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
enum NodeContext {
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
struct TextLayout {
    lines: Vec<String>,
    /// Byte ranges into the original text for each line (used for annotation splitting).
    line_ranges: Vec<(usize, usize)>,
    size_px: f32,
    line_h_px: f32,
    /// Pre-measured width per line.
    line_widths: Vec<f32>,
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutEngine {
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
            prev_observed_rects: FxHashMap::default(),
            focus_group_stack: Vec::new(),
        }
    }

    /// Get the Taffy layout for a NodeId, resolving scope membership.
    fn layout_for_node(&self, node_id: NodeId) -> taffy::prelude::Layout {
        // Scope root nodes: use the root tree layout (has correct position + size after flexbox resolve).
        // Their children use the scope tree layout (positions relative to scope root).
        if self.scope_root_map.contains_key(&node_id) {
            let tid = self.taffy_map[&node_id];
            return self.taffy.layout(tid).unwrap().clone();
        }
        if let Some(key) = self.node_to_scope.get(&node_id) {
            if let Some(st) = self.scope_trees.get(key) {
                let tid = st.taffy_map[&node_id];
                return st.taffy.layout(tid).unwrap().clone();
            }
        }
        let tid = self.taffy_map[&node_id];
        self.taffy.layout(tid).unwrap().clone()
    }

    /// Get Taffy children for a NodeId, resolving scope membership.
    fn taffy_children_for_node(&self, node_id: NodeId) -> Vec<taffy::NodeId> {
        if let Some(key) = self.node_to_scope.get(&node_id) {
            if let Some(st) = self.scope_trees.get(key) {
                let tid = st.taffy_map[&node_id];
                return st.taffy.children(tid).unwrap_or_default();
            }
        }
        let tid = self.taffy_map[&node_id];
        self.taffy.children(tid).unwrap_or_default()
    }

    fn ensure_view_id(&mut self, node_id: NodeId) -> u64 {
        if let Some(&id) = self.view_ids.get(&node_id) {
            return id;
        }
        let id = self.next_view_id;
        self.next_view_id += 1;
        self.view_ids.insert(node_id, id);
        id
    }

    /// Build `scope_root_map` and `node_to_scope` from `TreeNode.scope_key`.
    /// Must be called after `self.tree.update()` each frame.
    fn build_scope_maps(&mut self) {
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

    /// Recursively mark all nodes in a scope's subtree (stopping at nested scope boundaries).
    fn mark_scope_subtree(&mut self, root_id: NodeId, key: &str) {
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

    fn locals_stamp() -> u64 {
        let mut h = FxHasher::default();

        // These affect layout measurement and/or flex direction decisions.
        locals::density().scale.to_bits().hash(&mut h);
        locals::text_scale().0.to_bits().hash(&mut h);

        let dir_u8 = match locals::text_direction() {
            locals::TextDirection::Ltr => 0u8,
            locals::TextDirection::Rtl => 1u8,
        };
        dir_u8.hash(&mut h);

        h.finish()
    }

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

        // 1. Update tree
        let density_scale = locals::density().scale.max(0.0001);
        let max_w_dp = size_px.0 as f32 / density_scale;
        let max_h_dp = size_px.1 as f32 / density_scale;
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
        //     `window_size_class()` returns an up-to-date value even outside
        //     a `with_window_size_class { ... }` scope. We only touch the
        //     default when it actually changes to keep the lock uncontended.
        let density_scale = locals::density().scale * locals::ui_scale().0;
        let class = locals::calculate_window_size_class(size_px.0, size_px.1, density_scale);
        if class != locals::window_size_class() {
            locals::set_window_size_class_default(class);
        }
        let has_tree_mutation =
            !self.tree.dirty_nodes().is_empty() || !self.tree.removed_ids.is_empty();
        let need_layout = size_changed || !self.layout_valid || has_tree_mutation || locals_changed;

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
            for (_, st) in &mut self.scope_trees {
                for &node_id in st.text_cache.keys() {
                    if let Some(&tid) = st.taffy_map.get(&node_id) {
                        let _ = st.taffy.mark_dirty(tid);
                    }
                }
                st.text_cache.clear();
            }
        }
        if locals_changed {
            for (_, st) in &mut self.scope_trees {
                st.text_cache.clear();
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
                    let _ = self.taffy.set_style(taffy_root, style);
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
        let (scene, hits, sems) = self.paint(
            root_node_id,
            textfield_states,
            interactions,
            focused,
            &font_px,
        );

        // Fire focus change callbacks
        if self.prev_focused != focused {
            if let Some(old_id) = self.prev_focused
                && let Some(cb) = self.focus_callbacks.get(&old_id)
            {
                (cb)(false);
            }
            if let Some(new_id) = focused
                && let Some(cb) = self.focus_callbacks.get(&new_id)
            {
                (cb)(true);
            }
            self.prev_focused = focused;
        }

        // Clean up callbacks for removed nodes
        for &node_id in &self.tree.removed_ids {
            if let Some(&vid) = self.view_ids.get(&node_id) {
                self.focus_callbacks.remove(&vid);
            }
        }

        self.tree.clear_dirty();
        (scene, hits, sems)
    }

    /// Compute the intrinsic size of a view in pixels. The view is laid out
    /// in an isolated taffy tree, so calling this does not disturb the
    /// layout cache for the main `layout_frame` pass.
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

    /// Build a taffy subtree from a `&View`. Used by [`Self::intrinsic_size`]
    /// to lay out a view in isolation. The `font_px` closure converts dp font
    /// sizes to physical pixels.
    fn build_taffy_subtree(
        &self,
        view: &View,
        taffy: &mut taffy::TaffyTree<NodeContext>,
        font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::NodeId {
        let style = self.style_from_kind(&view.kind, &view.modifier, font_px);
        let ctx = Self::context_from_kind_or_modifier(&view.kind, &view.modifier);
        let is_zstack = matches!(view.kind, ViewKind::ZStack);
        let scroll_axis = view.modifier.scroll.as_ref().map(|s| s.axis());

        let child_tids: Vec<taffy::NodeId> = view
            .children
            .iter()
            .map(|c| self.build_taffy_subtree(c, taffy, font_px))
            .collect();

        let t = if child_tids.is_empty() {
            taffy.new_leaf_with_context(style, ctx).unwrap()
        } else {
            let t = taffy.new_with_children(style, &child_tids).unwrap();
            let _ = taffy.set_node_context(t, Some(ctx));
            t
        };

        Self::make_children_absolute_on(is_zstack, &child_tids, taffy);
        if let Some(axis) = scroll_axis {
            Self::apply_scroll_content_styles(axis, &child_tids, taffy);
        }
        t
    }

    /// Run a layout + measure pass over a Taffy tree, sharing the measure closure
    /// (custom layout callbacks, scope-tree on-demand compute, and `measure_node`)
    /// between the main incremental path and the one-shot `intrinsic_size` path.
    #[allow(clippy::too_many_arguments)]
    fn run_measure_pass(
        taffy: &mut TaffyTree<NodeContext>,
        taffy_root: taffy::NodeId,
        available: taffy::geometry::Size<taffy::style::AvailableSpace>,
        tree: &ViewTree,
        text_cache: &mut FxHashMap<NodeId, TextLayout>,
        reverse_map: &FxHashMap<taffy::NodeId, NodeId>,
        scope_root_map: &FxHashMap<NodeId, String>,
        node_to_scope: &FxHashMap<NodeId, String>,
        scope_trees: &mut HashMap<String, ScopeLayoutTree>,
        font_px: &dyn Fn(f32) -> f32,
        px: &dyn Fn(f32) -> f32,
    ) {
        let _ = taffy.compute_layout_with_measure(
            taffy_root,
            available,
            |known, avail, taffy_node, ctx, _style| {
                // Check if this is a scope root marker → return cached scope size
                if let Some(&node_id) = reverse_map.get(&taffy_node) {
                    // Custom layout modifier: delegate measurement to user callback
                    if let Some(node) = tree.get(node_id) {
                        if let Some(ref layout_cb) = node.modifier.layout {
                            let scale = dp_to_px(1.0);
                            let avail_w = match avail.width {
                                AvailableSpace::Definite(w) => w / scale,
                                _ => f32::INFINITY,
                            };
                            let avail_h = match avail.height {
                                AvailableSpace::Definite(h) => h / scale,
                                _ => f32::INFINITY,
                            };
                            let known_w =
                                known.width.map(|w| w / scale).unwrap_or(f32::INFINITY);
                            let known_h = known
                                .height
                                .map(|h| h / scale)
                                .unwrap_or(f32::INFINITY);
                            let constraints =
                                repose_core::modifier::LayoutConstraints {
                                    min_width: 0.0,
                                    max_width: avail_w.min(known_w),
                                    min_height: 0.0,
                                    max_height: avail_h.min(known_h),
                                };
                            let (w_dp, h_dp) = layout_cb(constraints);
                            return taffy::geometry::Size {
                                width: w_dp * scale,
                                height: h_dp * scale,
                            };
                        }
                    }
                    if scope_root_map.contains_key(&node_id) {
                        if let Some(key) = node_to_scope.get(&node_id) {
                            if let Some(st) = scope_trees.get_mut(key) {
                                // Compose-style constraint-equality skip:
                                // if content unchanged (valid) AND constraints match → skip scope compute
                                let constraints_changed = st
                                    .last_constraints
                                    .map(|(k, a)| k != known || a != avail)
                                    .unwrap_or(true);
                                let can_skip = st.valid && !constraints_changed;
                                if can_skip {
                                    if let Some(sz) = st.cached_size {
                                        return sz;
                                    }
                                }
                                // Compute scope's internal layout on-demand
                                if let Some(root_tid) = st.root_taffy_id {
                                    let scope_avail = taffy::geometry::Size {
                                        width: match known.width {
                                            Some(w) if w.is_finite() => {
                                                AvailableSpace::Definite(w)
                                            }
                                            _ => match avail.width {
                                                AvailableSpace::Definite(w) => {
                                                    AvailableSpace::Definite(w)
                                                }
                                                _ => AvailableSpace::MaxContent,
                                            },
                                        },
                                        height: match known.height {
                                            Some(h) if h.is_finite() => {
                                                AvailableSpace::Definite(h)
                                            }
                                            _ => match avail.height {
                                                AvailableSpace::Definite(h) => {
                                                    AvailableSpace::Definite(h)
                                                }
                                                _ => AvailableSpace::MaxContent,
                                            },
                                        },
                                    };
                                    let st_rev = &st.reverse_map;
                                    let st_tc = &mut st.text_cache;
                                    let _ = st.taffy.compute_layout_with_measure(
                                        root_tid,
                                        scope_avail,
                                        |known2, avail2, tn, ctx2, _style2| {
                                            Self::measure_node(
                                                known2,
                                                avail2,
                                                tn,
                                                ctx2.as_deref(),
                                                st_tc,
                                                st_rev,
                                                tree,
                                                font_px,
                                                px,
                                            )
                                        },
                                    );
                                    st.last_constraints = Some((known, avail));
                                    if let Ok(layout) = st.taffy.layout(root_tid) {
                                        st.cached_size = Some(taffy::Size {
                                            width: layout.size.width,
                                            height: layout.size.height,
                                        });
                                        st.valid = true;
                                        return st.cached_size.unwrap();
                                    }
                                }
                            }
                        }
                    }
                }
                Self::measure_node(
                    known,
                    avail,
                    taffy_node,
                    ctx.as_deref(),
                    text_cache,
                    reverse_map,
                    tree,
                    font_px,
                    px,
                )
            },
        );
    }

    /// Sync scope-internal TaffyTrees. Handles removed/dirty nodes within each scope.
    fn sync_scope_trees(&mut self, font_px: &dyn Fn(f32) -> f32) {
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

    /// Create or update a Taffy node within a scope's internal TaffyTree.
    /// Recurses into children but stops at nested scope boundaries.
    fn update_scope_taffy_node(
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

        // Recurse into children but stop at nested scope boundaries
        // Must collect entire result before holding scope_trees mutably
        let non_scope_children: Vec<NodeId> = children
            .iter()
            .filter(|&c| !self.scope_root_map.contains_key(c))
            .copied()
            .collect();
        let child_tids: Vec<taffy::NodeId> = non_scope_children
            .iter()
            .map(|&c| self.update_scope_taffy_node(scope_key, c, font_px))
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

    fn sync_taffy_tree(&mut self, root_id: NodeId, font_px: &dyn Fn(f32) -> f32) {
        // Removals from root tree (non-scope nodes + scope root markers)
        for &node_id in &self.tree.removed_ids {
            if self.node_to_scope.contains_key(&node_id) {
                continue; // scope tree handles its own removals
            }
            if let Some(taffy_id) = self.taffy_map.remove(&node_id) {
                let _ = self.taffy.remove(taffy_id);
                self.reverse_map.remove(&taffy_id);
                self.text_cache.remove(&node_id);
                self.paint_cache.remove(&node_id);
            }
            self.view_ids.remove(&node_id);
        }

        // Updates -> only non-scope and scope-root-marker nodes
        let dirty_nodes: Vec<NodeId> = self.tree.dirty_nodes().iter().copied().collect();
        for node_id in dirty_nodes {
            if self.node_to_scope.contains_key(&node_id)
                && !self.scope_root_map.contains_key(&node_id)
            {
                // Scope-internal, non-root → handled by sync_scope_trees
                continue;
            }
            self.update_taffy_node(node_id, font_px);
        }

        // Ensure root
        if !self.taffy_map.contains_key(&root_id) {
            self.update_taffy_node(root_id, font_px);
        }
    }

    fn update_taffy_node(
        &mut self,
        node_id: NodeId,
        font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::NodeId {
        // Ensure this node has a stable view id
        let _ = self.ensure_view_id(node_id);

        // Scope root → create leaf marker in root tree (no children in root tree).
        // Only margin + sizing constraints are kept; padding/gap/flex are handled by the scope tree.
        if self.scope_root_map.contains_key(&node_id) {
            if let Some(&t_id) = self.taffy_map.get(&node_id) {
                let (new_style, new_ctx) = {
                    let node = self.tree.get(node_id).unwrap();
                    let mut s = self.style_from_node(node, font_px);
                    s.padding = taffy::geometry::Rect::zero();
                    (s, self.context_from_node(node))
                };
                let _ = self.taffy.set_style(t_id, new_style);
                let _ = self.taffy.set_node_context(t_id, Some(new_ctx));
                return t_id;
            }
            let (style, ctx) = {
                let node = self.tree.get(node_id).unwrap();
                let mut s = self.style_from_node(node, font_px);
                s.padding = taffy::geometry::Rect::zero();
                (s, self.context_from_node(node))
            };
            let t_id = self.taffy.new_leaf_with_context(style, ctx).unwrap();
            self.taffy_map.insert(node_id, t_id);
            self.reverse_map.insert(t_id, node_id);
            self.stats.taffy_created += 1;
            return t_id;
        }

        // Non-scope node: standard path
        if let Some(&t_id) = self.taffy_map.get(&node_id) {
            self.apply_updates_to_taffy(node_id, t_id, font_px);
            return t_id;
        }

        let (style, ctx, children, is_zstack, scroll_axis) = {
            let node = self.tree.get(node_id).expect("Node missing in update");
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
                matches!(node.kind, ViewKind::ZStack),
                node.modifier.scroll.as_ref().map(|s| s.axis()),
            )
        };

        let non_scope_children: Vec<NodeId> = children
            .iter()
            .filter(|&c| !self.scope_root_map.contains_key(c))
            .copied()
            .collect();
        let child_taffy_ids: Vec<taffy::NodeId> = non_scope_children
            .iter()
            .map(|&child_id| self.update_taffy_node(child_id, font_px))
            .collect();

        let t_id = if child_taffy_ids.is_empty() {
            self.taffy.new_leaf_with_context(style, ctx).unwrap()
        } else {
            let t = self
                .taffy
                .new_with_children(style, &child_taffy_ids)
                .unwrap();
            let _ = self.taffy.set_node_context(t, Some(ctx));
            self.make_children_absolute(is_zstack, &child_taffy_ids);
            if let Some(axis) = scroll_axis {
                LayoutEngine::apply_scroll_content_styles(axis, &child_taffy_ids, &mut self.taffy);
            }
            t
        };

        self.taffy_map.insert(node_id, t_id);
        self.reverse_map.insert(t_id, node_id);
        self.stats.taffy_created += 1;
        t_id
    }

    fn apply_updates_to_taffy(
        &mut self,
        node_id: NodeId,
        taffy_id: taffy::NodeId,
        font_px: &dyn Fn(f32) -> f32,
    ) {
        // Ensure this node has a stable view id
        let _ = self.ensure_view_id(node_id);

        let (new_style, new_ctx, children, is_zstack, scroll_axis) = {
            let node = self.tree.get(node_id).unwrap();
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
                matches!(node.kind, ViewKind::ZStack),
                node.modifier.scroll.as_ref().map(|s| s.axis()),
            )
        };

        let _ = self.taffy.set_style(taffy_id, new_style);
        let _ = self.taffy.set_node_context(taffy_id, Some(new_ctx));

        // If this node's parent is a ZStack, style_from_node just set
        // position back to Relative (the default), clobbering the
        // Absolute that the parent's is_zstack block originally applied.
        // Re-apply Absolute here so the child stays in the overlap layer
        // even when only the child is dirty (e.g. during a dialog's
        // fade/scale animation).
        let parent_is_zstack = {
            let node = self.tree.get(node_id).unwrap();
            node.parent
                .and_then(|pid| self.tree.get(pid))
                .is_some_and(|p| matches!(p.kind, ViewKind::ZStack))
        };
        if parent_is_zstack && let Ok(cs) = self.taffy.style(taffy_id) {
            let mut new_cs = cs.clone();
            new_cs.position = Position::Absolute;
            let _ = self.taffy.set_style(taffy_id, new_cs);
        }

        let child_taffy_ids: Vec<taffy::NodeId> = children
            .iter()
            .map(|&child_id| self.update_taffy_node(child_id, font_px))
            .collect();
        let _ = self.taffy.set_children(taffy_id, &child_taffy_ids);

        self.make_children_absolute(is_zstack, &child_taffy_ids);
        if let Some(axis) = scroll_axis {
            LayoutEngine::apply_scroll_content_styles(axis, &child_taffy_ids, &mut self.taffy);
        }

        self.stats.taffy_reused += 1;
    }

    fn make_children_absolute_on(
        is_zstack: bool,
        child_taffy_ids: &[taffy::NodeId],
        taffy: &mut TaffyTree<NodeContext>,
    ) {
        if !is_zstack {
            return;
        }
        for &child_tid in child_taffy_ids {
            if let Ok(cs) = taffy.style(child_tid) {
                let mut new_cs = cs.clone();
                new_cs.position = Position::Absolute;
                let _ = taffy.set_style(child_tid, new_cs);
            }
        }
    }

    /// Apply scroll-content sizing to direct children of a scroll container.
    ///
    /// Scroll parents clip/overflow; children must:
    /// - not flex-shrink (so content can overflow the viewport),
    /// - size to content on the scroll axis (`auto`),
    /// - be at least as large as the viewport on both axes (`min_size` 100%)
    ///   so empty / short content still fills the viewport (nested scroll / nav pages).
    fn apply_scroll_content_styles(
        axis: ScrollAxis,
        child_taffy_ids: &[taffy::NodeId],
        taffy: &mut TaffyTree<NodeContext>,
    ) {
        for &child_tid in child_taffy_ids {
            let Ok(cs) = taffy.style(child_tid) else { continue };
            let mut new_cs = cs.clone();
            new_cs.flex_shrink = 0.0;
            // Fill viewport at minimum so nested scroll / short pages still work.
            new_cs.min_size.width = percent(1.0);
            new_cs.min_size.height = percent(1.0);
            match axis {
                ScrollAxis::Vertical => {
                    // Grow with content vertically; width already min 100%.
                    new_cs.size.height = Dimension::auto();
                }
                ScrollAxis::Horizontal => {
                    new_cs.size.width = Dimension::auto();
                }
                ScrollAxis::Both => {
                    new_cs.size.width = Dimension::auto();
                    new_cs.size.height = Dimension::auto();
                }
            }
            let _ = taffy.set_style(child_tid, new_cs);
        }
    }

    fn make_children_absolute(&mut self, is_zstack: bool, child_taffy_ids: &[taffy::NodeId]) {
        Self::make_children_absolute_on(is_zstack, child_taffy_ids, &mut self.taffy);
    }

    fn style_from_node(&self, node: &TreeNode, font_px: &dyn Fn(f32) -> f32) -> taffy::Style {
        self.style_from_kind(&node.kind, &node.modifier, font_px)
    }

    fn style_from_kind(
        &self,
        kind: &ViewKind,
        m: &repose_core::Modifier,
        _font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::Style {
        let px = |dp_val: f32| dp_to_px(dp_val);
        let mut s = taffy::Style::default();

        s.display = Display::Flex;
        match kind {
            ViewKind::Row => {
                s.flex_direction = if locals::text_direction() == locals::TextDirection::Rtl {
                    FlexDirection::RowReverse
                } else {
                    FlexDirection::Row
                };
            }
            ViewKind::Column | ViewKind::OverlayHost => {
                s.flex_direction = FlexDirection::Column;
            }
            ViewKind::ZStack => s.display = Display::Grid,
            _ => {}
        }
        // Modifier scroll overrides kind-based direction.
        if let Some(ref scroll) = m.scroll {
            match scroll.axis() {
                ScrollAxis::Vertical => s.flex_direction = FlexDirection::Column,
                ScrollAxis::Horizontal => s.flex_direction = FlexDirection::Row,
                ScrollAxis::Both => s.flex_direction = FlexDirection::Column,
            }
        }

        s.align_items = Some(AlignItems::STRETCH);
        // Needed for 2D scroll.
        let is_2d_scroll = matches!(m.scroll.as_ref().map(|s| s.axis()), Some(ScrollAxis::Both));
        if is_2d_scroll {
            s.align_items = Some(AlignItems::FLEX_START);
        }
        if s.display != Display::Grid {
            s.justify_content = Some(JustifyContent::FLEX_START);
        }

        if matches!(kind, ViewKind::Image { .. }) {
            s.flex_shrink = 0.0;
        } else {
            s.flex_shrink = 1.0;
        }

        if let Some(g) = m.flex_grow {
            s.flex_grow = g.max(0.0);
        }
        if let Some(sh) = m.flex_shrink {
            s.flex_shrink = sh.max(0.0);
        }
        if let Some(b) = m.flex_basis {
            s.flex_basis = length(px(b.max(0.0)));
        }
        if let Some(w) = m.flex_wrap {
            s.flex_wrap = w;
        }
        if let Some(d) = m.flex_dir {
            s.flex_direction = d;
        }
        if let Some(a) = m.align_self {
            s.align_self = Some(a);
        }
        if let Some(j) = m.justify_content {
            s.justify_content = Some(j);
        }
        if let Some(ai) = m.align_items_container {
            s.align_items = Some(ai);
        }
        if let Some(ac) = m.align_content {
            s.align_content = Some(ac);
        }

        let row_gap_dp = m.row_gap.or(m.gap);
        let column_gap_dp = m.column_gap.or(m.gap);

        if let Some(v) = column_gap_dp {
            s.gap.width = length(px(v.max(0.0)));
        }

        if let Some(v) = row_gap_dp {
            s.gap.height = length(px(v.max(0.0)));
        }

        if let Some(v) = m.margin_top {
            s.margin.top = length(px(v));
        }
        if let Some(v) = m.margin_left {
            s.margin.left = length(px(v));
        }
        if let Some(v) = m.margin_right {
            s.margin.right = length(px(v));
        }
        if let Some(v) = m.margin_bottom {
            s.margin.bottom = length(px(v));
        }

        if let Some(PositionType::Absolute) = m.position_type {
            s.position = Position::Absolute;
            s.inset = taffy::geometry::Rect {
                left: m.offset_left.map(|v| length(px(v))).unwrap_or_else(auto),
                right: m.offset_right.map(|v| length(px(v))).unwrap_or_else(auto),
                top: m.offset_top.map(|v| length(px(v))).unwrap_or_else(auto),
                bottom: m.offset_bottom.map(|v| length(px(v))).unwrap_or_else(auto),
            };
        }

        if let Some(cfg) = &m.grid {
            s.display = Display::Grid;
            s.grid_template_columns = (0..cfg.columns.max(1))
                .map(|_| GridTemplateComponent::Single(flex(1.0)))
                .collect();
            if column_gap_dp.is_none() {
                s.gap.width = length(px(cfg.column_gap));
            }
            if row_gap_dp.is_none() {
                s.gap.height = length(px(cfg.row_gap));
            }
        }

        if m.scroll.is_some() {
            // Clip on both axes: Taffy content-box / overflow behavior must match the
            // paint clip (PushClip + offset applied in `walk_paint`). Axis-aware child
            // sizing is handled separately via `apply_scroll_content_styles`.
            s.overflow = taffy::geometry::Point {
                x: Overflow::Hidden,
                y: Overflow::Hidden,
            };
        }

        if let Some(pv) = m.padding_values {
            s.padding = taffy::geometry::Rect {
                left: length(px(pv.left)),
                right: length(px(pv.right)),
                top: length(px(pv.top)),
                bottom: length(px(pv.bottom)),
            };
        } else if let Some(p) = m.padding {
            let v = length(px(p));
            s.padding = taffy::geometry::Rect {
                left: v,
                right: v,
                top: v,
                bottom: v,
            };
        }

        let mut width_set = false;
        let mut height_set = false;
        if let Some(sz) = m.size {
            if sz.width.is_finite() {
                s.size.width = length(px(sz.width.max(0.0)));
                width_set = true;
            }
            if sz.height.is_finite() {
                s.size.height = length(px(sz.height.max(0.0)));
                height_set = true;
            }
        }
        if let Some(w) = m.width {
            s.size.width = length(px(w.max(0.0)));
            width_set = true;
        }
        if let Some(h) = m.height {
            s.size.height = length(px(h.max(0.0)));
            height_set = true;
        }

        if let Some(sz) = m.required_size {
            let w = length(px(sz.width.max(0.0)));
            let h = length(px(sz.height.max(0.0)));
            s.size.width = w;
            s.size.height = h;
            s.min_size.width = w;
            s.min_size.height = h;
            s.max_size.width = w;
            s.max_size.height = h;
            width_set = true;
            height_set = true;
        }

        // Fill max (with optional fraction). Per-axis fields override both-dims field.
        let fill_w = m.fill_max_w.or(m.fill_max);
        if let Some(frac) = fill_w {
            if !width_set {
                s.size.width = percent(frac);
            }
            if s.min_size.width.is_auto() {
                s.min_size.width = length(0.0);
            }
        }
        let fill_h = m.fill_max_h.or(m.fill_max);
        if let Some(frac) = fill_h {
            if !height_set {
                s.size.height = percent(frac);
            }
            if s.min_size.height.is_auto() {
                s.min_size.height = length(0.0);
            }
        }

        //NOTE: Don't force min-width: 0 globally. The Auto default (min-content)
        // is correct - it prevents content from shrinking below its natural
        // size, which is essential for scroll containers to overflow properly.

        if let ViewKind::TreeRow {
            depth,
            has_children,
            ..
        } = kind
        {
            // Indent leaves more than parent nodes for visual tree structure:
            // depth * 16dp + chevron space if has_children.
            let indent_dp = *depth as f32 * 16.0 + if *has_children { 16.0 } else { 24.0 };
            s.padding.left = length(px(indent_dp));
        }

        if m.required_size.is_none() {
            if let Some(v) = m.min_width {
                s.min_size.width = length(px(v.max(0.0)));
            }
            if let Some(v) = m.min_height {
                s.min_size.height = length(px(v.max(0.0)));
            }
            if let Some(v) = m.max_width {
                s.max_size.width = length(px(v.max(0.0)));
            }
            if let Some(v) = m.max_height {
                s.max_size.height = length(px(v.max(0.0)));
            }
        }

        // Required range (overrides constraints, like required_size but per-axis)
        if let Some(v) = m.required_min_width {
            s.min_size.width = length(px(v.max(0.0)));
        }
        if let Some(v) = m.required_max_width {
            s.max_size.width = length(px(v.max(0.0)));
        }
        if let Some(v) = m.required_min_height {
            s.min_size.height = length(px(v.max(0.0)));
        }
        if let Some(v) = m.required_max_height {
            s.max_size.height = length(px(v.max(0.0)));
        }

        // Default min size (only applies when incoming constraint is 0 / unconstrained)
        // This is handled during constraint resolution in compute_layout, but we note
        // the values here. The actual enforcement happens when the parent gives 0 min.
        // For Taffy, we don't apply these unconditionally -> they must be constraint-aware.
        if m.default_min_width.is_some() || m.default_min_height.is_some() {
            // Store flags so the constraint-pass can check them.
            // Taffy style doesn't have a direct "default min" concept, so we defer
            // to the layout engine's constraint override logic.
            if let Some(v) = m.default_min_width {
                if s.min_size.width.is_auto() || s.min_size.width == length(0.0) {
                    s.min_size.width = length(px(v.max(0.0)));
                }
            }
            if let Some(v) = m.default_min_height {
                if s.min_size.height.is_auto() || s.min_size.height == length(0.0) {
                    s.min_size.height = length(px(v.max(0.0)));
                }
            }
        }
        if let Some(r) = m.aspect_ratio {
            s.aspect_ratio = Some(r.max(0.0));
        }

        if m.grid_col_span.is_some() || m.grid_row_span.is_some() {
            let col_span = m.grid_col_span.unwrap_or(1).max(1);
            let row_span = m.grid_row_span.unwrap_or(1).max(1);
            s.grid_column = taffy::geometry::Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Span(col_span),
            };
            s.grid_row = taffy::geometry::Line {
                start: GridPlacement::Auto,
                end: GridPlacement::Span(row_span),
            };
        }
        s
    }

    fn context_from_node(&self, node: &TreeNode) -> NodeContext {
        Self::context_from_kind_or_modifier(&node.kind, &node.modifier)
    }

    /// Shared context derivation used by both the persistent tree path and the
    /// measure-only `build_taffy_subtree` path so intrinsic sizing sees the same
    /// `NodeContext` (scroll containers, text inputs, ...).
    fn context_from_kind_or_modifier(kind: &ViewKind, m: &repose_core::Modifier) -> NodeContext {
        if m.scroll.is_some() {
            return NodeContext::ScrollContainer;
        }
        if let Some(ref ti) = m.text_input {
            return NodeContext::TextInput {
                multiline: ti.multiline,
            };
        }
        Self::context_from_kind(kind)
    }

    fn context_from_kind(kind: &ViewKind) -> NodeContext {
        match kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                soft_wrap,
                max_lines,
                overflow,
                font_family,
                annotations,
                text_align,
                font_weight,
                font_style,
                text_decoration,
                letter_spacing,
                line_height,
                font_variation_settings,
                ..
            } => NodeContext::Text {
                text: text.clone(),
                color: *color,
                font_dp: *font_size,
                soft_wrap: *soft_wrap,
                max_lines: *max_lines,
                overflow: *overflow,
                font_family: *font_family,
                annotations: annotations.clone(),
                text_align: *text_align,
                font_weight: *font_weight,
                font_style: *font_style,
                text_decoration: *text_decoration,
                letter_spacing: *letter_spacing,
                line_height: *line_height,
                font_variation_settings: font_variation_settings.clone(),
            },
            ViewKind::Expander { .. } => NodeContext::Container,
            ViewKind::OverlayHost => NodeContext::Container,
            ViewKind::SubcomposeLayout { .. } => NodeContext::Container,
            _ => NodeContext::Container,
        }
    }

    fn measure_node(
        known: taffy::geometry::Size<Option<f32>>,
        avail: taffy::geometry::Size<AvailableSpace>,
        taffy_node: taffy::NodeId,
        ctx: Option<&NodeContext>,
        text_cache: &mut FxHashMap<NodeId, TextLayout>,
        reverse_map: &FxHashMap<taffy::NodeId, NodeId>,
        _tree: &ViewTree,
        font_px: &dyn Fn(f32) -> f32,
        px: &dyn Fn(f32) -> f32,
    ) -> taffy::geometry::Size<f32> {
        match ctx {
            Some(NodeContext::Text {
                text,
                color: _,
                font_dp,
                soft_wrap,
                max_lines,
                overflow,
                font_family,
                annotations: _,
                text_align,
                font_weight,
                font_style,
                text_decoration,
                letter_spacing,
                line_height,
                font_variation_settings,
            }) => {
                let size_px_val = font_px(*font_dp);
                let lh = if *line_height > 0.0 {
                    font_px(*line_height)
                } else {
                    size_px_val
                };
                let line_h_px_val = lh;
                let fw = font_weight.0;
                let fs = if matches!(font_style, FontStyle::Italic) {
                    1
                } else {
                    0
                };
                let max_content_w = measure_text(text, size_px_val, TextMeasureConfig { font_family: *font_family, font_weight: fw, font_style: fs, letter_spacing: *letter_spacing, ..Default::default() })
                    .positions
                    .last()
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);

                let mut min_content_w = 0.0f32;
                for w in text.split_whitespace() {
                    let ww = measure_text(w, size_px_val, TextMeasureConfig { font_family: *font_family, font_weight: fw, font_style: fs, letter_spacing: *letter_spacing, ..Default::default() })
                        .positions
                        .last()
                        .copied()
                        .unwrap_or(0.0);
                    min_content_w = min_content_w.max(ww);
                }
                if min_content_w <= 0.0 {
                    min_content_w = max_content_w;
                }

                let wrap_w_px = if let Some(w) = known.width.filter(|w| *w > 0.5) {
                    w
                } else {
                    match avail.width {
                        AvailableSpace::Definite(w) if w > 0.5 => w,
                        AvailableSpace::MinContent => min_content_w,
                        AvailableSpace::MaxContent => max_content_w,
                        _ => max_content_w,
                    }
                };

                let fvs = font_variation_settings.as_deref();
                let (lines, line_ranges): (Vec<String>, Vec<(usize, usize)>) = if *soft_wrap {
                    let (ranges, truncated) = repose_text::wrap_line_ranges(
                        text,
                        size_px_val,
                        wrap_w_px,
                        *max_lines,
                        true,
                        fw,
                        fs,
                        *letter_spacing,
                        fvs,
                    );
                    let mut lns: Vec<String> = ranges
                        .iter()
                        .map(|&(s, e)| text[s..e].to_string())
                        .collect();
                    if truncated && matches!(overflow, TextOverflow::Ellipsis) {
                        if let Some(last) = lns.last_mut() {
                            let with_tail = format!("{}…", last);
                            *last =
                                repose_text::ellipsize_line(&with_tail, size_px_val, wrap_w_px, fw, fs, *letter_spacing, fvs);
                        }
                    }
                    (lns, ranges)
                } else if matches!(overflow, TextOverflow::Ellipsis) {
                    let elided = repose_text::ellipsize_line(text, size_px_val, wrap_w_px, fw, fs, *letter_spacing, fvs);
                    let elided_len = elided.len();
                    (vec![elided], vec![(0, elided_len)])
                } else {
                    let len = text.len();
                    (vec![text.clone()], vec![(0, len)])
                };

                let line_widths: Vec<f32> = lines
                    .iter()
                    .map(|line| {
                        measure_text(line, size_px_val, TextMeasureConfig { font_family: *font_family, font_weight: fw, font_style: fs, letter_spacing: *letter_spacing, ..Default::default() })
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .collect();
                let max_line_w = line_widths.iter().copied().fold(0.0f32, f32::max);

                if let Some(node_id) = reverse_map.get(&taffy_node) {
                    text_cache.insert(
                        *node_id,
                        TextLayout {
                            lines: lines.clone(),
                            line_ranges,
                            size_px: size_px_val,
                            line_h_px: line_h_px_val,
                            line_widths,
                        },
                    );
                }
                taffy::geometry::Size {
                    width: max_line_w,
                    height: line_h_px_val * lines.len().max(1) as f32,
                }
            }
            Some(NodeContext::TextInput { multiline }) => {
                let natural_w = px(160.0);
                let width = match avail.width {
                    AvailableSpace::Definite(w) if w > 0.5 => w,
                    AvailableSpace::MinContent => px(48.0).max(natural_w),
                    _ => known.width.unwrap_or(natural_w),
                };
                let natural_h = if *multiline { px(140.0) } else { px(36.0) };
                let height = match avail.height {
                    AvailableSpace::Definite(h) if h > 0.5 => h,
                    AvailableSpace::MinContent => px(28.0).max(natural_h),
                    _ => known.height.unwrap_or(natural_h),
                };
                taffy::geometry::Size { width, height }
            }
            _ => taffy::geometry::Size::ZERO,
        }
    }

    /// Advance scroll physics on all scrollable nodes before paint.
    /// Keeps mutation out of the render walk.
    fn walk_tick(&self, root_id: NodeId) {
        let mut stack = vec![root_id];
        while let Some(id) = stack.pop() {
            let Some(n) = self.tree.get(id) else { continue };
            let scroll_tick = n.modifier.scroll.as_ref().and_then(|s| match s {
                ScrollBinding::Vertical(b) => b.tick.clone(),
                ScrollBinding::Horizontal(b) => b.tick.clone(),
                ScrollBinding::Both(b) => b.tick.clone(),
            });
            if let Some(tick) = scroll_tick {
                tick();
            }
            for &ch in n.children.iter() {
                stack.push(ch);
            }
        }
    }

    fn paint(
        &mut self,
        root_id: NodeId,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        font_px: &dyn Fn(f32) -> f32,
    ) -> (Scene, Vec<HitRegion>, Vec<SemNode>) {
        let mut scene = Scene {
            clear_color: locals::theme().background,
            nodes: vec![],
        };
        let mut hits = Vec::new();
        let mut sems = Vec::new();
        let mut deferred: Vec<(NodeId, (f32, f32), f32, Option<u64>, f32)> = Vec::new();
        let mut deferred_blockers: Vec<(f32, repose_core::Rect)> = Vec::new();

        self.walk_paint(
            root_id,
            &mut scene,
            &mut hits,
            &mut sems,
            textfield_states,
            interactions,
            focused,
            (0.0, 0.0),
            1.0,
            None,
            None, // interaction_source
            font_px,
            true,
            &mut deferred,
            false, // Allow deferral in first pass
        );

        // Paint deferred nodes sorted by render_z_index (ascending = higher on top)
        deferred.sort_by(|a, b| a.4.partial_cmp(&b.4).unwrap_or(Ordering::Equal));
        for (node_id, parent_offset_px, alpha_accum, sem_parent, z) in deferred.iter().copied() {
            let view_id = *self.view_ids.get(&node_id).unwrap_or(&0);
            let layout = self.layout_for_node(node_id);
            let rect = repose_core::Rect {
                x: parent_offset_px.0 + layout.location.x,
                y: parent_offset_px.1 + layout.location.y,
                w: layout.size.width,
                h: layout.size.height,
            };
            if let Some(node) = self.tree.get(node_id)
                && node.modifier.input_blocker
                && !node.modifier.hit_passthrough
            {
                deferred_blockers.push((z, rect));
            }
            self.walk_paint(
                node_id,
                &mut scene,
                &mut hits,
                &mut sems,
                textfield_states,
                interactions,
                focused,
                parent_offset_px,
                alpha_accum,
                sem_parent,
                None, // interaction_source
                font_px,
                true,
                &mut Vec::new(), // No further deferral in second pass
                true,            // Skip defer check
            );
            let _ = view_id;
        }
        deferred.clear();

        if !deferred_blockers.is_empty() {
            deferred_blockers.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            let max_z = hits
                .iter()
                .map(|h| h.z_index)
                .fold(0.0_f32, |a, b| a.max(b));
            let bump = max_z + 1.0;
            for (i, (_z, rect)) in deferred_blockers.iter().enumerate() {
                let blocker_id = u64::MAX - i as u64;
                hits.push(HitRegion {
                    id: blocker_id,
                    rect: *rect,
                    z_index: bump + i as f32,
                    ..Default::default()
                });
            }
        }

        hits.sort_by(|a, b| a.z_index.partial_cmp(&b.z_index).unwrap_or(Ordering::Equal));
        (scene, hits, sems)
    }

    fn paint_stamp_hash(
        &self,
        root: NodeId,
        interactions: &Interactions,
        focused: Option<u64>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        sem_parent: Option<u64>,
        alpha_accum: f32,
    ) -> u64 {
        let mut h = FxHasher::default();
        sem_parent.hash(&mut h);
        let alpha_q: u8 = (alpha_accum.clamp(0.0, 1.0) * 255.0).round() as u8;
        alpha_q.hash(&mut h);
        interactions.hover.hash(&mut h);
        focused.hash(&mut h);
        if !interactions.pressed.is_empty() {
            let mut pressed: Vec<u64> = interactions.pressed.iter().copied().collect();
            pressed.sort_unstable();
            pressed.hash(&mut h);
        }

        let mut stack = Vec::new();
        stack.push(root);
        while let Some(id) = stack.pop() {
            let Some(n) = self.tree.get(id) else { continue };
            if let Some(s) = &n.modifier.scroll {
                match s {
                    ScrollBinding::Vertical(b) => {
                        if let Some(get) = &b.get_offset_main {
                            let q = (get() * 8.0) as i32;
                            q.hash(&mut h);
                        }
                    }
                    ScrollBinding::Horizontal(b) => {
                        if let Some(get) = &b.get_offset_main {
                            let q = (get() * 8.0) as i32;
                            q.hash(&mut h);
                        }
                    }
                    ScrollBinding::Both(b) => {
                        if let Some(get) = &b.get_offset_xy {
                            let (x, y) = get();
                            ((x * 8.0) as i32).hash(&mut h);
                            ((y * 8.0) as i32).hash(&mut h);
                        }
                    }
                }
            }
            if n.modifier.text_input.is_some() {
                let vid = *self.view_ids.get(&id).unwrap_or(&0);
                let tf_key = vid;
                if let Some(st_rc) = textfield_states.get(&tf_key) {
                    let st = st_rc.borrow();
                    let mut th = FxHasher::default();
                    st.text.hash(&mut th);
                    th.finish().hash(&mut h);
                    st.selection.start.hash(&mut h);
                    st.selection.end.hash(&mut h);
                    if let Some(r) = &st.composition {
                        r.start.hash(&mut h);
                        r.end.hash(&mut h);
                    } else {
                        0usize.hash(&mut h);
                        0usize.hash(&mut h);
                    }
                    ((st.scroll_offset * 8.0) as i32).hash(&mut h);
                    ((st.scroll_offset_y * 8.0) as i32).hash(&mut h);
                    st.caret_visible().hash(&mut h);
                }
            }
            for &ch in n.children.iter() {
                stack.push(ch);
            }
        }
        h.finish()
    }

    /// Walk up the ancestor tree to find the nearest node whose modifier has
    /// a `nested_scroll_connection`. Returns the connection if found.
    fn find_ancestor_nested_scroll(&self, node_id: NodeId) -> Option<NestedScrollConnection> {
        let mut current = node_id;
        loop {
            let node = self.tree.get(current)?;
            if node.parent.is_none() {
                return None;
            }
            let parent_id = node.parent?;
            let parent = self.tree.get(parent_id)?;
            if let Some(ref conn) = parent.modifier.nested_scroll_connection {
                return Some(conn.clone());
            }
            current = parent_id;
        }
    }

    fn walk_paint(
        &mut self,
        node_id: NodeId,
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        sems: &mut Vec<SemNode>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        parent_offset_px: (f32, f32),
        alpha_accum: f32,
        sem_parent: Option<u64>,
        interaction_source: Option<u64>,
        font_px: &dyn Fn(f32) -> f32,
        allow_cache: bool,
        deferred: &mut Vec<(NodeId, (f32, f32), f32, Option<u64>, f32)>,
        skip_defer: bool,
    ) {
        let (subtree_hash, modifier, kind, children) = {
            let n = self.tree.get(node_id).unwrap();
            (
                n.subtree_hash,
                n.modifier.clone(),
                n.kind.clone(),
                n.children.clone(),
            )
        };

        let view_id = *self.view_ids.get(&node_id).unwrap_or(&0);

        // Check if this node should be deferred for later painting
        if !skip_defer
            && let Some(render_z) = modifier.render_z_index
            && (!deferred.is_empty() || render_z != 0.0)
        {
            // Defer this node - it will be painted later based on render_z_index
            deferred.push((node_id, parent_offset_px, alpha_accum, sem_parent, render_z));
            return;
        }
        debug_assert!(view_id != 0);

        let layout = self.layout_for_node(node_id);

        let local_rect = repose_core::Rect {
            x: layout.location.x,
            y: layout.location.y,
            w: layout.size.width,
            h: layout.size.height,
        };
        let mut rect = repose_core::Rect {
            x: parent_offset_px.0 + local_rect.x,
            y: parent_offset_px.1 + local_rect.y,
            w: local_rect.w,
            h: local_rect.h,
        };

        let mut content_rect = {
            let pl = layout.padding.left;
            let pr = layout.padding.right;
            let pt = layout.padding.top;
            let pb = layout.padding.bottom;
            if pl > 0.0 || pr > 0.0 || pt > 0.0 || pb > 0.0 {
                repose_core::Rect {
                    x: rect.x + pl,
                    y: rect.y + pt,
                    w: (rect.w - pl - pr).max(0.0),
                    h: (rect.h - pt - pb).max(0.0),
                }
            } else {
                rect
            }
        };

        let base_px = (rect.x, rect.y);

        let is_hovered = interactions.hover == Some(view_id);
        let is_pressed = interactions.pressed.contains(&view_id);
        let effective_interaction = interaction_source.unwrap_or(view_id);
        let implicit_hovered = interactions.hover == Some(effective_interaction);
        let implicit_pressed = interactions.pressed.contains(&effective_interaction);
        let (state_hovered, state_pressed) = if let Some(ref src) = modifier.interaction_source {
            (src.collect_is_hovered(), src.collect_is_pressed())
        } else {
            (implicit_hovered, implicit_pressed)
        };
        let is_focused = focused == Some(view_id);
        let this_alpha = modifier.alpha.unwrap_or(1.0);
        let alpha_accum = (alpha_accum * this_alpha).clamp(0.0, 1.0);
        let alpha_q: u8 = (alpha_accum * 255.0).round() as u8;

        // Repaint Boundary
        if allow_cache && modifier.repaint_boundary {
            let stamp = self.paint_stamp_hash(
                node_id,
                interactions,
                focused,
                textfield_states,
                sem_parent,
                alpha_accum,
            );
            if let Some(entry) = self.paint_cache.get(&node_id)
                && entry.subtree_hash == subtree_hash
                && entry.stamp == stamp
                && entry.rect == rect
                && entry.parent_offset_px == parent_offset_px
                && entry.sem_parent == sem_parent
                && entry.alpha_q == alpha_q
            {
                self.stats.paint_cache_hits += 1;
                scene.nodes.extend(entry.nodes.iter().cloned());
                hits.extend(entry.hits.iter().cloned());
                sems.extend(entry.sems.iter().cloned());
                return;
            }
            self.stats.paint_cache_misses += 1;
            let mut local_scene = Scene {
                clear_color: scene.clear_color,
                nodes: Vec::new(),
            };
            let mut local_hits = Vec::new();
            let mut local_sems = Vec::new();
            self.walk_paint(
                node_id,
                &mut local_scene,
                &mut local_hits,
                &mut local_sems,
                textfield_states,
                interactions,
                focused,
                parent_offset_px,
                alpha_accum / this_alpha.max(1e-6),
                sem_parent,
                interaction_source,
                font_px,
                false,
                &mut Vec::new(), // Don't defer within repaint boundary
                true,            // Skip defer check in repaint boundary
            );

            let entry = PaintCacheEntry {
                subtree_hash,
                stamp,
                rect,
                parent_offset_px,
                sem_parent,
                alpha_q,
                nodes: Arc::new(local_scene.nodes.clone()),
                hits: Arc::new(local_hits.clone()),
                sems: Arc::new(local_sems.clone()),
            };
            self.paint_cache.insert(node_id, entry);
            scene.nodes.extend(local_scene.nodes);
            hits.extend(local_hits);
            sems.extend(local_sems);
            return;
        }

        let round_clip_px = clamp_radii(
            modifier
                .clip_rounded
                .map(|r| r.map(dp_to_px))
                .unwrap_or([0.0; 4]),
            rect.w,
            rect.h,
        );
        let push_round_clip =
            round_clip_px.iter().any(|&r| r > 0.5) && rect.w > 0.5 && rect.h > 0.5;

        if let Some(anim_spec) = &modifier.animate_content_size {
            let target = repose_core::Size {
                width: rect.w,
                height: rect.h,
            };

            let anim_key = format!("anim_cs:{view_id}");
            let anim =
                remember_state_with_key(&anim_key, || AnimatedValue::new(target, *anim_spec));
            let last_target = remember_state_with_key(format!("anim_cs_last:{view_id}"), || {
                repose_core::Size::default()
            });

            // Check if target changed, and re-start animation from current value
            let mut lt = last_target.borrow_mut();
            if (lt.width - target.width).abs() > 0.5 || (lt.height - target.height).abs() > 0.5 {
                *lt = target;
                drop(lt);
                let mut a = anim.borrow_mut();
                a.set_spec(*anim_spec);
                a.set_target(target);

                // Register with AnimationDriver for pre-composition advancement
                let reg_key = anim_key;
                let reg_anim = anim.clone();
                repose_core::animation_driver::register(
                    reg_key,
                    std::rc::Rc::new(std::cell::RefCell::new(move || {
                        reg_anim.borrow_mut().update()
                    })),
                );
                request_frame();
            } else {
                drop(lt);
            }

            let s = anim.borrow().get().clone();
            let animated = repose_core::Size {
                width: s.width.max(1.0),
                height: s.height.max(1.0),
            };

            // Override rect and content_rect dimensions with animated values
            let dw = rect.w - animated.width;
            let dh = rect.h - animated.height;
            rect.w = animated.width;
            rect.h = animated.height;
            content_rect.w = (content_rect.w - dw).max(0.0);
            content_rect.h = (content_rect.h - dh).max(0.0);
        }

        if let Some(tf) = modifier.transform {
            let mut adjusted = tf;
            let pivot_x = rect.x + rect.w * tf.origin_x;
            let pivot_y = rect.y + rect.h * tf.origin_y;
            let cos_a = tf.rotate.cos();
            let sin_a = tf.rotate.sin();
            let sp_x = pivot_x * tf.scale_x;
            let sp_y = pivot_y * tf.scale_y;
            adjusted.translate_x += pivot_x - (sp_x * cos_a - sp_y * sin_a);
            adjusted.translate_y += pivot_y - (sp_x * sin_a + sp_y * cos_a);
            adjusted.origin_x = 0.0;
            adjusted.origin_y = 0.0;
            scene.nodes.push(SceneNode::PushTransform { transform: adjusted });
        }
        let overflow_clip = modifier
            .overflow
            .map_or(true, |o| o == repose_core::Overflow::Clip);
        if push_round_clip && overflow_clip {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: round_clip_px,
                op: ClipOp::Intersect,
            });
        }
        if let Some(cr) = modifier.clip_rect
            && overflow_clip
        {
            scene.nodes.push(SceneNode::PushClip {
                rect: repose_core::Rect {
                    x: rect.x + dp_to_px(cr.left),
                    y: rect.y + dp_to_px(cr.top),
                    w: (dp_to_px(cr.right) - dp_to_px(cr.left)).max(0.0),
                    h: (dp_to_px(cr.bottom) - dp_to_px(cr.top)).max(0.0),
                },
                radius: [0.0; 4],
                op: cr.op,
            });
        }

        // rendered behind the component
        if let Some(se) = &modifier.state_elevation {
            let target = if modifier.disabled {
                se.disabled
            } else if state_pressed {
                se.pressed
            } else if state_hovered {
                se.hovered
            } else {
                se.default
            };
            let elev = animate_f32(
                format!("m3_elev:{view_id}"),
                target,
                locals::theme().motion.shape,
            );
            if elev > 0.5 {
                let shadow_offset = elev * 0.5;
                let shadow_alpha = ((elev / 24.0).clamp(0.0, 1.0) * 0.25 * 255.0) as u8;
                scene.nodes.push(SceneNode::Shadow {
                    rect: repose_core::Rect {
                        x: rect.x + shadow_offset * 0.5,
                        y: rect.y + shadow_offset,
                        w: rect.w,
                        h: rect.h,
                    },
                    radius: round_clip_px,
                    elevation: elev,
                    color: Color(0, 0, 0, shadow_alpha),
                });
            }
        }

        // Draw background (always)
        if let Some(bg) = modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                brush: mul_alpha_brush(bg, alpha_accum),
                radius: round_clip_px,
            });
        }
        // State layer as overlay on top (independently animated alpha)
        if let Some(sc) = &modifier.state_colors {
            let target = if modifier.disabled {
                sc.disabled
            } else if state_pressed {
                sc.pressed
            } else if state_hovered {
                sc.hovered
            } else {
                sc.default
            };
            let overlay = animate_color(
                format!("m3_sc:{view_id}"),
                target,
                locals::theme().motion.color,
            );
            if overlay.3 > 0 {
                scene.nodes.push(SceneNode::Rect {
                    rect,
                    brush: mul_alpha_brush(Brush::Solid(overlay), alpha_accum),
                    radius: round_clip_px,
                });
            }
        }

        if let Some(b) = &modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: mul_alpha_color(b.color, alpha_accum),
                width: dp_to_px(b.width),
                radius: clamp_radii(
                    max_radii(
                        b.radius.map(dp_to_px),
                        modifier
                            .clip_rounded
                            .map(|r| r.map(dp_to_px))
                            .unwrap_or([0.0; 4]),
                    ),
                    rect.w,
                    rect.h,
                ),
            });
        }
        // Native text field painting (Compose-aligned: text_input modifier triggers built-in paint)
        if let Some(ref ti) = modifier.text_input {
            let tf_key = view_id;
            let state = textfield_states.get(&tf_key).cloned();
            let is_focused = focused == Some(view_id);

            if let Some(ref state_rc) = state {
                let mut st = state_rc.borrow_mut();
                st.set_inner_width(content_rect.w);
                st.set_inner_height(content_rect.h);
                st.tick_scroll_animation();
                if let Some(ref vt) = ti.visual_transformation.as_ref() {
                    let empty = repose_core::AnnotatedString::new(String::new(), vec![]);
                    let tfmd = vt.filter(&empty);
                    st.offset_map = Some(tfmd.offset_mapping.clone_box());
                    st.visual_transformation = Some((*vt).clone());
                } else {
                    st.offset_map = None;
                    st.visual_transformation = None;
                }
                drop(st);
            }

            crate::textfield::paint_text_field(
                scene,
                content_rect,
                ti,
                state.as_ref(),
                is_focused,
                modifier.clip_rounded,
                alpha_accum,
            );
        }
        if let Some(p) = &modifier.painter {
            (p)(scene, rect, alpha_accum);
        }

        // Draw indication (ripple, etc.) on top of content but behind hit regions
        if let Some(factory) = modifier.indication.clone().or_else(|| local_indication()) {
            if let Some(ref interaction_source) = modifier.interaction_source {
                let draw_node = factory.create(interaction_source);
                draw_node.draw(scene, rect, alpha_accum);
            }
        }

        let has_pointer = modifier.on_pointer_down.is_some()
            || modifier.on_pointer_move.is_some()
            || modifier.on_pointer_up.is_some()
            || modifier.on_pointer_enter.is_some()
            || modifier.on_pointer_leave.is_some()
            || modifier.on_double_click.is_some()
            || modifier.on_long_click.is_some();

        let has_dnd = modifier.on_drag_start.is_some()
            || modifier.on_drag_end.is_some()
            || modifier.on_drag_enter.is_some()
            || modifier.on_drag_over.is_some()
            || modifier.on_drag_leave.is_some()
            || modifier.on_drop.is_some();

        let kind_handles_hit = modifier.text_input.is_some()
            || modifier.scroll.is_some()
            || matches!(kind, ViewKind::Expander { .. } | ViewKind::TreeRow { .. });

        let needs_hit = !modifier.disabled
            && (has_pointer
                || modifier.click
                || has_dnd
                || modifier.on_action.is_some()
                || modifier.focusable == Some(true)
                || (modifier.input_blocker && !modifier.hit_passthrough));

        if needs_hit && !kind_handles_hit && !modifier.hit_passthrough {
            let focusable = modifier.focusable.unwrap_or(true);
            let mut hit = HitRegion {
                id: view_id,
                rect,
                z_index: modifier.z_index,
                focusable,
                focus_group_id: if modifier.focus_group {
                    Some(view_id)
                } else {
                    self.focus_group_stack.last().copied()
                },
                ..HitRegion::from_modifier(view_id, rect, &modifier)
            };

            // Auto-wire InteractionSource to pointer/hover callbacks.
            // The source's state is OR'd with the implicit view-ID state in state resolution above.
            if let Some(ref src) = modifier.interaction_source {
                let msrc = src.to_mutable();
                let last_press_id: Rc<Cell<Option<PressId>>> = Rc::new(Cell::new(None));

                // Wrap on_pointer_down to emit Press with position + unique ID
                let orig_down = hit.on_pointer_down.take();
                let s_down = msrc.clone();
                let lpid_down = last_press_id.clone();
                hit.on_pointer_down = Some(Rc::new(move |ev| {
                    let press = Interaction::new_press(ev.position);
                    if let Interaction::Press(id, _) = press {
                        lpid_down.set(Some(id));
                    }
                    s_down.emit(press);
                    if let Some(ref f) = orig_down {
                        f(ev);
                    }
                }));

                // Wrap on_pointer_up to emit Release with the last press ID.
                // Always emit Release even when the Cell is empty (composition can
                // change between press and release, creating a fresh Cell per frame).
                let orig_up = hit.on_pointer_up.take();
                let s_up = msrc.clone();
                let lpid_up = last_press_id.clone();
                hit.on_pointer_up = Some(Rc::new(move |ev| {
                    let pid = lpid_up.take().unwrap_or(0);
                    s_up.emit(Interaction::Release(pid));
                    if let Some(ref f) = orig_up {
                        f(ev);
                    }
                }));

                // Wrap on_pointer_cancel to emit Cancel with the last press ID.
                let orig_cancel = hit.on_pointer_cancel.take();
                let s_cancel = msrc.clone();
                hit.on_pointer_cancel = Some(Rc::new(move |ev| {
                    let pid = last_press_id.take().unwrap_or(0);
                    s_cancel.emit(Interaction::Cancel(pid));
                    if let Some(ref f) = orig_cancel {
                        f(ev);
                    }
                }));

                let orig_enter = hit.on_pointer_enter.take();
                let s_enter = msrc.clone();
                hit.on_pointer_enter = Some(Rc::new(move |ev| {
                    s_enter.emit(Interaction::HoverEnter);
                    if let Some(ref f) = orig_enter {
                        f(ev);
                    }
                }));

                let orig_leave = hit.on_pointer_leave.take();
                hit.on_pointer_leave = Some(Rc::new(move |ev| {
                    msrc.emit(Interaction::HoverLeave);
                    if let Some(ref f) = orig_leave {
                        f(ev);
                    }
                }));
            }

            hits.push(hit);
        }

        // Focus ring for interactive views
        if is_focused
            && (has_pointer
                || modifier.click
                || modifier.on_action.is_some()
                || modifier.focusable == Some(true))
        {
            push_focus_ring(scene, rect, focus_radius(&modifier));
        }

        let child_interaction_source =
            if needs_hit && !kind_handles_hit && !modifier.hit_passthrough {
                Some(view_id)
            } else {
                interaction_source
            };

        let mut next_sem_parent = sem_parent;

        // Push focus group scope for child traversal
        if modifier.focus_group {
            self.focus_group_stack.push(view_id);
        }

        match &kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                overflow,
                font_family,
                annotations,
                text_align,
                font_weight,
                font_style,
                text_decoration,
                letter_spacing,
                line_height,
                url,
                font_variation_settings,
                ..
            } => {
                let tl = self.text_cache.get(&node_id);
                let (size_px, line_h_px, lines, line_ranges) = if let Some(tl) = tl {
                    (
                        tl.size_px,
                        tl.line_h_px,
                        tl.lines.clone(),
                        Some(tl.line_ranges.clone()),
                    )
                } else {
                    let px = font_px(*font_size);
                    let lh = if *line_height > 0.0 {
                        font_px(*line_height)
                    } else {
                        px
                    };
                    (px, lh, vec![text.clone()], None)
                };
                let total_h = lines.len() as f32 * line_h_px;
                let need_v_clip =
                    total_h > content_rect.h + 0.5 && *overflow != TextOverflow::Visible;

                let need_clip =
                    *overflow != TextOverflow::Visible && (need_v_clip || content_rect.w > 0.0);
                if need_clip {
                    scene.nodes.push(SceneNode::PushClip {
                        rect: content_rect,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });
                }

                let has_annotations = annotations.as_ref().map(|a| !a.is_empty()).unwrap_or(false);

                if has_annotations {
                    // Emit one SceneNode::Text per styled segment per line
                    let annos = annotations.as_ref().unwrap();
                    for (i, ln) in lines.iter().enumerate() {
                        let line_start = line_ranges
                            .as_ref()
                            .and_then(|r| r.get(i).map(|&(s, _)| s))
                            .unwrap_or(0);
                        let line_end = line_ranges
                            .as_ref()
                            .and_then(|r| r.get(i).map(|&(_, e)| e))
                            .unwrap_or(ln.len());

                        struct SegInfo {
                            start: usize,
                            end: usize,
                            color: Color,
                            font_dp: f32,
                            decoration: TextDecoration,
                            url: Option<Arc<str>>,
                            font_weight: u16,
                            font_family: Option<&'static str>,
                            font_style: u8,
                            letter_spacing: f32,
                            line_height: f32,
                            background: Option<Color>,
                            alpha: f32,
                            text_direction: TextDirection,
                            font_synthesis: FontSynthesis,
                            baseline_shift: BaselineShift,
                            hyphens: Hyphens,
                            line_break: LineBreak,
                            text_indent: Option<TextIndent>,
                            draw_style: DrawStyle,
                            w: f32,
                            px: f32,
                            font_variation_settings: Option<Arc<str>>,
                        }
                        fn style_to_fs(style: &FontStyle) -> u8 {
                            if matches!(style, FontStyle::Italic) { 1 } else { 0 }
                        }
                        // Build segments from spans
                        let mut segments: Vec<SegInfo> = Vec::new();
                        let mut cursor = line_start;

                        let mut relevant: Vec<&TextSpan> = annos
                            .iter()
                            .filter(|s| s.start < line_end && s.end > line_start)
                            .collect();
                        relevant.sort_by_key(|s| s.start);

                        for span in &relevant {
                            let seg_start = span.start.max(line_start);
                            let seg_end = span.end.min(line_end);

                            if seg_start > cursor {
                                // Default-styled segment before this span
                                segments.push(SegInfo {
                                    start: cursor,
                                    end: seg_start,
                                    color: *color,
                                    font_dp: *font_size,
                                    decoration: *text_decoration,
                                    url: None,
                                    font_weight: font_weight.0,
                                    font_family: *font_family,
                                    font_style: style_to_fs(font_style),
                                    letter_spacing: *letter_spacing,
                                    line_height: *line_height,
                                    background: None,
                                    alpha: 0.0,
                                    text_direction: text_direction(),
                                    font_synthesis: FontSynthesis::Unspecified,
                                    baseline_shift: BaselineShift::Unspecified,
                                    hyphens: Hyphens::Unspecified,
                                    line_break: LineBreak::Unspecified,
                                    text_indent: None,
                                    draw_style: DrawStyle::Fill,
                                    w: 0.0,
                                    px: 0.0,
                                    font_variation_settings: None,
                                });
                            }

                            let span_color = span.style.color.unwrap_or(*color);
                            let span_size = span.style.font_size.unwrap_or(*font_size);
                            let span_decoration = span.style.text_decoration.unwrap_or(*text_decoration);
                            let span_weight = span.style.font_weight.unwrap_or(font_weight.0);
                            let span_family = span.style.font_family.or(*font_family);
                            let span_style = span.style.font_style.unwrap_or(style_to_fs(font_style));
                            let span_ls = span.style.letter_spacing.unwrap_or(*letter_spacing);
                            let span_lh = span.style.line_height.unwrap_or(*line_height);
                            let span_bg = span.style.background;
                            let span_alpha = span.style.alpha;
                            let span_td = span.style.text_direction.unwrap_or(text_direction());
                            let span_fs = span.style.font_synthesis.unwrap_or(FontSynthesis::Unspecified);
                            let span_bs = span.style.baseline_shift.unwrap_or(BaselineShift::Unspecified);
                            let span_h = span.style.hyphens.unwrap_or(Hyphens::Unspecified);
                            let span_lb = span.style.line_break.unwrap_or(LineBreak::Unspecified);
                            let span_ti = span.style.text_indent;
                            let span_ds = span.style.draw_style.clone().unwrap_or(DrawStyle::Fill);
                            let span_url = span.url.clone();
                            let span_fvs = span.style.font_variation_settings.clone().map(Arc::from);
                            segments.push(SegInfo {
                                start: seg_start,
                                end: seg_end,
                                color: span_color,
                                font_dp: span_size,
                                decoration: span_decoration,
                                url: span_url,
                                font_weight: span_weight,
                                font_family: span_family,
                                font_style: span_style,
                                letter_spacing: span_ls,
                                line_height: span_lh,
                                background: span_bg,
                                alpha: span_alpha,
                                text_direction: span_td,
                                font_synthesis: span_fs,
                                baseline_shift: span_bs,
                                hyphens: span_h,
                                line_break: span_lb,
                                text_indent: span_ti,
                                draw_style: span_ds,
                                w: 0.0,
                                px: 0.0,
                                font_variation_settings: span_fvs,
                            });
                            cursor = seg_end;
                        }

                        if cursor < line_end {
                            segments.push(SegInfo {
                                start: cursor,
                                end: line_end,
                                color: *color,
                                font_dp: *font_size,
                                decoration: *text_decoration,
                                url: None,
                                font_weight: font_weight.0,
                                font_family: *font_family,
                                font_style: style_to_fs(font_style),
                                letter_spacing: *letter_spacing,
                                line_height: *line_height,
                                background: None,
                                alpha: 0.0,
                                text_direction: text_direction(),
                                font_synthesis: FontSynthesis::Unspecified,
                                baseline_shift: BaselineShift::Unspecified,
                                hyphens: Hyphens::Unspecified,
                                line_break: LineBreak::Unspecified,
                                text_indent: None,
                                draw_style: DrawStyle::Fill,
                                w: 0.0,
                                px: 0.0,
                                font_variation_settings: None,
                            });
                        }

                        // Measure and emit each segment
                        let seg_font_px =
                            |dp: f32| dp_to_px(dp) * repose_core::locals::text_scale().0;
                        let mut total_w = 0.0f32;
                        let mut seg_measurements: Vec<&mut SegInfo> = segments.iter_mut().collect();
                        for info in &mut seg_measurements {
                            let seg_text = &text[info.start..info.end];
                            if seg_text.is_empty() {
                                continue;
                            }
                            info.px = seg_font_px(info.font_dp);
                            info.w =
                                measure_text(seg_text, info.px, TextMeasureConfig { font_family: info.font_family, font_weight: info.font_weight, font_style: info.font_style, letter_spacing: info.letter_spacing, ..Default::default() })
                                    .positions
                                    .last()
                                    .copied()
                                    .unwrap_or(0.0);
                            total_w += info.w;
                        }
                        let align_x_offset: f32 = match text_align {
                            TextAlign::End | TextAlign::Right => {
                                (content_rect.w - total_w).max(0.0)
                            }
                            TextAlign::Center => {
                                (content_rect.w - total_w).max(0.0) * 0.5
                            }
                            _ => 0.0,
                        };
                        let mut seg_x = content_rect.x + align_x_offset;
                        for info in &segments {
                            let seg_text = &text[info.start..info.end];
                            if seg_text.is_empty() {
                                continue;
                            }
                            let seg_rect = repose_core::Rect {
                                x: seg_x,
                                y: content_rect.y + i as f32 * line_h_px,
                                w: info.w,
                                h: line_h_px,
                            };
                            let seg_color = if info.alpha > 0.0 {
                                mul_alpha_color(info.color, info.alpha)
                            } else {
                                info.color
                            };
                            if let Some(bg) = &info.background {
                                let bg_rect = repose_core::Rect {
                                    x: seg_x,
                                    y: seg_rect.y,
                                    w: info.w,
                                    h: line_h_px,
                                };
                                scene.nodes.push(SceneNode::Rect {
                                    rect: bg_rect,
                                    brush: Brush::Solid(mul_alpha_color(*bg, alpha_accum)),
                                    radius: [0.0; 4],
                                });
                            }
                            scene.nodes.push(SceneNode::Text {
                                rect: seg_rect,
                                text: Arc::<str>::from(seg_text.to_string().into_boxed_str()),
                                color: mul_alpha_color(seg_color, alpha_accum),
                                size: info.px,
                                font_family: info.font_family,
                                text_align: *text_align,
                                font_weight: FontWeight(info.font_weight),
                                font_style: if info.font_style == 1 { FontStyle::Italic } else { FontStyle::Normal },
                                text_decoration: info.decoration,
                                letter_spacing: info.letter_spacing,
                                line_height: info.line_height,
                                extra_style: TextExtraStyle {
                                    text_direction: info.text_direction,
                                    font_synthesis: info.font_synthesis,
                                    baseline_shift: info.baseline_shift,
                                    draw_style: info.draw_style.clone(),
                                },
                                url: info.url.clone(),
                                font_variation_settings: info.font_variation_settings.clone(),
                            });
                            // Create hit region for clickable links
                            if let Some(url) = &info.url {
                                let link_id =
                                    view_id ^ ((info.start as u64) << 32) | (info.end as u64);
                                let link_url = url.clone();
                                hits.push(HitRegion {
                                    id: link_id,
                                    rect: seg_rect,
                                    cursor: Some(CursorIcon::Pointer),
                                    on_click: Some(Rc::new(move || {
                                        open_url(&link_url);
                                    })),
                                    ..Default::default()
                                });
                            }
                            seg_x += info.w;
                        }
                    }
                } else {
                    let fw_val = font_weight.0;
                    let fs_val = if matches!(font_style, FontStyle::Italic) {
                        1
                    } else {
                        0
                    };
                    for (i, ln) in lines.iter().enumerate() {
                        let line_w = measure_text(ln, size_px, TextMeasureConfig { font_family: *font_family, font_weight: fw_val, font_style: fs_val, letter_spacing: *letter_spacing, ..Default::default() })
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0);
                        let align_x = |line_w: f32| -> f32 {
                            match text_align {
                                TextAlign::End | TextAlign::Right => {
                                    content_rect.x + (content_rect.w - line_w).max(0.0)
                                }
                                TextAlign::Center => {
                                    content_rect.x + (content_rect.w - line_w).max(0.0) * 0.5
                                }
                                _ => content_rect.x,
                            }
                        };
                        let seg_rect = repose_core::Rect {
                            x: align_x(line_w),
                            y: content_rect.y + i as f32 * line_h_px,
                            w: content_rect.w,
                            h: line_h_px,
                        };
                        scene.nodes.push(SceneNode::Text {
                            rect: seg_rect,
                            text: Arc::<str>::from(ln.clone()),
                            color: mul_alpha_color(*color, alpha_accum),
                            size: size_px,
                            font_family: *font_family,
                            text_align: *text_align,
                            font_weight: *font_weight,
                            font_style: *font_style,
                            text_decoration: *text_decoration,
                            letter_spacing: *letter_spacing,
                            line_height: *line_height,
                            extra_style: Default::default(),
                            url: url.clone(),
                            font_variation_settings: font_variation_settings.clone(),
                        });
                        // Create hit region for view-level URL
                        if let Some(link_url) = url {
                            let link_id = view_id ^ 0x8000_0000_0000_0000;
                            let lu = link_url.clone();
                            hits.push(HitRegion {
                                id: link_id,
                                rect: seg_rect,
                                cursor: Some(CursorIcon::Pointer),
                                on_click: Some(Rc::new(move || {
                                    open_url(&lu);
                                })),
                                ..Default::default()
                            });
                        }
                    }
                }

                if need_clip {
                    scene.nodes.push(SceneNode::PopClip);
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Text,
                    label: Some(text.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Image { handle, tint, fit } => {
                scene.nodes.push(SceneNode::Image {
                    rect,
                    handle: *handle,
                    tint: mul_alpha_color(*tint, alpha_accum),
                    fit: *fit,
                });
            }
            ViewKind::Box if modifier.text_input.is_some() => {
                let ti = modifier.text_input.as_ref().unwrap();
                let multiline = ti.multiline;
                let hint = &ti.hint;
                let on_change = &ti.on_change;
                let on_submit = &ti.on_submit;
                let focus_tracker = &ti.focus_tracker;
                let value = &ti.value;
                let tf_key = view_id;

                if let Some(cell) = focus_tracker.as_ref() {
                    cell.set(is_focused);
                }

                // Sync the controlled value into the TextFieldState
                if let Some(state_rc) = textfield_states.get(&tf_key) {
                    crate::textfield::set_textfield_state(tf_key, state_rc.clone());
                    let mut st = state_rc.borrow_mut();
                    if st.text != *value {
                        st.text = value.clone();
                        st.composition = None;
                        st.drag_anchor = None;
                        let len = st.text.len();
                        let ns = st.selection.start.min(len);
                        let ne = st.selection.end.min(len);
                        if ns != st.selection.start || ne != st.selection.end {
                            st.selection = ns..ne;
                        }
                    }
                }

                // Scroll wheel support for multiline text areas
                let on_scroll = if multiline {
                    let key = tf_key;
                    let h = rect.h;
                    let font_val = font_px(TF_FONT_DP);
                    let wrap_w = rect.w.max(1.0);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_height(h);
                        let layout =
                            crate::textfield::layout_text_area(&st.text, font_val, wrap_w, 400, 0, 0.0, None);
                        let content_h = layout.ranges.len().max(1) as f32 * layout.line_h_px;
                        let max_y = (content_h - st.inner_height).max(0.0);

                        let before = st.scroll_target_y;
                        let target = (st.scroll_target_y + d.y).clamp(0.0, max_y);
                        st.scroll_target_y = target;

                        let consumed = target - before;
                        Vec2 {
                            x: d.x,
                            y: d.y - consumed,
                        }
                    }) as Rc<dyn Fn(Vec2) -> Vec2>)
                } else {
                    // Single-line horizontal scroll (mouse wheel or trackpad)
                    let key = tf_key;
                    let inner_w = rect.w.max(1.0);
                    let font_val = font_px(TF_FONT_DP);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_width(inner_w);
                        let m = crate::textfield::measure_text(&st.text, font_val, TextMeasureConfig::default());
                        let content_w = m.positions.last().copied().unwrap_or(0.0);
                        let max_x = (content_w - st.inner_width).max(0.0);

                        let before = st.scroll_target;
                        let target = (st.scroll_target - d.y).clamp(0.0, max_x);
                        st.scroll_target = target;

                        let consumed = before - target;
                        Vec2 {
                            x: d.x,
                            y: d.y - consumed,
                        }
                    }) as Rc<dyn Fn(Vec2) -> Vec2>)
                };

                if !modifier.hit_passthrough {
                    let user_on_action = modifier.on_action.clone();
                    let change_cb = on_change.clone();
                    let is_multiline = multiline;
                    let tf_on_action: Option<Rc<dyn Fn(repose_core::shortcuts::Action) -> bool>> =
                        Some(Rc::new(move |action| {
                            use repose_core::shortcuts::Action;
                            let Some(st) = crate::textfield::get_textfield_state(tf_key) else {
                                return false;
                            };
                            let mut s = st.borrow_mut();
                            let mut handled = false;
                            match action {
                                Action::Copy => {
                                    let txt = s.selected_text();
                                    if !txt.is_empty() {
                                        repose_core::clipboard::copy_to_clipboard(&txt);
                                        handled = true;
                                    }
                                }
                                Action::Cut => {
                                    let txt = s.selected_text();
                                    if !txt.is_empty() {
                                        repose_core::clipboard::copy_to_clipboard(&txt);
                                        s.insert_text_atomic("");
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                Action::Paste => {
                                    let Some(mut txt) = repose_core::clipboard::paste_text() else {
                                        return false;
                                    };
                                    if is_multiline {
                                        txt.retain(|c| c == '\n' || (!c.is_control() && c != '\r'));
                                    } else {
                                        txt.retain(|c| !c.is_control() && c != '\n' && c != '\r');
                                    }
                                    if txt.is_empty() {
                                        return false;
                                    }
                                    s.insert_text_atomic(&txt);
                                    crate::textfield::ensure_caret_visible(&mut s, is_multiline);
                                    let text = s.text.clone();
                                    drop(s);
                                    if let Some(cb) = &change_cb {
                                        cb(text);
                                    }
                                    handled = true;
                                }
                                Action::SelectAll => {
                                    s.selection = 0..s.text.len();
                                    crate::textfield::ensure_caret_visible(&mut s, is_multiline);
                                    if !s.text.is_empty() {
                                        repose_core::clipboard::set_primary_selection(&s.text);
                                    }
                                    handled = true;
                                }
                                Action::Undo => {
                                    if s.can_undo() {
                                        s.undo();
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                Action::Redo => {
                                    if s.can_redo() {
                                        s.redo();
                                        crate::textfield::ensure_caret_visible(
                                            &mut s,
                                            is_multiline,
                                        );
                                        let text = s.text.clone();
                                        drop(s);
                                        if let Some(cb) = &change_cb {
                                            cb(text);
                                        }
                                        handled = true;
                                    }
                                }
                                _ => {}
                            }
                            handled
                        }));

                    let combined: Option<Rc<dyn Fn(repose_core::shortcuts::Action) -> bool>> =
                        match (user_on_action, tf_on_action) {
                            (Some(u), Some(t)) => Some(Rc::new(move |a| u(a.clone()) || t(a))),
                            (Some(u), None) => Some(u),
                            (None, Some(t)) => Some(t),
                            (None, None) => None,
                        };

                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll,
                        focusable: true,
                        z_index: modifier.z_index,
                        on_text_change: on_change.clone(),
                        on_text_submit: on_submit.clone(),
                        tf_state_key: Some(tf_key),
                        tf_multiline: multiline,
                        tf_content_origin: Some((content_rect.x, content_rect.y)),
                        on_action: combined,
                        cursor: Some(crate::CursorIcon::Text),
                        focus_group_id: if modifier.focus_group {
                            Some(view_id)
                        } else {
                            self.focus_group_stack.last().copied()
                        },
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }

                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::TextField,
                    label: Some(hint.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Expander { on_toggle, .. } => {
                if let Some(cb) = on_toggle.clone() {
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_click: Some(cb),
                        focusable: true,
                        z_index: modifier.z_index,
                        focus_group_id: if modifier.focus_group {
                            Some(view_id)
                        } else {
                            self.focus_group_stack.last().copied()
                        },
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Button,
                    label: infer_label(&self.tree, node_id),
                    rect,
                    focused: is_focused,
                    enabled: !modifier.disabled,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::TreeRow {
                depth,
                has_children,
                is_expanded,
                is_selected,
                on_toggle,
                on_select,
            } => {
                let indent_px = dp_to_px(*depth as f32 * 16.0);
                let chevron_w = if *has_children { dp_to_px(16.0) } else { 0.0 };

                // Selection highlight
                if *is_selected {
                    let th = locals::theme();
                    scene.nodes.push(SceneNode::Rect {
                        rect,
                        brush: Brush::Solid(th.primary.with_alpha_f32(0.15)),
                        radius: [0.0; 4],
                    });
                }

                // Expand/collapse chevron
                if *has_children {
                    let chevron_text = if *is_expanded { "▼" } else { "▶" };
                    let chevron_px = dp_to_px(12.0);
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: rect.x + indent_px,
                            y: rect.y + (rect.h - chevron_px) * 0.5,
                            w: chevron_w,
                            h: chevron_px,
                        },
                        text: Arc::from(chevron_text),
                        color: mul_alpha_color(locals::theme().on_surface, alpha_accum),
                        size: chevron_px,
                        font_family: None,
                        text_align: TextAlign::Start,
                        font_weight: FontWeight::NORMAL,
                        font_style: FontStyle::Normal,
                        text_decoration: TextDecoration::default(),
                        letter_spacing: 0.0,
                        line_height: 0.0,
                        extra_style: Default::default(),
                        url: None,
                        font_variation_settings: None,
                    });

                    // Toggle hit region (chevron area)
                    if let Some(cb) = on_toggle.clone() {
                        hits.push(HitRegion {
                            id: view_id.wrapping_mul(2).wrapping_add(1),
                            rect: repose_core::Rect {
                                x: rect.x + indent_px,
                                y: rect.y,
                                w: chevron_w,
                                h: rect.h,
                            },
                            on_click: Some(cb),
                            focusable: false,
                            z_index: modifier.z_index,
                            ..HitRegion::from_modifier(view_id, rect, &modifier)
                        });
                    }
                }

                // Select hit region (label area)
                if let Some(cb) = on_select.clone() {
                    hits.push(HitRegion {
                        id: view_id,
                        rect: repose_core::Rect {
                            x: rect.x + indent_px + chevron_w,
                            y: rect.y,
                            w: (rect.w - indent_px - chevron_w).max(0.0),
                            h: rect.h,
                        },
                        on_click: Some(cb),
                        focusable: true,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }

                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Button,
                    label: infer_label(&self.tree, node_id),
                    rect,
                    focused: is_focused,
                    enabled: !modifier.disabled,
                    selectable_group: false,
                });
                next_sem_parent = Some(view_id);
            }
            _ => {
                if let Some(s) = &modifier.semantics {
                    sems.push(SemNode {
                        id: view_id,
                        parent: sem_parent,
                        role: s.role,
                        label: s.label.clone(),
                        rect,
                        focused: is_focused,
                        enabled: !modifier.disabled,
                        selectable_group: s.selectable_group,
                    });
                    next_sem_parent = Some(view_id);
                }
            }
        }

        // Fire on_globally_positioned / on_size_changed if position/size changed
        let view_id_for_pos = view_id;
        let dp_rect = repose_core::Rect {
            x: px_to_dp(rect.x),
            y: px_to_dp(rect.y),
            w: px_to_dp(rect.w),
            h: px_to_dp(rect.h),
        };
        if let Some(cb) = &modifier.on_globally_positioned {
            let prev = self.prev_observed_rects.get(&view_id_for_pos).copied();
            if prev != Some(dp_rect) {
                cb(dp_rect);
            }
        }
        if let Some(cb) = &modifier.on_size_changed {
            let prev = self.prev_observed_rects.get(&view_id_for_pos).copied();
            if prev.map(|r| (r.w, r.h)) != Some((dp_rect.w, dp_rect.h)) {
                cb(Vec2 {
                    x: dp_rect.w,
                    y: dp_rect.h,
                });
            }
        }
        if modifier.on_globally_positioned.is_some() || modifier.on_size_changed.is_some() {
            self.prev_observed_rects.insert(view_id_for_pos, dp_rect);
        }

        // Children
        let child_offset_px = base_px;
        let has_blur = modifier
            .blur
            .map_or(false, |b| b.radius_x > 0.0 || b.radius_y > 0.0);
        let layer_id = if modifier.graphics_layer.is_some() || has_blur {
            let id = self.layer_id_counter;
            self.layer_id_counter = self.layer_id_counter.wrapping_add(1);
            let blur_style = modifier.blur.unwrap_or(BlurStyle {
                radius_x: 0.0,
                radius_y: 0.0,
                edge_treatment: BlurredEdgeTreatment::Rectangle,
            });
            let blur_radius_x = dp_to_px(blur_style.radius_x);
            let blur_radius_y = dp_to_px(blur_style.radius_y);
            let alpha = modifier.graphics_layer.unwrap_or(1.0);
            scene.nodes.push(SceneNode::BeginLayer {
                rect,
                layer_id: id,
                alpha,
                blur_radius_x,
                blur_radius_y,
                rectangle_edge: matches!(
                    blur_style.edge_treatment,
                    BlurredEdgeTreatment::Rectangle
                ),
            });
            scene.nodes.push(SceneNode::PushTransform {
                transform: Transform {
                    translate_x: -rect.x,
                    translate_y: -rect.y,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotate: 0.0,
                    origin_x: 0.5,
                    origin_y: 0.5,
                },
            });
            Some(id)
        } else {
            None
        };
        // Handle modifier-based scroll containers.
        // This runs before the match on kind, so modifier scroll takes priority.
        if let Some(scroll) = &modifier.scroll {
            match scroll {
                ScrollBinding::Vertical(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_main {
                        s(vp.h.max(0.0));
                    }
                    let mut ch = 0.0f32;
                    for &c in &children {
                        let l = self.layout_for_node(c);
                        ch = ch.max(l.location.y + l.size.height);
                    }
                    if let Some(s) = &b.set_content_main {
                        s(ch);
                    }
                    let off = b.get_offset_main.as_ref().map(|f| f()).unwrap_or(0.0);

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });

                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0, child_offset_px.1 - off);

                    for &child_id in &children {
                        let l = self.layout_for_node(child_id);
                        let child_rect = repose_core::Rect {
                            x: scrolled_offset.0 + l.location.x,
                            y: scrolled_offset.1 + l.location.y,
                            w: l.size.width,
                            h: l.size.height,
                        };
                        if intersect_rect(child_rect, vp).is_none() {
                            self.stats.paint_culled += 1;
                            continue;
                        }
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }

                    clip_hits_to_viewport(hits, hits_start, vp);

                    if b.show_scrollbar {
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            ch,
                            off,
                            modifier.z_index,
                            ScrollbarAxis::V,
                            b.set_offset_main.clone(),
                        );
                    }

                    scene.nodes.push(SceneNode::PopClip);
                }
                ScrollBinding::Horizontal(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_main {
                        s(vp.w.max(0.0));
                    }
                    let mut cw = 0.0f32;
                    for &c in &children {
                        let l = self.layout_for_node(c);
                        cw = cw.max(l.location.x + l.size.width);
                    }
                    if let Some(s) = &b.set_content_main {
                        s(cw);
                    }
                    let off = b.get_offset_main.as_ref().map(|f| f()).unwrap_or(0.0);

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });

                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0 - off, child_offset_px.1);

                    for &child_id in &children {
                        let l = self.layout_for_node(child_id);
                        let child_rect = repose_core::Rect {
                            x: scrolled_offset.0 + l.location.x,
                            y: scrolled_offset.1 + l.location.y,
                            w: l.size.width,
                            h: l.size.height,
                        };
                        if intersect_rect(child_rect, vp).is_none() {
                            self.stats.paint_culled += 1;
                            continue;
                        }
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }

                    clip_hits_to_viewport(hits, hits_start, vp);

                    if b.show_scrollbar {
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            cw,
                            off,
                            modifier.z_index,
                            ScrollbarAxis::H,
                            b.set_offset_main.clone(),
                        );
                    }

                    scene.nodes.push(SceneNode::PopClip);
                }
                ScrollBinding::Both(b) => {
                    if let Some(set_parent) = &b.set_nested_scroll_parent {
                        if let Some(conn) = self.find_ancestor_nested_scroll(node_id) {
                            set_parent(conn);
                        }
                    }
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: b.on_scroll.clone(),
                        focusable: false,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                    let vp = content_rect;
                    if let Some(s) = &b.set_viewport_width {
                        s(vp.w.max(0.0));
                    }
                    if let Some(s) = &b.set_viewport_height {
                        s(vp.h.max(0.0));
                    }
                    let mut cw = 0.0f32;
                    let mut ch = 0.0f32;
                    for &c in &children {
                        let mut stack: Vec<(NodeId, f32, f32)> = vec![(c, 0.0f32, 0.0f32)];
                        while let Some((cid, ox, oy)) = stack.pop() {
                            let l = self.layout_for_node(cid);
                            let ax = ox + l.location.x;
                            let ay = oy + l.location.y;
                            cw = cw.max(ax + l.size.width);
                            ch = ch.max(ay + l.size.height);
                            if let Some(node) = self.tree.get(cid) {
                                for &k in &node.children {
                                    stack.push((k, ax, ay));
                                }
                            }
                        }
                    }
                    if let Some(s) = &b.set_content_width {
                        s(cw);
                    }
                    if let Some(s) = &b.set_content_height {
                        s(ch);
                    }
                    let (ox, oy) = b.get_offset_xy.as_ref().map(|f| f()).unwrap_or((0.0, 0.0));

                    scene.nodes.push(SceneNode::PushClip {
                        rect: vp,
                        radius: [0.0; 4],
                        op: crate::ClipOp::Intersect,
                    });
                    let hits_start = hits.len();
                    let scrolled_offset = (child_offset_px.0 - ox, child_offset_px.1 - oy);
                    for &child_id in &children {
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            scrolled_offset,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }
                    let mut i = hits_start;
                    while i < hits.len() {
                        if let Some(r) = intersect_rect(hits[i].rect, vp) {
                            hits[i].rect = r;
                            i += 1;
                        } else {
                            hits.remove(i);
                        }
                    }
                    if b.show_scrollbar {
                        let set_y = b
                            .set_offset_xy
                            .clone()
                            .map(|s| Rc::new(move |y| s(ox, y)) as Rc<dyn Fn(f32)>);
                        let set_x = b
                            .set_offset_xy
                            .clone()
                            .map(|s| Rc::new(move |x| s(x, oy)) as Rc<dyn Fn(f32)>);
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            ch,
                            oy,
                            modifier.z_index,
                            ScrollbarAxis::V,
                            set_y,
                        );
                        push_scrollbar(
                            scene,
                            hits,
                            interactions,
                            view_id,
                            vp,
                            cw,
                            ox,
                            modifier.z_index,
                            ScrollbarAxis::H,
                            set_x,
                        );
                    }
                    scene.nodes.push(SceneNode::PopClip);
                }
            }
            // Pop focus group scope
            if modifier.focus_group {
                self.focus_group_stack.pop();
            }
            // Pop layer if present
            if let Some(id) = layer_id {
                scene.nodes.push(SceneNode::PopTransform);
                scene.nodes.push(SceneNode::EndLayer { layer_id: id });
            }
            // Pop clips and transforms pushed before the scroll branch
            if modifier.clip_rect.is_some() && overflow_clip {
                scene.nodes.push(SceneNode::PopClip);
            }
            if push_round_clip && overflow_clip {
                scene.nodes.push(SceneNode::PopClip);
            }
            if modifier.transform.is_some() {
                scene.nodes.push(SceneNode::PopTransform);
            }
            // Wire up FocusRequester if present on the modifier
            set_focus_requester(&modifier, view_id);
            if let Some(cb) = &modifier.on_focus_changed {
                self.focus_callbacks.insert(view_id, cb.clone());
            }
            return;
        }
        // Non-scroll children
        // For non-scroll containers, the ViewKind determines the children walk pattern.
        match &kind {
            ViewKind::OverlayHost => {
                for &child_id in &children {
                    self.walk_paint(
                        child_id,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
            }
            ViewKind::Expander { expanded, .. } => {
                if let Some(&first) = children.first() {
                    self.walk_paint(
                        first,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
                if *expanded {
                    for &child_id in children.iter().skip(1) {
                        self.walk_paint(
                            child_id,
                            scene,
                            hits,
                            sems,
                            textfield_states,
                            interactions,
                            focused,
                            child_offset_px,
                            alpha_accum,
                            next_sem_parent,
                            child_interaction_source,
                            font_px,
                            allow_cache,
                            deferred,
                            skip_defer,
                        );
                    }
                }
            }
            _ => {
                for &child_id in &children {
                    self.walk_paint(
                        child_id,
                        scene,
                        hits,
                        sems,
                        textfield_states,
                        interactions,
                        focused,
                        child_offset_px,
                        alpha_accum,
                        next_sem_parent,
                        child_interaction_source,
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
            }
        }

        // Pop focus group scope
        if modifier.focus_group {
            self.focus_group_stack.pop();
        }

        if let Some(id) = layer_id {
            scene.nodes.push(SceneNode::PopTransform);
            scene.nodes.push(SceneNode::EndLayer { layer_id: id });
            if let Some(shadow) = &modifier.shadow {
                scene.nodes.push(SceneNode::CompositeShadow {
                    layer_id: id,
                    blur_px: dp_to_px(shadow.blur_radius),
                    offset_px: (0.0, dp_to_px(shadow.offset_y)),
                    color: shadow.color,
                });
            }
        }

        if modifier.clip_rect.is_some() && overflow_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if push_round_clip && overflow_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if modifier.transform.is_some() {
            scene.nodes.push(SceneNode::PopTransform);
        }

        // Wire up FocusRequester if present on the modifier
        set_focus_requester(&modifier, view_id);

        if let Some(cb) = &modifier.on_focus_changed {
            self.focus_callbacks.insert(view_id, cb.clone());
        }
    }
}

// Helpers
fn infer_label(tree: &ViewTree, node_id: NodeId) -> Option<String> {
    let mut stack = vec![node_id];
    while let Some(id) = stack.pop() {
        let n = tree.get(id)?;
        if let ViewKind::Text { text, .. } = &n.kind
            && !text.is_empty()
        {
            return Some(text.clone());
        }
        for &ch in n.children.iter().rev() {
            stack.push(ch);
        }
    }
    None
}

fn intersect_rect(a: repose_core::Rect, b: repose_core::Rect) -> Option<repose_core::Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.w).min(b.x + b.w);
    let y1 = (a.y + a.h).min(b.y + b.h);
    let w = (x1 - x0).max(0.0);
    let h = (y1 - y0).max(0.0);
    if w <= 0.0 || h <= 0.0 {
        None
    } else {
        Some(repose_core::Rect { x: x0, y: y0, w, h })
    }
}

fn clip_hits_to_viewport(hits: &mut Vec<HitRegion>, start: usize, vp: repose_core::Rect) {
    let mut i = start;
    while i < hits.len() {
        if let Some(r) = intersect_rect(hits[i].rect, vp) {
            hits[i].rect = r;
            i += 1;
        } else {
            hits.remove(i);
        }
    }
}

pub(crate) fn mul_alpha_color(c: Color, a: f32) -> Color {
    Color(c.0, c.1, c.2, ((c.3 as f32) * a).clamp(0.0, 255.0) as u8)
}
fn mul_alpha_brush(b: Brush, a: f32) -> Brush {
    match b {
        Brush::Solid(c) => Brush::Solid(mul_alpha_color(c, a)),
        Brush::Linear {
            start,
            end,
            start_color,
            end_color,
        } => Brush::Linear {
            start,
            end,
            start_color: mul_alpha_color(start_color, a),
            end_color: mul_alpha_color(end_color, a),
        },
        _ => b,
    }
}

fn clamp_radius(r: f32, w: f32, h: f32) -> f32 {
    r.max(0.0).min(0.5 * w.max(0.0)).min(0.5 * h.max(0.0))
}
fn clamp_radii(r: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    [
        clamp_radius(r[0], w, h),
        clamp_radius(r[1], w, h),
        clamp_radius(r[2], w, h),
        clamp_radius(r[3], w, h),
    ]
}
fn max_radii(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

#[derive(Clone, Copy)]
enum ScrollbarAxis {
    V,
    H,
}

fn push_scrollbar(
    scene: &mut Scene,
    hits: &mut Vec<HitRegion>,
    interactions: &Interactions,
    vid: u64,
    vp: repose_core::Rect,
    content_len: f32,
    offset: f32,
    z: f32,
    axis: ScrollbarAxis,
    set_offset: Option<Rc<dyn Fn(f32)>>,
) {
    let vp_len = match axis {
        ScrollbarAxis::V => vp.h,
        ScrollbarAxis::H => vp.w,
    };
    if content_len <= vp_len + 0.5 {
        return;
    }

    let thick = dp_to_px(4.0);
    let main_inset = dp_to_px(2.0);

    let (track_x, track_y, track_main, track_cross) = match axis {
        ScrollbarAxis::V => (
            vp.x + vp.w - thick,
            vp.y + main_inset,
            (vp.h - 2.0 * main_inset).max(0.0),
            thick,
        ),
        ScrollbarAxis::H => (
            vp.x + main_inset,
            vp.y + vp.h - thick,
            (vp.w - 2.0 * main_inset).max(0.0),
            thick,
        ),
    };
    if track_main <= 0.5 {
        return;
    }

    let ratio = (vp_len / content_len).clamp(0.0, 1.0);
    let thumb_len = (track_main * ratio).max(dp_to_px(24.0)).min(track_main);
    let tpos = (offset / (content_len - vp_len).max(1.0)).clamp(0.0, 1.0);
    let thumb_offset = tpos * (track_main - thumb_len);

    let (track_rect, thumb_rect) = match axis {
        ScrollbarAxis::V => (
            repose_core::Rect {
                x: track_x,
                y: track_y,
                w: track_cross,
                h: track_main,
            },
            repose_core::Rect {
                x: track_x,
                y: track_y + thumb_offset,
                w: track_cross,
                h: thumb_len,
            },
        ),
        ScrollbarAxis::H => (
            repose_core::Rect {
                x: track_x,
                y: track_y,
                w: track_main,
                h: track_cross,
            },
            repose_core::Rect {
                x: track_x + thumb_offset,
                y: track_y,
                w: thumb_len,
                h: track_cross,
            },
        ),
    };

    scene.nodes.push(SceneNode::Rect {
        rect: track_rect,
        brush: Brush::Solid(locals::theme().scrollbar_track),
        radius: [thick * 0.5; 4],
    });
    scene.nodes.push(SceneNode::Rect {
        rect: thumb_rect,
        brush: Brush::Solid(locals::theme().scrollbar_thumb),
        radius: [thick * 0.5; 4],
    });

    if let Some(s) = set_offset {
        let tid = match axis {
            ScrollbarAxis::V => vid ^ 0x8000_0001,
            ScrollbarAxis::H => vid ^ 0x8000_0002,
        };
        let track_start = match axis {
            ScrollbarAxis::V => track_y,
            ScrollbarAxis::H => track_x,
        };
        let max_scroll = (content_len - vp_len).max(1.0);

        let map = Rc::new(move |pos: f32| -> f32 {
            let max_p = (track_main - thumb_len).max(0.0);
            let p = ((pos - track_start) - thumb_len * 0.5).clamp(0.0, max_p);
            (if max_p > 0.0 { p / max_p } else { 0.0 }) * max_scroll
        });

        let extract = match axis {
            ScrollbarAxis::V => (|pe: &PointerEvent| pe.position.y) as fn(&PointerEvent) -> f32,
            ScrollbarAxis::H => (|pe: &PointerEvent| pe.position.x) as fn(&PointerEvent) -> f32,
        };

        let on_pd = {
            let s = s.clone();
            let m = map.clone();
            Rc::new(move |pe: PointerEvent| s(m(extract(&pe))))
        };
        let on_pm = if interactions.pressed.contains(&tid) {
            let s = s.clone();
            let m = map.clone();
            Some(Rc::new(move |pe: PointerEvent| s(m(extract(&pe)))) as Rc<dyn Fn(PointerEvent)>)
        } else {
            None
        };
        hits.push(HitRegion {
            id: tid,
            rect: thumb_rect,
            z_index: z + 1000.0,
            on_pointer_down: Some(on_pd),
            on_pointer_move: on_pm,
            on_pointer_up: Some(Rc::new(|_| {})),
            ..Default::default()
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Box as RBox, Column, Text, ViewExt};

    fn font_px(dp: f32) -> f32 {
        dp // 1:1 for tests
    }

    #[test]
    fn test_render_z_index_paints_last() {
        // Create a Stack with two children:
        // 1. A red box (painted first, no render_z_index)
        // 2. A blue box with render_z_index(100.0) (should be painted last)

        let red = Color::from_rgb(255, 0, 0);
        let blue = Color::from_rgb(0, 0, 255);

        let red_box = RBox(Modifier::new().size(100.0, 100.0).background(red));
        let blue_box = RBox(
            Modifier::new()
                .size(100.0, 100.0)
                .background(blue)
                .render_z_index(100.0),
        );

        let root = Column(Modifier::new().size(200.0, 200.0)).child((red_box, blue_box));

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        // Find the rect nodes - there should be two (red and blue backgrounds)
        let rects: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                if let SceneNode::Rect { brush, .. } = n {
                    Some(brush.clone())
                } else {
                    None
                }
            })
            .collect();

        assert!(
            rects.len() >= 2,
            "Expected at least 2 rect nodes, got {}",
            rects.len()
        );

        // The blue box (with render_z_index) should be painted LAST
        // So its brush should be the last rect in the scene
        let last_rect_brush = rects.last().unwrap();
        assert!(
            matches!(last_rect_brush, Brush::Solid(c) if *c == blue),
            "Expected blue box to be painted last, but got {:?}",
            last_rect_brush
        );

        // And the red box should be painted before the blue
        let second_to_last = rects.get(rects.len() - 2);
        assert!(second_to_last.is_some(), "Expected at least 2 rect nodes");
        let second_brush = second_to_last.unwrap();
        assert!(
            matches!(second_brush, Brush::Solid(c) if *c == red),
            "Expected red box to be painted before blue, but got {:?}",
            second_brush
        );
    }

    #[test]
    fn test_render_z_index_order_by_value() {
        // Test that higher render_z_index values are painted later

        let red = Color::from_rgb(255, 0, 0);
        let green = Color::from_rgb(0, 255, 0);
        let blue = Color::from_rgb(0, 0, 255);

        let box1 = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(red)
                .render_z_index(10.0),
        );
        let box2 = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(green)
                .render_z_index(20.0),
        );
        let box3 = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(blue)
                .render_z_index(5.0),
        );

        let root = Column(Modifier::new().size(200.0, 200.0)).child((box1, box2, box3));

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        let rects: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                if let SceneNode::Rect { brush, .. } = n {
                    Some(brush.clone())
                } else {
                    None
                }
            })
            .collect();

        // Order should be: BLUE (z=5), RED (z=10), GREEN (z=20)
        assert!(rects.len() >= 3, "Expected at least 3 rects");

        let len = rects.len();
        assert!(
            matches!(&rects[len - 3], Brush::Solid(c) if *c == blue),
            "Expected BLUE (z=5) third from last"
        );
        assert!(
            matches!(&rects[len - 2], Brush::Solid(c) if *c == red),
            "Expected RED (z=10) second from last"
        );
        assert!(
            matches!(&rects[len - 1], Brush::Solid(c) if *c == green),
            "Expected GREEN (z=20) last"
        );
    }

    #[test]
    fn test_render_z_index_with_nested_children() {
        // This test mimics the showcase scenario:
        // Stack {
        //   Column { red_box, green_box }  // This is like the main content
        //   blue_box with render_z_index(1000)  // This is like the hint overlay
        // }
        // The blue_box should be painted AFTER all contents of Column

        let red = Color::from_rgb(255, 0, 0);
        let green = Color::from_rgb(0, 255, 0);
        let blue = Color::from_rgb(0, 0, 255);

        let red_box = RBox(Modifier::new().size(50.0, 50.0).background(red));
        let green_box = RBox(Modifier::new().size(50.0, 50.0).background(green));

        let content = Column(Modifier::new()).child((red_box, green_box));

        let overlay = RBox(
            Modifier::new()
                .size(30.0, 30.0)
                .background(blue)
                .render_z_index(1000.0),
        );

        let root = Column(Modifier::new().size(200.0, 200.0)).child((content, overlay));

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        let rects: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                if let SceneNode::Rect { brush, .. } = n {
                    Some(brush.clone())
                } else {
                    None
                }
            })
            .collect();

        // Order should be: RED, GREEN, then BLUE (because blue has render_z_index)
        assert!(
            rects.len() >= 3,
            "Expected at least 3 rects, got {}",
            rects.len()
        );

        let len = rects.len();
        // Blue should be LAST
        assert!(
            matches!(&rects[len - 1], Brush::Solid(c) if *c == blue),
            "Expected BLUE (z=1000) to be painted last, but got {:?}",
            &rects[len - 1]
        );

        // Red and Green should be before blue
        // Find their positions
        let blue_pos = rects
            .iter()
            .position(|b| matches!(b, Brush::Solid(c) if *c == blue))
            .unwrap();
        let red_pos = rects
            .iter()
            .position(|b| matches!(b, Brush::Solid(c) if *c == red))
            .unwrap();
        let green_pos = rects
            .iter()
            .position(|b| matches!(b, Brush::Solid(c) if *c == green))
            .unwrap();

        assert!(red_pos < blue_pos, "Red should be painted before blue");
        assert!(green_pos < blue_pos, "Green should be painted before blue");
    }

    #[test]
    fn test_render_z_index_paints_over_scrollbars() {
        // This test verifies that a node with render_z_index paints AFTER scrollbars
        // Structure:
        // Stack {
        //   Scroll(tall content)  // This will show a scrollbar
        //   Box with render_z_index(1000)  // This should paint LAST
        // }

        let content_color = Color::from_rgb(100, 100, 100);
        let overlay_color = Color::from_rgb(0, 0, 255);

        // Tall content inside scroll - 500px tall in 200px viewport
        let tall_content = RBox(Modifier::new().size(180.0, 500.0).background(content_color));

        let scroll = RBox(
            Modifier::new()
                .size(200.0, 200.0)
                .vertical_scroll(ScrollAxisBinding {
                    show_scrollbar: true,
                    ..Default::default()
                }),
        )
        .child(tall_content);

        let overlay = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(overlay_color)
                .render_z_index(1000.0),
        );

        let root = Column(Modifier::new().size(200.0, 200.0)).child((scroll, overlay));

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        // Collect all rect nodes (this includes content, scrollbar track, scrollbar thumb, and overlay)
        let rects: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                if let SceneNode::Rect { brush, .. } = n {
                    Some(brush.clone())
                } else {
                    None
                }
            })
            .collect();

        // The overlay (blue) should be painted LAST
        let overlay_pos = rects
            .iter()
            .position(|b| matches!(b, Brush::Solid(c) if *c == overlay_color));
        assert!(
            overlay_pos.is_some(),
            "Overlay should be present in scene, rects: {:?}",
            rects
        );
        let overlay_pos = overlay_pos.unwrap();

        // Check that overlay is painted last (after scrollbar)
        assert_eq!(
            overlay_pos,
            rects.len() - 1,
            "Overlay should be the last rect, but it's at position {} of {}. Rects: {:?}",
            overlay_pos,
            rects.len(),
            rects
        );
    }

    #[test]
    fn test_render_z_index_with_overlay_host() {
        // This test mimics the showcase app structure more closely:
        // Stack {
        //   OverlayHost { content with Scroll }
        //   Box with render_z_index(1000)  // Hint box
        // }
        use crate::overlay::OverlayHandle;

        let content_color = Color::from_rgb(100, 100, 100);
        let overlay_color = Color::from_rgb(0, 0, 255);

        // Tall content inside scroll - 500px tall in 200px viewport
        let tall_content = RBox(Modifier::new().size(180.0, 500.0).background(content_color));
        let scroll = RBox(
            Modifier::new()
                .size(200.0, 200.0)
                .vertical_scroll(ScrollAxisBinding {
                    show_scrollbar: true,
                    ..Default::default()
                }),
        )
        .child(tall_content);

        // Create an OverlayHost wrapping the scroll content
        let overlay_handle = OverlayHandle::new();
        let overlay_host = overlay_handle.host(Modifier::new().fill_max_size(), scroll);

        // The hint box with render_z_index should paint on top
        let hint_box = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(overlay_color)
                .render_z_index(1000.0),
        );

        // Final structure: Stack { OverlayHost, HintBox }
        let root = Column(Modifier::new().size(200.0, 200.0)).child((overlay_host, hint_box));

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        // Collect all rect nodes
        let rects: Vec<_> = scene
            .nodes
            .iter()
            .filter_map(|n| {
                if let SceneNode::Rect { brush, .. } = n {
                    Some(brush.clone())
                } else {
                    None
                }
            })
            .collect();

        // The hint box (blue) should be painted LAST
        let overlay_pos = rects
            .iter()
            .position(|b| matches!(b, Brush::Solid(c) if *c == overlay_color));
        assert!(
            overlay_pos.is_some(),
            "Hint box should be present in scene, rects: {:?}",
            rects
        );
        let overlay_pos = overlay_pos.unwrap();

        // Check that hint box is painted last (after scrollbar)
        assert_eq!(
            overlay_pos,
            rects.len() - 1,
            "Hint box should be the last rect, but it's at position {} of {}. Rects: {:?}",
            overlay_pos,
            rects.len(),
            rects
        );
    }

    #[test]
    fn test_subcompose_layout_runs_closure_and_lays_out() {
        use crate::subcompose::SubcomposeLayout;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let red = Color::from_rgb(255, 0, 0);
        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        let sub = SubcomposeLayout(Modifier::new().size(200.0, 100.0), move |scope| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            assert!(scope.max_width > 0.0, "scope.max_width should be positive");
            RBox(Modifier::new().size(100.0, 50.0).background(red))
        });

        let root = Column(Modifier::new()).child(sub);

        let mut engine = LayoutEngine::new();
        let (scene, _hits, _sems) = engine.layout_frame(
            &root,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        assert!(
            call_count.load(Ordering::SeqCst) >= 1,
            "SubcomposeLayout closure should have been invoked"
        );

        let has_red_rect = scene
            .nodes
            .iter()
            .any(|n| matches!(n, SceneNode::Rect { brush: Brush::Solid(c), .. } if *c == red));
        assert!(
            has_red_rect,
            "Subcomposed child (red box) should produce a Rect scene node"
        );
    }

    #[test]
    fn test_subcompose_layout_caches_closure_across_frames() {
        use crate::subcompose::SubcomposeLayout;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let call_count = Arc::new(AtomicUsize::new(0));
        let count_clone = call_count.clone();

        let sub = SubcomposeLayout(Modifier::new().size(200.0, 100.0), move |_scope| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            RBox(Modifier::new().size(50.0, 50.0))
        });

        let root = Column(Modifier::new()).child(sub);
        let mut engine = LayoutEngine::new();

        // First frame - closure runs.
        let _ = engine.layout_frame(
            &root,
            (400, 400),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        let after_first = call_count.load(Ordering::SeqCst);
        assert_eq!(after_first, 1, "closure should run once on first frame");

        // Subsequent frames with no changes - closure stays cached.
        for _ in 0..10 {
            let _ = engine.layout_frame(
                &root,
                (400, 400),
                &HashMap::new(),
                &Interactions::default(),
                None,
            );
        }
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "closure should NOT re-run when content and scope are stable"
        );
    }

    #[test]
    fn test_subcompose_layout_ancestor_modifier_narrows_scope_through_engine() {
        use crate::subcompose::SubcomposeLayout;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicU32, Ordering};

        let captured_max_w = Arc::new(AtomicU32::new(0));
        let cap = captured_max_w.clone();

        // Box(width=320) -> SubcomposeLayout(closure) - closure should see
        // max_width == 320.0, not the window's 800.0.
        let sub = SubcomposeLayout(Modifier::new(), move |scope| {
            cap.store(scope.max_width.to_bits(), Ordering::SeqCst);
            RBox(Modifier::new().size(100.0, 50.0))
        });
        let root =
            Column(Modifier::new()).child(crate::Box(Modifier::new().width(320.0)).child(sub));

        let mut engine = LayoutEngine::new();
        let _ = engine.layout_frame(
            &root,
            (800, 600),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );

        let observed = f32::from_bits(captured_max_w.load(Ordering::SeqCst));
        assert_eq!(observed, 320.0, "ancestor width should propagate to scope");
    }

    fn make_engine() -> LayoutEngine {
        LayoutEngine::new()
    }

    #[test]
    fn test_vertical_scroll_content_can_exceed_viewport() {
        use crate::scroll::{ScrollArea, ScrollState};
        use std::rc::Rc;

        let state = ScrollState::new();
        // Tall content inside fixed-height scroll
        let root = ScrollArea(
            Modifier::new().height(100.0).width(200.0),
            Rc::new(state),
            Column(Modifier::new())
                .child(RBox(Modifier::new().height(80.0).background(Color::WHITE)))
                .child(RBox(Modifier::new().height(80.0).background(Color::BLACK))),
        );
        let mut eng = LayoutEngine::new();
        let (_scene, hits, _) = eng.layout_frame(
            &root,
            (200, 100),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        // Content is taller than the viewport -> a scroll hit region must exist.
        assert!(
            hits.iter().any(|h| h.on_scroll.is_some()),
            "scroll container should emit a scroll hit region when content overflows"
        );
    }

    #[test]
    fn test_nested_scroll_nav_like() {
        use crate::scroll::{ScrollArea, ScrollState};
        use std::rc::Rc;

        // Outer column fill + inner ScrollArea with tall page (showcase nav pattern).
        // Inner content layout height must exceed the viewport while the outer
        // container still lays out without panicking.
        let white = Color::WHITE;
        let state = ScrollState::new();
        let inner = ScrollArea(
            Modifier::new().height(100.0).fill_max_width(),
            Rc::new(state),
            Column(Modifier::new())
                .child(RBox(Modifier::new().height(300.0).background(white))),
        );
        let root = Column(Modifier::new().size(200.0, 200.0)).child(inner);

        let mut eng = LayoutEngine::new();
        let (scene, hits, _) = eng.layout_frame(
            &root,
            (200, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        assert!(
            hits.iter().any(|h| h.on_scroll.is_some()),
            "inner ScrollArea should emit a scroll hit region"
        );
        // The 300dp-tall page must still be laid out (taller than the 100dp viewport),
        // proving scroll content styles let it grow on the scroll axis.
        assert!(
            scene.nodes.iter().any(|n| matches!(
                n,
                SceneNode::Rect {
                    brush: Brush::Solid(c),
                    rect,
                    ..
                } if *c == white && (rect.h - 300.0).abs() < 1.0
            )),
            "tall page should be laid out at its full height inside the scroller"
        );
    }

    #[test]
    fn test_intrinsic_size_scroll_content_height() {
        use crate::scroll::{ScrollArea, ScrollState};
        use std::rc::Rc;

        let state = ScrollState::new();
        let v = ScrollArea(
            Modifier::new().width(200.0),
            Rc::new(state),
            Column(Modifier::new())
                .child(RBox(Modifier::new().height(80.0)))
                .child(RBox(Modifier::new().height(80.0))),
        );
        let mut eng = make_engine();
        let (w, h) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
        assert!(
            h >= 160.0 - 1.0,
            "scroll max-content height should reach child sum, got {}",
            h
        );
        assert!(w > 0.0, "width should be positive, got {}", w);
    }

    #[test]
    fn test_intrinsic_size_text_max_content() {
        let mut eng = make_engine();
        let v = Column(Modifier::new()).child(Text("Hello"));
        let (w, h) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
        assert!(
            w > 0.0 && h > 0.0,
            "text must have positive size, got ({}, {})",
            w,
            h
        );
    }

    #[test]
    fn test_intrinsic_size_min_content_shrinks() {
        let mut eng = make_engine();
        let v = Column(Modifier::new()).child(Text("Hello"));
        let (min_w, _) = eng.intrinsic_size(&v, IntrinsicSizeMode::MinContent);
        let (max_w, _) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
        assert!(
            min_w <= max_w,
            "min-content width should be <= max-content width (min={}, max={})",
            min_w,
            max_w
        );
    }

    #[test]
    fn test_intrinsic_size_column_uses_max_child_width() {
        let mut eng = make_engine();
        let v = Column(Modifier::new())
            .child(Text("Hi"))
            .child(Text("Hello world"));
        let (max_w, _) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
        let single_w = eng
            .intrinsic_size(
                &Column(Modifier::new()).child(Text("Hello world")),
                IntrinsicSizeMode::MaxContent,
            )
            .0;
        assert!(
            (max_w - single_w).abs() < 1.0,
            "column max-content width should match widest child (col={}, single={})",
            max_w,
            single_w
        );
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use crate::{Column, Text, ViewExt};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn collect_nodes(view: &View, _font_px: &dyn Fn(f32) -> f32) -> Vec<SceneNode> {
        let mut engine = LayoutEngine::new();
        let state: HashMap<u64, Rc<RefCell<crate::TextFieldState>>> = HashMap::new();
        let interactions = crate::Interactions::default();
        let (scene, _, _) = engine.layout_frame(view, (400, 400), &state, &interactions, None);
        scene.nodes
    }

    #[test]
    fn test_graphics_layer_emits_begin_end() {
        let view = Column(Modifier::new().graphics_layer(0.5)).child(Text("hello"));
        let nodes = collect_nodes(&view, &|d| d);
        let begin_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::BeginLayer { .. }))
            .count();
        let end_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::EndLayer { .. }))
            .count();
        assert_eq!(
            begin_count, 1,
            "expected exactly one BeginLayer, got {}",
            begin_count
        );
        assert_eq!(
            end_count, 1,
            "expected exactly one EndLayer, got {}",
            end_count
        );
    }

    #[test]
    fn test_no_graphics_layer_means_no_begin_end() {
        let view = Column(Modifier::new()).child(Text("hello"));
        let nodes = collect_nodes(&view, &|d| d);
        let begin_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::BeginLayer { .. }))
            .count();
        let end_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::EndLayer { .. }))
            .count();
        assert_eq!(begin_count, 0);
        assert_eq!(end_count, 0);
    }

    #[test]
    fn test_nested_graphics_layers_emit_nested_pairs() {
        let view = Column(Modifier::new().graphics_layer(0.9))
            .child(Column(Modifier::new().graphics_layer(0.5)).child(Text("nested")));
        let nodes = collect_nodes(&view, &|d| d);
        let begin_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::BeginLayer { .. }))
            .count();
        let end_count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::EndLayer { .. }))
            .count();
        assert_eq!(
            begin_count, 2,
            "expected two BeginLayer nodes for nested layers"
        );
        assert_eq!(
            end_count, 2,
            "expected two EndLayer nodes for nested layers"
        );
    }

    #[test]
    fn test_begin_end_are_balanced() {
        // Walk through nodes; BeginLayer +1, EndLayer -1; final depth should be 0.
        let view = Column(Modifier::new().graphics_layer(0.7))
            .child(Column(Modifier::new()).child(Text("inner")));
        let nodes = collect_nodes(&view, &|d| d);
        let mut depth: i32 = 0;
        for n in &nodes {
            match n {
                SceneNode::BeginLayer { .. } => depth += 1,
                SceneNode::EndLayer { .. } => depth -= 1,
                _ => {}
            }
        }
        assert_eq!(depth, 0, "Begin/EndLayer must be balanced");
    }

    #[test]
    fn test_graphics_layer_passes_alpha_through() {
        let view = Column(Modifier::new().graphics_layer(0.42)).child(Text("x"));
        let nodes = collect_nodes(&view, &|d| d);
        let begin = nodes.iter().find_map(|n| match n {
            SceneNode::BeginLayer { alpha, .. } => Some(*alpha),
            _ => None,
        });
        assert_eq!(
            begin,
            Some(0.42),
            "graphics_layer alpha should pass through"
        );
    }

    #[test]
    fn test_graphics_layer_alpha_is_clamped() {
        let m = Modifier::new().graphics_layer(2.0);
        assert_eq!(
            m.graphics_layer,
            Some(1.0),
            "alpha above 1.0 should clamp to 1.0"
        );
        let m = Modifier::new().graphics_layer(-0.5);
        assert_eq!(
            m.graphics_layer,
            Some(0.0),
            "negative alpha should clamp to 0.0"
        );
    }
}

#[cfg(test)]
mod shadow_tests {
    use super::*;
    use crate::{Column, Text, ViewExt};
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn collect_nodes(view: &View) -> Vec<SceneNode> {
        let mut engine = LayoutEngine::new();
        let state: HashMap<u64, Rc<RefCell<crate::TextFieldState>>> = HashMap::new();
        let interactions = crate::Interactions::default();
        let (scene, _, _) = engine.layout_frame(view, (400, 400), &state, &interactions, None);
        scene.nodes
    }

    #[test]
    fn test_shadow_alone_does_not_emit_composite_shadow() {
        // Shadow without graphics_layer: nothing to composite.
        let view = Column(Modifier::new().shadow(8.0, 4.0)).child(Text("x"));
        let nodes = collect_nodes(&view);
        let count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::CompositeShadow { .. }))
            .count();
        assert_eq!(
            count, 0,
            "shadow without layer must not emit CompositeShadow"
        );
    }

    #[test]
    fn test_layer_with_shadow_emits_composite_shadow() {
        let view = Column(Modifier::new().graphics_layer(1.0).shadow(8.0, 4.0)).child(Text("x"));
        let nodes = collect_nodes(&view);
        let count = nodes
            .iter()
            .filter(|n| matches!(n, SceneNode::CompositeShadow { .. }))
            .count();
        assert_eq!(count, 1, "expected one CompositeShadow");
    }

    #[test]
    fn test_shadow_appears_after_end_layer() {
        // Order: BeginLayer, ...content..., EndLayer, CompositeShadow, (any)CompositeLayer.
        let view = Column(Modifier::new().graphics_layer(1.0).shadow(8.0, 4.0)).child(Text("x"));
        let nodes = collect_nodes(&view);
        let end_idx = nodes
            .iter()
            .position(|n| matches!(n, SceneNode::EndLayer { .. }))
            .expect("EndLayer should be present");
        let shadow_idx = nodes
            .iter()
            .position(|n| matches!(n, SceneNode::CompositeShadow { .. }))
            .expect("CompositeShadow should be present");
        assert!(
            shadow_idx > end_idx,
            "CompositeShadow (idx {}) should come after EndLayer (idx {})",
            shadow_idx,
            end_idx
        );
    }

    #[test]
    fn test_shadow_passes_through_blur_and_offset() {
        let view = Column(Modifier::new().graphics_layer(1.0).shadow(10.0, 6.0)).child(Text("x"));
        let nodes = collect_nodes(&view);
        let shadow = nodes
            .iter()
            .find_map(|n| match n {
                SceneNode::CompositeShadow {
                    blur_px, offset_px, ..
                } => Some((*blur_px, *offset_px)),
                _ => None,
            })
            .expect("CompositeShadow present");
        let (blur, offset) = shadow;
        // 1 dp = 1 px in tests (density scale = 1).
        assert!(blur > 0.0, "blur should be > 0, got {}", blur);
        assert!(offset.1 > 0.0, "offset_y should be > 0, got {}", offset.1);
    }

    #[test]
    fn test_elevation_helper_sets_shadow() {
        let m4 = Modifier::new().elevation(4.0);
        assert!(m4.shadow.is_some(), "elevation(4) should set shadow");
        let m0 = Modifier::new().elevation(0.0);
        assert!(m0.shadow.is_none(), "elevation(0) should not set shadow");
    }
}
