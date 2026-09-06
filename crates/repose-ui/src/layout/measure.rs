#![allow(non_snake_case)]

use std::collections::HashMap;

use repose_core::*;
use repose_tree::{NodeId, ViewTree};
use rustc_hash::FxHashMap;
use taffy::TaffyTree;
use taffy::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

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
            |inputs, taffy_node, ctx, style| {
                taffy::compute_leaf_layout(
                    inputs,
                    style,
                    |_, _| 0.0,
                    |known, avail| {
                        // Check if this is a scope root marker -> return cached scope size
                        if let Some(&node_id) = reverse_map.get(&taffy_node) {
                            // Custom layout modifier: delegate measurement to user callback
                            if let Some(node) = tree.get(node_id)
                                && let Some(ref layout_cb) = node.modifier.layout
                            {
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
                                let known_h =
                                    known.height.map(|h| h / scale).unwrap_or(f32::INFINITY);
                                let constraints = repose_core::modifier::LayoutConstraints {
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
                            if scope_root_map.contains_key(&node_id)
                                && let Some(key) = node_to_scope.get(&node_id)
                                && let Some(sz) = Self::compute_scope_layout(
                                    scope_trees,
                                    scope_root_map,
                                    node_to_scope,
                                    tree,
                                    font_px,
                                    px,
                                    key,
                                    known,
                                    avail,
                                )
                            {
                                return sz;
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
        let st_rev = std::mem::take(&mut st.reverse_map);
        let mut st_tc = std::mem::take(&mut st.text_cache);

        let _ = st_taffy.compute_layout_with_measure(
            root_tid,
            scope_avail,
            |inputs, tn, ctx2, style2| {
                taffy::compute_leaf_layout(
                    inputs,
                    style2,
                    |_, _| 0.0,
                    |known2, avail2| {
                        if let Some(&nid) = st_rev.get(&tn)
                            && scope_root_map.contains_key(&nid)
                            && let Some(nested_key) = node_to_scope.get(&nid)
                            && let Some(sz) = Self::compute_scope_layout(
                                scope_trees,
                                scope_root_map,
                                node_to_scope,
                                tree,
                                font_px,
                                px,
                                nested_key,
                                known2,
                                avail2,
                            )
                        {
                            return sz;
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
                font_dp,
                soft_wrap,
                max_lines,
                overflow,
                font_family,
                font_weight,
                font_style,
                letter_spacing,
                line_height,
                font_variation_settings,
                annotations,
            }) => {
                let has_annotations = annotations.as_ref().is_some_and(|a| !a.is_empty());
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

                let annotated_width = |start_b: usize, end_b: usize| -> f32 {
                    if !has_annotations || start_b >= end_b {
                        return measure_text(
                            &text[start_b..end_b],
                            size_px_val,
                            TextMeasureConfig {
                                font_family: *font_family,
                                font_weight: fw,
                                font_style: fs,
                                letter_spacing: *letter_spacing,
                                ..Default::default()
                            },
                        )
                        .positions
                        .last()
                        .copied()
                        .unwrap_or(0.0);
                    }
                    let annos = annotations.as_ref().unwrap();
                    let mut width = 0.0f32;
                    let mut cursor = start_b;
                    for span in annos.iter().filter(|s| s.start < end_b && s.end > start_b) {
                        let seg_start = span.start.max(start_b);
                        let seg_end = span.end.min(end_b);
                        if seg_start > cursor {
                            let seg_text = &text[cursor..seg_start];
                            if !seg_text.is_empty() {
                                width += measure_text(
                                    seg_text,
                                    size_px_val,
                                    TextMeasureConfig {
                                        font_family: *font_family,
                                        font_weight: fw,
                                        font_style: fs,
                                        letter_spacing: *letter_spacing,
                                        ..Default::default()
                                    },
                                )
                                .positions
                                .last()
                                .copied()
                                .unwrap_or(0.0);
                            }
                        }
                        let seg_text = &text[seg_start..seg_end];
                        if !seg_text.is_empty() {
                            let seg_font_dp = span.style.font_size.unwrap_or(*font_dp);
                            let seg_px = font_px(seg_font_dp);
                            let seg_fw = span.style.font_weight.unwrap_or(font_weight.0);
                            let seg_fs = span.style.font_style.unwrap_or(fs);
                            let seg_ls = span.style.letter_spacing.unwrap_or(*letter_spacing);
                            let seg_family = span.style.font_family.or(*font_family);
                            width += measure_text(
                                seg_text,
                                seg_px,
                                TextMeasureConfig {
                                    font_family: seg_family,
                                    font_weight: seg_fw,
                                    font_style: seg_fs,
                                    letter_spacing: seg_ls,
                                    ..Default::default()
                                },
                            )
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0);
                            if let Some(repose_core::DrawStyle::Stroke { width: sw, .. }) =
                                &span.style.draw_style
                            {
                                width += *sw * seg_px;
                            }
                        }
                        cursor = seg_end;
                    }
                    if cursor < end_b {
                        let seg_text = &text[cursor..end_b];
                        if !seg_text.is_empty() {
                            width += measure_text(
                                seg_text,
                                size_px_val,
                                TextMeasureConfig {
                                    font_family: *font_family,
                                    font_weight: fw,
                                    font_style: fs,
                                    letter_spacing: *letter_spacing,
                                    ..Default::default()
                                },
                            )
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0);
                        }
                    }
                    width
                };

                let max_content_w = if has_annotations {
                    annotated_width(0, text.len())
                } else {
                    measure_text(
                        text,
                        size_px_val,
                        TextMeasureConfig {
                            font_family: *font_family,
                            font_weight: fw,
                            font_style: fs,
                            letter_spacing: *letter_spacing,
                            ..Default::default()
                        },
                    )
                    .positions
                    .last()
                    .copied()
                    .unwrap_or(0.0)
                    .max(0.0)
                };

                let mut min_content_w = 0.0f32;
                if has_annotations {
                    let mut byte_offset = 0usize;
                    for token in text.split_word_bounds() {
                        let tok_start = byte_offset;
                        let tok_end = tok_start + token.len();
                        byte_offset = tok_end;
                        let trimmed = token.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        let ww = annotated_width(tok_start, tok_end);
                        min_content_w = min_content_w.max(ww);
                    }
                    if min_content_w <= 0.0 {
                        min_content_w = max_content_w;
                    }
                } else {
                    for w in text.split_whitespace() {
                        let ww = measure_text(
                            w,
                            size_px_val,
                            TextMeasureConfig {
                                font_family: *font_family,
                                font_weight: fw,
                                font_style: fs,
                                letter_spacing: *letter_spacing,
                                ..Default::default()
                            },
                        )
                        .positions
                        .last()
                        .copied()
                        .unwrap_or(0.0);
                        min_content_w = min_content_w.max(ww);
                    }
                    if min_content_w <= 0.0 {
                        min_content_w = max_content_w;
                    }
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
                    if has_annotations {
                        let width_of = |s: usize, e: usize| annotated_width(s, e);
                        if max_content_w <= wrap_w_px + 0.5 {
                            (vec![text.clone()], vec![(0, text.len())])
                        } else {
                            let mut all_ranges: Vec<(usize, usize)> = Vec::new();
                            let mut truncated = false;
                            let mut line0_start = 0usize;
                            let max_lines_remaining =
                                |out_len: usize| max_lines.map(|ml| ml.saturating_sub(out_len));
                            let wrap_hard =
                                |start: usize,
                                 end: usize,
                                 max_lines_opt: Option<usize>,
                                 width_of: &dyn Fn(usize, usize) -> f32|
                                 -> (Vec<(usize, usize)>, bool) {
                                    if start >= end {
                                        return (vec![(start, start)], false);
                                    }
                                    if width_of(start, end) <= wrap_w_px + 0.5 {
                                        return (vec![(start, end)], false);
                                    }
                                    let mut out = Vec::new();
                                    let mut line_start = start;
                                    let mut best_break = line_start;
                                    let mut unconsumed_start = start;
                                    let mut t = false;
                                    for tok in text[line_start..end].split_word_bounds() {
                                        let tok_abs_start = unconsumed_start;
                                        let tok_abs_end = tok_abs_start + tok.len();
                                        unconsumed_start = tok_abs_end;
                                        let w = width_of(line_start, tok_abs_end);
                                        if w <= wrap_w_px + 0.5 {
                                            best_break = tok_abs_end;
                                            continue;
                                        }
                                        if best_break > line_start {
                                            out.push((line_start, best_break));
                                            line_start = best_break;
                                        } else {
                                            let mut cut = tok_abs_start;
                                            for (ofs, g) in tok.grapheme_indices(true) {
                                                let next = tok_abs_start + ofs + g.len();
                                                if width_of(line_start, next) <= wrap_w_px + 0.5 {
                                                    cut = next;
                                                } else {
                                                    break;
                                                }
                                            }
                                            if cut == line_start
                                                && let Some((ofs, gr)) =
                                                    tok.grapheme_indices(true).next()
                                            {
                                                cut = tok_abs_start + ofs + gr.len();
                                            }
                                            out.push((line_start, cut));
                                            line_start = cut;
                                        }
                                        if let Some(ml) = max_lines_opt
                                            && out.len() >= ml
                                        {
                                            t = true;
                                            break;
                                        }
                                        best_break = line_start;
                                    }
                                    if !t
                                        && line_start < end
                                        && max_lines_opt.is_none_or(|ml| out.len() < ml)
                                    {
                                        out.push((line_start, end));
                                    }
                                    (out, t)
                                };
                            for (i, ch) in text.char_indices() {
                                if ch == '\n' {
                                    let (mut ranges, tr) = wrap_hard(
                                        line0_start,
                                        i,
                                        max_lines_remaining(all_ranges.len()),
                                        &width_of,
                                    );
                                    all_ranges.append(&mut ranges);
                                    if tr {
                                        truncated = true;
                                        break;
                                    }
                                    line0_start = i + 1;
                                    if let Some(ml) = max_lines
                                        && all_ranges.len() >= *ml
                                    {
                                        truncated = true;
                                        break;
                                    }
                                }
                            }
                            if !truncated {
                                let (mut ranges, tr) = wrap_hard(
                                    line0_start,
                                    text.len(),
                                    max_lines_remaining(all_ranges.len()),
                                    &width_of,
                                );
                                all_ranges.append(&mut ranges);
                                truncated = tr;
                            }
                            if all_ranges.is_empty() {
                                all_ranges.push((0, 0));
                            }
                            let mut lns: Vec<String> = all_ranges
                                .iter()
                                .map(|&(s, e)| text[s..e].to_string())
                                .collect();
                            if truncated
                                && matches!(overflow, TextOverflow::Ellipsis)
                                && let Some(last) = lns.last_mut()
                            {
                                let with_tail = format!("{}…", last);
                                *last = repose_text::ellipsize_line(
                                    &with_tail,
                                    size_px_val,
                                    wrap_w_px,
                                    fw,
                                    fs,
                                    *letter_spacing,
                                    fvs,
                                );
                            }
                            (lns, all_ranges)
                        }
                    } else {
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
                        if truncated
                            && matches!(overflow, TextOverflow::Ellipsis)
                            && let Some(last) = lns.last_mut()
                        {
                            let with_tail = format!("{}…", last);
                            *last = repose_text::ellipsize_line(
                                &with_tail,
                                size_px_val,
                                wrap_w_px,
                                fw,
                                fs,
                                *letter_spacing,
                                fvs,
                            );
                        }
                        (lns, ranges)
                    }
                } else if matches!(overflow, TextOverflow::Ellipsis) {
                    let elided = repose_text::ellipsize_line(
                        text,
                        size_px_val,
                        wrap_w_px,
                        fw,
                        fs,
                        *letter_spacing,
                        fvs,
                    );
                    let elided_len = elided.len();
                    (vec![elided], vec![(0, elided_len)])
                } else {
                    let len = text.len();
                    (vec![text.clone()], vec![(0, len)])
                };

                let (line_widths, line_heights): (Vec<f32>, Vec<f32>) = if has_annotations {
                    let annos = annotations.as_ref().unwrap();
                    let mut widths = Vec::with_capacity(lines.len());
                    let mut heights = Vec::with_capacity(lines.len());
                    for (idx, _) in lines.iter().enumerate() {
                        let (s, e) = line_ranges[idx];
                        let w = annotated_width(s, e);
                        widths.push(w);
                        let mut max_h = line_h_px_val;
                        for span in annos.iter().filter(|sp| sp.start < e && sp.end > s) {
                            let seg_font_dp = span.style.font_size.unwrap_or(*font_dp);
                            let seg_px = font_px(seg_font_dp);
                            let seg_line_h = if let Some(lh) = span.style.line_height {
                                if lh > 0.0 { font_px(lh) } else { seg_px }
                            } else if *line_height > 0.0 {
                                font_px(*line_height)
                            } else {
                                seg_px
                            };
                            let baseline = span
                                .style
                                .baseline_shift
                                .map(|b| b.0 * seg_px)
                                .unwrap_or(0.0);
                            let stroke_expand =
                                if let Some(repose_core::DrawStyle::Stroke { width, .. }) =
                                    &span.style.draw_style
                                {
                                    *width * seg_px * 0.5
                                } else {
                                    0.0
                                };
                            let top = baseline.min(0.0) - stroke_expand - 2.0;
                            let bottom = baseline.max(0.0) + seg_line_h + stroke_expand + 6.0;
                            let seg_h = bottom - top;
                            max_h = max_h.max(seg_h);
                            max_h = max_h.max(seg_px + baseline.abs() + stroke_expand * 2.0 + 8.0);
                        }
                        heights.push(max_h);
                    }
                    (widths, heights)
                } else {
                    let ws: Vec<f32> = lines
                        .iter()
                        .map(|line| {
                            measure_text(
                                line,
                                size_px_val,
                                TextMeasureConfig {
                                    font_family: *font_family,
                                    font_weight: fw,
                                    font_style: fs,
                                    letter_spacing: *letter_spacing,
                                    ..Default::default()
                                },
                            )
                            .positions
                            .last()
                            .copied()
                            .unwrap_or(0.0)
                        })
                        .collect();
                    let regular_line_h = line_h_px_val + 8.0;
                    let hs = vec![regular_line_h; lines.len()];
                    (ws, hs)
                };
                let max_line_w = line_widths.iter().copied().fold(0.0f32, f32::max);
                let cache_line_h = if has_annotations {
                    line_heights.iter().copied().fold(0.0f32, f32::max)
                } else {
                    line_h_px_val + 8.0
                };
                let total_h: f32 = if has_annotations {
                    line_heights.iter().copied().sum()
                } else {
                    cache_line_h * lines.len().max(1) as f32
                };

                if let Some(node_id) = reverse_map.get(&taffy_node) {
                    text_cache.insert(
                        *node_id,
                        TextLayout {
                            lines: lines.clone(),
                            line_ranges,
                            size_px: size_px_val,
                            line_h_px: cache_line_h,
                            line_widths,
                            line_heights: if has_annotations {
                                line_heights.clone()
                            } else {
                                vec![]
                            },
                        },
                    );
                }
                taffy::geometry::Size {
                    width: max_line_w,
                    height: total_h,
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
