use std::cell::RefCell;
use std::rc::Rc;

use repose_app::{ReposeRuntime, TouchGestureState};
use repose_core::HitRegion;
use repose_core::runtime::Frame;
use repose_core::{Rect, Vec2};

const SCALE: f32 = 2.75;

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect { x, y, w, h }
}

fn click_region(id: u64, r: Rect, on_click: Rc<dyn Fn()>) -> HitRegion {
    HitRegion {
        id,
        rect: r,
        on_click: Some(on_click),
        ..Default::default()
    }
}

fn drag_region(id: u64, r: Rect) -> HitRegion {
    let moved = Rc::new(RefCell::new(0u32));
    HitRegion {
        id,
        rect: r,
        on_pointer_move: Some(Rc::new(move |_ev| *moved.borrow_mut() += 1)),
        ..Default::default()
    }
}

fn scroll_region(id: u64, r: Rect, scrolled: Rc<RefCell<f32>>) -> HitRegion {
    HitRegion {
        id,
        rect: r,
        on_scroll: Some(Rc::new(move |d: Vec2| {
            *scrolled.borrow_mut() += d.y;
            Vec2::default()
        })),
        ..Default::default()
    }
}

fn frame(regions: Vec<HitRegion>) -> Frame {
    Frame {
        scene: Default::default(),
        hit_regions: regions,
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    }
}

#[test]
fn tap_with_finger_jitter_fires_click() {
    let clicks = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(frame(vec![click_region(
        1,
        rect(0.0, 0.0, 400.0, 400.0),
        Rc::new(move || *c.borrow_mut() += 1),
    )]));

    let mut g = TouchGestureState::default();
    let pos = (100.0, 100.0);
    g.touch_started(&mut rt, 0, pos);
    // A real finger dwells before it lifts; the OS streams jitter moves.
    std::thread::sleep(std::time::Duration::from_millis(60));
    g.touch_moved(&mut rt, 0, (pos.0 + 1.0, pos.1), SCALE);
    g.touch_ended(&mut rt, 0, (pos.0 + 1.0, pos.1), false);

    assert_eq!(*clicks.borrow(), 1, "a jittery tap must still click");
}

#[test]
fn dragging_a_drag_surface_does_not_scroll_the_parent() {
    let scrolled = Rc::new(RefCell::new(0.0f32));
    let s = scrolled.clone();
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(frame(vec![
        scroll_region(1, rect(0.0, 0.0, 400.0, 900.0), s),
        drag_region(2, rect(0.0, 0.0, 400.0, 200.0)),
    ]));

    let mut g = TouchGestureState::default();
    g.touch_started(&mut rt, 0, (100.0, 100.0));
    for step in 1..=6 {
        let y = 100.0 + step as f32 * 12.0;
        g.touch_moved(&mut rt, 0, (100.0, y), SCALE);
    }
    g.touch_ended(&mut rt, 0, (100.0, 172.0), false);

    assert_eq!(
        *scrolled.borrow(),
        0.0,
        "a drag on a drag surface must not scroll the page"
    );
}

#[test]
fn dragging_the_page_scrolls() {
    let scrolled = Rc::new(RefCell::new(0.0f32));
    let s = scrolled.clone();
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(frame(vec![scroll_region(
        1,
        rect(0.0, 0.0, 400.0, 900.0),
        s,
    )]));

    let mut g = TouchGestureState::default();
    g.touch_started(&mut rt, 0, (100.0, 500.0));
    for step in 1..=6 {
        let y = 500.0 - step as f32 * 12.0;
        g.touch_moved(&mut rt, 0, (100.0, y), SCALE);
    }
    g.touch_ended(&mut rt, 0, (100.0, 428.0), false);

    assert!(*scrolled.borrow() > 0.0, "page drag must scroll");
}
