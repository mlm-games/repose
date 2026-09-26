//! Integration tests for the embeddable runtime's event handling core.
//!
//! These drive `ReposeRuntime` with hand-built frames so the behaviour does
//! not depend on the layout engine: Escape fallthrough, built-in text editing
//! shortcuts, and pointer-release click reporting.

use std::cell::RefCell;
use std::rc::Rc;

use repose_app::ReposeRuntime;
use repose_core::input::{Key, KeyEvent, KeyEventType, Modifiers, PhysicalKey, PointerButton};
use repose_core::runtime::{Frame, SemNode};
use repose_core::semantics::Role;
use repose_core::shortcuts::Action;
use repose_core::{CursorIcon, HitRegion, Rect, Scene, Vec2};

const TF_ID: u64 = 100;
const BTN_ID: u64 = 200;

fn textfield_frame(id: u64) -> Frame {
    let hr = HitRegion {
        id,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 30.0,
        },
        focusable: true,
        tf_state_key: Some(id),
        tf_multiline: false,
        tf_enabled: true,
        tf_read_only: false,
        cursor: Some(CursorIcon::Text),
        ..Default::default()
    };

    let rect = hr.rect;
    let sem = SemNode {
        id,
        role: Role::TextField,
        label: Some("Field".into()),
        rect,
        focused: true,
        ..Default::default()
    };

    Frame {
        scene: Scene::default(),
        hit_regions: vec![hr],
        semantics_nodes: vec![sem],
        focus_chain: vec![id],
    }
}

fn key_down(key: Key, modifiers: Modifiers, repeat: bool) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: repeat,
        event_type: KeyEventType::Down,
        utf16_code_point: 0,
        physical: None,
    }
}

#[test]
fn escape_not_consumed_when_idle() {
    let mut rt = ReposeRuntime::new();
    // No focus, no frame cache, no active drag: Escape must fall through so
    // hosts can implement back / quit.
    let ev = key_down(Key::Escape, Modifiers::default(), false);
    assert!(!rt.handle_key(&ev));

    // With a cached frame but no focus and no root key handler, Escape
    // still isn't consumed.
    rt.cache_frame(textfield_frame(TF_ID));
    rt.sched.focused = None;
    assert!(!rt.handle_key(&ev));
}

#[test]
fn release_reports_clicked_id() {
    let mut rt = ReposeRuntime::new();
    let clicked = Rc::new(RefCell::new(false));
    let clicked_cb = clicked.clone();
    let hr = HitRegion {
        id: BTN_ID,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 50.0,
            h: 50.0,
        },
        on_click: Some(Rc::new(move || {
            *clicked_cb.borrow_mut() = true;
        })),
        ..Default::default()
    };
    let frame = Frame {
        scene: Scene::default(),
        hit_regions: vec![hr],
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    };
    rt.cache_frame(frame);

    let pos = Vec2 { x: 10.0, y: 10.0 };
    let _press = rt.handle_pointer_press(pos, PointerButton::Primary);

    let result = rt.handle_pointer_release(pos, PointerButton::Primary);
    assert!(
        result.clicked_id == Some(BTN_ID),
        "clicked_id should be reported: {result:?}"
    );
    assert!(result.needs_a11y_announce);
    assert!(result.consumed);
    assert!(*clicked.borrow(), "on_click should have fired");
    // Runtime state is cleared by the time the host reads the result.
    assert!(rt.capture_id.is_none());
}

#[test]
fn release_click_off_target_reports_nothing() {
    let mut rt = ReposeRuntime::new();
    let hr = HitRegion {
        id: BTN_ID,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 50.0,
            h: 50.0,
        },
        on_click: Some(Rc::new(|| {})),
        ..Default::default()
    };
    let frame = Frame {
        scene: Scene::default(),
        hit_regions: vec![hr],
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    };
    rt.cache_frame(frame);

    // Press inside, release far outside: no click.
    let _press = rt.handle_pointer_press(Vec2 { x: 10.0, y: 10.0 }, PointerButton::Primary);
    let result = rt.handle_pointer_release(
        Vec2 {
            x: 1000.0,
            y: 1000.0,
        },
        PointerButton::Primary,
    );
    assert!(result.clicked_id.is_none());
    assert!(!result.needs_a11y_announce);
}

