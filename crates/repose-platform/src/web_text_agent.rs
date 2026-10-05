//! Hidden `<input>` that gives mobile browsers an editor to hang the soft
//! keyboard off, plus the DOM -> runtime translation for its events.
//!
//! winit's web backend has no IME support: `set_ime_allowed` and
//! `set_ime_cursor_area` are no-ops and `WindowEvent::Ime` is never emitted,
//! so a bare `<canvas>` never raises the on-screen keyboard. The agent is a 1x1
//! transparent `<input>` parked as a sibling of the canvas (so focusing it
//! cannot scroll the page away), dragged onto the focused field's caret, and
//! focused for as long as an editable text field holds focus.
//!
//! The element keeps its own buffer rather than mirroring the Repose field:
//! each `input` event is diffed against the previous buffer to recover the
//! committed prefix, the preedit tail, and deletions, which is what Gboard and
//! iOS need in order to run suggestions and autocorrect.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlInputElement};

use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::Window;

use repose_core::input::ImeEvent;
use repose_core::{ImeAction, ImePurposeHint, KeyboardCapitalization, KeyboardType};

/// Runtime work produced by a DOM callback. DOM handlers cannot touch the
/// runtime (winit's event loop owns it), so they queue and the runner drains.
pub enum AgentEvent {
    Ime(ImeEvent),
    /// Text typed outside any composition. Uses the runtime's insertion path
    /// rather than `ImeEvent::Commit`, which appends at the caret instead of
    /// replacing the selection.
    Insert(String),
    Key(DomKey),
}

pub struct DomKey {
    pub physical: PhysicalKey,
    pub pressed: bool,
    pub repeat: bool,
    pub modifiers: ModifiersState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Keyboard {
    input_mode: &'static str,
    auto_correct: bool,
    capitalization: &'static str,
    enter_key_hint: &'static str,
}

pub struct TextAgent {
    input: HtmlInputElement,
    state: Rc<RefCell<InputState>>,
    events: Rc<RefCell<Vec<AgentEvent>>>,
    _listeners: Vec<Closure<dyn FnMut(web_sys::Event)>>,
    caret: Option<(f64, f64, f64, f64)>,
    keyboard: Option<Keyboard>,
    focused_field: Option<u64>,
}

impl TextAgent {
    pub fn attach(window: &Arc<Window>, canvas: &HtmlCanvasElement) -> Result<Self, JsValue> {
        let document = canvas
            .owner_document()
            .ok_or_else(|| JsValue::from_str("canvas has no owner document"))?;

        // Never `type=password`: Chrome's password manager would then treat
        // every keystroke as a new password.
        let input: HtmlInputElement = document
            .create_element("input")?
            .dyn_into::<HtmlInputElement>()?;
        input.set_type("text");
        input.set_attribute("autocapitalize", "off")?;
        input.set_attribute("autocomplete", "off")?;
        input.set_attribute("aria-hidden", "true")?;

        let style = input.style();
        for (name, value) in [
            ("background-color", "transparent"),
            ("border", "none"),
            ("outline", "none"),
            ("color", "transparent"),
            ("caret-color", "transparent"),
            ("position", "absolute"),
            ("width", "1px"),
            ("height", "1px"),
            // Below 16px iOS Safari zooms the page when the field focuses.
            ("font-size", "16px"),
            // The agent sits on top of the canvas; taps must still reach it.
            ("pointer-events", "none"),
            ("opacity", "0"),
        ] {
            style.set_property(name, value)?;
        }
        if let Some(parent) = canvas.parent_node() {
            parent.insert_before(&input, canvas.next_sibling().as_ref())?;
        }

        let state = Rc::new(RefCell::new(InputState::default()));
        let events: Rc<RefCell<Vec<AgentEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let redraw: Rc<RefCell<Option<Arc<Window>>>> = Rc::new(RefCell::new(Some(window.clone())));

        let mut listeners: Vec<Closure<dyn FnMut(web_sys::Event)>> = Vec::new();
        let mut listen = |target: &web_sys::EventTarget,
                          name: &str,
                          handler: Box<dyn FnMut(web_sys::Event)>|
         -> Result<(), JsValue> {
            let closure = Closure::wrap(handler);
            target.add_event_listener_with_callback(name, closure.as_ref().unchecked_ref())?;
            listeners.push(closure);
            Ok(())
        };

        let element: web_sys::EventTarget = input.clone().into();
        let canvas_target: web_sys::EventTarget = canvas.clone().into();

        let wake = redraw.clone();
        listen(
            &element,
            "compositionstart",
            Box::new(move |_| wake_up(&wake)),
        )?;

        let agent_input = input.clone();
        let agent_state = state.clone();
        let agent_events = events.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "input",
            Box::new(move |event| {
                if let Ok(event) = event.dyn_into::<web_sys::InputEvent>() {
                    let mut state = agent_state.borrow_mut();
                    let mut events = agent_events.borrow_mut();
                    state.handle_input(&agent_input, &event, &mut events);
                }
                wake_up(&wake);
            }),
        )?;

