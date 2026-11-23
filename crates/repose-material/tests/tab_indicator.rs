use repose_core::view::View;
use repose_core::*;
use repose_material::material3::{SecondaryTabRow, Tab, TabRow, TabRowConfig};
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
fn tab_indicator_follows_tab_content_from_the_cell_start() {
    let indicator = settled_indicator(|| TabRow(1, tabs(), config()));
    assert!(
        indicator.w >= 24.0 && indicator.w < 100.0,
        "primary indicator matches content width: {indicator:?}"
    );
    assert!(
        (indicator.x - 100.0).abs() < 1.0,
        "indicator starts at tab 1 cell left: {indicator:?}"
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
}