fn button_frame(
    id: u64,
    on_click: Option<Rc<dyn Fn()>>,
    on_double_click: Option<Rc<dyn Fn()>>,
    on_long_click: Option<Rc<dyn Fn()>>,
) -> Frame {
    let hr = HitRegion {
        id,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        },
        on_click,
        on_double_click,
        on_long_click,
        ..Default::default()
    };
    Frame {
        scene: Scene::default(),
        hit_regions: vec![hr],
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    }
}

#[test]
fn combined_clickable_delays_single_click_when_double_configured() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let doubles = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let d = doubles.clone();
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        Some(Rc::new(move || *d.borrow_mut() += 1)),
        None,
    ));
    let pos = Vec2 { x: 10.0, y: 10.0 };
    rt.handle_pointer_press(pos, PointerButton::Primary);
    rt.handle_pointer_release(pos, PointerButton::Primary);
    assert_eq!(*clicks.borrow(), 0);
    std::thread::sleep(std::time::Duration::from_millis(350));
    rt.poll_gesture_timers();
    assert_eq!(*clicks.borrow(), 1);
    assert_eq!(*doubles.borrow(), 0);
}

#[test]
fn combined_clickable_double_tap_skips_on_click() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let doubles = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let d = doubles.clone();
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        Some(Rc::new(move || *d.borrow_mut() += 1)),
        None,
    ));
    let pos = Vec2 { x: 10.0, y: 10.0 };
    rt.handle_pointer_press(pos, PointerButton::Primary);
    rt.handle_pointer_release(pos, PointerButton::Primary);
    std::thread::sleep(std::time::Duration::from_millis(50));
    rt.handle_pointer_press(pos, PointerButton::Primary);
    rt.handle_pointer_release(pos, PointerButton::Primary);
    assert_eq!(*doubles.borrow(), 1);
    assert_eq!(*clicks.borrow(), 0);
}

#[test]
fn long_press_fires_while_held_and_suppresses_click() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let longs = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let l = longs.clone();
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        None,
        Some(Rc::new(move || *l.borrow_mut() += 1)),
    ));
    let pos = Vec2 { x: 10.0, y: 10.0 };
    rt.handle_pointer_press(pos, PointerButton::Primary);
    std::thread::sleep(std::time::Duration::from_millis(550));
    rt.poll_gesture_timers();
    assert_eq!(*longs.borrow(), 1);
    rt.handle_pointer_release(pos, PointerButton::Primary);
    assert_eq!(*clicks.borrow(), 0);
}

#[test]
fn pending_click_survives_press_on_another_id() {
    let mut rt = ReposeRuntime::new();
    let a_clicks = Rc::new(RefCell::new(0u32));
    let b_clicks = Rc::new(RefCell::new(0u32));
    let ac = a_clicks.clone();
    let bc = b_clicks.clone();
    let a = button_frame(
        BTN_ID,
        Some(Rc::new(move || *ac.borrow_mut() += 1)),
        Some(Rc::new(|| {})),
        None,
    );
    let mut b = button_frame(
        BTN_ID + 1,
        Some(Rc::new(move || *bc.borrow_mut() += 1)),
        None,
        None,
    );
    b.hit_regions[0].rect.x = 200.0;
    rt.cache_frame(Frame {
        scene: Scene::default(),
        hit_regions: [a.hit_regions, b.hit_regions].concat(),
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    });
    let pos_a = Vec2 { x: 10.0, y: 10.0 };
    let pos_b = Vec2 { x: 210.0, y: 10.0 };
    rt.handle_pointer_press(pos_a, PointerButton::Primary);
    rt.handle_pointer_release(pos_a, PointerButton::Primary);
    std::thread::sleep(std::time::Duration::from_millis(50));
    rt.handle_pointer_press(pos_b, PointerButton::Primary);
    rt.handle_pointer_release(pos_b, PointerButton::Primary);
    assert_eq!(*b_clicks.borrow(), 1);
    assert_eq!(*a_clicks.borrow(), 0);
    std::thread::sleep(std::time::Duration::from_millis(350));
    rt.poll_gesture_timers();
    assert_eq!(*a_clicks.borrow(), 1);
}

