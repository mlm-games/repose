#![allow(non_snake_case)]

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use repose_core::*;
use repose_tree::{NodeId, TreeNode, TreeStats, ViewTree};
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use taffy::TaffyTree;
use taffy::prelude::*;
use taffy::style::FlexDirection;
use taffy::style::Overflow;

use crate::Interactions;
use crate::textfield::{
    TF_FONT_DP, TF_PADDING_X_DP, TextFieldState, byte_to_char_index, measure_text,
};

fn push_focus_ring(scene: &mut Scene, rect: repose_core::Rect, radius_dp: f32) {
    scene.nodes.push(SceneNode::Border {
        rect,
        color: locals::theme().focus,
        width: dp_to_px(2.0),
        radius: dp_to_px(radius_dp),
    });
}

fn focus_radius(modifier: &Modifier) -> f32 {
    modifier.clip_rounded.unwrap_or(6.0)
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

    /// Last "locals" stamp used for layout decisions (density/text scale/dir).
    last_locals_stamp: Option<u64>,

    /// Stable, unique ViewId per ViewTree NodeId.
    view_ids: FxHashMap<NodeId, u64>,
    next_view_id: u64,

    /// Slider drag state - tracks which sliders are being dragged.
    slider_dragging: Rc<RefCell<FxHashSet<u64>>>,
    /// Range slider active thumb - false=start, true=end.
    range_active_thumb: Rc<RefCell<FxHashMap<u64, bool>>>,
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
    },
    Button {
        label: String,
    },
    TextField {
        multiline: bool,
    },
    Checkbox,
    Radio,
    Switch,
    Slider,
    Range,
    Progress,
    Container,
    ScrollContainer,
}

#[derive(Clone)]
struct TextLayout {
    lines: Vec<String>,
    size_px: f32,
    line_h_px: f32,
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
            slider_dragging: Rc::new(RefCell::new(FxHashSet::default())),
            range_active_thumb: Rc::new(RefCell::new(FxHashMap::default())),
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
        let root_node_id = self.tree.update(root);
        self.stats.tree = self.tree.stats.clone();

        // 2. Determine layout need
        let size_changed = self.last_size_px != Some(size_px);
        let has_tree_mutation =
            !self.tree.dirty_nodes().is_empty() || !self.tree.removed_ids.is_empty();
        let need_layout = size_changed || !self.layout_valid || has_tree_mutation || locals_changed;

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

                self.last_locals_stamp = Some(locals_stamp);

