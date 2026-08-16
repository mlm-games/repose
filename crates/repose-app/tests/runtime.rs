//! Integration tests for the embeddable runtime's event handling core.
//!
//! These drive `ReposeRuntime` with hand-built frames so the behaviour does
//! not depend on the layout engine: Escape fallthrough, built-in text editing
//! shortcuts, and pointer-release click reporting.

use std::cell::RefCell;
use std::rc::Rc;

use repose_app::ReposeRuntime;
use repose_core::input::{Key, KeyEvent, KeyEventType, Modifiers, PointerButton};
use repose_core::runtime::{Frame, SemNode};
use repose_core::semantics::Role;
use repose_core::shortcuts::Action;
use repose_core::{CursorIcon, HitRegion, Rect, Scene, Vec2};

const TF_ID: u64 = 100;
const BTN_ID: u64 = 200;

fn textfield_frame(id: u64) -> Frame {
    let mut hr = HitRegion::default();
    hr.id = id;
    hr.rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 200.0,
        h: 30.0,
    };
    hr.focusable = true;
    hr.tf_state_key = Some(id);
    hr.tf_multiline = false;
    hr.cursor = Some(CursorIcon::Text);

    let rect = hr.rect;
    let sem = SemNode {
        id,
        parent: None,
        role: Role::TextField,
        label: Some("Field".into()),
        rect,
        focused: true,
        enabled: true,
        selectable_group: false,
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
    }
}

#[test]
fn escape_not_consumed_when_idle() {
    let mut rt = ReposeRuntime::new();
    // No focus, no frame cache, no active drag: Escape must fall through so
    // hosts can implement back / quit.
    let ev = key_down(Key::Escape, Modifiers::default(), false);
    assert!(!rt.handle_key(&ev));

    // With a cached frame but no focus, Escape still isn't consumed.
    rt.cache_frame(textfield_frame(TF_ID));
    rt.sched.focused = None;
    assert!(!rt.handle_key(&ev));
}

#[test]
fn release_reports_clicked_id() {
    let mut rt = ReposeRuntime::new();
    let mut hr = HitRegion::default();
    hr.id = BTN_ID;
    hr.rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 50.0,
        h: 50.0,
    };
    let clicked = Rc::new(RefCell::new(false));
    let clicked_cb = clicked.clone();
    hr.on_click = Some(Rc::new(move || {
        *clicked_cb.borrow_mut() = true;
    }));
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
    let mut hr = HitRegion::default();
    hr.id = BTN_ID;
    hr.rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 50.0,
        h: 50.0,
    };
    hr.on_click = Some(Rc::new(|| {}));
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
