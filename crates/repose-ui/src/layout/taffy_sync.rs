#![allow(non_snake_case)]


use repose_core::*;
use repose_tree::{NodeId, TreeNode};
use taffy::TaffyTree;
use taffy::prelude::*;
use taffy::style::FlexDirection;
use taffy::style::Overflow;


use super::*;

impl LayoutEngine {
    pub(crate) fn build_taffy_subtree(
        &self,
        view: &View,
        taffy: &mut taffy::TaffyTree<NodeContext>,
        font_px: &dyn Fn(f32) -> f32,
    ) -> taffy::NodeId {
        let style = self.style_from_kind(&view.kind, &view.modifier, font_px);
        let ctx = Self::context_from_kind_or_modifier(&view.kind, &view.modifier);
        let is_zstack = matches!(view.kind, ViewKind::ZStack);
        let scroll_axis = view.modifier.scroll.as_ref().map(|s| s.axis());

        let visible_children: Vec<&View> = if matches!(
            view.kind,
            ViewKind::Expander { expanded: false, .. }
        ) {
            view.children.iter().take(1).collect()
        } else {
            view.children.iter().collect()
        };
        let child_tids: Vec<taffy::NodeId> = visible_children
            .into_iter()
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

    pub(crate) fn sync_taffy_tree(&mut self, root_id: NodeId, font_px: &dyn Fn(f32) -> f32) {
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
            if self.scope_root_map.contains_key(&node_id)
                && self
                    .tree
                    .get(node_id)
                    .and_then(|n| n.parent)
                    .map(|p| self.node_to_scope.contains_key(&p))
                    .unwrap_or(false)
            {
                continue;
            }
            self.update_taffy_node(node_id, font_px);
        }

        // Ensure root
        if !self.taffy_map.contains_key(&root_id) {
            self.update_taffy_node(root_id, font_px);
        }
    }

    pub(crate) fn update_taffy_node(
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
            let collapsed = matches!(node.kind, ViewKind::Expander { expanded: false, .. });
            let children: Vec<NodeId> = if collapsed {
                node.children.iter().take(1).copied().collect()
            } else {
                node.children.to_vec()
            };
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                children,
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

    pub(crate) fn apply_updates_to_taffy(
        &mut self,
        node_id: NodeId,
        taffy_id: taffy::NodeId,
        font_px: &dyn Fn(f32) -> f32,
    ) {
        // Ensure this node has a stable view id
        let _ = self.ensure_view_id(node_id);

        let (new_style, new_ctx, children, is_zstack, scroll_axis) = {
            let node = self.tree.get(node_id).unwrap();
            let collapsed = matches!(node.kind, ViewKind::Expander { expanded: false, .. });
            let children: Vec<NodeId> = if collapsed {
                node.children.iter().take(1).copied().collect()
            } else {
                node.children.to_vec()
            };
            (
                self.style_from_node(node, font_px),
                self.context_from_node(node),
                children,
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

    pub(crate) fn make_children_absolute_on(
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

    pub(crate) fn apply_scroll_content_styles(
        axis: ScrollAxis,
        child_taffy_ids: &[taffy::NodeId],
        taffy: &mut TaffyTree<NodeContext>,
    ) {
        for &child_tid in child_taffy_ids {
            let Ok(cs) = taffy.style(child_tid) else { continue };
            let mut new_cs = cs.clone();
            new_cs.flex_shrink = 0.0;
            // Fill viewport at minimum so nested scroll / short pages still work.
            new_cs.min_size.width = percent(1.0_f32);
            new_cs.min_size.height = percent(1.0_f32);
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

    pub(crate) fn make_children_absolute(&mut self, is_zstack: bool, child_taffy_ids: &[taffy::NodeId]) {
        Self::make_children_absolute_on(is_zstack, child_taffy_ids, &mut self.taffy);
    }

    pub(crate) fn style_from_node(&self, node: &TreeNode, font_px: &dyn Fn(f32) -> f32) -> taffy::Style {
        self.style_from_kind(&node.kind, &node.modifier, font_px)
    }

    pub(crate) fn style_from_kind(
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
            ViewKind::Column | ViewKind::OverlayHost | ViewKind::Expander { .. } => {
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
                .map(|_| GridTemplateComponent::Single(flex(1.0_f32)))
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
                s.min_size.width = length(0.0_f32);
            }
        }
        let fill_h = m.fill_max_h.or(m.fill_max);
        if let Some(frac) = fill_h {
            if !height_set {
                s.size.height = percent(frac);
            }
            if s.min_size.height.is_auto() {
                s.min_size.height = length(0.0_f32);
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
                if s.min_size.width.is_auto() || s.min_size.width == length(0.0_f32) {
                    s.min_size.width = length(px(v.max(0.0)));
                }
            }
            if let Some(v) = m.default_min_height {
                if s.min_size.height.is_auto() || s.min_size.height == length(0.0_f32) {
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

    pub(crate) fn context_from_node(&self, node: &TreeNode) -> NodeContext {
        Self::context_from_kind_or_modifier(&node.kind, &node.modifier)
    }

    pub(crate) fn context_from_kind_or_modifier(kind: &ViewKind, m: &repose_core::Modifier) -> NodeContext {
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

    pub(crate) fn context_from_kind(kind: &ViewKind) -> NodeContext {
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
}