/// A press that travels past tap slop is a drag: Compose's
/// `detectTapGestures` drops the tap, so releasing mid-element must not
/// activate it. Guards against re-introducing click-on-drag-release.
#[test]
fn drag_past_slop_does_not_fire_click() {
    for drag in [0.0f32, 4.0, 12.0] {
        let mut rt = ReposeRuntime::new();
        let clicks = Rc::new(RefCell::new(0u32));
        let c = clicks.clone();
        rt.cache_frame(button_frame(
            BTN_ID,
            Some(Rc::new(move || *c.borrow_mut() += 1)),
            None,
            None,
        ));
        let start = Vec2 { x: 10.0, y: 10.0 };
        let end = Vec2 {
            x: 10.0 + drag,
            y: 10.0,
        };
        rt.handle_pointer_press(start, PointerButton::Primary);
        if drag > 0.0 {
            rt.handle_pointer_move(end);
        }
        let result = rt.handle_pointer_release(end, PointerButton::Primary);
        rt.poll_gesture_timers();
        std::thread::sleep(std::time::Duration::from_millis(350));
        rt.poll_gesture_timers();
        assert_eq!(
            *clicks.borrow(),
            1,
            "a {drag}px move is within slop and should still click"
        );
        assert_eq!(result.clicked_id, Some(BTN_ID));
    }
}

#[test]
fn drag_past_slop_cancels_click() {
    for drag in [25.0f32, 60.0, 90.0] {
        let mut rt = ReposeRuntime::new();
        let clicks = Rc::new(RefCell::new(0u32));
        let c = clicks.clone();
        rt.cache_frame(button_frame(
            BTN_ID,
            Some(Rc::new(move || *c.borrow_mut() += 1)),
            None,
            None,
        ));
        let start = Vec2 { x: 10.0, y: 10.0 };
        // Stay inside the 100x50 hit region so only slop can cancel the tap.
        let end = Vec2 {
            x: 10.0 + drag.min(80.0),
            y: 10.0,
        };
        rt.handle_pointer_press(start, PointerButton::Primary);
        rt.handle_pointer_move(end);
        let result = rt.handle_pointer_release(end, PointerButton::Primary);
        rt.poll_gesture_timers();
        std::thread::sleep(std::time::Duration::from_millis(350));
        rt.poll_gesture_timers();
        assert_eq!(*clicks.borrow(), 0, "a {drag}px drag must not click");
        assert_eq!(result.clicked_id, None);
        assert!(!result.needs_a11y_announce);
    }
}

/// Compose requires the two taps of a double tap to land within touch slop of
/// each other; taps far apart are two independent taps.
#[test]
fn double_tap_beyond_slop_is_not_a_double_tap() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let doubles = Rc::new(RefCell::new(0u32));
    let (c, d) = (clicks.clone(), doubles.clone());
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        Some(Rc::new(move || *d.borrow_mut() += 1)),
        None,
    ));
    let near = Vec2 { x: 10.0, y: 10.0 };
    // 80px apart, well past the 18dp tap slop, but both inside the region.
    let far = Vec2 { x: 90.0, y: 10.0 };
    rt.handle_pointer_press(near, PointerButton::Primary);
    rt.handle_pointer_release(near, PointerButton::Primary);
    std::thread::sleep(std::time::Duration::from_millis(60));
    rt.handle_pointer_press(far, PointerButton::Primary);
    rt.handle_pointer_release(far, PointerButton::Primary);
    rt.poll_gesture_timers();
    std::thread::sleep(std::time::Duration::from_millis(350));
    rt.poll_gesture_timers();
    assert_eq!(*doubles.borrow(), 0, "taps 80px apart are not a double tap");
    assert_eq!(*clicks.borrow(), 2, "each distant tap is its own click");
}

/// A drag that ends on the element must not be able to manufacture a double
/// tap, even when both halves fall inside the timing window.
#[test]
fn dragging_twice_is_not_a_double_tap() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let doubles = Rc::new(RefCell::new(0u32));
    let (c, d) = (clicks.clone(), doubles.clone());
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        Some(Rc::new(move || *d.borrow_mut() += 1)),
        None,
    ));
    let start = Vec2 { x: 10.0, y: 10.0 };
    let moved = Vec2 { x: 90.0, y: 10.0 };
    for _ in 0..2 {
        rt.handle_pointer_press(start, PointerButton::Primary);
        rt.handle_pointer_move(moved);
        rt.handle_pointer_release(moved, PointerButton::Primary);
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
    rt.poll_gesture_timers();
    std::thread::sleep(std::time::Duration::from_millis(350));
    rt.poll_gesture_timers();
    assert_eq!(
        *doubles.borrow(),
        0,
        "drags must not compose into a double tap"
    );
    assert_eq!(*clicks.borrow(), 0, "drags must not click either");
}