        let agent_input = input.clone();
        let agent_state = state.clone();
        let agent_events = events.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "compositionend",
            Box::new(move |_| {
                let mut state = agent_state.borrow_mut();
                let mut events = agent_events.borrow_mut();
                state.handle_composition_end(&agent_input, &mut events);
                wake_up(&wake);
            }),
        )?;

        let agent_state = state.clone();
        let agent_events = events.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "keydown",
            Box::new(move |event| {
                if let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() {
                    let mut state = agent_state.borrow_mut();
                    let mut events = agent_events.borrow_mut();
                    state.handle_key(&event, true, &mut events);
                }
                wake_up(&wake);
            }),
        )?;

        let agent_state = state.clone();
        let agent_events = events.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "keyup",
            Box::new(move |event| {
                if let Ok(event) = event.dyn_into::<web_sys::KeyboardEvent>() {
                    let mut state = agent_state.borrow_mut();
                    let mut events = agent_events.borrow_mut();
                    state.handle_key(&event, false, &mut events);
                }
                wake_up(&wake);
            }),
        )?;

        // Mobile browsers only re-show the keyboard when focus is re-taken
        // inside the gesture that asked for it, which a later frame is not.
        let touch_input = input.clone();
        listen(
            &canvas_target,
            "touchend",
            Box::new(move |_| {
                if has_focus(&touch_input) {
                    let _ = touch_input.blur();
                    focus_without_scroll(&touch_input);
                }
            }),
        )?;

        Ok(Self {
            input,
            state,
            events,
            _listeners: listeners,
            caret: None,
            keyboard: None,
            focused_field: None,
        })
    }

    /// Move the agent onto the focused field and reconfigure the keyboard the
    /// browser derives from its attributes. `focused` is the runner's verdict
    /// on whether an editable text field currently holds focus.
    #[expect(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        canvas: &HtmlCanvasElement,
        focused: Option<u64>,
        caret: Option<(f64, f64, f64, f64)>,
        purpose: ImePurposeHint,
        auto_correct: bool,
        capitalization: KeyboardCapitalization,
        keyboard_type: KeyboardType,
        action: ImeAction,
    ) {
        if focused != self.focused_field {
            self.reset();
            self.focused_field = focused;
        }

        let keyboard =
            keyboard_attributes(purpose, auto_correct, capitalization, keyboard_type, action);
        if self.keyboard != Some(keyboard) {
            self.keyboard = Some(keyboard);
            self.input.set_input_mode(keyboard.input_mode);
            let _ = self.input.set_attribute(
                "autocorrect",
                if keyboard.auto_correct { "on" } else { "off" },
            );
            let _ = self
                .input
                .set_attribute("autocapitalize", keyboard.capitalization);
            let _ = self
                .input
                .set_attribute("enterkeyhint", keyboard.enter_key_hint);
        }

        if caret != self.caret {
            self.caret = caret;
            if let Some((x, y, w, h)) = caret {
                let dpr = web_sys::window()
                    .map(|window| window.device_pixel_ratio())
                    .filter(|dpr| dpr.is_finite() && *dpr > 0.0)
                    .unwrap_or(1.0);
                let max_x = canvas.width() as f64 / dpr;
                let max_y = canvas.height() as f64 / dpr;
                let style = self.input.style();
                let left = canvas.offset_left() as f64 + (x + w * 0.5).clamp(0.0, max_x);
                let top = canvas.offset_top() as f64 + (y + h * 0.5).clamp(0.0, max_y);
                let _ = style.set_property("left", &format!("{left}px"));
                let _ = style.set_property("top", &format!("{top}px"));
            }
        }

        if focused.is_some() {
            if !self.has_focus() {
                self.state.borrow_mut().last_text.clear();
                focus_without_scroll(&self.input);
            }
        } else if self.has_focus() {
            let _ = self.input.blur();
            self.reset();
        }
    }

    /// `true` while the agent owns DOM focus, i.e. the soft keyboard's editor.
    pub fn has_focus(&self) -> bool {
        has_focus(&self.input)
    }

    /// Take everything the DOM queued since the last drain.
    pub fn drain_events(&self) -> Vec<AgentEvent> {
        std::mem::take(&mut self.events.borrow_mut())
    }

    /// Give DOM focus back to the page and discard pending input: the window
    /// lost focus, so no further text can land in the runtime.
    pub fn hide(&mut self) {
        if self.has_focus() {
            let _ = self.input.blur();
        }
        self.reset();
        self.focused_field = None;
        self.events.borrow_mut().clear();
    }

    /// Drop the DOM buffer. The Repose field owns the committed text, so a
    /// stale buffer would make the next diff delete or duplicate characters.
    fn reset(&mut self) {
        self.state.borrow_mut().last_text.clear();
        self.input.set_value("");
    }
}

