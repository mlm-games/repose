use repose_core::view::View;
use repose_core::*;
use repose_material::material3::{
    ScrollableTabRow, ScrollableTabRowConfig, SecondaryTabRow, Tab, TabRow, TabRowConfig,
};
use repose_ui_test::UiTest;
use std::rc::Rc;

fn tabs() -> Vec<Tab> {
    ["A", "B", "C"]
        .iter()
        .map(|label| Tab {
            label: (*label).to_string(),
            icon: None,
            on_click: Rc::new(|| {}),
            enabled: true,
            interaction_source: None,
        })
        .collect()
}

fn config() -> TabRowConfig {
    TabRowConfig {
        container_color: Color::TRANSPARENT,
        indicator_color: Color::from_rgb(255, 0, 0),
        ..Default::default()
    }
}

fn settled_indicator(build: impl Fn() -> View + 'static) -> Rect {
    let harness = UiTest::new((300, 48), build);
    harness.pump();
    harness.advance_time_by(std::time::Duration::from_millis(600));
    let marker = Color::from_rgb(255, 0, 0);
    let mut rects: Vec<Rect> = harness
        .frame()
        .scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Rect {
                rect,
                brush: Brush::Solid(color),
                ..
            } if *color == marker => Some(*rect),
            _ => None,
        })
        .collect();
    assert_eq!(rects.len(), 1, "one shared indicator, not one per tab");
    rects.remove(0)
}

#[test]
fn tab_indicator_centers_content_width_under_the_label() {
    let indicator = settled_indicator(|| TabRow(1, tabs(), config()));
    assert!(
        indicator.w >= 24.0 && indicator.w < 100.0,
        "primary indicator matches content width: {indicator:?}"
    );
    assert!(
        ((indicator.x + indicator.w / 2.0) - 150.0).abs() < 1.0,
        "indicator must be centered on tab 1: {indicator:?}"
    );
    assert!(
        indicator.y > 40.0,
        "indicator must sit at the bottom: {indicator:?}"
    );
}

#[test]
fn secondary_tab_indicator_fills_the_tab_cell() {
    let indicator = settled_indicator(|| SecondaryTabRow(2, tabs(), config()));
    assert!(
        (indicator.w - 100.0).abs() < 1.0,
        "secondary indicator fills the cell: {indicator:?}"
    );
    assert!(
        (indicator.x - 200.0).abs() < 1.0,
        "secondary indicator starts at the cell: {indicator:?}"
    );
}

fn many_tabs() -> Vec<Tab> {
    (0..8)
        .map(|i| Tab {
            label: format!("T{i}"),
            icon: None,
            on_click: Rc::new(|| {}),
            enabled: true,
            interaction_source: None,
        })
        .collect()
}

fn scrollable_config() -> ScrollableTabRowConfig {
    ScrollableTabRowConfig {
        container_color: Color::TRANSPARENT,
        indicator_color: Color::from_rgb(255, 0, 0),
        ..Default::default()
    }
}

#[test]
fn scrollable_tab_indicator_stays_within_the_selected_tab() {
    let indicator = settled_indicator(|| ScrollableTabRow(3, many_tabs(), scrollable_config()));
    assert!(
        indicator.w > 0.0 && indicator.w <= 90.0,
        "indicator fits a tab: {indicator:?}"
    );
    assert!(
        indicator.x >= 0.0 && indicator.x + indicator.w <= 300.0,
        "indicator stays inside the viewport: {indicator:?}"
    );
}
