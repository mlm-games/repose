use repose_core::*;
use repose_ui::layout::LayoutEngine;
use repose_ui::{
    Box as RBox, Interactions, LazyColumn, LazyColumnConfig, LazyColumnState, LazyRow,
    LazyRowConfig, LazyRowState,
};
use std::collections::HashMap;
use std::rc::Rc;

fn visible_data(scene: &Scene) -> Vec<u8> {
    let mut out: Vec<u8> = scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Rect {
                rect,
                brush: Brush::Solid(color),
                ..
            } if rect.h > 1.0 && rect.y >= -10.0 && rect.y < 95.0 => Some(color.0 / 30),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn scrolled_visible(state: &Rc<LazyColumnState>, reverse: bool, delta_y: f32) -> Vec<u8> {
    state.set_vp_height(90.0);
    let list = LazyColumn(
        (0..6u64).collect::<Vec<_>>(),
        30.0_f32,
        |item: &u64| *item,
        |item: u64, _| {
            RBox(
                Modifier::new()
                    .fill_max_width()
                    .height(Dp(30.0))
                    .background(Color::from_rgb(item as u8 * 30, 0, 0)),
            )
        },
        LazyColumnConfig {
            state: state.clone(),
            modifier: Modifier::new().fill_max_size(),
            reverse_layout: reverse,
            ..Default::default()
        },
    );
    let mut engine = LayoutEngine::new();
    let (_, hits, _) = engine.layout_frame(
        &list,
        (100, 90),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    let hit = hits
        .iter()
        .find(|h| h.on_scroll.is_some())
        .expect("scroll hit region");
    (hit.on_scroll.as_ref().unwrap())(Vec2 { x: 0.0, y: delta_y });
    let (scene, _, _) = engine.layout_frame(
        &list,
        (100, 90),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    visible_data(&scene)
}

#[test]
fn reverse_lazy_column_scrolls_toward_older_items_on_drag_down() {
    let states: Vec<Rc<LazyColumnState>> =
        (0..3).map(|_| Rc::new(LazyColumnState::new())).collect();
    assert_eq!(scrolled_visible(&states[0], false, 45.0), vec![2, 3, 4]);
    assert_eq!(scrolled_visible(&states[1], true, -45.0), vec![1, 2, 3]);
    assert_eq!(scrolled_visible(&states[2], true, 45.0), vec![0, 1]);
}

fn scrolled_visible_row(state: &Rc<LazyRowState>, reverse: bool, delta_x: f32) -> Vec<u8> {
    state.set_vp_width(100.0);
    let list = LazyRow(
        (0..6u64).collect::<Vec<_>>(),
        30.0_f32,
        |item: u64, _| {
            RBox(
                Modifier::new()
                    .fill_max_height()
                    .width(Dp(30.0))
                    .background(Color::from_rgb(item as u8 * 30, 0, 0)),
            )
        },
        LazyRowConfig {
            state: state.clone(),
            modifier: Modifier::new().fill_max_size(),
            reverse_layout: reverse,
            ..Default::default()
        },
    );
    let mut engine = LayoutEngine::new();
    let (_, hits, _) = engine.layout_frame(
        &list,
        (100, 90),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    let hit = hits
        .iter()
        .find(|h| h.on_scroll.is_some())
        .expect("scroll hit region");
    (hit.on_scroll.as_ref().unwrap())(Vec2 { x: delta_x, y: 0.0 });
    let (scene, _, _) = engine.layout_frame(
        &list,
        (100, 90),
        &HashMap::new(),
        &Interactions::default(),
        None,
    );
    let mut out: Vec<u8> = scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Rect {
                rect,
                brush: Brush::Solid(color),
                ..
            } if rect.w > 1.0 && rect.x >= -10.0 && rect.x < 95.0 => Some(color.0 / 30),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[test]
fn reverse_lazy_row_scrolls_toward_older_items_on_drag_left() {
    let states: Vec<Rc<LazyRowState>> = (0..3).map(|_| Rc::new(LazyRowState::new())).collect();
    assert_eq!(scrolled_visible_row(&states[0], false, 45.0), vec![2, 3, 4]);
    assert_eq!(
        scrolled_visible_row(&states[1], true, -45.0),
        vec![1, 2, 3, 4]
    );
    assert_eq!(scrolled_visible_row(&states[2], true, 45.0), vec![0, 1]);
}