impl Drop for TextAgent {
    fn drop(&mut self) {
        self.input.remove();
    }
}

#[derive(Default)]
struct InputState {
    last_text: String,
    special: KeydownSpecialCase,
}

/// Mobile keyboards that fake Backspace as a nameless keydown still report the
/// real deletion through `deleteContentBackward`, so those keydowns carry no
/// information and must not reach the runtime as keys.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum KeydownSpecialCase {
    #[default]
    None,
    /// Gboard reports key code 229 for Backspace while its suggestion strip is
    /// showing, and applies corrections as `deleteContentBackward` +
    /// `insertText`.
    AndroidKeycode229,
    /// iOS' built-in Korean keyboard composes Hangul through
    /// `deleteContentBackward` with key code 0.
    IosKeycode0,
}

const MOD_KEYS: ModifiersState = ModifiersState::CONTROL
    .union(ModifiersState::ALT)
    .union(ModifiersState::SUPER);

impl InputState {
    fn handle_input(
        &mut self,
        input: &HtmlInputElement,
        event: &web_sys::InputEvent,
        out: &mut Vec<AgentEvent>,
    ) {
        let input_type = event.input_type();
        let composing = event.is_composing();
        if !composing
            && input_type != "insertText"
            && input_type != "insertReplacementText"
            && input_type != "insertLineBreak"
            && !(input_type == "deleteContentBackward" && self.special != KeydownSpecialCase::None)
        {
            self.last_text.clear();
            input.set_value("");
            return;
        }

        let text = input.value();
        let prefix = common_prefix_len(&text, &self.last_text);
        if prefix < self.last_text.chars().count() {
            push_key_chord(out, KeyCode::Backspace);
        }
        let tail: String = text.chars().skip(prefix).collect();

        if composing {
            let cursor = active_range(input, &text, prefix, &tail);
            out.push(AgentEvent::Ime(ImeEvent::Update { text: tail, cursor }));
            self.last_text = text.chars().take(prefix).collect();
        } else if input_type == "insertLineBreak" || tail == "\n" {
            push_key_chord(out, KeyCode::Enter);
            self.last_text = text;
        } else {
            if !tail.is_empty() {
                out.push(AgentEvent::Insert(tail));
            }
            self.last_text = text;
        }
    }