#[test]
fn keyboard_hold_long_press_fires_and_suppresses_click() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let longs = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let l = longs.clone();
    rt.cache_frame(button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        None,
        Some(Rc::new(move || *l.borrow_mut() += 1)),
    ));
    rt.sched.focused = Some(BTN_ID);
    let down = KeyEvent {
        key: Key::Space,
        modifiers: Modifiers::default(),
        is_repeat: false,
        event_type: KeyEventType::Down,
        utf16_code_point: 0,
        physical: Some(PhysicalKey::Space),
    };
    let up = KeyEvent { ..down.clone() };
    let mut up = up;
    up.event_type = KeyEventType::Up;
    assert!(rt.handle_key(&down));
    std::thread::sleep(std::time::Duration::from_millis(550));
    rt.poll_gesture_timers();
    assert_eq!(*longs.borrow(), 1);
    assert!(rt.handle_key(&up));
    assert_eq!(*clicks.borrow(), 0);
}

#[test]
fn stale_capture_dispatches_one_cancel_before_pruning() {
    use repose_core::input::{PointerEvent, PointerEventKind};
    let mut rt = ReposeRuntime::new();
    let cancels = Rc::new(RefCell::new(0u32));
    let observer = cancels.clone();
    let hit = HitRegion {
        id: BTN_ID,
        rect: Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        },
        on_pointer_down: Some(Rc::new(|_: PointerEvent| {})),
        on_pointer_move: Some(Rc::new(|_: PointerEvent| {})),
        on_pointer_up: Some(Rc::new(|_: PointerEvent| {})),
        on_pointer_cancel: Some(Rc::new(move |event: PointerEvent| {
            assert!(matches!(event.event, PointerEventKind::Cancel));
            *observer.borrow_mut() += 1;
        })),
        ..Default::default()
    };
    rt.cache_frame(Frame {
        scene: Scene::default(),
        hit_regions: vec![hit],
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    });
    let pos = Vec2 { x: 10.0, y: 10.0 };
    rt.handle_pointer_press(pos, PointerButton::Primary);
    rt.cache_frame(Frame {
        scene: Scene::default(),
        hit_regions: Vec::new(),
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    });
    assert_eq!(*cancels.borrow(), 1);
    assert!(rt.hit_path.is_none() && rt.capture_id.is_none());
    rt.handle_pointer_move(pos);
    rt.handle_pointer_release(pos, PointerButton::Primary);
    assert_eq!(*cancels.borrow(), 1);
}

#[test]
fn pointer_cancel_clears_capture_for_reentry() {
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(button_frame(BTN_ID, None, None, None));
    let pos = Vec2 { x: 10.0, y: 10.0 };
    let _ = rt.handle_pointer_press(pos, PointerButton::Primary);
    assert!(rt.hit_path.is_some());
    rt.handle_pointer_cancel();
    assert!(rt.hit_path.is_none() && rt.capture_id.is_none());
}

#[test]
fn plain_clickable_is_immediate() {
    let mut rt = ReposeRuntime::new();
    let clicks = Rc::new(RefCell::new(0u32));
    let c = clicks.clone();
    let frame = button_frame(
        BTN_ID,
        Some(Rc::new(move || *c.borrow_mut() += 1)),
        None,
        None,
    );
    rt.cache_frame(frame);
    let pos = Vec2 { x: 10.0, y: 10.0 };
    let _ = rt.handle_pointer_press(pos, PointerButton::Primary);
    let result = rt.handle_pointer_release(pos, PointerButton::Primary);
    assert_eq!(*clicks.borrow(), 1, "plain click must fire on release");
    assert_eq!(result.clicked_id, Some(BTN_ID));
}

fn focused_textfield_rt() -> ReposeRuntime {
    let mut rt = ReposeRuntime::new();
    rt.sched.focused = Some(TF_ID);
    rt.cache_frame(textfield_frame(TF_ID));
    // Pre-create the persistent state so editing paths can act on it.
    let _ = rt.ensure_textfield_state(TF_ID);
    rt
}

