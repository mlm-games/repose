#![allow(non_snake_case)]

use std::collections::HashMap;

use repose_core::*;
use repose_tree::{NodeId, ViewTree};
use rustc_hash::FxHashMap;
use taffy::TaffyTree;
use taffy::prelude::*;

use crate::textfield::{TextMeasureConfig, measure_text};

use super::*;

impl LayoutEngine {
    pub(crate) fn run_measure_pass(
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
                            if let Some(sz) = Self::compute_scope_layout(
                                scope_trees,
                                scope_root_map,
                                node_to_scope,
                                tree,
                                font_px,
                                px,
                                key,
                                known,
                                avail,
                            ) {
                                return sz;
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

    fn compute_scope_layout(
        scope_trees: &mut HashMap<String, ScopeLayoutTree>,
        scope_root_map: &FxHashMap<NodeId, String>,
        node_to_scope: &FxHashMap<NodeId, String>,
        tree: &ViewTree,
        font_px: &dyn Fn(f32) -> f32,
        px: &dyn Fn(f32) -> f32,
        key: &str,
        known: taffy::geometry::Size<Option<f32>>,
        avail: taffy::geometry::Size<AvailableSpace>,
    ) -> Option<taffy::geometry::Size<f32>> {
        {
            let st = scope_trees.get(key)?;
            let constraints_changed = st
                .last_constraints
                .map(|(k, a)| k != known || a != avail)
                .unwrap_or(true);
            if st.valid && !constraints_changed {
                return st.cached_size;
            }
        }

        let mut st = scope_trees.remove(key)?;
        let root_tid = st.root_taffy_id?;
        let scope_avail = taffy::geometry::Size {
            width: match known.width {
                Some(w) if w.is_finite() => AvailableSpace::Definite(w),
                _ => match avail.width {
                    AvailableSpace::Definite(w) => AvailableSpace::Definite(w),
                    _ => AvailableSpace::MaxContent,
                },
            },
            height: match known.height {
                Some(h) if h.is_finite() => AvailableSpace::Definite(h),
                _ => match avail.height {
                    AvailableSpace::Definite(h) => AvailableSpace::Definite(h),
                    _ => AvailableSpace::MaxContent,
                },
            },
        };

        let mut st_taffy = std::mem::replace(&mut st.taffy, taffy::TaffyTree::new());
        let mut st_rev = std::mem::take(&mut st.reverse_map);
        let mut st_tc = std::mem::take(&mut st.text_cache);

        let _ = st_taffy.compute_layout_with_measure(
            root_tid,
            scope_avail,
            |known2, avail2, tn, ctx2, _style2| {
                if let Some(&nid) = st_rev.get(&tn) {
                    if scope_root_map.contains_key(&nid) {
                        if let Some(nested_key) = node_to_scope.get(&nid) {
                            if let Some(sz) = Self::compute_scope_layout(
                                scope_trees,
                                scope_root_map,
                                node_to_scope,
                                tree,
                                font_px,
                                px,
                                nested_key,
                                known2,
                                avail2,
                            ) {
                                return sz;
                            }
                        }
                    }
                }
                Self::measure_node(
                    known2,
                    avail2,
                    tn,
                    ctx2.as_deref(),
                    &mut st_tc,
                    &st_rev,
                    tree,
                    font_px,
                    px,
                )
            },
        );

        st.taffy = st_taffy;
        st.reverse_map = st_rev;
        st.text_cache = st_tc;
        st.last_constraints = Some((known, avail));
        if let Ok(layout) = st.taffy.layout(root_tid) {
            let sz = taffy::geometry::Size {
                width: layout.size.width,
                height: layout.size.height,
            };
            st.cached_size = Some(sz);
            st.valid = true;
            scope_trees.insert(key.to_string(), st);
            return Some(sz);
        }
        scope_trees.insert(key.to_string(), st);
        None
    }

    pub(crate) fn measure_node(
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
                text_align: _,
                font_weight,
                font_style,
                text_decoration: _,
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
}