    fn handle_composition_end(&mut self, input: &HtmlInputElement, out: &mut Vec<AgentEvent>) {
        let text = input.value();
        let commit: String = text.chars().skip(self.last_text.chars().count()).collect();
        if !commit.is_empty() {
            out.push(AgentEvent::Ime(ImeEvent::Commit(commit)));
        }
        self.last_text = text;
    }

    fn handle_key(
        &mut self,
        event: &web_sys::KeyboardEvent,
        pressed: bool,
        out: &mut Vec<AgentEvent>,
    ) {
        let key_code = event.key_code();
        let code = event.code();
        if !pressed {
            self.special = KeydownSpecialCase::None;
            if event.is_composing() || key_code == 229 {
                return;
            }
            out.push(AgentEvent::Key(DomKey {
                physical: dom_physical_key(&code),
                pressed: false,
                repeat: false,
                modifiers: dom_modifiers(event),
            }));
            return;
        }

        self.special = match key_code {
            229 => KeydownSpecialCase::AndroidKeycode229,
            0 => KeydownSpecialCase::IosKeycode0,
            _ => KeydownSpecialCase::None,
        };
        if event.is_composing() || self.special != KeydownSpecialCase::None {
            return;
        }

        let modifiers = dom_modifiers(event);
        let printable = event.key().chars().count() == 1;
        if !printable || modifiers.intersects(MOD_KEYS) {
            self.last_text.clear();
        }
        // Printable characters reach the runtime through the following `input`
        // event, so forwarding the key too would insert them twice.
        if printable && !modifiers.intersects(MOD_KEYS) {
            return;
        }

        out.push(AgentEvent::Key(DomKey {
            physical: dom_physical_key(&code),
            pressed: true,
            repeat: event.repeat(),
            modifiers,
        }));
    }
}

fn push_key_chord(out: &mut Vec<AgentEvent>, code: KeyCode) {
    let modifiers = ModifiersState::empty();
    out.push(AgentEvent::Key(DomKey {
        physical: PhysicalKey::Code(code),
        pressed: true,
        repeat: false,
        modifiers,
    }));
    out.push(AgentEvent::Key(DomKey {
        physical: PhysicalKey::Code(code),
        pressed: false,
        repeat: false,
        modifiers,
    }));
}

fn dom_modifiers(event: &web_sys::KeyboardEvent) -> ModifiersState {
    let mut modifiers = ModifiersState::empty();
    if event.shift_key() {
        modifiers.insert(ModifiersState::SHIFT);
    }
    if event.ctrl_key() {
        modifiers.insert(ModifiersState::CONTROL);
    }
    if event.alt_key() {
        modifiers.insert(ModifiersState::ALT);
    }
    if event.meta_key() {
        modifiers.insert(ModifiersState::SUPER);
    }
    modifiers
}

fn dom_physical_key(code: &str) -> PhysicalKey {
    match dom_key_code(code) {
        Some(key) => PhysicalKey::Code(key),
        None => PhysicalKey::Unidentified(winit::keyboard::NativeKeyCode::Unidentified),
    }
}

