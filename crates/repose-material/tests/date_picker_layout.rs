use repose_core::*;
use repose_material::material3::{DatePicker, DatePickerConfig, DatePickerState};
use repose_ui_test::UiTest;
use std::rc::Rc;

#[test]
fn calendar_columns_are_spread_across_the_container() {
    let state: Rc<DatePickerState> = Rc::new(DatePickerState::new(2026, 11, 23));
    let harness = UiTest::new((400, 600), move || {
        DatePicker(
            state.clone(),
            Rc::new(|_, _, _| {}),
            Rc::new(|| {}),
            DatePickerConfig::default(),
        )
    });
    harness.pump();
    let columns: Vec<(String, f32)> = harness
        .frame()
        .scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Text { rect, text, .. }
                if ["\"M\"", "\"T\"", "\"W\"", "\"F\"", "\"S\""]
                    .contains(&format!("{text:?}").as_str()) =>
            {
                Some((text.to_string(), rect.x))
            }
            _ => None,
        })
        .collect();
    assert!(
        columns.iter().any(|(_, x)| *x < 60.0),
        "weekday headers must start near the leading edge: {columns:?}"
    );
    assert!(
        columns.iter().any(|(_, x)| *x > 300.0),
        "weekday headers must reach the trailing edge: {columns:?}"
    );
}