#[test]
fn undo_redo_via_action() {
    let mut rt = focused_textfield_rt();

    rt.ensure_textfield_state(TF_ID)
        .borrow_mut()
        .insert_text_atomic("hello");

    assert!(rt.dispatch_action(Action::Undo));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "");

    assert!(rt.dispatch_action(Action::Redo));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "hello");
}

#[test]
fn copy_cut_paste_via_action() {
    let mut rt = focused_textfield_rt();
    let clipboard = Rc::new(RefCell::new(String::new()));

    let write = clipboard.clone();
    repose_core::clipboard::set_clipboard_fn(Box::new(move |t| {
        *write.borrow_mut() = t.to_string();
    }));

    rt.ensure_textfield_state(TF_ID)
        .borrow_mut()
        .insert_text_atomic("hello");

    // Select all + copy.
    assert!(rt.dispatch_action(Action::SelectAll));
    assert!(rt.dispatch_action(Action::Copy));
    assert_eq!(*clipboard.borrow(), "hello");

    // Cut removes the selection and copies it too.
    assert!(rt.dispatch_action(Action::Cut));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "");
    assert_eq!(*clipboard.borrow(), "hello");

    // Paste reads back from the (mock) OS clipboard.
    let read_clipboard = clipboard.clone();
    repose_core::clipboard::set_clipboard_read_fn(Box::new(move || {
        Some(read_clipboard.borrow().clone())
    }));
    assert!(rt.dispatch_action(Action::Paste));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "hello");
}

#[test]
fn insert_text_into_focused_filters_controls() {
    let mut rt = focused_textfield_rt();

    // Single-line: newlines and CR are stripped, control chars dropped.
    assert!(rt.insert_text_into_focused("a\nb\r\x01c"));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "abc");

    // Multiline keeps newlines.
    let mut rt = ReposeRuntime::new();
    let mut multiline_frame = textfield_frame(TF_ID);
    multiline_frame.hit_regions[0].tf_multiline = true;
    multiline_frame.hit_regions[0].tf_content_origin = Some((0.0, 0.0));
    rt.sched.focused = Some(TF_ID);
    rt.cache_frame(multiline_frame);
    let _ = rt.ensure_textfield_state(TF_ID);

    assert!(rt.insert_text_into_focused("x\ny"));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "x\ny");

    // Ctrl-modified insert is rejected (host should never send this, but be safe).
    rt.modifiers.ctrl = true;
    assert!(!rt.insert_text_into_focused("z"));
}

#[test]
fn insert_text_respects_ime_preedit() {
    let mut rt = focused_textfield_rt();
    rt.ime_preedit = true;
    assert!(!rt.insert_text_into_focused("ignored"));
    assert_eq!(rt.ensure_textfield_state(TF_ID).borrow().text, "");
}

#[test]
fn after_compose_lazy_inits_focused_textfield() {
    let mut rt = ReposeRuntime::new();
    rt.sched.focused = Some(TF_ID);
    let frame = textfield_frame(TF_ID);
    assert!(!rt.textfield_states.contains_key(&TF_ID));

    rt.after_compose(&frame, 1.0);
    assert!(
        rt.textfield_states.contains_key(&TF_ID),
        "after_compose should lazy-init the focused text field"
    );
}

#[test]
fn physical_keys_track_without_focus() {
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(textfield_frame(TF_ID));
    rt.sched.focused = None;

    rt.set_physical_key("KeyW", true);
    assert!(rt.physical_key_held("KeyW"));
    assert_eq!(rt.held_physical_keys(), vec!["KeyW".to_string()]);

    // A stuck down survives redundant presses; a release clears it.
    rt.set_physical_key("KeyW", true);
    assert!(rt.physical_key_held("KeyW"));
    rt.set_physical_key("KeyW", false);
    assert!(!rt.physical_key_held("KeyW"));
}

#[test]
fn focus_loss_clears_polled_levels() {
    let mut rt = ReposeRuntime::new();
    rt.set_physical_key("KeyW", true);
    let pos = Vec2 { x: 10.0, y: 10.0 };
    rt.cache_frame(textfield_frame(TF_ID));
    let _ = rt.handle_pointer_press(pos, PointerButton::Primary);
    assert!(rt.mouse_button_held(PointerButton::Primary));

    rt.handle_focus_lost();
    assert!(
        rt.held_physical_keys().is_empty(),
        "alt-tab must not leave a stuck key"
    );
    assert!(!rt.mouse_button_held(PointerButton::Primary));
}

