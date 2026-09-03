//! Shim: TouchGestureState lives in `repose-app::touch_gesture`.
//! Platform-specific winit helpers remain here until fully moved to `repose-app`.

#[allow(unused_imports)]
pub use repose_app::MultiTouchDelta;
pub use repose_app::TouchGestureState;

use repose_core::input::Modifiers;
use repose_core::runtime::Frame;

pub(crate) fn request_redraw(window: &Option<std::sync::Arc<winit::window::Window>>) {
    if let Some(w) = window {
        w.request_redraw();
    }
}

#[allow(dead_code)]
pub(crate) fn is_textfield_in_frame(frame_cache: &Option<Frame>, id: u64) -> bool {
    repose_app::is_textfield_in_frame_cache(frame_cache, id)
}

pub(crate) fn update_modifiers(modifiers: &mut Modifiers, state: &winit::keyboard::ModifiersState) {
    modifiers.shift = state.shift_key();
    modifiers.ctrl = state.control_key();
    modifiers.alt = state.alt_key();
    modifiers.meta = state.super_key();
    modifiers.command = if cfg!(target_os = "macos") {
        modifiers.meta
    } else {
        modifiers.ctrl
    };
}

#[allow(dead_code)]
pub(crate) fn hit_index_by_id(frame: &Frame, id: u64) -> Option<usize> {
    repose_app::hit_index_by_id(frame, id)
}