                self.layout_valid = true;
                self.last_size_px = Some(size_px);
                self.stats.layout_misses += 1;
            } else {
                self.stats.layout_hits += 1;
            }
        }
        self.stats.layout_time_ms = (web_time::Instant::now() - start).as_secs_f32() * 1000.0;

        // 5. Paint
        let (scene, hits, sems) = self.paint(
            root_node_id,
            textfield_states,
            interactions,
            focused,
            &font_px,
        );

        self.tree.clear_dirty();
        (scene, hits, sems)
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
            // Clean up view_ids and drag state
            if let Some(&vid) = self.view_ids.get(&node_id) {
                self.slider_dragging.borrow_mut().remove(&vid);
                self.range_active_thumb.borrow_mut().remove(&vid);
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

        let (style, ctx, children) = {
            let node = self.tree.get(node_id).expect("Node missing in update");
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
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

        let (new_style, new_ctx, children) = {
            let node = self.tree.get(node_id).unwrap();
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                node.children.clone(),
            )
        };

        let _ = self.taffy.set_style(taffy_id, new_style);
        let _ = self.taffy.set_node_context(taffy_id, Some(new_ctx));

        let child_taffy_ids: Vec<taffy::NodeId> = children
            .iter()
            .map(|&child_id| self.update_taffy_node(child_id, font_px))
            .collect();
        let _ = self.taffy.set_children(taffy_id, &child_taffy_ids);
        self.stats.taffy_reused += 1;
    }

    fn style_from_node(&self, node: &TreeNode, font_px: &dyn Fn(f32) -> f32) -> taffy::Style {
        let m = &node.modifier;
        let kind = &node.kind;
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
            ViewKind::Column
            | ViewKind::Surface
            | ViewKind::ScrollV { .. }
            | ViewKind::OverlayHost => {
                s.flex_direction = FlexDirection::Column;
            }
            ViewKind::ScrollXY {
                set_viewport_height, ..
            } => {
                if set_viewport_height.is_none() {
                    s.flex_direction = FlexDirection::Row;
                } else {
                    s.flex_direction = FlexDirection::Column;
                }
            }
            ViewKind::Stack => s.display = Display::Grid,
            _ => {}
        }

        s.align_items = Some(AlignItems::Stretch);
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

        if matches!(
            kind,
            ViewKind::Checkbox { .. }
                | ViewKind::RadioButton { .. }
                | ViewKind::Switch { .. }
                | ViewKind::Slider { .. }
                | ViewKind::RangeSlider { .. }
                | ViewKind::Image { .. }
        ) {
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

        if s.min_size.width.is_auto() {
            s.min_size.width = length(0.0);
        }

        if matches!(kind, ViewKind::Button { .. }) {
            s.display = Display::Flex;
            s.flex_direction = if locals::text_direction() == locals::TextDirection::Rtl {
                FlexDirection::RowReverse
            } else {
                FlexDirection::Row
            };
            if m.justify_content.is_none() {
                s.justify_content = Some(JustifyContent::Center);
            }
            if m.align_items_container.is_none() {
                s.align_items = Some(AlignItems::Center);
            }
            if m.padding.is_none() && m.padding_values.is_none() {
                let ph = px(14.0);
                let pv = px(10.0);
                s.padding = taffy::geometry::Rect {
                    left: length(ph),
                    right: length(ph),
                    top: length(pv),
                    bottom: length(pv),
                };
            }
            if m.min_height.is_none() && s.min_size.height.is_auto() {
                s.min_size.height = length(px(40.0));
            }
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
        match &node.kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                soft_wrap,
                max_lines,
                overflow,
                ..
            } => NodeContext::Text {
                text: text.clone(),
                color: *color,
                font_dp: *font_size,
                soft_wrap: *soft_wrap,
                max_lines: *max_lines,
                overflow: *overflow,
            },
            ViewKind::Button { .. } => NodeContext::Button {
                label: String::new(),
            },
            ViewKind::TextField { multiline, .. } => NodeContext::TextField {
                multiline: *multiline,
            },
            ViewKind::Checkbox { .. } => NodeContext::Checkbox,
            ViewKind::RadioButton { .. } => NodeContext::Radio,
            ViewKind::Switch { .. } => NodeContext::Switch,
            ViewKind::Slider { .. } => NodeContext::Slider,
            ViewKind::RangeSlider { .. } => NodeContext::Range,
            ViewKind::ProgressBar { .. } => NodeContext::Progress,
            ViewKind::ScrollV { .. } | ViewKind::ScrollXY { .. } => NodeContext::ScrollContainer,
            ViewKind::OverlayHost => NodeContext::Container,
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
            }) => {
                let size_px_val = font_px(*font_dp);
                let line_h_px_val = size_px_val * 1.3;
                let max_content_w = measure_text(text, size_px_val)
                    .positions
                    .last()
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0);

                let mut min_content_w = 0.0f32;
                for w in text.split_whitespace() {
                    let ww = measure_text(w, size_px_val)
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

                let lines = if *soft_wrap {
                    repose_text::wrap_lines(text, size_px_val, wrap_w_px, *max_lines, true).0
                } else if matches!(overflow, TextOverflow::Ellipsis)
                    && max_content_w > wrap_w_px + 0.5
                {
                    vec![repose_text::ellipsize_line(text, size_px_val, wrap_w_px)]
                } else {
                    vec![text.clone()]
                };

                let max_line_w = lines
                    .iter()
                    .map(|line| {
                        measure_text(line, size_px_val)
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0)
                    })
                    .fold(0.0f32, f32::max);

                if let Some(node_id) = reverse_map.get(&taffy_node) {
                    text_cache.insert(
                        *node_id,
                        TextLayout {
                            lines: lines.clone(),
                            size_px: size_px_val,
                            line_h_px: line_h_px_val,
                        },
                    );
                }
                taffy::geometry::Size {
                    width: max_line_w,
                    height: line_h_px_val * lines.len().max(1) as f32,
                }
            }
            Some(NodeContext::Button { label }) => taffy::geometry::Size {
                width: (label.len() as f32 * font_px(16.0) * 0.6) + px(24.0),
                height: px(36.0),
            },
            Some(NodeContext::TextField { multiline }) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(160.0)),
                height: if *multiline {
                    known.height.unwrap_or(px(140.0))
                } else {
                    px(36.0)
                },
            },
            Some(NodeContext::Checkbox) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(24.0)),
                height: px(24.0),
            },
            Some(NodeContext::Radio) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(18.0)),
                height: px(18.0),
            },
            Some(NodeContext::Switch) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(46.0)),
                height: px(28.0),
            },
            Some(NodeContext::Slider) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(200.0)),
                height: px(28.0),
            },
            Some(NodeContext::Range) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(220.0)),
                height: px(28.0),
            },
            Some(NodeContext::Progress) => taffy::geometry::Size {
                width: known.width.unwrap_or(px(200.0)),
                height: px(12.0),
            },
            _ => taffy::geometry::Size::ZERO,
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
            if let Some(node) = self.tree.get(node_id) {
                if node.modifier.input_blocker && !node.modifier.hit_passthrough {
                    deferred_blockers.push((z, rect));
                }
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
                ViewKind::TextField { state_key, .. } => {
                    let vid = *self.view_ids.get(&id).unwrap_or(&0);
                    let tf_key = if *state_key != 0 { *state_key } else { vid };
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
        if !skip_defer {
            if let Some(render_z) = modifier.render_z_index {
                if !deferred.is_empty() || render_z != 0.0 {
                    // Defer this node - it will be painted later based on render_z_index
                    deferred.push((node_id, parent_offset_px, alpha_accum, sem_parent, render_z));
                    return;
                }
            }
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
        let rect = repose_core::Rect {
            x: parent_offset_px.0 + local_rect.x,
            y: parent_offset_px.1 + local_rect.y,
            w: local_rect.w,
            h: local_rect.h,
        };

        let content_rect = if let Some(pv) = modifier.padding_values {
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
            if let Some(entry) = self.paint_cache.get(&node_id) {
                if entry.subtree_hash == subtree_hash
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

        let round_clip_px = clamp_radius(
            modifier.clip_rounded.map(dp_to_px).unwrap_or(0.0),
            rect.w,
            rect.h,
        );
        let push_round_clip = round_clip_px > 0.5 && rect.w > 0.5 && rect.h > 0.5;
        if (this_alpha - 1.0).abs() > 1e-6 {}
        if let Some(tf) = modifier.transform {
            scene.nodes.push(SceneNode::PushTransform { transform: tf });
        }
        if push_round_clip {
            scene.nodes.push(SceneNode::PushClip {
                rect,
                radius: round_clip_px,
            });
        }

        if let Some(bg) = modifier.background {
            scene.nodes.push(SceneNode::Rect {
                rect,
                brush: mul_alpha_brush(bg, alpha_accum),
                radius: round_clip_px,
            });
        }
        if let Some(b) = &modifier.border {
            scene.nodes.push(SceneNode::Border {
                rect,
                color: mul_alpha_color(b.color, alpha_accum),
                width: dp_to_px(b.width),
                radius: clamp_radius(
                    dp_to_px(b.radius.max(modifier.clip_rounded.unwrap_or(0.0))),
                    rect.w,
                    rect.h,
                ),
            });
        }
        if let Some(p) = &modifier.painter {
            (p)(scene, rect);
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

        let kind_handles_hit = matches!(
            kind,
            ViewKind::Button { .. }
                | ViewKind::TextField { .. }
                | ViewKind::Checkbox { .. }
                | ViewKind::RadioButton { .. }
                | ViewKind::Switch { .. }
                | ViewKind::Slider { .. }
                | ViewKind::RangeSlider { .. }
                | ViewKind::ScrollV { .. }
                | ViewKind::ScrollXY { .. }
        );

        let needs_hit = has_pointer || modifier.click || has_dnd || modifier.on_action.is_some();

        if needs_hit && !kind_handles_hit && !modifier.hit_passthrough {
            hits.push(HitRegion {
                id: view_id,
                rect,
                z_index: modifier.z_index,
                focusable: false,
                ..HitRegion::from_modifier(view_id, rect, &modifier)
            });
        }

        let mut next_sem_parent = sem_parent;

        match &kind {
            ViewKind::Text {
                text,
                color,
                font_size,
                soft_wrap,
                overflow,
                ..
            } => {
                let tl = self.text_cache.get(&node_id);
                let (size_px, line_h_px, lines) = if let Some(tl) = tl {
                    (tl.size_px, tl.line_h_px, tl.lines.clone())
                } else {
                    let px = font_px(*font_size);
                    (px, px * 1.3, vec![text.clone()])
                };
                let total_h = lines.len() as f32 * line_h_px;
                let need_v_clip =
                    total_h > content_rect.h + 0.5 && *overflow != TextOverflow::Visible;
                if lines.len() > 1 && !*soft_wrap { /* single line center handled elsewhere */ }

                let need_clip =
                    *overflow != TextOverflow::Visible && (need_v_clip || content_rect.w > 0.0);
                if need_clip {
                    scene.nodes.push(SceneNode::PushClip {
                        rect: content_rect,
                        radius: 0.0,
                    });
                }
                for (i, ln) in lines.iter().enumerate() {
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: content_rect.x,
                            y: content_rect.y + i as f32 * line_h_px,
                            w: content_rect.w,
                            h: line_h_px,
                        },
                        text: Arc::<str>::from(ln.clone()),
                        color: mul_alpha_color(*color, alpha_accum),
                        size: size_px,
                    });
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
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Button { on_click } => {
                if modifier.background.is_none() {
                    let th = locals::theme();
                    let base = if is_pressed {
                        th.button_bg_pressed
                    } else if is_hovered {
                        th.button_bg_hover
                    } else {
                        th.button_bg
                    };
                    scene.nodes.push(SceneNode::Rect {
                        rect,
                        brush: Brush::Solid(base),
                        radius: modifier.clip_rounded.map(dp_to_px).unwrap_or(dp_to_px(6.0)),
                    });
                }
                if (modifier.click || on_click.is_some()) && !modifier.hit_passthrough {
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_click: on_click.clone(),
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
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    push_focus_ring(scene, rect, focus_radius(&modifier));
                }
            }
            ViewKind::Image { handle, tint, fit } => {
                scene.nodes.push(SceneNode::Image {
                    rect,
                    handle: *handle,
                    tint: mul_alpha_color(*tint, alpha_accum),
                    fit: *fit,
                });
            }
            ViewKind::TextField {
                state_key,
                hint,
                multiline,
                on_change,
                on_submit,
                ..
            } => {
                let tf_key = if *state_key != 0 { *state_key } else { view_id };

                let pad_x = dp_to_px(TF_PADDING_X_DP);
                let inner = repose_core::Rect {
                    x: rect.x + pad_x,
                    y: rect.y + dp_to_px(8.0),
                    w: rect.w - 2.0 * pad_x,
                    h: rect.h - dp_to_px(16.0),
                };

                // Scroll wheel support for multiline text areas
                let on_scroll = if *multiline {
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
                        let layout = crate::textfield::layout_text_area(&st.text, font_val, wrap_w);
                        let content_h = layout.ranges.len().max(1) as f32 * layout.line_h_px;
                        let max_y = (content_h - st.inner_height).max(0.0);

                        let before = st.scroll_offset_y;
                        let target = (st.scroll_offset_y - d.y).clamp(0.0, max_y);
                        st.scroll_offset_y = target;

                        let consumed = before - target;
                        Vec2 {
                            x: d.x,
                            y: d.y - consumed,
                        }
                    }) as Rc<dyn Fn(Vec2) -> Vec2>)
                } else {
                    None
                };

                if !modifier.hit_passthrough {
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll,
                        focusable: true,
                        z_index: modifier.z_index,
                        on_text_change: on_change.clone(),
                        on_text_submit: on_submit.clone(),
                        tf_state_key: Some(tf_key),
                        tf_multiline: *multiline,
                        cursor: Some(crate::CursorIcon::Text),
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }

                scene.nodes.push(SceneNode::PushClip {
                    rect: inner,
                    radius: 0.0,
                });

                if is_focused {
                    push_focus_ring(scene, rect, focus_radius(&modifier));
                }

                if let Some(state_rc) = textfield_states.get(&tf_key) {
                    {
                        let mut st = state_rc.borrow_mut();
                        st.set_inner_width(inner.w);
                        st.set_inner_height(inner.h);
                    }

                    let st = state_rc.borrow();
                    let font_val = font_px(TF_FONT_DP);

                    if !*multiline {
                        let m = measure_text(&st.text, font_val);

                        if st.selection.start != st.selection.end {
                            let sx = m
                                .positions
                                .get(byte_to_char_index(&m, st.selection.start))
                                .copied()
                                .unwrap_or(0.0)
                                - st.scroll_offset;
                            let ex = m
                                .positions
                                .get(byte_to_char_index(&m, st.selection.end))
                                .copied()
                                .unwrap_or(sx)
                                - st.scroll_offset;

                            let th = locals::theme();
                            let selection = mul_alpha_color(th.focus, 85.0 / 255.0);
                            scene.nodes.push(SceneNode::Rect {
                                rect: repose_core::Rect {
                                    x: inner.x + sx.max(0.0),
                                    y: inner.y,
                                    w: (ex - sx).max(0.0),
                                    h: inner.h,
                                },
                                brush: Brush::Solid(selection),
                                radius: 0.0,
                            });
                        }

                        let th = locals::theme();
                        let txt_col = if st.text.is_empty() {
                            th.on_surface_variant
                        } else {
                            th.on_surface
                        };

                        scene.nodes.push(SceneNode::Text {
                            rect: repose_core::Rect {
                                x: inner.x - st.scroll_offset,
                                y: inner.y,
                                w: inner.w,
                                h: inner.h,
                            },
                            text: Arc::from(if st.text.is_empty() {
                                hint.clone()
                            } else {
                                st.text.clone()
                            }),
                            color: txt_col,
                            size: font_val,
                        });

                        if is_focused && st.selection.start == st.selection.end && st.caret_visible() {
                            let cx = m
                                .positions
                                .get(byte_to_char_index(&m, st.selection.end))
                                .copied()
                                .unwrap_or(0.0)
                                - st.scroll_offset;
                            scene.nodes.push(SceneNode::Rect {
                                rect: repose_core::Rect {
                                    x: inner.x + cx.max(0.0),
                                    y: inner.y,
                                    w: dp_to_px(1.0),
                                    h: inner.h,
                                },
                                brush: Brush::Solid(th.on_surface),
                                radius: 0.0,
                            });
                        }
                    } else {
                        let layout = crate::textfield::layout_text_area(
                            &st.text,
                            font_val,
                            inner.w.max(1.0),
                        );
                        let line_h = layout.line_h_px;
                        let content_h = layout.ranges.len().max(1) as f32 * line_h;
                        drop(st);

                        // Reborrow mutably to clamp scroll after we know content height
                        {
                            let mut st = state_rc.borrow_mut();
                            st.clamp_scroll(content_h);
                        }

                        // Render
                        let st = state_rc.borrow();
                        let th = locals::theme();
                        let selection = mul_alpha_color(th.focus, 85.0 / 255.0);
                        if st.text.is_empty() {
                            scene.nodes.push(SceneNode::Text {
                                rect: repose_core::Rect {
                                    x: inner.x,
                                    y: inner.y,
                                    w: inner.w,
                                    h: inner.h,
                                },
                                text: Arc::from(hint.clone()),
                                color: th.on_surface_variant,
                                size: font_val,
                            });
                        } else {
                            for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                                let ln = &st.text[s..e];
                                let draw_y = inner.y + (i as f32) * line_h - st.scroll_offset_y;
                                if draw_y + line_h < inner.y - 1.0
                                    || draw_y > inner.y + inner.h + 1.0
                                {
                                    continue;
                                }
                                scene.nodes.push(SceneNode::Text {
                                    rect: repose_core::Rect {
                                        x: inner.x,
                                        y: draw_y,
                                        w: inner.w,
                                        h: line_h,
                                    },
                                    text: Arc::<str>::from(ln.to_string()),
                                    color: locals::theme().on_surface,
                                    size: font_val,
                                });
                            }
                        }

                        // Selection (multi-line)
                        if st.selection.start != st.selection.end {
                            let (sel_a, sel_b) = if st.selection.start < st.selection.end {
                                (st.selection.start, st.selection.end)
                            } else {
                                (st.selection.end, st.selection.start)
                            };
                            for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                                let os = sel_a.max(s);
                                let oe = sel_b.min(e);
                                if os >= oe {
                                    continue;
                                }
                                let ln = &st.text[s..e];
                                let m = measure_text(ln, font_val);

                                let ls = os - s;
                                let le = oe - s;

                                let sx = m
                                    .positions
                                    .get(byte_to_char_index(&m, ls))
                                    .copied()
                                    .unwrap_or(0.0);
                                let ex = m
                                    .positions
                                    .get(byte_to_char_index(&m, le))
                                    .copied()
                                    .unwrap_or(sx);

                                let draw_y = inner.y + (i as f32) * line_h - st.scroll_offset_y;
                                scene.nodes.push(SceneNode::Rect {
                                    rect: repose_core::Rect {
                                        x: inner.x + sx,
                                        y: draw_y,
                                        w: (ex - sx).max(0.0),
                                        h: line_h,
                                    },
                                    brush: Brush::Solid(selection),
                                    radius: 0.0,
                                });
                            }
                        }

                        // Caret (multi-line)
                        if is_focused && st.selection.start == st.selection.end && st.caret_visible() {
                            let caret = st.selection.end.min(st.text.len());
                            let (cx, cy, _li) = crate::textfield::caret_xy_for_byte(
                                &st.text,
                                font_val,
                                inner.w.max(1.0),
                                caret,
                            );
                            let draw_x = inner.x + cx;
                            let draw_y = inner.y + cy - st.scroll_offset_y;
                            scene.nodes.push(SceneNode::Rect {
                                rect: repose_core::Rect {
                                    x: draw_x,
                                    y: draw_y,
                                    w: dp_to_px(1.0),
                                    h: line_h,
                                },
                                brush: Brush::Solid(th.on_surface),
                                radius: 0.0,
                            });
                        }
                    }
                } else {
                    let th = locals::theme();
                    scene.nodes.push(SceneNode::Text {
                        rect: inner,
                        text: Arc::from(hint.clone()),
                        color: th.on_surface_variant,
                        size: font_px(TF_FONT_DP),
                    });
                }

                scene.nodes.push(SceneNode::PopClip);

                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::TextField,
                    label: Some(hint.clone()),
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
            }
            ViewKind::Checkbox { checked, on_change } => {
                let th = locals::theme();
                let sz = dp_to_px(18.0);
                let bx = rect.x;
                let by = rect.y + (rect.h - sz) * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: sz,
                        h: sz,
                    },
                    brush: Brush::Solid(if *checked { th.primary } else { th.surface }),
                    radius: dp_to_px(3.0),
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: bx,
                        y: by,
                        w: sz,
                        h: sz,
                    },
                    color: th.outline,
                    width: dp_to_px(1.0),
                    radius: dp_to_px(3.0),
                });
                if *checked {
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: bx + dp_to_px(3.0),
                            y: rect.y + rect.h * 0.5 - font_px(16.0) * 0.6,
                            w: sz,
                            h: font_px(16.0),
                        },
                        text: Arc::from("✓"),
                        color: th.on_primary,
                        size: font_px(16.0),
                    });
                }
                let toggled = !*checked;
                let on_click = on_change.as_ref().map(|cb| {
                    let cb = cb.clone();
                    Rc::new(move || cb(toggled)) as Rc<dyn Fn()>
                });
                hits.push(HitRegion {
                    id: view_id,
                    rect,
                    on_click,
                    focusable: true,
                    z_index: modifier.z_index,
                    ..HitRegion::from_modifier(view_id, rect, &modifier)
                });
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Checkbox,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    push_focus_ring(scene, rect, 6.0);
                }
            }
            ViewKind::RadioButton {
                selected,
                on_select,
            } => {
                let th = locals::theme();
                let d = dp_to_px(18.0);
                let cy = rect.y + (rect.h - d) * 0.5;
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: cy,
                        w: d,
                        h: d,
                    },
                    color: th.outline,
                    width: dp_to_px(1.5),
                    radius: d * 0.5,
                });
                if *selected {
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: rect.x + dp_to_px(4.0),
                            y: cy + dp_to_px(4.0),
                            w: d - dp_to_px(8.0),
                            h: d - dp_to_px(8.0),
                        },
                        brush: Brush::Solid(th.primary),
                        radius: (d - dp_to_px(8.0)) * 0.5,
                    });
                }
                if !modifier.hit_passthrough {
                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_click: on_select.clone(),
                        focusable: true,
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::RadioButton,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    push_focus_ring(scene, rect, 6.0);
                }
            }
            ViewKind::Switch { checked, on_change } => {
                let th = locals::theme();
                let tw = dp_to_px(46.0);
                let th_h = dp_to_px(26.0);
                let ty = rect.y + (rect.h - th_h) * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: ty,
                        w: tw,
                        h: th_h,
                    },
                    brush: Brush::Solid(if *checked { th.primary } else { th.outline }),
                    radius: th_h * 0.5,
                });
                let kw = dp_to_px(22.0);
                let kx = if *checked {
                    rect.x + tw - kw - dp_to_px(2.0)
                } else {
                    rect.x + dp_to_px(2.0)
                };
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx,
                        y: ty + (th_h - kw) * 0.5,
                        w: kw,
                        h: kw,
                    },
                    brush: Brush::Solid(th.on_surface),
                    radius: kw * 0.5,
                });
                let t = !*checked;
                let on_click = on_change.as_ref().map(|cb| {
                    let cb = cb.clone();
                    Rc::new(move || cb(t)) as Rc<dyn Fn()>
                });
                hits.push(HitRegion {
                    id: view_id,
                    rect,
                    on_click,
                    focusable: true,
                    z_index: modifier.z_index,
                    ..HitRegion::from_modifier(view_id, rect, &modifier)
                });
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Switch,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    push_focus_ring(scene, rect, 6.0);
                }
            }
            ViewKind::Slider {
                value,
                min,
                max,
                step,
                on_change,
            } => {
                let th = locals::theme();
                let th_h = dp_to_px(4.0);
                let kn_d = dp_to_px(20.0);
                let cy = rect.y + rect.h * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: cy - th_h * 0.5,
                        w: rect.w,
                        h: th_h,
                    },
                    brush: Brush::Solid(th.outline),
                    radius: th_h * 0.5,
                });
                let t = norm(*value, *min, *max).clamp(0.0, 1.0);
                let kx = rect.x + t * rect.w;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: kx - kn_d * 0.5,
                        y: cy - kn_d * 0.5,
                        w: kn_d,
                        h: kn_d,
                    },
                    brush: Brush::Solid(th.surface),
                    radius: kn_d * 0.5,
                });
                scene.nodes.push(SceneNode::Border {
                    rect: repose_core::Rect {
                        x: kx - kn_d * 0.5,
                        y: cy - kn_d * 0.5,
                        w: kn_d,
                        h: kn_d,
                    },
                    color: th.outline,
                    width: dp_to_px(1.0),
                    radius: kn_d * 0.5,
                });
                if let Some(cb) = on_change.clone() {
                    let dragging = self.slider_dragging.clone();
                    let id = view_id;
                    let r = rect;
                    let minv = *min;
                    let maxv = *max;
                    let stepv = *step;

                    let on_pd = {
                        let cb = cb.clone();
                        let dragging = dragging.clone();
                        Rc::new(move |pe: PointerEvent| {
                            dragging.borrow_mut().insert(id);
                            (cb)(value_from_x(pe.position.x, r, minv, maxv, stepv));
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    let on_pm = {
                        let cb = cb.clone();
                        let dragging = dragging.clone();
                        Rc::new(move |pe: PointerEvent| {
                            if !dragging.borrow().contains(&id) {
                                return; // ignore hover moves
                            }
                            (cb)(value_from_x(pe.position.x, r, minv, maxv, stepv));
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    let on_pu = {
                        let dragging = dragging.clone();
                        Rc::new(move |_pe: PointerEvent| {
                            dragging.borrow_mut().remove(&id);
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    // Build on_scroll for wheel support
                    let on_scroll = {
                        let cb = cb.clone();
                        let cur = *value;
                        Rc::new(move |d: Vec2| -> Vec2 {
                            let dir = wheel_to_steps(d.y);
                            if dir == 0 {
                                return d;
                            }
                            let next = apply_step(cur, dir, minv, maxv, stepv);
                            if (next - cur).abs() > 1e-6 {
                                (cb)(next);
                                Vec2 { x: d.x, y: 0.0 }
                            } else {
                                d
                            }
                        }) as Rc<dyn Fn(Vec2) -> Vec2>
                    };

                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: Some(on_scroll),
                        focusable: true,
                        on_pointer_down: Some(on_pd),
                        on_pointer_move: Some(on_pm),
                        on_pointer_up: Some(on_pu),
                        on_pointer_enter: modifier.on_pointer_enter.clone(),
                        on_pointer_leave: modifier.on_pointer_leave.clone(),
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Slider,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    push_focus_ring(scene, rect, 6.0);
                }
            }
            ViewKind::RangeSlider {
                start,
                end,
                min,
                max,
                step,
                on_change,
            } => {
                let th = locals::theme();
                let th_h = dp_to_px(4.0);
                let kn_d = dp_to_px(20.0);
                let cy = rect.y + rect.h * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: cy - th_h * 0.5,
                        w: rect.w,
                        h: th_h,
                    },
                    brush: Brush::Solid(th.outline),
                    radius: th_h * 0.5,
                });
                let t0 = norm(*start, *min, *max).clamp(0.0, 1.0);
                let t1 = norm(*end, *min, *max).clamp(0.0, 1.0);
                let k0 = rect.x + t0 * rect.w;
                let k1 = rect.x + t1 * rect.w;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: k0.min(k1),
                        y: cy - th_h * 0.5,
                        w: (k1 - k0).abs(),
                        h: th_h,
                    },
                    brush: Brush::Solid(th.primary),
                    radius: th_h * 0.5,
                });
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: k0 - kn_d * 0.5,
                        y: cy - kn_d * 0.5,
                        w: kn_d,
                        h: kn_d,
                    },
                    brush: Brush::Solid(th.surface),
                    radius: kn_d * 0.5,
                });
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: k1 - kn_d * 0.5,
                        y: cy - kn_d * 0.5,
                        w: kn_d,
                        h: kn_d,
                    },
                    brush: Brush::Solid(th.surface),
                    radius: kn_d * 0.5,
                });
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::Slider,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
                if is_focused {
                    scene.nodes.push(SceneNode::Border {
                        rect,
                        color: th.focus,
                        width: dp_to_px(2.0),
                        radius: dp_to_px(6.0),
                    });
                }

                if let Some(cb) = on_change.clone() {
                    let dragging = self.slider_dragging.clone();
                    let active = self.range_active_thumb.clone();

                    let id = view_id;
                    let r = rect;
                    let minv = *min;
                    let maxv = *max;
                    let stepv = *step;

                    let start0 = *start;
                    let end0 = *end;

                    let on_pd = {
                        let cb = cb.clone();
                        let dragging = dragging.clone();
                        let active = active.clone();
                        Rc::new(move |pe: PointerEvent| {
                            dragging.borrow_mut().insert(id);

                            // choose thumb based on which value is closer at press-time
                            let v = value_from_x(pe.position.x, r, minv, maxv, stepv);
                            let use_end = (v - end0).abs() < (v - start0).abs();
                            active.borrow_mut().insert(id, use_end);

                            let (mut a, mut b) = (start0, end0);
                            if use_end {
                                b = v.max(a);
                            } else {
                                a = v.min(b);
                            }
                            (cb)(a, b);
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    let on_pm = {
                        let cb = cb.clone();
                        let dragging = dragging.clone();
                        let active = active.clone();
                        Rc::new(move |pe: PointerEvent| {
                            if !dragging.borrow().contains(&id) {
                                return;
                            }
                            let v = value_from_x(pe.position.x, r, minv, maxv, stepv);
                            let use_end = *active.borrow().get(&id).unwrap_or(&false);

                            let (mut a, mut b) = (start0, end0);
                            if use_end {
                                b = v.max(a);
                            } else {
                                a = v.min(b);
                            }
                            (cb)(a, b);
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    let on_pu = {
                        let dragging = dragging.clone();
                        let active = active.clone();
                        Rc::new(move |_pe: PointerEvent| {
                            dragging.borrow_mut().remove(&id);
                            active.borrow_mut().remove(&id);
                        }) as Rc<dyn Fn(PointerEvent)>
                    };

                    // Build on_scroll for wheel support
                    let on_scroll = {
                        let cb = cb.clone();
                        let cur_a = *start;
                        let cur_b = *end;
                        let active_map = active.clone();

                        Rc::new(move |d: Vec2| -> Vec2 {
                            let dir = wheel_to_steps(d.y);
                            if dir == 0 {
                                return d;
                            }

                            // Use end thumb by default if no active thumb stored
                            let use_end = *active_map.borrow().get(&id).unwrap_or(&true);

                            let (mut a, mut b) = (cur_a, cur_b);
                            if use_end {
                                b = apply_step(b, dir, minv, maxv, stepv).max(a);
                            } else {
                                a = apply_step(a, dir, minv, maxv, stepv).min(b);
                            }

                            if (a - cur_a).abs() > 1e-6 || (b - cur_b).abs() > 1e-6 {
                                (cb)(a, b);
                                Vec2 { x: d.x, y: 0.0 }
                            } else {
                                d
                            }
                        }) as Rc<dyn Fn(Vec2) -> Vec2>
                    };

                    hits.push(HitRegion {
                        id: view_id,
                        rect,
                        on_scroll: Some(on_scroll),
                        focusable: true,
                        on_pointer_down: Some(on_pd),
                        on_pointer_move: Some(on_pm),
                        on_pointer_up: Some(on_pu),
                        on_pointer_enter: modifier.on_pointer_enter.clone(),
                        on_pointer_leave: modifier.on_pointer_leave.clone(),
                        z_index: modifier.z_index,
                        ..HitRegion::from_modifier(view_id, rect, &modifier)
                    });
                }
            }
            ViewKind::ProgressBar {
                value, min, max, ..
            } => {
                let th = locals::theme();
                let th_h = dp_to_px(6.0);
                let cy = rect.y + rect.h * 0.5;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: cy - th_h * 0.5,
                        w: rect.w,
                        h: th_h,
                    },
                    brush: Brush::Solid(th.outline),
                    radius: th_h * 0.5,
                });
                let t = norm(*value, *min, *max).clamp(0.0, 1.0);
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: cy - th_h * 0.5,
                        w: rect.w * t,
                        h: th_h,
                    },
                    brush: Brush::Solid(th.primary),
                    radius: th_h * 0.5,
                });
                sems.push(SemNode {
                    id: view_id,
                    parent: sem_parent,
                    role: Role::ProgressBar,
                    label: None,
                    rect,
                    focused: is_focused,
                    enabled: true,
                });
                next_sem_parent = Some(view_id);
            }
            _ => {}
        }

        // Children
        let child_offset_px = base_px;
        match &kind {
            ViewKind::ScrollV {
                on_scroll,
                set_viewport_height,
                set_content_height,
                get_scroll_offset,
                set_scroll_offset,
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
                    radius: 0.0,
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
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }

                // Clip hits to viewport
                let mut i = hits_start;
                while i < hits.len() {
                    if let Some(r) = intersect_rect(hits[i].rect, vp) {
                        hits[i].rect = r;
                        i += 1;
                    } else {
                        hits.remove(i);
                    }
                }

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
                    let l = self.taffy.layout(self.taffy_map[&c]).unwrap();
                    cw = cw.max(l.location.x + l.size.width);
                    ch = ch.max(l.location.y + l.size.height);
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
                    radius: 0.0,
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
                let set_y = set_scroll_offset_xy.clone().map(|s| {
                    let ox = ox;
                    Rc::new(move |y| s(ox, y)) as Rc<dyn Fn(f32)>
                });
                let set_x = set_scroll_offset_xy.clone().map(|s| {
                    let oy = oy;
                    Rc::new(move |x| s(x, oy)) as Rc<dyn Fn(f32)>
                });
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
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
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
                        font_px,
                        allow_cache,
                        deferred,
                        skip_defer,
                    );
                }
            }
        }

        if push_round_clip {
            scene.nodes.push(SceneNode::PopClip);
        }
        if modifier.transform.is_some() {
            scene.nodes.push(SceneNode::PopTransform);
        }
    }
}

