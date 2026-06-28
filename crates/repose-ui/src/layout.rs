#![allow(non_snake_case)]

use std::cell::RefCell;
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
use crate::textfield::{TF_FONT_DP, TF_PADDING_X_DP, TextFieldState, measure_text};

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

/// The incremental layout engine.
pub struct LayoutEngine {
    /// Persistent view tree.
    tree: ViewTree,

    /// Taffy layout tree.
    taffy: TaffyTree<NodeContext>,

    /// Map from ViewTree NodeId to Taffy NodeId.
    taffy_map: FxHashMap<NodeId, taffy::NodeId>,

    /// Reverse map: Taffy NodeId to ViewTree NodeId.
    reverse_map: FxHashMap<taffy::NodeId, NodeId>,

    /// Cached text layouts (persists across frames).
    text_cache: FxHashMap<NodeId, TextLayout>,

    /// Last window size used for layout.
    last_size_px: Option<(u32, u32)>,

    /// Whether Taffy has a valid computed layout for `last_size_px`.
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
        }
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

        // 0. Check global invalidation
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
            for &node_id in self.text_cache.keys() {
                if let Some(&taffy_id) = self.taffy_map.get(&node_id) {
                    let _ = self.taffy.mark_dirty(taffy_id);
                }
            }
            self.text_cache.clear();
        }

        // Helpers
        let px = |dp_val: f32| dp_to_px(dp_val);
        let font_px = |dp_font: f32| dp_to_px(dp_font) * locals::text_scale().0;

        // 3. Sync Taffy
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

                {
                    let text_cache = &mut self.text_cache;
                    let reverse_map = &self.reverse_map;
                    let tree = &self.tree;

                    let _ = self.taffy.compute_layout_with_measure(
                        taffy_root,
                        available,
                        |known, avail, taffy_node, ctx, _style| {
                            Self::measure_node(
                                known,
                                avail,
                                taffy_node,
                                ctx.as_deref(),
                                text_cache,
                                reverse_map,
                                tree,
                                &font_px,
                                &px,
                            )
                        },
                    );
                }

                // 4a. Store Taffy-computed sizes for all nodes
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

        let _ = temp_taffy.compute_layout_with_measure(
            root_tid,
            avail,
            |known, avail, taffy_node, ctx, _style| {
                Self::measure_node(
                    known,
                    avail,
                    taffy_node,
                    ctx.as_deref(),
                    &mut text_cache,
                    &reverse_map,
                    &self.tree,
                    &font_px_closure,
                    &px_closure,
                )
            },
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
        let ctx = self.context_from_kind(&view.kind);

        let child_tids: Vec<taffy::NodeId> = view
            .children
            .iter()
            .map(|c| self.build_taffy_subtree(c, taffy, font_px))
            .collect();

        if child_tids.is_empty() {
            taffy.new_leaf_with_context(style, ctx).unwrap()
        } else {
            let t = taffy.new_with_children(style, &child_tids).unwrap();
            let _ = taffy.set_node_context(t, Some(ctx));
            t
        }
    }

    fn sync_taffy_tree(&mut self, root_id: NodeId, font_px: &dyn Fn(f32) -> f32) {
        // Removals
        for &node_id in &self.tree.removed_ids {
            if let Some(taffy_id) = self.taffy_map.remove(&node_id) {
                let _ = self.taffy.remove(taffy_id);
                self.reverse_map.remove(&taffy_id);
                self.text_cache.remove(&node_id);
                self.paint_cache.remove(&node_id);
            }
            self.view_ids.remove(&node_id);
        }

        // Updates
        let dirty_nodes: Vec<NodeId> = self.tree.dirty_nodes().iter().copied().collect();
        for node_id in dirty_nodes {
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

        if let Some(&t_id) = self.taffy_map.get(&node_id) {
            self.apply_updates_to_taffy(node_id, t_id, font_px);
            return t_id;
        }

        let (style, ctx, children, is_zstack, is_scroll) = {
            let node = self.tree.get(node_id).expect("Node missing in update");
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
                matches!(node.kind, ViewKind::ZStack),
                matches!(
                    node.kind,
                    ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. }
                ),
            )
        };

        let child_taffy_ids: Vec<taffy::NodeId> = children
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
            self.make_scroll_child(is_scroll, &child_taffy_ids);
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

        let (new_style, new_ctx, children, is_zstack, is_scroll) = {
            let node = self.tree.get(node_id).unwrap();
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
                matches!(node.kind, ViewKind::ZStack),
                matches!(
                    node.kind,
                    ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. }
                ),
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
        self.make_scroll_child(is_scroll, &child_taffy_ids);

        self.stats.taffy_reused += 1;
    }

    fn make_children_absolute(&mut self, is_zstack: bool, child_taffy_ids: &[taffy::NodeId]) {
        if !is_zstack {
            return;
        }
        for &child_tid in child_taffy_ids {
            if let Ok(cs) = self.taffy.style(child_tid) {
                let mut new_cs = cs.clone();
                new_cs.position = Position::Absolute;
                let _ = self.taffy.set_style(child_tid, new_cs);
            }
        }
    }

    fn make_scroll_child(&mut self, is_scroll: bool, child_taffy_ids: &[taffy::NodeId]) {
        if !is_scroll {
            return;
        }
        for &child_tid in child_taffy_ids {
            if let Ok(cs) = self.taffy.style(child_tid) {
                let mut new_cs = cs.clone();
                new_cs.size.height = Dimension::auto();
                new_cs.min_size.height = percent(1.0);
                new_cs.flex_shrink = 0.0;
                let _ = self.taffy.set_style(child_tid, new_cs);
            }
        }
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
            ViewKind::Column | ViewKind::ScrollV { .. } | ViewKind::OverlayHost => {
                s.flex_direction = FlexDirection::Column;
            }
            ViewKind::ScrollXY {
                set_viewport_height,
                ..
            } => {
                // Horizontal-only: Row so width is content-based (scrollable axis).
                // 2D: Column keeps vertical axis scrollable, horizontal relies on
                // descendant-extent computation in the paint walk.
                if set_viewport_height.is_none() {
                    s.flex_direction = FlexDirection::Row;
                } else {
                    s.flex_direction = FlexDirection::Column;
                }
            }
            ViewKind::Stack | ViewKind::ZStack => s.display = Display::Grid,
            _ => {}
        }

        s.align_items = Some(AlignItems::Stretch);
        // Needed for 2D scroll.
        if matches!(
            kind,
            ViewKind::ScrollXY {
                set_viewport_height: Some(_),
                ..
            }
        ) {
            s.align_items = Some(AlignItems::FlexStart);
        }
        s.justify_content = Some(JustifyContent::FlexStart);

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

        if matches!(kind, ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. }) {
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

        if (m.fill_max || m.fill_max_w) && !width_set {
            s.size.width = percent(1.0);
            if s.min_size.width.is_auto() {
                s.min_size.width = length(0.0);
            }
        }
        if (m.fill_max || m.fill_max_h) && !height_set {
            s.size.height = percent(1.0);
            if matches!(kind, ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. })
                && s.min_size.height.is_auto()
            {
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
        if let Some(ref ti) = node.modifier.text_input {
            return NodeContext::TextInput {
                multiline: ti.multiline,
            };
        }
        self.context_from_kind(&node.kind)
    }

    fn context_from_kind(&self, kind: &ViewKind) -> NodeContext {
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
            },
            ViewKind::Expander { .. } => NodeContext::Container,
            ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. } => NodeContext::ScrollContainer,
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
            }) => {
                let size_px_val = font_px(*font_dp);
                let lh = if *line_height > 0.0 {
                    size_px_val * *line_height
                } else {
                    size_px_val * 1.3
                };
                let line_h_px_val = lh;
                let fw = font_weight.0;
                let fs = if matches!(font_style, FontStyle::Italic) { 1 } else { 0 };
                let max_content_w = measure_text(text, size_px_val, *font_family, fw, fs)
                    .positions
                    .last()
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);

                let mut min_content_w = 0.0f32;
                for w in text.split_whitespace() {
                    let ww = measure_text(w, size_px_val, *font_family, fw, fs)
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

                let (lines, line_ranges): (Vec<String>, Vec<(usize, usize)>) = if *soft_wrap {
                    let (ranges, _) = repose_text::wrap_line_ranges(
                        text,
                        size_px_val,
                        wrap_w_px,
                        *max_lines,
                        true,
                        fw,
                        fs,
                    );
                    let lns: Vec<String> = ranges
                        .iter()
                        .map(|&(s, e)| text[s..e].to_string())
                        .collect();
                    (lns, ranges)
                } else if matches!(overflow, TextOverflow::Ellipsis)
                    && max_content_w > wrap_w_px + 0.5
                {
                    let elided = repose_text::ellipsize_line(text, size_px_val, wrap_w_px, fw, fs);
                    let elided_len = elided.len();
                    (vec![elided], vec![(0, elided_len)])
                } else {
                    let len = text.len();
                    (vec![text.clone()], vec![(0, len)])
                };

                let line_widths: Vec<f32> = lines
                    .iter()
                    .map(|line| {
                        measure_text(line, size_px_val, *font_family, fw, fs)
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
            match &n.kind {
                ViewKind::ScrollV { tick_scroll, .. } | ViewKind::ScrollXY { tick_scroll, .. } => {
                    if let Some(tick) = tick_scroll {
                        tick();
                    }
                }
                _ => {}
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
            let taffy_id = self.taffy_map[&node_id];
            let layout = self.taffy.layout(taffy_id).unwrap();
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
            match &n.kind {
                ViewKind::ScrollV {
                    get_scroll_offset, ..
                } => {
                    if let Some(get) = get_scroll_offset {
                        let q = (get() * 8.0) as i32;
                        q.hash(&mut h);
                    }
                }
                ViewKind::ScrollXY {
                    get_scroll_offset_xy,
                    ..
                } => {
                    if let Some(get) = get_scroll_offset_xy {
                        let (x, y) = get();
                        ((x * 8.0) as i32).hash(&mut h);
                        ((y * 8.0) as i32).hash(&mut h);
                    }
                }
                _ if n.modifier.text_input.is_some() => {
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
                _ => {}
            }
            for &ch in n.children.iter() {
                stack.push(ch);
            }
        }
        h.finish()
    }

    fn walk_paint_view(
        &mut self,
        view: &View,
        scene: &mut Scene,
        hits: &mut Vec<HitRegion>,
        sems: &mut Vec<SemNode>,
        textfield_states: &HashMap<u64, Rc<RefCell<TextFieldState>>>,
        interactions: &Interactions,
        focused: Option<u64>,
        parent_offset_px: (f32, f32),
        alpha_accum: f32,
        sem_parent: Option<u64>,
        font_px: &dyn Fn(f32) -> f32,
    ) {
        let root_id = self.tree.update(view);
        self.sync_taffy_tree(root_id, font_px);
        self.walk_paint(
            root_id,
            scene,
            hits,
            sems,
            textfield_states,
            interactions,
            focused,
            parent_offset_px,
            alpha_accum,
            sem_parent,
            None, // interaction_source
            font_px,
            false,
            &mut Vec::new(),
            false,
        );
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

        let taffy_id = self.taffy_map[&node_id];
        let layout = self.taffy.layout(taffy_id).unwrap();

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

        let mut content_rect = if let Some(pv) = modifier.padding_values {
            repose_core::Rect {
                x: rect.x + dp_to_px(pv.left),
                y: rect.y + dp_to_px(pv.top),
                w: (rect.w - dp_to_px(pv.left) - dp_to_px(pv.right)).max(0.0),
                h: (rect.h - dp_to_px(pv.top) - dp_to_px(pv.bottom)).max(0.0),
            }
        } else if let Some(p) = modifier.padding {
            let p_px = dp_to_px(p);
            repose_core::Rect {
                x: rect.x + p_px,
                y: rect.y + p_px,
                w: (rect.w - 2.0 * p_px).max(0.0),
                h: (rect.h - 2.0 * p_px).max(0.0),
            }
        } else {
            rect
        };

        let base_px = (rect.x, rect.y);

        let is_hovered = interactions.hover == Some(view_id);
        let is_pressed = interactions.pressed.contains(&view_id);
        let effective_interaction = interaction_source.unwrap_or(view_id);
        let state_hovered = interactions.hover == Some(effective_interaction);
        let state_pressed = interactions.pressed.contains(&effective_interaction);
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
            modifier.clip_rounded.map(|r| r.map(dp_to_px)).unwrap_or([0.0; 4]),
            rect.w,
            rect.h,
        );
        let push_round_clip = round_clip_px.iter().any(|&r| r > 0.5) && rect.w > 0.5 && rect.h > 0.5;

        if let Some(anim_spec) = &modifier.animate_content_size {
            let target = repose_core::Size {
                width: rect.w,
                height: rect.h,
            };

            let anim = remember_state_with_key(format!("anim_cs:{view_id}"), || {
                AnimatedValue::new(target, *anim_spec)
            });
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
            } else {
                drop(lt);
            }

            let mut still = false;
            let animated = {
                let mut a = anim.borrow_mut();
                still |= a.update();
                let s = *a.get();
                repose_core::Size {
                    width: s.width.max(1.0),
                    height: s.height.max(1.0),
                }
            };
            if still {
                request_frame();
            }

            // Override rect and content_rect dimensions with animated values
            let dw = rect.w - animated.width;
            let dh = rect.h - animated.height;
            rect.w = animated.width;
            rect.h = animated.height;
            content_rect.w = (content_rect.w - dw).max(0.0);
            content_rect.h = (content_rect.h - dh).max(0.0);
        }

        if let Some(tf) = modifier.transform {
            scene.nodes.push(SceneNode::PushTransform { transform: tf });
        }
        if push_round_clip {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: round_clip_px,
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
                        modifier.clip_rounded.map(|r| r.map(dp_to_px)).unwrap_or([0.0; 4]),
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
                let pad_x = dp_to_px(TF_PADDING_X_DP);
                let inner_rect = repose_core::Rect {
                    x: rect.x + pad_x,
                    y: rect.y + dp_to_px(8.0),
                    w: (rect.w - 2.0 * pad_x).max(0.0),
                    h: (rect.h - dp_to_px(16.0)).max(0.0),
                };
                let mut st = state_rc.borrow_mut();
                st.set_inner_width(inner_rect.w);
                st.set_inner_height(inner_rect.h);
                st.tick_scroll_animation();
                if let Some(ref vt) = ti.visual_transformation.as_ref() {
                    let tfmd = vt.filter("");
                    st.offset_map = Some(tfmd.offset_map.clone());
                    st.visual_transformation = Some((*vt).clone());
                } else {
                    st.offset_map = None;
                    st.visual_transformation = None;
                }
                drop(st);
            }

            crate::textfield::paint_text_field(
                scene,
                rect,
                ti,
                state.as_ref(),
                is_focused,
                modifier.clip_rounded,
            );
        }
        if let Some(p) = &modifier.painter {
            (p)(scene, rect, alpha_accum);
        }

        let has_pointer = modifier.on_pointer_down.is_some()
            || modifier.on_pointer_move.is_some()
            || modifier.on_pointer_up.is_some()
            || modifier.on_pointer_enter.is_some()
            || modifier.on_pointer_leave.is_some();

        let has_dnd = modifier.on_drag_start.is_some()
            || modifier.on_drag_end.is_some()
            || modifier.on_drag_enter.is_some()
            || modifier.on_drag_over.is_some()
            || modifier.on_drag_leave.is_some()
            || modifier.on_drop.is_some();

        let kind_handles_hit = modifier.text_input.is_some()
            || matches!(
                kind,
                ViewKind::ScrollV { .. }
                    | ViewKind::ScrollXY { .. }
                    | ViewKind::Expander { .. }
                    | ViewKind::TreeRow { .. }
            );

        let needs_hit = !modifier.disabled
            && (has_pointer || modifier.click || has_dnd || modifier.on_action.is_some());

        if needs_hit && !kind_handles_hit && !modifier.hit_passthrough {
            hits.push(HitRegion {
                id: view_id,
                rect,
                z_index: modifier.z_index,
                focusable: true,
                ..HitRegion::from_modifier(view_id, rect, &modifier)
            });
        }

        // Focus ring for interactive views
        if is_focused && (has_pointer || modifier.click || modifier.on_action.is_some()) {
            push_focus_ring(scene, rect, focus_radius(&modifier));
        }

        let child_interaction_source =
            if needs_hit && !kind_handles_hit && !modifier.hit_passthrough {
                Some(view_id)
            } else {
                interaction_source
            };

        let mut next_sem_parent = sem_parent;

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
                    let lh = if *line_height > 0.0 { px * *line_height } else { px * 1.3 };
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

                        // Find spans that overlap this line's byte range
                        let mut segments: Vec<(usize, usize, Color, f32)> = Vec::new();
                        let mut cursor = line_start;

                        // Sort overlapping spans by start position
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
                                segments.push((cursor, seg_start, *color, *font_size));
                            }

                            let span_color = span.style.color.unwrap_or(*color);
                            let span_size = span.style.font_size.unwrap_or(*font_size);
                            segments.push((seg_start, seg_end, span_color, span_size));
                            cursor = seg_end;
                        }

                        if cursor < line_end {
                            segments.push((cursor, line_end, *color, *font_size));
                        }

                        // Measure and emit each segment
                        let seg_font_px =
                            |dp: f32| dp_to_px(dp) * repose_core::locals::text_scale().0;
                        let mut seg_x = content_rect.x;
                        let fw_val = font_weight.0;
                        let fs_val = if matches!(font_style, FontStyle::Italic) { 1 } else { 0 };
                        for (seg_start, seg_end, seg_color, seg_font_dp) in &segments {
                            let seg_text = &text[*seg_start..*seg_end];
                            if seg_text.is_empty() {
                                continue;
                            }
                            let seg_px = seg_font_px(*seg_font_dp);
                            let seg_w = measure_text(seg_text, seg_px, *font_family, fw_val, fs_val)
                                .positions
                                .last()
                                .copied()
                                .unwrap_or(0.0);
                            scene.nodes.push(SceneNode::Text {
                                rect: repose_core::Rect {
                                    x: seg_x,
                                    y: content_rect.y + i as f32 * line_h_px,
                                    w: seg_w,
                                    h: line_h_px,
                                },
                                text: Arc::<str>::from(seg_text.to_string().into_boxed_str()),
                                color: mul_alpha_color(*seg_color, alpha_accum),
                                size: seg_px,
                                font_family: *font_family,
                                text_align: *text_align,
                                font_weight: *font_weight,
                                font_style: *font_style,
                                text_decoration: *text_decoration,
                                letter_spacing: *letter_spacing,
                                line_height: *line_height,
                            });
                            seg_x += seg_w;
                        }
                    }
                } else {
                    let fw_val = font_weight.0;
                    let fs_val = if matches!(font_style, FontStyle::Italic) { 1 } else { 0 };
                    for (i, ln) in lines.iter().enumerate() {
                        let line_w = measure_text(ln, size_px, *font_family, fw_val, fs_val)
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
                        scene.nodes.push(SceneNode::Text {
                            rect: repose_core::Rect {
                                x: align_x(line_w),
                                y: content_rect.y + i as f32 * line_h_px,
                                w: content_rect.w,
                                h: line_h_px,
                            },
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
                        });
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

                let pad_x = dp_to_px(TF_PADDING_X_DP);
                let inner = repose_core::Rect {
                    x: rect.x + pad_x,
                    y: rect.y + dp_to_px(8.0),
                    w: (rect.w - 2.0 * pad_x).max(0.0),
                    h: (rect.h - dp_to_px(16.0)).max(0.0),
                };

                // Scroll wheel support for multiline text areas
                let on_scroll = if multiline {
                    let key = tf_key;
                    let h = inner.h;
                    let font_val = font_px(TF_FONT_DP);
                    let wrap_w = inner.w.max(1.0);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_height(h);
                        let layout = crate::textfield::layout_text_area(&st.text, font_val, wrap_w, 400, 0);
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
                    let inner_w = inner.w.max(1.0);
                    let font_val = font_px(TF_FONT_DP);
                    let states = textfield_states.get(&key).cloned();
                    Some(Rc::new(move |d: Vec2| -> Vec2 {
                        let Some(st_rc) = states.as_ref() else {
                            return d;
                        };
                        let mut st = st_rc.borrow_mut();
                        st.set_inner_width(inner_w);
                        let m = crate::textfield::measure_text(&st.text, font_val, None, 400, 0);
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
                        on_action: combined,
                        cursor: Some(crate::CursorIcon::Text),
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

        // Children
        let child_offset_px = base_px;
        let layer_id = if let Some(layer_alpha) = modifier.graphics_layer {
            let id = self.layer_id_counter;
            self.layer_id_counter = self.layer_id_counter.wrapping_add(1);
            scene.nodes.push(SceneNode::BeginLayer {
                rect,
                layer_id: id,
                alpha: layer_alpha,
            });
            scene.nodes.push(SceneNode::PushTransform {
                transform: Transform {
                    translate_x: -rect.x,
                    translate_y: -rect.y,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotate: 0.0,
                },
            });
            Some(id)
        } else {
            None
        };
        match &kind {
            ViewKind::ScrollV {
                on_scroll,
                set_viewport_height,
                set_content_height,
                get_scroll_offset,
                set_scroll_offset,
                show_scrollbar,
                ..
            } => {
                hits.push(HitRegion {
                    id: view_id,
                    rect,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                    z_index: modifier.z_index,
                    ..HitRegion::from_modifier(view_id, rect, &modifier)
                });
                let vp = content_rect;
                if let Some(s) = set_viewport_height {
                    s(vp.h.max(0.0));
                }
                let mut ch = 0.0f32;
                for &c in &children {
                    let l = self.taffy.layout(self.taffy_map[&c]).unwrap();
                    ch = ch.max(l.location.y + l.size.height);
                }
                if let Some(s) = set_content_height {
                    s(ch);
                }
                let off = get_scroll_offset.as_ref().map(|f| f()).unwrap_or(0.0);

                scene.nodes.push(SceneNode::PushClip {
                    rect: vp,
                    radius: [0.0; 4],
                });

                let hits_start = hits.len();
                let scrolled_offset = (child_offset_px.0, child_offset_px.1 - off);

                // Optional (recommended): cull children outside the viewport to help LazyColumn
                for &child_id in &children {
                    let l = self.taffy.layout(self.taffy_map[&child_id]).unwrap();
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

                if *show_scrollbar {
                    push_scrollbar(
                        scene,
                        hits,
                        interactions,
                        view_id,
                        vp,
                        ch,
                        off,
                        modifier.z_index,
                        ScrollAxis::V,
                        set_scroll_offset.clone(),
                    );
                }

                scene.nodes.push(SceneNode::PopClip);
            }
            ViewKind::ScrollXY {
                on_scroll,
                set_viewport_width,
                set_viewport_height,
                set_content_width,
                set_content_height,
                get_scroll_offset_xy,
                set_scroll_offset_xy,
                show_scrollbar,
                ..
            } => {
                hits.push(HitRegion {
                    id: view_id,
                    rect,
                    on_scroll: on_scroll.clone(),
                    focusable: false,
                    z_index: modifier.z_index,
                    ..HitRegion::from_modifier(view_id, rect, &modifier)
                });
                let vp = content_rect;
                if let Some(s) = set_viewport_width {
                    s(vp.w.max(0.0));
                }
                if let Some(s) = set_viewport_height {
                    s(vp.h.max(0.0));
                }
                let mut cw = 0.0f32;
                let mut ch = 0.0f32;
                for &c in &children {
                    let mut stack = vec![(self.taffy_map[&c], 0.0f32, 0.0f32)];
                    while let Some((t_id, ox, oy)) = stack.pop() {
                        if let Ok(l) = self.taffy.layout(t_id) {
                            let ax = ox + l.location.x;
                            let ay = oy + l.location.y;
                            cw = cw.max(ax + l.size.width);
                            ch = ch.max(ay + l.size.height);
                            if let Ok(kids) = self.taffy.children(t_id) {
                                for k in kids {
                                    stack.push((k, ax, ay));
                                }
                            }
                        }
                    }
                }
                if let Some(s) = set_content_width {
                    s(cw);
                }
                if let Some(s) = set_content_height {
                    s(ch);
                }
                let (ox, oy) = get_scroll_offset_xy
                    .as_ref()
                    .map(|f| f())
                    .unwrap_or((0.0, 0.0));

                scene.nodes.push(SceneNode::PushClip {
                    rect: vp,
                    radius: [0.0; 4],
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
                if *show_scrollbar {
                    let set_y = set_scroll_offset_xy
                        .clone()
                        .map(|s| Rc::new(move |y| s(ox, y)) as Rc<dyn Fn(f32)>);
                    let set_x = set_scroll_offset_xy
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
                        ScrollAxis::V,
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
                        ScrollAxis::H,
                        set_x,
                    );
                }
                scene.nodes.push(SceneNode::PopClip);
            }
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
                // First child is always visible (the header)
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
                // Remaining children visible only when expanded
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

        if push_round_clip {
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

fn mul_alpha_color(c: Color, a: f32) -> Color {
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
    [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

#[derive(Clone, Copy)]
enum ScrollAxis {
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
    axis: ScrollAxis,
    set_offset: Option<Rc<dyn Fn(f32)>>,
) {
    let vp_len = match axis {
        ScrollAxis::V => vp.h,
        ScrollAxis::H => vp.w,
    };
    if content_len <= vp_len + 0.5 {
        return;
    }

    let thick = dp_to_px(4.0);
    let main_inset = dp_to_px(2.0);

    let (track_x, track_y, track_main, track_cross) = match axis {
        ScrollAxis::V => (
            vp.x + vp.w - thick,
            vp.y + main_inset,
            (vp.h - 2.0 * main_inset).max(0.0),
            thick,
        ),
        ScrollAxis::H => (
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
        ScrollAxis::V => (
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
        ScrollAxis::H => (
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
            ScrollAxis::V => vid ^ 0x8000_0001,
            ScrollAxis::H => vid ^ 0x8000_0002,
        };
        let track_start = match axis {
            ScrollAxis::V => track_y,
            ScrollAxis::H => track_x,
        };
        let max_scroll = (content_len - vp_len).max(1.0);

        let map = Rc::new(move |pos: f32| -> f32 {
            let max_p = (track_main - thumb_len).max(0.0);
            let p = ((pos - track_start) - thumb_len * 0.5).clamp(0.0, max_p);
            (if max_p > 0.0 { p / max_p } else { 0.0 }) * max_scroll
        });

        let extract = match axis {
            ScrollAxis::V => (|pe: &PointerEvent| pe.position.y) as fn(&PointerEvent) -> f32,
            ScrollAxis::H => (|pe: &PointerEvent| pe.position.x) as fn(&PointerEvent) -> f32,
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
    use crate::{Box as RBox, Column, Stack, Text, ViewExt};

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

        let root = Stack(Modifier::new().size(200.0, 200.0)).child((red_box, blue_box));

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

        let root = Stack(Modifier::new().size(200.0, 200.0)).child((box1, box2, box3));

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

        let root = Stack(Modifier::new().size(200.0, 200.0)).child((content, overlay));

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
        use crate::Scroll;

        let content_color = Color::from_rgb(100, 100, 100);
        let overlay_color = Color::from_rgb(0, 0, 255);

        // Tall content inside scroll - 500px tall in 200px viewport
        let tall_content = RBox(Modifier::new().size(180.0, 500.0).background(content_color));

        let scroll = Scroll(Modifier::new().size(200.0, 200.0)).child(tall_content);

        let overlay = RBox(
            Modifier::new()
                .size(50.0, 50.0)
                .background(overlay_color)
                .render_z_index(1000.0),
        );

        let root = Stack(Modifier::new().size(200.0, 200.0)).child((scroll, overlay));

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
        use crate::Scroll;
        use crate::overlay::OverlayHandle;

        let content_color = Color::from_rgb(100, 100, 100);
        let overlay_color = Color::from_rgb(0, 0, 255);

        // Tall content inside scroll - 500px tall in 200px viewport
        let tall_content = RBox(Modifier::new().size(180.0, 500.0).background(content_color));
        let scroll = Scroll(Modifier::new().size(200.0, 200.0)).child(tall_content);

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
        let root = Stack(Modifier::new().size(200.0, 200.0)).child((overlay_host, hint_box));

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
    fn test_intrinsic_size_text_max_content() {
        let mut eng = make_engine();
        let v = Stack(Modifier::new()).child(Text("Hello"));
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
        let v = Stack(Modifier::new()).child(Text("Hello"));
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
                &Stack(Modifier::new()).child(Text("Hello world")),
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
    use crate::{Stack, Text, ViewExt};
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
        let view = Stack(Modifier::new().graphics_layer(0.5)).child(Text("hello"));
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
        let view = Stack(Modifier::new()).child(Text("hello"));
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
        let view = Stack(Modifier::new().graphics_layer(0.9))
            .child(Stack(Modifier::new().graphics_layer(0.5)).child(Text("nested")));
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
        let view = Stack(Modifier::new().graphics_layer(0.7))
            .child(Stack(Modifier::new()).child(Text("inner")));
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
        let view = Stack(Modifier::new().graphics_layer(0.42)).child(Text("x"));
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
    use crate::{Stack, Text, ViewExt};
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
        let view = Stack(Modifier::new().shadow(8.0, 4.0)).child(Text("x"));
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
        let view = Stack(Modifier::new().graphics_layer(1.0).shadow(8.0, 4.0)).child(Text("x"));
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
        let view = Stack(Modifier::new().graphics_layer(1.0).shadow(8.0, 4.0)).child(Text("x"));
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
        let view = Stack(Modifier::new().graphics_layer(1.0).shadow(10.0, 6.0)).child(Text("x"));
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
