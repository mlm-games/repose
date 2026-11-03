use std::cell::Cell;
use std::rc::Rc;

use repose_core::*;
use repose_ui::Box as RBox;
use repose_ui::pager::{HorizontalPager, PagerConfig, PagerState};
use repose_ui_test::UiTest;

fn pager(state: Rc<PagerState>) -> View {
    HorizontalPager(
        "pager",
        state,
        |page| {
            RBox(
                Modifier::new()
                    .fill_max_size()
                    .background(Color::from_rgb(page as u8 * 10 + 1, 0, 0)),
            )
        },
        PagerConfig {
            modifier: Modifier::new()
                .size(Dp(300.0), Dp(200.0))
                .test_tag("pager"),
            ..Default::default()
        },
    )
}

fn page_rects(ui: &UiTest) -> Vec<(Color, Rect)> {
    ui.frame()
        .scene
        .nodes
        .iter()
        .filter_map(|node| match node {
            SceneNode::Rect { rect, brush, .. } => match brush {
                Brush::Solid(color) if color.1 == 0 && color.2 == 0 => Some((*color, *rect)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

fn page_rect(ui: &UiTest, page: u8) -> Rect {
    page_rects(ui)
        .into_iter()
        .find(|(color, _)| color.0 == page * 10 + 1)
        .map(|(_, rect)| rect)
        .unwrap_or_else(|| panic!("page {page} not painted"))
}

fn ui_with_pager(state: &Rc<PagerState>) -> UiTest {
    let state = state.clone();
    UiTest::new((300, 200), move || pager(state.clone()))
}

#[test]
fn pages_are_laid_out_around_the_position() {
    let state = Rc::new(PagerState::new(3));
    let ui = ui_with_pager(&state);
    assert!(
        page_rect(&ui, 0).x.abs() < 0.5,
        "page 0 at {}",
        page_rect(&ui, 0).x
    );

    state.scroll_to_page(1);
    ui.pump();
    assert!(
        page_rect(&ui, 1).x.abs() < 0.5,
        "page 1 at {}",
        page_rect(&ui, 1).x
    );
}

#[test]
fn drag_moves_pages_with_the_finger() {
    let state = Rc::new(PagerState::new(3));
    let ui = ui_with_pager(&state);

    ui.on_node_with_tag("pager").perform_touch_input(|touch| {
        touch.down_center();
        touch.move_by_steps(Vec2 { x: -150.0, y: 0.0 }, 40);
        touch.pump();
        assert!(
            (state.position() - 0.5).abs() < 1e-4,
            "{}",
            state.position()
        );
        let first = page_rect(touch.test(), 0);
        let second = page_rect(touch.test(), 1);
        assert!((first.x + 150.0).abs() < 0.5, "page 0 at {}", first.x);
        assert!((second.x - 150.0).abs() < 0.5, "page 1 at {}", second.x);
        touch.up();
    });
    ui.advance_until_idle();
    assert_eq!(state.current_page(), 1);
}

#[test]
fn a_fast_flick_advances_even_below_halfway() {
    let state = Rc::new(PagerState::new(3));
    let ui = ui_with_pager(&state);

    ui.on_node_with_tag("pager").perform_touch_input(|touch| {
        touch.down_center();
        touch.move_by(Vec2 { x: -30.0, y: 0.0 });
        assert!(state.position() < 0.2, "{}", state.position());
        touch.up();
    });
    ui.advance_until_idle();
    assert_eq!(state.current_page(), 1);
}

#[test]
fn a_fast_flick_back_returns_to_the_previous_page() {
    let state = Rc::new(PagerState::new(3));
    state.scroll_to_page(2);
    let ui = ui_with_pager(&state);

    ui.on_node_with_tag("pager").perform_touch_input(|touch| {
        touch.down_center();
        touch.move_by(Vec2 { x: 30.0, y: 0.0 });
        assert!(state.position() > 1.8, "{}", state.position());
        touch.up();
    });
    ui.advance_until_idle();
    assert_eq!(state.current_page(), 1);
}

#[test]
fn a_tap_during_a_snap_settles_on_a_page() {
    let state = Rc::new(PagerState::new(3));
    let ui = ui_with_pager(&state);
    state.animate_scroll_to_page(2, AnimationSpec::spring_gentle());
    ui.advance_time_by(std::time::Duration::from_millis(50));
    assert!(state.position() > 0.0 && state.position() < 2.0);

    ui.on_node_with_tag("pager").perform_click();
    ui.advance_until_idle();

    let position = state.position();
    assert!(
        (position - position.round()).abs() < 1e-3,
        "left stranded at {position}"
    );
}

#[test]
fn release_below_halfway_snaps_back() {
    let state = Rc::new(PagerState::new(3));
    let ui = ui_with_pager(&state);

    ui.on_node_with_tag("pager").perform_touch_input(|touch| {
        touch.down_center();
        touch.move_by_steps(Vec2 { x: -60.0, y: 0.0 }, 10);
        touch.up();
    });
    ui.advance_until_idle();

    assert!(!state.is_animating(), "snap did not settle");
    assert!((state.position()).abs() < 1e-3, "{}", state.position());
    assert!(page_rect(&ui, 0).x.abs() < 0.5, "page 0 moved");
}

#[test]
fn page_state_survives_a_drag() {
    let state = Rc::new(PagerState::new(3));
    let last_build = Rc::new(Cell::new(0usize));
    let build = {
        let state = state.clone();
        let last_build = last_build.clone();
        move || {
            let state = state.clone();
            let last_build = last_build.clone();
            HorizontalPager(
                "stateful",
                state,
                move |page| {
                    let count =
                        remember_state_with_key(format!("pager_page_builds:{page}"), || 0usize);
                    *count.borrow_mut() += 1;
                    if page == 0 {
                        last_build.set(*count.borrow());
                    }
                    RBox(
                        Modifier::new()
                            .fill_max_size()
                            .background(Color::from_rgb(page as u8, 0, 0)),
                    )
                },
                PagerConfig {
                    modifier: Modifier::new().size(Dp(300.0), Dp(200.0)),
                    ..Default::default()
                },
            )
        }
    };

    let ui = UiTest::new((300, 200), build);
    assert_eq!(last_build.get(), 1);
    state.scroll_by_pages(0.5);
    ui.pump();
    assert_eq!(last_build.get(), 2, "page state reset during drag");
    state.scroll_by_pages(0.5);
    ui.pump();
    assert_eq!(last_build.get(), 3, "page state reset after drag");
}
