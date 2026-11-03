use std::rc::Rc;

use repose_core::*;
use repose_navigation::*;
use repose_ui::Box as RBox;
use repose_ui_test::UiTest;

fn page(label: &'static str) -> View {
    RBox(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_rgb(20, 20, 20))
            .test_tag(label),
    )
}

fn entry_renderer() -> EntryRenderer<String> {
    renderer(|entry: &EntryScope<String>| match entry.key().as_str() {
        "list" => page("list"),
        "detail" => page("detail"),
        _ => page("dialog"),
    })
}

fn stack_with(keys: &[&str]) -> Rc<NavBackStack<String>> {
    let stack = remember_back_stack(keys[0].to_string());
    if stack.size() >= keys.len() {
        return stack;
    }
    let nav = Navigator {
        stack: (*stack).clone(),
    };
    for key in &keys[stack.size()..] {
        nav.push((*key).to_string());
    }
    stack
}

fn display(strategy: Rc<dyn SceneStrategy<String>>, keys: &'static [&'static str]) -> View {
    NavDisplayWithStrategy(
        stack_with(keys),
        strategy,
        entry_renderer(),
        None,
        NavTransition::default(),
    )
}

#[test]
fn list_detail_shows_both_entries() {
    let ui = UiTest::new((900, 600), || {
        display(Rc::new(ListDetailSceneStrategy::always()), &["list", "detail"])
    });
    ui.advance_until_idle();
    ui.on_node_with_tag("list").assert_is_displayed();
    ui.on_node_with_tag("detail").assert_is_displayed();
    let left = ui.on_node_with_tag("list").bounds_in_window();
    let right = ui.on_node_with_tag("detail").bounds_in_window();
    assert!(left.x < right.x, "panes are not side by side");
    assert!(
        right.x >= left.x + left.w - 0.5,
        "panes overlap: left {}..{}, right {}..{}",
        left.x,
        left.x + left.w,
        right.x,
        right.x + right.w
    );
}

#[test]
fn narrow_list_detail_falls_back_to_the_top_entry() {
    let ui = UiTest::new((400, 600), || {
        display(Rc::new(ListDetailSceneStrategy::single()), &["list", "detail"])
    });
    ui.advance_until_idle();
    ui.on_node_with_tag("detail").assert_is_displayed();
    ui.on_node_with_tag("list").assert_does_not_exist();
}

#[test]
fn dialog_keeps_the_area_scene_underneath() {
    let strategy = Rc::new(DialogSceneStrategy::single_pane(|key: &String| key == "dialog"));
    let ui = UiTest::new((600, 400), move || {
        display(strategy.clone(), &["list", "dialog"])
    });
    ui.advance_until_idle();
    ui.on_node_with_tag("list").assert_is_displayed();
    ui.on_node_with_tag("dialog").assert_is_displayed();
}
