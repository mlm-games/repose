use repose_core::*;
use repose_material::material3::{Tab, TabRow, TabRowConfig};
use repose_ui_test::UiTest;
use std::rc::Rc;

fn tabs() -> Vec<Tab> {
    (0..3)
        .map(|i| Tab {
            label: format!("Tab{i}"),
            icon: None,
            on_click: Rc::new(|| {}),
            enabled: true,
            interaction_source: None,
        })
        .collect()
}

#[test]
fn indicator_offset_uses_density_scaled_tab_width() {
    repose_core::set_density_default(Density { scale: 2.0 });
    let marker = Color::from_rgb(255, 0, 0);
    let config = TabRowConfig {
        indicator_color: marker,
        ..Default::default()
    };
    let harness = UiTest::new((600, 100), move || TabRow(1, tabs(), config.clone()));
    harness.pump();
    harness.advance_time_by(std::time::Duration::from_millis(600));
    let rects: Vec<Rect> = harness
        .frame()
        .scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Rect {
                rect,
                brush: Brush::Solid(c),
                ..
            } if *c == marker => Some(*rect),
            _ => None,
        })
        .collect();
    repose_core::set_density_default(Density { scale: 1.0 });
    assert_eq!(rects.len(), 1);
    let center = rects[0].x + rects[0].w / 2.0;
    assert!(
        (center - 300.0).abs() < 2.0,
        "indicator must be centered on tab 1's 200px cell: {:?}",
        rects[0]
    );
}
