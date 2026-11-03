use std::time::Duration;

use repose_core::*;
use repose_navigation::{
    EntryScope, NavDisplay, NavTransition, Navigator, remember_back_stack, renderer,
};
use repose_ui::Box as RBox;
use repose_ui_test::UiTest;

const PAGE_WIDTH: f32 = 400.0;

fn screen(tag: &str, color: Color) -> View {
    RBox(
        Modifier::new()
            .fill_max_size()
            .background(color)
            .on_pointer_down(|_| {})
            .test_tag(tag.to_string()),
    )
}

fn app() -> impl FnMut() -> View {
    || {
        let stack = remember_back_stack("home".to_string());
        if stack.size() == 1 {
            Navigator {
                stack: (*stack).clone(),
            }
            .push("details".to_string());
        }
        NavDisplay(
            stack,
            renderer(|scope: &EntryScope<String>| {
                if scope.key().as_str() == "home" {
                    screen("screen:home", Color::from_rgb(255, 0, 0))
                } else {
                    screen("screen:details", Color::from_rgb(0, 255, 0))
                }
            }),
            None,
            NavTransition::default(),
        )
    }
}

#[test]
fn predictive_back_follows_the_gesture_and_returns_to_rest() {
    let ui = UiTest::new((PAGE_WIDTH as u32, 800), app());
    ui.advance_until_idle();
    ui.on_node_with_tag("screen:details").assert_is_displayed();

    repose_core::back::set_gesture(true, 0.5);
    ui.pump();
    assert!((repose_core::back::progress() - 0.5).abs() < 1e-4);

    let details = ui.on_node_with_tag("screen:details").origin_in_window();
    let home = ui.on_node_with_tag("screen:home").origin_in_window();
    assert!(
        (details.x - PAGE_WIDTH / 2.0).abs() < 0.5,
        "current screen at {}",
        details.x
    );
    assert!(
        (home.x + PAGE_WIDTH / 2.0).abs() < 0.5,
        "previous screen at {}",
        home.x
    );
    assert!(
        (details.x - (home.x + PAGE_WIDTH)).abs() < 0.5,
        "screens do not tile: {home:?} {details:?}"
    );

    repose_core::back::cancel();
    ui.advance_time_by(Duration::from_millis(250));

    assert_eq!(repose_core::back::progress(), 0.0);
    let details = ui.on_node_with_tag("screen:details").origin_in_window();
    assert!(details.x.abs() < 0.5, "screen did not return: {details:?}");
}