pub(crate) fn winit_key_to_repose(
    ev: &winit::event::KeyEvent,
    mapped_key: &repose_core::input::Key,
    mods: &repose_core::input::Modifiers,
) -> repose_core::input::KeyEvent {
    let utf16 = match mapped_key {
        repose_core::input::Key::Character(c) => *c as u16,
        _ => 0,
    };
    repose_core::input::KeyEvent {
        key: mapped_key.clone(),
        modifiers: *mods,
        is_repeat: ev.repeat,
        event_type: if ev.state == winit::event::ElementState::Pressed {
            repose_core::input::KeyEventType::Down
        } else {
            repose_core::input::KeyEventType::Up
        },
        utf16_code_point: utf16,
    }
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub(crate) fn map_cursor(c: repose_core::CursorIcon) -> winit::window::CursorIcon {
    use winit::window::CursorIcon as W;
    match c {
        repose_core::CursorIcon::Default => W::Default,
        repose_core::CursorIcon::Pointer => W::Pointer,
        repose_core::CursorIcon::Text => W::Text,
        repose_core::CursorIcon::EwResize => W::EwResize,
        repose_core::CursorIcon::NsResize => W::NsResize,
        repose_core::CursorIcon::Grab => W::Grab,
        repose_core::CursorIcon::Grabbing => W::Grabbing,
    }
}

// IME helpers.
pub fn map_ime_purpose(hint: repose_core::ImePurposeHint) -> winit::window::ImePurpose {
    match hint {
        repose_core::ImePurposeHint::Password => winit::window::ImePurpose::Password,
        _ => winit::window::ImePurpose::Normal,
    }
}

pub fn set_ime_for_textfield(window: &winit::window::Window, is_textfield: bool) {
    set_ime_for_textfield_ex(
        window,
        is_textfield,
        repose_core::ImePurposeHint::Normal,
        true,
        repose_core::KeyboardCapitalization::Unspecified,
    );
}

pub fn set_ime_for_textfield_ex(
    window: &winit::window::Window,
    is_textfield: bool,
    purpose: repose_core::ImePurposeHint,
    _auto_correct: bool,
    _capitalization: repose_core::KeyboardCapitalization,
) {
    if is_textfield {
        window.set_ime_allowed(true);
        window.set_ime_purpose(map_ime_purpose(purpose));
    } else {
        window.set_ime_allowed(false);
    }
}

/// Map a physical key to a Repose key. Letters are always lowercase (chord
/// matching keys off the modifier flags); digits and punctuation produce the
/// US-layout (most used) character the key would type, honoring `mods.shift`, so Shift+8
/// arrives as '*' and Shift+Equal as '+'.
pub(crate) fn map_key(
    key: winit::keyboard::PhysicalKey,
    mods: &repose_core::input::Modifiers,
) -> repose_core::input::Key {
    use repose_core::input::Key;
    use winit::keyboard::{KeyCode, PhysicalKey};

    let shifted = |a: char, b: char| Key::Character(if mods.shift { b } else { a });

    match key {
        PhysicalKey::Code(KeyCode::Enter) => Key::Enter,
        PhysicalKey::Code(KeyCode::Tab) => Key::Tab,
        PhysicalKey::Code(KeyCode::Backspace) => Key::Backspace,
        PhysicalKey::Code(KeyCode::Delete) => Key::Delete,
        PhysicalKey::Code(KeyCode::Insert) => Key::Insert,
        PhysicalKey::Code(KeyCode::Escape) => Key::Escape,
        PhysicalKey::Code(KeyCode::ArrowLeft) => Key::ArrowLeft,
        PhysicalKey::Code(KeyCode::ArrowRight) => Key::ArrowRight,
        PhysicalKey::Code(KeyCode::ArrowUp) => Key::ArrowUp,
        PhysicalKey::Code(KeyCode::ArrowDown) => Key::ArrowDown,
        PhysicalKey::Code(KeyCode::Home) => Key::Home,
        PhysicalKey::Code(KeyCode::End) => Key::End,
        PhysicalKey::Code(KeyCode::PageUp) => Key::PageUp,
        PhysicalKey::Code(KeyCode::PageDown) => Key::PageDown,
        PhysicalKey::Code(KeyCode::Space) => Key::Space,
        PhysicalKey::Code(KeyCode::KeyA) => Key::Character('a'),
        PhysicalKey::Code(KeyCode::KeyB) => Key::Character('b'),
        PhysicalKey::Code(KeyCode::KeyC) => Key::Character('c'),
        PhysicalKey::Code(KeyCode::KeyD) => Key::Character('d'),
        PhysicalKey::Code(KeyCode::KeyE) => Key::Character('e'),
        PhysicalKey::Code(KeyCode::KeyF) => Key::Character('f'),
        PhysicalKey::Code(KeyCode::KeyG) => Key::Character('g'),
        PhysicalKey::Code(KeyCode::KeyH) => Key::Character('h'),
        PhysicalKey::Code(KeyCode::KeyI) => Key::Character('i'),
        PhysicalKey::Code(KeyCode::KeyJ) => Key::Character('j'),
        PhysicalKey::Code(KeyCode::KeyK) => Key::Character('k'),
        PhysicalKey::Code(KeyCode::KeyL) => Key::Character('l'),
        PhysicalKey::Code(KeyCode::KeyM) => Key::Character('m'),
        PhysicalKey::Code(KeyCode::KeyN) => Key::Character('n'),
        PhysicalKey::Code(KeyCode::KeyO) => Key::Character('o'),
        PhysicalKey::Code(KeyCode::KeyP) => Key::Character('p'),
        PhysicalKey::Code(KeyCode::KeyQ) => Key::Character('q'),
        PhysicalKey::Code(KeyCode::KeyR) => Key::Character('r'),
        PhysicalKey::Code(KeyCode::KeyS) => Key::Character('s'),
        PhysicalKey::Code(KeyCode::KeyT) => Key::Character('t'),
        PhysicalKey::Code(KeyCode::KeyU) => Key::Character('u'),
        PhysicalKey::Code(KeyCode::KeyV) => Key::Character('v'),
        PhysicalKey::Code(KeyCode::KeyW) => Key::Character('w'),
        PhysicalKey::Code(KeyCode::KeyX) => Key::Character('x'),
        PhysicalKey::Code(KeyCode::KeyY) => Key::Character('y'),
        PhysicalKey::Code(KeyCode::KeyZ) => Key::Character('z'),
        PhysicalKey::Code(KeyCode::Digit0) => shifted('0', ')'),
        PhysicalKey::Code(KeyCode::Digit1) => shifted('1', '!'),
        PhysicalKey::Code(KeyCode::Digit2) => shifted('2', '@'),
        PhysicalKey::Code(KeyCode::Digit3) => shifted('3', '#'),
        PhysicalKey::Code(KeyCode::Digit4) => shifted('4', '$'),
        PhysicalKey::Code(KeyCode::Digit5) => shifted('5', '%'),
        PhysicalKey::Code(KeyCode::Digit6) => shifted('6', '^'),
        PhysicalKey::Code(KeyCode::Digit7) => shifted('7', '&'),
        PhysicalKey::Code(KeyCode::Digit8) => shifted('8', '*'),
        PhysicalKey::Code(KeyCode::Digit9) => shifted('9', '('),
        PhysicalKey::Code(KeyCode::Minus) => shifted('-', '_'),
        PhysicalKey::Code(KeyCode::Equal) => shifted('=', '+'),
        PhysicalKey::Code(KeyCode::BracketLeft) => shifted('[', '{'),
        PhysicalKey::Code(KeyCode::BracketRight) => shifted(']', '}'),
        PhysicalKey::Code(KeyCode::Backslash) => shifted('\\', '|'),
        PhysicalKey::Code(KeyCode::Semicolon) => shifted(';', ':'),
        PhysicalKey::Code(KeyCode::Quote) => shifted('\'', '"'),
        PhysicalKey::Code(KeyCode::Comma) => shifted(',', '<'),
        PhysicalKey::Code(KeyCode::Period) => shifted('.', '>'),
        PhysicalKey::Code(KeyCode::Slash) => shifted('/', '?'),
        PhysicalKey::Code(KeyCode::Backquote) => shifted('`', '~'),
        PhysicalKey::Code(KeyCode::NumpadAdd) => Key::Character('+'),
        PhysicalKey::Code(KeyCode::NumpadSubtract) => Key::Character('-'),
        PhysicalKey::Code(KeyCode::NumpadMultiply) => Key::Character('*'),
        PhysicalKey::Code(KeyCode::NumpadDivide) => Key::Character('/'),
        PhysicalKey::Code(KeyCode::NumpadEnter) => Key::Enter,
        PhysicalKey::Code(KeyCode::NumpadDecimal) => Key::Character('.'),
        PhysicalKey::Code(KeyCode::Numpad0) => Key::Character('0'),
        PhysicalKey::Code(KeyCode::Numpad1) => Key::Character('1'),
        PhysicalKey::Code(KeyCode::Numpad2) => Key::Character('2'),
        PhysicalKey::Code(KeyCode::Numpad3) => Key::Character('3'),
        PhysicalKey::Code(KeyCode::Numpad4) => Key::Character('4'),
        PhysicalKey::Code(KeyCode::Numpad5) => Key::Character('5'),
        PhysicalKey::Code(KeyCode::Numpad6) => Key::Character('6'),
        PhysicalKey::Code(KeyCode::Numpad7) => Key::Character('7'),
        PhysicalKey::Code(KeyCode::Numpad8) => Key::Character('8'),
        PhysicalKey::Code(KeyCode::Numpad9) => Key::Character('9'),
        PhysicalKey::Code(KeyCode::F1) => Key::F(1),
        PhysicalKey::Code(KeyCode::F2) => Key::F(2),
        PhysicalKey::Code(KeyCode::F3) => Key::F(3),
        PhysicalKey::Code(KeyCode::F4) => Key::F(4),
        PhysicalKey::Code(KeyCode::F5) => Key::F(5),
        PhysicalKey::Code(KeyCode::F6) => Key::F(6),
        PhysicalKey::Code(KeyCode::F7) => Key::F(7),
        PhysicalKey::Code(KeyCode::F8) => Key::F(8),
        PhysicalKey::Code(KeyCode::F9) => Key::F(9),
        PhysicalKey::Code(KeyCode::F10) => Key::F(10),
        PhysicalKey::Code(KeyCode::F11) => Key::F(11),
        PhysicalKey::Code(KeyCode::F12) => Key::F(12),
        _ => Key::Unknown,
    }
}