/// Map a UI Events `KeyboardEvent.code` onto winit's physical key. Character
/// keys are irrelevant here: they reach the runtime through the `input` event.
fn dom_key_code(code: &str) -> Option<KeyCode> {
    use KeyCode as K;
    let mapped = match code {
        "KeyA" => K::KeyA,
        "KeyB" => K::KeyB,
        "KeyC" => K::KeyC,
        "KeyD" => K::KeyD,
        "KeyE" => K::KeyE,
        "KeyF" => K::KeyF,
        "KeyG" => K::KeyG,
        "KeyH" => K::KeyH,
        "KeyI" => K::KeyI,
        "KeyJ" => K::KeyJ,
        "KeyK" => K::KeyK,
        "KeyL" => K::KeyL,
        "KeyM" => K::KeyM,
        "KeyN" => K::KeyN,
        "KeyO" => K::KeyO,
        "KeyP" => K::KeyP,
        "KeyQ" => K::KeyQ,
        "KeyR" => K::KeyR,
        "KeyS" => K::KeyS,
        "KeyT" => K::KeyT,
        "KeyU" => K::KeyU,
        "KeyV" => K::KeyV,
        "KeyW" => K::KeyW,
        "KeyX" => K::KeyX,
        "KeyY" => K::KeyY,
        "KeyZ" => K::KeyZ,
        "Digit0" => K::Digit0,
        "Digit1" => K::Digit1,
        "Digit2" => K::Digit2,
        "Digit3" => K::Digit3,
        "Digit4" => K::Digit4,
        "Digit5" => K::Digit5,
        "Digit6" => K::Digit6,
        "Digit7" => K::Digit7,
        "Digit8" => K::Digit8,
        "Digit9" => K::Digit9,
        "NumpadMultiply" => K::NumpadMultiply,
        "NumpadAdd" => K::NumpadAdd,
        "NumpadSubtract" => K::NumpadSubtract,
        "NumpadDecimal" => K::NumpadDecimal,
        "NumpadDivide" => K::NumpadDivide,
        "Numpad0" => K::Numpad0,
        "Numpad1" => K::Numpad1,
        "Numpad2" => K::Numpad2,
        "Numpad3" => K::Numpad3,
        "Numpad4" => K::Numpad4,
        "Numpad5" => K::Numpad5,
        "Numpad6" => K::Numpad6,
        "Numpad7" => K::Numpad7,
        "Numpad8" => K::Numpad8,
        "Numpad9" => K::Numpad9,
        "Backspace" => K::Backspace,
        "Delete" => K::Delete,
        "Insert" => K::Insert,
        "Tab" => K::Tab,
        "Enter" | "NumpadEnter" => K::Enter,
        "ShiftLeft" | "ShiftRight" => K::ShiftLeft,
        "ControlLeft" | "ControlRight" => K::ControlLeft,
        "AltLeft" | "AltRight" => K::AltLeft,
        "MetaLeft" | "MetaRight" | "OSLeft" | "OSRight" => K::SuperLeft,
        "CapsLock" => K::CapsLock,
        "Escape" => K::Escape,
        "Space" => K::Space,
        "PageUp" => K::PageUp,
        "PageDown" => K::PageDown,
        "Home" => K::Home,
        "End" => K::End,
        "ArrowLeft" => K::ArrowLeft,
        "ArrowUp" => K::ArrowUp,
        "ArrowRight" => K::ArrowRight,
        "ArrowDown" => K::ArrowDown,
        "Minus" => K::Minus,
        "Equal" => K::Equal,
        "BracketLeft" => K::BracketLeft,
        "BracketRight" => K::BracketRight,
        "Backslash" => K::Backslash,
        "IntlBackslash" => K::IntlBackslash,
        "Semicolon" => K::Semicolon,
        "Quote" => K::Quote,
        "Comma" => K::Comma,
        "Period" => K::Period,
        "Slash" => K::Slash,
        "Backquote" => K::Backquote,
        "ContextMenu" => K::ContextMenu,
        "BrowserBack" | "GoBack" => K::BrowserBack,
        "BrowserForward" | "GoForward" => K::BrowserForward,
        "BrowserHome" | "GoHome" => K::BrowserHome,
        "F1" => K::F1,
        "F2" => K::F2,
        "F3" => K::F3,
        "F4" => K::F4,
        "F5" => K::F5,
        "F6" => K::F6,
        "F7" => K::F7,
        "F8" => K::F8,
        "F9" => K::F9,
        "F10" => K::F10,
        "F11" => K::F11,
        "F12" => K::F12,
        _ => return None,
    };
    Some(mapped)
}