// Helpers
fn infer_label(tree: &ViewTree, node_id: NodeId) -> Option<String> {
    let mut stack = vec![node_id];
    while let Some(id) = stack.pop() {
        let n = tree.get(id)?;
        if let ViewKind::Text { text, .. } = &n.kind {
            if !text.is_empty() {
                return Some(text.clone());
            }
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
    }
}

fn clamp_radius(r: f32, w: f32, h: f32) -> f32 {
    r.max(0.0).min(0.5 * w.max(0.0)).min(0.5 * h.max(0.0))
}
fn norm(v: f32, min: f32, max: f32) -> f32 {
    if max > min {
        (v - min) / (max - min)
    } else {
        0.0
    }
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

    let thick = dp_to_px(6.0);
    let m = dp_to_px(2.0);

    let (track_x, track_y, track_main, track_cross) = match axis {
        ScrollAxis::V => (
            vp.x + vp.w - m - thick,
            vp.y + m,
            (vp.h - 2.0 * m).max(0.0),
            thick,
        ),
        ScrollAxis::H => (
            vp.x + m,
            vp.y + vp.h - m - thick,
            (vp.w - 2.0 * m).max(0.0),
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
        radius: thick * 0.5,
    });
    scene.nodes.push(SceneNode::Rect {
        rect: thumb_rect,
        brush: Brush::Solid(locals::theme().scrollbar_thumb),
        radius: thick * 0.5,
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

fn snap_step(v: f32, min: f32, max: f32, step: Option<f32>) -> f32 {
    let v = v.clamp(min, max);
    if let Some(s) = step.filter(|s| *s > 0.0) {
        let t = ((v - min) / s).round();
        (min + t * s).clamp(min, max)
    } else {
        v
    }
}

fn value_from_x(x: f32, rect: repose_core::Rect, min: f32, max: f32, step: Option<f32>) -> f32 {
    let w = rect.w.max(1.0);
    let t = ((x - rect.x) / w).clamp(0.0, 1.0);
    let v = min + t * (max - min);
    snap_step(v, min, max, step)
}

fn wheel_to_steps(dy_px: f32) -> i32 {
    if dy_px < -0.5 {
        1
    } else if dy_px > 0.5 {
        -1
    } else {
        0
    }
}

fn apply_step(v: f32, dir: i32, min: f32, max: f32, step: Option<f32>) -> f32 {
    if dir == 0 {
        return v;
    }
    let s = step.unwrap_or(1.0).max(1e-6);
    snap_step(v + (dir as f32) * s, min, max, step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Box as RBox, Column, Stack, ViewExt};

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
}
