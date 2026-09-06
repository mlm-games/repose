use super::{IntrinsicSizeMode, LayoutEngine};
use crate::Interactions;
use crate::{Box as RBox, Column, Row, Text, TextStyle, ViewExt, row_scope};
use repose_core::*;
use std::collections::HashMap;

#[test]
fn test_render_z_index_paints_last() {
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
                Some(*brush)
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
                Some(*brush)
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
                Some(*brush)
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
        rects[len - 1]
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
                Some(*brush)
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
                Some(*brush)
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
    let root = Column(Modifier::new()).child(crate::Box(Modifier::new().width(320.0)).child(sub));

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
        Column(Modifier::new()).child(RBox(Modifier::new().height(300.0).background(white))),
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

#[test]
fn test_annotated_text_large_font_not_clipped() {
    use repose_core::SpanStyle;
    let big = SpanStyle::default().font_size(24.0);
    let anno = build_annotated_string(|b| {
        b.push("Normal ");
        b.push_with_style("Big", big.clone());
        b.push(" normal");
    });
    let v = crate::AnnotatedText(anno).size(14.0);
    let mut eng = make_engine();
    let (w_anno, h_anno) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
    let (w_base, h_base) = eng.intrinsic_size(
        &crate::Text("Normal Big normal").size(14.0),
        IntrinsicSizeMode::MaxContent,
    );
    assert!(
        w_anno > w_base,
        "annotated big span should be wider: anno={} base={}",
        w_anno,
        w_base
    );
    assert!(
        h_anno >= 24.0 - 1.0,
        "height must fit 24dp big span, got {}",
        h_anno
    );
    assert!(
        h_anno > h_base,
        "height with big span should exceed base 14dp line"
    );
}

#[test]
fn test_annotated_stroke_expands_bounds() {
    use repose_core::{DrawStyle, SpanStyle};
    let s = SpanStyle::default()
        .draw_style(DrawStyle::stroke(0.1))
        .font_size(20.0);
    let anno = build_annotated_string(|b| {
        b.push_with_style("Stroked", s);
    });
    let v = crate::AnnotatedText(anno).size(16.0);
    let mut eng = make_engine();
    let (w, h) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
    let (w0, h0) = eng.intrinsic_size(
        &crate::Text("Stroked").size(16.0),
        IntrinsicSizeMode::MaxContent,
    );
    assert!(w > w0, "stroke should expand width");
    assert!(h > h0, "stroke should expand height");
}

#[test]
fn test_annotated_superscript_expands_height() {
    use repose_core::{BaselineShift, SpanStyle};
    let sp = SpanStyle::default()
        .font_size(14.0)
        .baseline_shift(BaselineShift::Superscript);
    let anno = build_annotated_string(|b| {
        b.push("a");
        b.push_with_style("super", sp);
        b.push("b");
    });
    let v = crate::AnnotatedText(anno).size(16.0);
    let mut eng = make_engine();
    let (_, h) = eng.intrinsic_size(&v, IntrinsicSizeMode::MaxContent);
    assert!(
        h > 16.0,
        "superscript should increase measured height, got {}",
        h
    );
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

    #[test]
    fn test_zstack_unkeyed_text_reuse_no_stale() {
        use crate::{Column, ZStack};
        let bigname_at = |label: &str, gx: f32, gy: f32| {
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding_values(PaddingValues {
                        left: gx - 60.0,
                        top: gy - 11.0,
                        right: 0.0,
                        bottom: 0.0,
                    }),
            )
            .child(
                Column(
                    Modifier::new()
                        .width(120.0)
                        .height(22.0)
                        .justify_content(JustifyContent::CENTER)
                        .align_items(AlignItems::CENTER),
                )
                .child(Text(label.to_string()).size(10.0).font_family("Silkscreen")),
            )
        };
        let normal_at = |label: String, gx: f32, gy: f32| {
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding_values(PaddingValues {
                        left: gx,
                        top: gy,
                        right: 0.0,
                        bottom: 0.0,
                    }),
            )
            .child(
                Column(Modifier::new().width(200.0).height(16.0))
                    .child(Text(label).size(7.0).font_family("Silkscreen")),
            )
        };
        let hitbox_at = |x: f32, y: f32| {
            Column(
                Modifier::new()
                    .fill_max_size()
                    .padding_values(PaddingValues {
                        left: x,
                        top: y,
                        right: 0.0,
                        bottom: 0.0,
                    }),
            )
            .child(Column(
                Modifier::new()
                    .width(24.0)
                    .height(16.0)
                    .clickable()
                    .on_click(|| {}),
            ))
        };
        let make_page = |page: u8| {
            let mut layers: Vec<View> = Vec::new();
            layers.push(Column(
                Modifier::new()
                    .fill_max_size()
                    .background(Color::from_rgba(0, 0, 0, 230)),
            ));
            if page == 0 {
                for i in 0..4 {
                    let gy = 56.0 + i as f32 * 20.0;
                    layers.push(normal_at(format!("VOL {}", i), 80.0, gy));
                    layers.push(normal_at(format!("{}%", 80 + i * 5), 200.0, gy));
                    layers.push(hitbox_at(40.0, gy - 6.0));
                    layers.push(hitbox_at(240.0, gy - 6.0));
                    layers.push(normal_at("<".into(), 44.0, gy));
                    layers.push(normal_at(">".into(), 252.0, gy));
                }
                layers.push(bigname_at("BACK", 160.0, 200.0));
            } else {
                for i in 0..6 {
                    let gy = 48.0 + i as f32 * 18.0;
                    layers.push(normal_at(format!("V Row {}", i), 80.0, gy));
                    layers.push(hitbox_at(40.0, gy - 6.0));
                    layers.push(hitbox_at(240.0, gy - 6.0));
                }
                for (j, label) in ["VIEW CREDITS", "PROFILE", "COLOR"].iter().enumerate() {
                    let gy = 140.0 + j as f32 * 20.0;
                    layers.push(bigname_at(label, 160.0, gy));
                }
                layers.push(bigname_at("BACK", 160.0, 200.0));
            }
            ZStack(Modifier::new().fill_max_size()).child(layers)
        };
        let mut engine = LayoutEngine::new();
        let root0 = Column(Modifier::new().fill_max_size()).child(make_page(0));
        let _ = engine.layout_frame(
            &root0,
            (800, 600),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        let root1 = Column(Modifier::new().fill_max_size()).child(make_page(1));
        let _ = engine.layout_frame(
            &root1,
            (800, 600),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        let _ = engine.layout_frame(
            &root0,
            (800, 600),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        let (scene, _, _) = engine.layout_frame(
            &root1,
            (800, 600),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        let mut backs: Vec<Rect> = Vec::new();
        for node in &scene.nodes {
            if let SceneNode::Text { rect, text, .. } = node
                && text.as_ref() == "BACK"
            {
                backs.push(*rect);
            }
        }
        assert_eq!(backs.len(), 1, "expected 1 BACK, got {:?}", backs);
        let r = backs[0];
        assert!(r.x > 100.0 && r.x < 200.0, "BACK x off: {}", r.x);
        assert!(
            r.y > 150.0 && r.y < 250.0,
            "BACK y off: {} rect {:?}",
            r.y,
            r
        );
        let mut Credits: Vec<Rect> = Vec::new();
        for node in &scene.nodes {
            if let SceneNode::Text { rect, text, .. } = node
                && text.as_ref() == "VIEW CREDITS"
            {
                Credits.push(*rect);
            }
        }
        assert_eq!(Credits.len(), 1);
        let rc = Credits[0];
        assert!(
            rc.y > 100.0 && rc.y < 200.0,
            "VIEW CREDITS y off: {} {:?}",
            rc.y,
            rc
        );
    }
}

#[test]
fn test_row_baseline_alignment() {
    // Compose-style: the RowScope-flagged children form the baseline group
    // and coincide; a lone flagged child would define the group alone and
    // not move, and unflagged siblings keep normal alignment.
    let root = Row(Modifier::new().size(400.0, 200.0)).child(row_scope(|s| {
        vec![
            s.align_by_baseline(Text("Ag").size(32.0).single_line()),
            s.align_by_baseline(Text("Ag").size(16.0).single_line()),
        ]
    }));

    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (400, 200),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    let mut baselines = Vec::new();
    for node in &scene.nodes {
        if let SceneNode::Text { rect, size, .. } = node {
            let (ascent, _) =
                repose_text::primary_font_vertical_metrics(Some("sans-serif"), 400, *size);
            baselines.push(rect.y + ascent);
        }
    }
    assert_eq!(baselines.len(), 2, "expected two text runs: {baselines:?}");
    assert!(
        (baselines[0] - baselines[1]).abs() < 2.0,
        "first baselines should coincide: {baselines:?}"
    );
}

#[test]
fn test_row_baseline_alignment_grows_row() {
    // Compose parity: aligning a multi-line child's first baseline to a
    // deep single-line baseline pushes the child's later lines below the
    // row's natural height, so an auto-sized row must grow to fit them
    // (a fixed-size row keeps its height and clips instead).
    let root = Column(Modifier::new()).child((
        Row(Modifier::new()).child(row_scope(|s| {
            vec![
                s.align_by_baseline(Text("Ag").size(64.0).single_line()),
                s.align_by_baseline(Text("Ag\nAg").size(32.0)),
            ]
        })),
        RBox(Modifier::new().size(400.0, 10.0).background(Color::WHITE)),
    ));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (400, 400),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    // (baseline, top, bottom, height) per text run; the multi-line child
    // paints one run per line, so group by font size.
    let mut big = Vec::new();
    let mut small = Vec::new();
    for node in &scene.nodes {
        if let SceneNode::Text { rect, size, .. } = node {
            let (ascent, _) =
                repose_text::primary_font_vertical_metrics(Some("sans-serif"), 400, *size);
            let run = (rect.y + ascent, rect.y, rect.y + rect.h, rect.h);
            if *size >= 64.0 {
                big.push(run);
            } else {
                small.push(run);
            }
        }
    }
    assert_eq!(big.len(), 1, "expected the 64sp run");
    assert_eq!(small.len(), 2, "expected two 32sp runs, got {small:?}");
    let first_small_baseline = small.iter().map(|r| r.0).fold(f32::INFINITY, f32::min);
    assert!(
        (big[0].0 - first_small_baseline).abs() < 2.5,
        "first baselines should coincide: {big:?} {small:?}"
    );
    let bottom = big
        .iter()
        .chain(small.iter())
        .map(|r| r.2)
        .fold(f32::NEG_INFINITY, f32::max);
    let small_top = small.iter().map(|r| r.1).fold(f32::INFINITY, f32::min);
    let small_bottom = small.iter().map(|r| r.2).fold(f32::NEG_INFINITY, f32::max);
    let tallest = big[0].3.max(small_bottom - small_top);
    // The sentinel box sits directly below the row, so its y is the row height.
    let boxes: Vec<f32> = scene
        .nodes
        .iter()
        .filter_map(|n| match n {
            SceneNode::Rect { rect, .. } => Some(rect.y),
            _ => None,
        })
        .collect();
    assert_eq!(boxes.len(), 1, "expected the sentinel box");
    let row_h = boxes[0];
    assert!(
        row_h > tallest,
        "row should grow beyond its tallest child ({tallest}): {row_h}"
    );
    assert!(
        row_h >= bottom - 1.0,
        "shifted children must fit inside the row (bottom {bottom}): {row_h}"
    );
    assert!(
        row_h <= bottom + 1.0,
        "row should grow exactly enough (bottom {bottom}): {row_h}"
    );
}

#[test]
fn test_intrinsic_and_fit_content_sizing() {
    // `intrinsic_width(Max)` sizes to max-content; `fit_content_width(limit)`
    // shrink-wraps clamped to the limit.
    let wide = Text("hello world hello world").single_line();
    let root = Column(Modifier::new().size(400.0, 400.0))
        .child(RBox(Modifier::new().intrinsic_width(IntrinsicSize::Max)).child(wide));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (400, 400),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(
        !scene.nodes.is_empty(),
        "intrinsic-width subtree should lay out"
    );

    let narrow = Text("hello world hello world").single_line();
    let root = Column(Modifier::new().size(400.0, 400.0))
        .child(RBox(Modifier::new().fit_content_width(50.0)).child(narrow));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (400, 400),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(
        !scene.nodes.is_empty(),
        "fit-content-width subtree should lay out"
    );
}

#[test]
fn test_balanced_flow_row_lays_out() {
    use crate::{FlowColumnConfig, FlowRowConfig};

    let white = Color::WHITE;
    let root = crate::FlowRow(
        Modifier::new().size(300.0, 300.0),
        FlowRowConfig {
            balanced: true,
            min_lines: 2,
            ..Default::default()
        },
    )
    .child(RBox(Modifier::new().size(100.0, 20.0).background(white)))
    .child(RBox(Modifier::new().size(100.0, 20.0).background(white)))
    .child(RBox(Modifier::new().size(100.0, 20.0).background(white)));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (300, 300),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(
        !scene.nodes.is_empty(),
        "balanced flow row should lay out without errors"
    );

    // Same for the vertical variant.
    let root = crate::FlowColumn(
        Modifier::new().size(300.0, 300.0),
        FlowColumnConfig {
            balanced: true,
            min_lines: 2,
            ..Default::default()
        },
    )
    .child(RBox(Modifier::new().size(20.0, 100.0).background(white)));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (300, 300),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(
        !scene.nodes.is_empty(),
        "balanced flow column should lay out without errors"
    );
}

#[test]
fn test_contain_modifiers_lay_out() {
    // Containment must not break layout of ordinary content.
    let white = Color::WHITE;
    let root = Column(Modifier::new().size(200.0, 200.0).contain_content())
        .child(RBox(Modifier::new().size(50.0, 50.0).background(white)))
        .child(
            Column(Modifier::new().contain_layout())
                .child(RBox(Modifier::new().size(30.0, 30.0).background(white))),
        );
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (200, 200),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(!scene.nodes.is_empty(), "contained subtree should lay out");
}

#[test]
fn test_safe_center_alignment_lays_out() {
    use crate::Center;

    // Safe centering keeps content reachable; at minimum it must lay out
    // identically to unsafe centering for fitting content.
    let white = Color::WHITE;
    let root = Center(Modifier::new().size(200.0, 200.0))
        .child(RBox(Modifier::new().size(50.0, 50.0).background(white)));
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (200, 200),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(!scene.nodes.is_empty());

    let safe = Text("hi").single_line();
    let root = Column(
        Modifier::new()
            .size(200.0, 200.0)
            .content_alignment_safe(Alignment::Center),
    )
    .child(safe);
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (200, 200),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(!scene.nodes.is_empty());
}

#[test]
fn test_debug_taffy_subtree_and_grid_summary() {
    use repose_tree::NodeId;

    let white = Color::WHITE;
    // Grid container with 2 columns; devtools helpers must report it.
    let root = Column(Modifier::new().size(300.0, 300.0)).child(
        RBox(Modifier::new().size(300.0, 200.0).grid(2, 4.0, 4.0))
            .child(RBox(Modifier::new().size(50.0, 50.0).background(white)))
            .child(RBox(Modifier::new().size(50.0, 50.0).background(white))),
    );
    let mut eng = make_engine();
    let (scene, _, _) = eng.layout_frame(
        &root,
        (300, 300),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    assert!(!scene.nodes.is_empty());

    // Find the grid container's ViewTree NodeId via its size: it is the only
    // 300x200 node. We walk the engine instead: every mapped node that yields
    // a grid summary must be exactly one.
    let mut grid_nodes = 0;
    let mut other_node = None;
    let ids: Vec<NodeId> = eng.taffy_map.keys().copied().collect();
    for nid in &ids {
        if eng.debug_grid_summary(*nid).is_some() {
            grid_nodes += 1;
        } else {
            other_node = Some(*nid);
        }
    }
    assert_eq!(grid_nodes, 1, "expected exactly one grid container");
    // Non-grid nodes yield no grid summary.
    assert!(
        eng.debug_grid_summary(other_node.expect("leaf node"))
            .is_none()
    );

    // debug_taffy_subtree returns non-empty dumps for mapped nodes.
    let some_id = *ids.first().expect("mapped nodes");
    assert!(!eng.debug_taffy_subtree(some_id).is_empty());
}
