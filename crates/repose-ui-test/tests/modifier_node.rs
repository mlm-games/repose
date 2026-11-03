use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use repose_core::*;
use repose_ui::Box as RBox;
use repose_ui_test::UiTest;

struct CounterNode {
    updates: Rc<Cell<u32>>,
    attached: Rc<Cell<u32>>,
    detaches: Rc<Cell<u32>>,
}

impl ModifierNode for CounterNode {
    fn on_attach(&self) {
        self.attached.set(self.attached.get() + 1);
    }
    fn on_detach(&self) {
        self.detaches.set(self.detaches.get() + 1);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct CounterElement {
    updates: Rc<Cell<u32>>,
    attached: Rc<Cell<u32>>,
    detaches: Rc<Cell<u32>>,
}

impl ModifierNodeElement for CounterElement {
    fn create(&self) -> Rc<dyn ModifierNode> {
        Rc::new(CounterNode {
            updates: self.updates.clone(),
            attached: self.attached.clone(),
            detaches: self.detaches.clone(),
        })
    }

    fn update(&self, node: &Rc<dyn ModifierNode>) {
        let Some(node) = node.as_any().downcast_ref::<CounterNode>() else {
            return;
        };
        node.updates.set(node.updates.get() + 1);
    }
}

#[test]
fn modifier_node_state_survives_recomposition_and_detaches_once() {
    let updates = Rc::new(Cell::new(0));
    let attached = Rc::new(Cell::new(0));
    let detaches = Rc::new(Cell::new(0));
    let present = Rc::new(Cell::new(true));
    let content = {
        let updates = updates.clone();
        let attached = attached.clone();
        let detaches = detaches.clone();
        let present = present.clone();
        move || {
            let mut modifier = Modifier::new().size(Dp(100.0), Dp(100.0));
            if present.get() {
                modifier = modifier.node(CounterElement {
                    updates: updates.clone(),
                    attached: attached.clone(),
                    detaches: detaches.clone(),
                });
            }
            RBox(modifier)
        }
    };
    let ui = UiTest::new((200, 200), content);
    ui.pump();
    ui.pump();

    assert_eq!(attached.get(), 1, "node attached more than once");
    assert_eq!(updates.get(), 2, "node was rebuilt instead of updated");
    assert_eq!(detaches.get(), 0);

    present.set(false);
    ui.pump();
    assert_eq!(detaches.get(), 1, "node did not detach on removal");
}

fn gesture_ui(
    make: Rc<dyn Fn() -> Modifier>,
) -> UiTest {
    UiTest::new((200, 100), move || {
        let modifier = make().fill_max_size().test_tag("target");
        RBox(modifier)
    })
}

#[test]
fn tap_gestures_report_a_tap() {
    let taps = Rc::new(Cell::new(0));
    let make = {
        let taps = taps.clone();
        Rc::new(move || {
            let taps = taps.clone();
            Modifier::new().pointer_input(TapGestures::new().on_tap(move |_| {
                taps.set(taps.get() + 1);
            }))
        })
    };
    let ui = gesture_ui(make);
    ui.on_node_with_tag("target").perform_click();
    assert_eq!(taps.get(), 1);
}

#[test]
fn double_tap_waits_for_the_window_and_wins() {
    let taps = Rc::new(Cell::new(0));
    let doubles = Rc::new(Cell::new(0));
    let make = {
        let taps = taps.clone();
        let doubles = doubles.clone();
        Rc::new(move || {
            let taps = taps.clone();
            let doubles = doubles.clone();
            Modifier::new().pointer_input(
                TapGestures::new()
                    .on_tap(move |_| taps.set(taps.get() + 1))
                    .on_double_tap(move |_| doubles.set(doubles.get() + 1)),
            )
        })
    };
    let ui = gesture_ui(make);
    let target = ui.on_node_with_tag("target");
    target.perform_click();
    assert_eq!(taps.get(), 0, "first tap fired before the window closed");
    target.perform_click();
    assert_eq!(doubles.get(), 1);
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(taps.get(), 0, "a double tap also reported a tap");
}

#[test]
fn a_lone_tap_fires_when_the_double_tap_window_closes() {
    let taps = Rc::new(Cell::new(0));
    let make = {
        let taps = taps.clone();
        Rc::new(move || {
            let taps = taps.clone();
            Modifier::new().pointer_input(
                TapGestures::new()
                    .on_tap(move |_| taps.set(taps.get() + 1))
                    .on_double_tap(|_| {}),
            )
        })
    };
    let ui = gesture_ui(make);
    ui.on_node_with_tag("target").perform_click();
    ui.advance_time_by(Duration::from_millis(400));
    assert_eq!(taps.get(), 1);
}

#[test]
fn long_press_fires_while_the_finger_is_down() {
    let taps = Rc::new(Cell::new(0));
    let longs = Rc::new(Cell::new(0));
    let make = {
        let taps = taps.clone();
        let longs = longs.clone();
        Rc::new(move || {
            let taps = taps.clone();
            let longs = longs.clone();
            Modifier::new().pointer_input(
                TapGestures::new()
                    .on_tap(move |_| taps.set(taps.get() + 1))
                    .on_long_press(move |_| longs.set(longs.get() + 1)),
            )
        })
    };
    let ui = gesture_ui(make);
    ui.on_node_with_tag("target").perform_touch_input(|input| {
        input.down_center();
        input.test().advance_time_by(Duration::from_millis(600));
        assert_eq!(longs.get(), 1, "long press did not fire during the hold");
        input.up();
    });
    assert_eq!(taps.get(), 0);
}

struct DetachHandler {
    detaches: Rc<Cell<u32>>,
}

impl PointerInputHandler for DetachHandler {
    fn clone_handler(&self) -> Box<dyn PointerInputHandler> {
        Box::new(Self {
            detaches: self.detaches.clone(),
        })
    }
    fn on_event(&mut self, _event: &PointerEvent) -> bool {
        false
    }
    fn on_detach(&mut self) {
        self.detaches.set(self.detaches.get() + 1);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[test]
fn pointer_node_detaches_once_after_the_frame_cache_release() {
    let detaches = Rc::new(Cell::new(0));
    let present = Rc::new(Cell::new(true));
    let content = {
        let detaches = detaches.clone();
        let present = present.clone();
        move || {
            let mut modifier = Modifier::new()
                .size(Dp(100.0), Dp(100.0))
                .test_tag("gesture");
            if present.get() {
                modifier = modifier.pointer_input(DetachHandler {
                    detaches: detaches.clone(),
                });
            }
            RBox(modifier)
        }
    };
    let ui = UiTest::new((200, 200), content);
    ui.pump();
    ui.on_node_with_tag("gesture").perform_click();
    assert_eq!(detaches.get(), 0);

    present.set(false);
    ui.pump();
    ui.pump();
    assert_eq!(detaches.get(), 1, "gesture node detached more than once");
}

#[test]
fn a_stale_first_tap_does_not_become_a_double_tap() {
    let doubles = Rc::new(Cell::new(0));
    let make = {
        let doubles = doubles.clone();
        Rc::new(move || {
            let doubles = doubles.clone();
            Modifier::new().pointer_input(TapGestures::new().on_double_tap(move |_| {
                doubles.set(doubles.get() + 1);
            }))
        })
    };
    let ui = gesture_ui(make);
    ui.on_node_with_tag("target").perform_click();
    ui.advance_time_by(Duration::from_millis(400));
    ui.on_node_with_tag("target").perform_click();
    assert_eq!(doubles.get(), 0, "an expired first tap paired with a new one");
}

#[test]
fn drag_accumulates_across_recomposition() {
    let total = Rc::new(RefCell::new(Vec2::default()));
    let ended = Rc::new(Cell::new(0));
    let make = {
        let total = total.clone();
        let ended = ended.clone();
        Rc::new(move || {
            let total = total.clone();
            let ended = ended.clone();
            Modifier::new().pointer_input(
                DragGestures::new()
                    .on_drag(move |update| *total.borrow_mut() = update.total)
                    .on_end(move |_| ended.set(ended.get() + 1)),
            )
        })
    };
    let ui = gesture_ui(make);
    ui.on_node_with_tag("target").perform_touch_input(|input| {
        input.down_center();
        input.move_by_steps(Vec2 { x: 80.0, y: 0.0 }, 8);
        input.up();
    });
    assert_eq!(ended.get(), 1, "drag never ended");
    assert!(
        (*total.borrow()).x >= 45.0,
        "drag total was {:?}",
        *total.borrow()
    );
}