/// Caret (or conversion segment) position inside the preedit, as a byte range.
fn active_range(
    input: &HtmlInputElement,
    text: &str,
    prefix_chars: usize,
    preedit: &str,
) -> Option<(usize, usize)> {
    let start = input.selection_start().ok().flatten()? as usize;
    let end = input.selection_end().ok().flatten()? as usize;
    let utf16 = text.encode_utf16().collect::<Vec<_>>();
    if start > utf16.len() || end > utf16.len() {
        // Android Chrome can report selections past the value's length here.
        return None;
    }
    let chars_before = String::from_utf16_lossy(&utf16[..start]).chars().count();
    let chars_in = String::from_utf16_lossy(&utf16[start..end]).chars().count();
    let index = chars_before.saturating_sub(prefix_chars);
    let byte_start = char_byte_offset(preedit, index);
    Some((byte_start, byte_start + chars_in))
}

fn char_byte_offset(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map_or(text.len(), |(offset, _)| offset)
}

fn keyboard_attributes(
    purpose: ImePurposeHint,
    auto_correct: bool,
    capitalization: KeyboardCapitalization,
    keyboard_type: KeyboardType,
    action: ImeAction,
) -> Keyboard {
    use KeyboardType as T;
    let input_mode = match purpose {
        ImePurposeHint::Password => "text",
        ImePurposeHint::Email => "email",
        ImePurposeHint::Url => "url",
        ImePurposeHint::Phone => "tel",
        ImePurposeHint::Number => match keyboard_type {
            T::Decimal | T::DecimalSigned => "decimal",
            _ => "numeric",
        },
        ImePurposeHint::Normal => match keyboard_type {
            T::Email | T::EmailSubject => "email",
            T::Uri => "url",
            T::Phone => "tel",
            T::Number | T::NumberSigned => "numeric",
            T::Decimal | T::DecimalSigned => "decimal",
            _ => "text",
        },
    };
    let capitalization = match capitalization {
        KeyboardCapitalization::None => "none",
        KeyboardCapitalization::Characters => "characters",
        KeyboardCapitalization::Words => "words",
        KeyboardCapitalization::Sentences => "sentences",
        KeyboardCapitalization::Unspecified => "off",
    };
    let enter_key_hint = match action {
        ImeAction::Go => "go",
        ImeAction::Search => "search",
        ImeAction::Send => "send",
        ImeAction::Next => "next",
        ImeAction::Previous => "previous",
        ImeAction::None => "enter",
        ImeAction::Done | ImeAction::Default | ImeAction::Unspecified => "done",
    };
    Keyboard {
        input_mode,
        auto_correct,
        capitalization,
        enter_key_hint,
    }
}

fn wake_up(window: &Rc<RefCell<Option<Arc<Window>>>>) {
    repose_core::request_frame();
    if let Some(window) = window.borrow().as_ref() {
        window.request_redraw();
    }
}

fn has_focus(element: &HtmlInputElement) -> bool {
    let active: Option<web_sys::Element> = element
        .owner_document()
        .and_then(|document| document.active_element());
    active.as_ref() == Some(element)
}

fn focus_without_scroll(element: &HtmlInputElement) {
    let options = web_sys::FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = element.focus_with_options(&options);
}

fn common_prefix_len(a: &str, b: &str) -> usize {
    core::iter::zip(a.chars(), b.chars())
        .take_while(|(a, b)| a == b)
        .count()
}
