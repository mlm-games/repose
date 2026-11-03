use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use repose_core::*;
use repose_ui::Box as RBox;
use repose_ui::Text;
use repose_ui::ViewExt;
use repose_ui_test::UiTest;

#[test]
fn movable_content_preserves_state_across_locations() {
    let moved = Rc::new(Cell::new(false));
    let movable = MovableContent::new(|label: &'static str| {
        let count = remember_mutable(|| 0u32);
        count.update(|value| *value += 1);
        Text(format!("{label}:{}", *count.get()))
            .modifier(Modifier::new().test_tag("movable"))
    });

    let content = {
        let moved = moved.clone();
        let movable = movable.clone();
        move || {
            if moved.get() {
                RBox(Modifier::new().size(Dp(200.0), Dp(100.0)))
                    .child(movable.invoke("moved"))
            } else {
                RBox(Modifier::new().size(Dp(100.0), Dp(100.0)))
                    .child(movable.invoke("start"))
            }
        }
    };
    let ui = UiTest::new((200, 100), content);
    assert_eq!(
        ui.on_node_with_tag("movable").text().as_deref(),
        Some("start:1")
    );

    moved.set(true);
    ui.pump();
    assert_eq!(
        ui.on_node_with_tag("movable").text().as_deref(),
        Some("moved:2"),
        "movable content reset its remembered state"
    );
}

fn moving_box(state: &SharedElementState, x: f32, tag: &str) -> View {
    RBox(
        Modifier::new()
            .absolute()
            .offset_left(Dp(x))
            .offset_top(Dp(0.0))
            .size(Dp(50.0), Dp(50.0))
            .background(Color::from_rgb(40, 90, 200))
            .on_pointer_down(|_| {})
            .shared_element(state.clone())
            .test_tag(tag.to_string()),
    )
}

#[test]
fn shared_element_animates_to_its_new_layout_position() {
    let state = SharedElementState::new();
    let moved = Rc::new(Cell::new(false));
    let content = {
        let state = state.clone();
        let moved = moved.clone();
        move || {
            let x = if moved.get() { 100.0 } else { 0.0 };
            RBox(Modifier::new().size(Dp(300.0), Dp(100.0)))
                .child(moving_box(&state, x, "moving"))
        }
    };
    let ui = UiTest::new((300, 100), content);
    assert!(ui.on_node_with_tag("moving").bounds_in_window().x.abs() < 0.5);

    moved.set(true);
    ui.advance_time_by(Duration::from_millis(50));
    let mid = ui.on_node_with_tag("moving").bounds_in_window();
    assert!(
        mid.x > 0.0 && mid.x < 100.0,
        "shared element did not animate: {}",
        mid.x
    );

    ui.advance_until_idle();
    let settled = ui.on_node_with_tag("moving").bounds_in_window();
    assert!(
        (settled.x - 100.0).abs() < 0.5,
        "shared element landed at {}",
        settled.x
    );
}

#[test]
fn one_shared_state_paints_only_the_last_claim() {
    let state = SharedElementState::new();
    let both = Rc::new(Cell::new(false));
    let content = {
        let state = state.clone();
        let both = both.clone();
        move || {
            let mut root = RBox(Modifier::new().size(Dp(300.0), Dp(100.0)));
            root = root.child(moving_box(&state, 0.0, "copy-a"));
            if both.get() {
                root = root.child(moving_box(&state, 100.0, "copy-b"));
            }
            root
        }
    };
    let ui = UiTest::new((300, 100), content);
    ui.on_node_with_tag("copy-a").assert_is_displayed();
    assert!(ui.on_node_with_tag("copy-a").bounds_in_window().x.abs() < 0.5);

    both.set(true);
    ui.advance_time_by(Duration::from_millis(30));

    let frame = ui.frame();
    let tags: Vec<_> = frame
        .semantics_nodes
        .iter()
        .filter_map(|node| node.test_tag.as_deref())
        .collect();
    assert!(!tags.contains(&"copy-a"), "the source copy stayed painted");
    assert!(tags.contains(&"copy-b"), "the target copy is missing");
    let target = ui.on_node_with_tag("copy-b").bounds_in_window();
    assert!(
        (0.0..100.0).contains(&target.x),
        "target did not travel from the source: {}",
        target.x
    );
}