#[test]
fn mouse_buttons_track_press_release() {
    let mut rt = ReposeRuntime::new();
    rt.cache_frame(textfield_frame(TF_ID));
    let pos = Vec2 { x: 10.0, y: 10.0 };
    assert!(!rt.mouse_button_held(PointerButton::Secondary));
    let _ = rt.handle_pointer_press(pos, PointerButton::Secondary);
    assert!(rt.mouse_button_held(PointerButton::Secondary));
    let _ = rt.handle_pointer_release(pos, PointerButton::Secondary);
    assert!(!rt.mouse_button_held(PointerButton::Secondary));
}

#[test]
fn pointer_pos_tracks_mouse_only() {
    let mut rt = ReposeRuntime::new();
    assert_eq!(rt.sched.pointer_pos_px, None);
    let pos = Vec2 { x: 10.0, y: 20.0 };
    rt.cache_frame(textfield_frame(TF_ID));
    let _ = rt.handle_pointer_move(pos);
    assert_eq!(rt.sched.pointer_pos_px, Some((10.0, 20.0)));
    let _ = rt.handle_touch_move(Some(7), Vec2 { x: 99.0, y: 99.0 });
    assert_eq!(rt.sched.pointer_pos_px, Some((10.0, 20.0)));
    rt.handle_pointer_cancel();
    assert_eq!(rt.sched.pointer_pos_px, None);
    let _ = rt.handle_pointer_press(pos, PointerButton::Primary);
    assert_eq!(rt.sched.pointer_pos_px, Some((10.0, 20.0)));
    rt.handle_focus_lost();
    assert_eq!(rt.sched.pointer_pos_px, None);
}

#[test]
fn shortcut_resolution_uses_event_modifiers() {
    use std::cell::Cell;

    let mut rt = ReposeRuntime::new();
    let fired = Rc::new(Cell::new(false));
    let handler_fired = fired.clone();
    rt.shortcuts.default_map.insert(
        Key::Character('s'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        Action::Custom("save".into()),
    );
    rt.shortcuts.handler = Some(Rc::new(move |action| {
        if matches!(action, Action::Custom(ref key) if key.as_ref() == "save") {
            handler_fired.set(true);
            true
        } else {
            false
        }
    }));
    rt.cache_frame(Frame {
        scene: Scene::default(),
        hit_regions: Vec::new(),
        semantics_nodes: Vec::new(),
        focus_chain: Vec::new(),
    });
    let event = key_down(
        Key::Character('s'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        false,
    );
    assert!(rt.handle_key(&event));
    assert!(fired.get());
}

#[test]
fn shortcuts_work_before_the_first_frame() {
    use std::cell::Cell;

    let mut rt = ReposeRuntime::new();
    let fired = Rc::new(Cell::new(false));
    let handler_fired = fired.clone();
    rt.shortcuts.default_map.insert(
        Key::Character('s'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        Action::Custom("save".into()),
    );
    rt.shortcuts.handler = Some(Rc::new(move |action| {
        if matches!(action, Action::Custom(ref key) if key.as_ref() == "save") {
            handler_fired.set(true);
            true
        } else {
            false
        }
    }));
    let event = key_down(
        Key::Character('s'),
        Modifiers {
            ctrl: true,
            ..Modifiers::default()
        },
        false,
    );
    assert!(rt.handle_key(&event));
    assert!(fired.get());
}

#[test]
fn runtime_shortcuts_are_per_runtime() {
    use repose_core::shortcuts::KeyChord;
    let mut a = ReposeRuntime::new();
    let mut b = ReposeRuntime::new();
    a.shortcuts.default_map.insert(
        Key::Character('k'),
        Modifiers::default(),
        Action::Custom("a".into()),
    );
    b.shortcuts.default_map.insert(
        Key::Character('k'),
        Modifiers::default(),
        Action::Custom("b".into()),
    );
    let chord = KeyChord::new(Key::Character('k'), Modifiers::default());
    assert_eq!(
        a.shortcuts.resolve_action(&chord),
        Some(Action::Custom("a".into()))
    );
    assert_eq!(
        b.shortcuts.resolve_action(&chord),
        Some(Action::Custom("b".into()))
    );
}
