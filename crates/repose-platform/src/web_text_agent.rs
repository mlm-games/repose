//! Hidden `<textarea>` that gives mobile browsers an editor to hang the soft
//! keyboard off, plus the DOM -> runtime translation for its events.
//!
//! winit's web backend has no IME support: `set_ime_allowed` and
//! `set_ime_cursor_area` are no-ops and `WindowEvent::Ime` is never emitted,
//! so a bare `<canvas>` never raises the on-screen keyboard. The agent is a 1x1
//! transparent `<textarea>` parked as a sibling of the canvas (so focusing it
//! cannot scroll the page away), dragged onto the focused field and focused for
//! as long as an editable text field holds focus.
//!
//! The focused field's text and selection are copied into the element on every
//! frame, which makes the browser's editor agree with Repose's caret: the soft
//! keyboard's backspace and word-delete then act where the caret actually is,
//! and `input` events diff against the field's real content rather than a
//! scratch buffer. The two can only disagree while an IME composition is
//! running, which is the one moment the keyboard owns the buffer.

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{HtmlCanvasElement, HtmlTextAreaElement};

use winit::keyboard::{KeyCode, ModifiersState, PhysicalKey};
use winit::window::Window;

use repose_core::input::ImeEvent;
use repose_core::{ImeAction, ImePurposeHint, KeyboardCapitalization, KeyboardType};
use repose_ui::textfield::TextFieldState;

/// Runtime work produced by a DOM callback. DOM handlers cannot touch the
/// runtime (winit's event loop owns it), so they queue and the runner drains.
pub enum AgentEvent {
    Ime(ImeEvent),
    /// Text typed outside any composition. Uses the runtime's insertion path
    /// rather than `ImeEvent::Commit`, which appends at the caret instead of
    /// replacing the selection.
    Insert(String),
    /// Bytes the keyboard removed around the caret, as one edit. A count rather
    /// than a chord: deleting a selection is a single operation however long it
    /// is, so a chord per character would over-delete.
    DeleteSurrounding {
        before: usize,
        after: usize,
    },
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
    editor: HtmlTextAreaElement,
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

        // A `<textarea>` rather than an `<input>`: a single-line input silently
        // drops newlines from its value, which would desync the mirror below
        // for every multiline field.
        let editor: HtmlTextAreaElement = document
            .create_element("textarea")?
            .dyn_into::<HtmlTextAreaElement>()?;
        editor.set_rows(1);
        editor.set_attribute("autocapitalize", "off")?;
        editor.set_attribute("autocomplete", "off")?;
        editor.set_attribute("aria-hidden", "true")?;

        let style = editor.style();
        for (name, value) in [
            ("background-color", "transparent"),
            ("border", "none"),
            ("outline", "none"),
            ("color", "transparent"),
            ("caret-color", "transparent"),
            ("position", "absolute"),
            ("width", "1px"),
            ("height", "1px"),
            ("padding", "0"),
            ("margin", "0"),
            ("resize", "none"),
            ("overflow", "hidden"),
            ("white-space", "pre"),
            // Below 16px iOS Safari zooms the page when the field focuses.
            ("font-size", "16px"),
            // The agent sits on top of the canvas; taps must still reach it.
            ("pointer-events", "none"),
            ("opacity", "0"),
        ] {
            style.set_property(name, value)?;
        }
        if let Some(parent) = canvas.parent_node() {
            parent.insert_before(&editor, canvas.next_sibling().as_ref())?;
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

        let element: web_sys::EventTarget = editor.clone().into();
        let canvas_target: web_sys::EventTarget = canvas.clone().into();

        let agent_state = state.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "compositionstart",
            Box::new(move |_| {
                agent_state.borrow_mut().composing = true;
                wake_up(&wake);
            }),
        )?;

        let agent_editor = editor.clone();
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
                    state.handle_input(&agent_editor, &event, &mut events);
                }
                wake_up(&wake);
            }),
        )?;

        let agent_editor = editor.clone();
        let agent_state = state.clone();
        let agent_events = events.clone();
        let wake = redraw.clone();
        listen(
            &element,
            "compositionend",
            Box::new(move |_| {
                let mut state = agent_state.borrow_mut();
                let mut events = agent_events.borrow_mut();
                state.handle_composition_end(&agent_editor, &mut events);
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
        let touch_editor = editor.clone();
        listen(
            &canvas_target,
            "touchend",
            Box::new(move |_| {
                if has_focus(&touch_editor) {
                    let _ = touch_editor.blur();
                    focus_without_scroll(&touch_editor);
                }
            }),
        )?;

        Ok(Self {
            editor,
            state,
            events,
            _listeners: listeners,
            caret: None,
            keyboard: None,
            focused_field: None,
        })
    }

    /// Move the agent onto the focused field, mirror that field's text and
    /// selection into it, and reconfigure the keyboard the browser derives
    /// from its attributes. `focused` is the runner's verdict on whether an
    /// editable text field currently holds focus.
    #[expect(clippy::too_many_arguments)]
    pub fn sync(
        &mut self,
        canvas: &HtmlCanvasElement,
        focused: Option<u64>,
        field: Option<&TextFieldState>,
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
            let _ = self.editor.set_attribute("inputmode", keyboard.input_mode);
            let _ = self.editor.set_attribute(
                "autocorrect",
                if keyboard.auto_correct { "on" } else { "off" },
            );
            let _ = self
                .editor
                .set_attribute("autocapitalize", keyboard.capitalization);
            let _ = self
                .editor
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
                let style = self.editor.style();
                let left = canvas.offset_left() as f64 + (x + w * 0.5).clamp(0.0, max_x);
                let top = canvas.offset_top() as f64 + (y + h * 0.5).clamp(0.0, max_y);
                let _ = style.set_property("left", &format!("{left}px"));
                let _ = style.set_property("top", &format!("{top}px"));
            }
        }

        match field {
            Some(field) => {
                // Mirror before focusing: the keyboard reads the editor's
                // content the moment it opens.
                self.mirror(field);
                if !self.has_focus() {
                    focus_without_scroll(&self.editor);
                }
            }
            None => {
                if self.has_focus() {
                    let _ = self.editor.blur();
                }
                self.reset();
            }
        }
    }

    /// `true` while the agent owns DOM focus, i.e. the soft keyboard's editor.
    pub fn has_focus(&self) -> bool {
        has_focus(&self.editor)
    }

    /// Take everything the DOM queued since the last drain.
    pub fn drain_events(&self) -> Vec<AgentEvent> {
        std::mem::take(&mut self.events.borrow_mut())
    }

    /// Give DOM focus back to the page and discard pending input: the window
    /// lost focus, so no further text can land in the runtime.
    pub fn hide(&mut self) {
        if self.has_focus() {
            let _ = self.editor.blur();
        }
        self.reset();
        self.focused_field = None;
        self.events.borrow_mut().clear();
    }

    /// Copy the field's text and selection into the element. Everything the
    /// keyboard derives from that buffer - where backspace deletes, which word
    /// the suggestion strip replaces - then matches what Repose will do.
    fn mirror(&mut self, field: &TextFieldState) {
        let mut state = self.state.borrow_mut();
        if state.composing {
            return;
        }

        let text = field.text.as_str();
        if self.editor.value() != text {
            self.editor.set_value(text);
        }
        let start = char_boundary(text, field.selection.start);
        let end = char_boundary(text, field.selection.end);
        let (from, to) = (utf16_len(&text[..start]), utf16_len(&text[..end]));
        if self.editor.selection_start().ok().flatten() != Some(from)
            || self.editor.selection_end().ok().flatten() != Some(to)
        {
            let _ = self.editor.set_selection_range(from, to);
        }
        state.text.clear();
        state.text.push_str(text);
        state.selection = start..end;
    }

    /// Drop the DOM buffer and the diff baseline together. Splitting the two is
    /// what makes the next diff read the whole buffer as freshly typed text.
    fn reset(&mut self) {
        let mut state = self.state.borrow_mut();
        state.text.clear();
        state.selection = 0..0;
        state.composing = false;
        state.preedit_sent = false;
        state.served = None;
        state.special = KeydownSpecialCase::None;
        self.editor.set_value("");
    }
}

impl Drop for TextAgent {
    fn drop(&mut self) {
        self.editor.remove();
    }
}

#[derive(Default)]
struct InputState {
    /// The Repose field's text as of the last event the runtime has consumed.
    text: String,
    /// The Repose field's selection, which the runtime still holds until the
    /// queued events are drained.
    selection: Range<usize>,
    composing: bool,
    /// A preedit has been reported, so the composition must be closed even if
    /// the IME commits nothing.
    preedit_sent: bool,
    /// `inputType` already served by a chord from the keydown that preceded it.
    served: Option<&'static str>,
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
        editor: &HtmlTextAreaElement,
        event: &web_sys::InputEvent,
        out: &mut Vec<AgentEvent>,
    ) {
        let input_type = event.input_type();
        let value = editor.value();

        if event.is_composing() {
            let prefix = common_prefix_bytes(&value, &self.text);
            // Whatever follows the composing segment is the field's own tail,
            // untouched by the IME. It must stay out of the preedit or Repose
            // underlines it as composing text and re-inserts it.
            let suffix = common_suffix_bytes(&value, &self.text, prefix);
            let preedit: String = value[prefix..value.len() - suffix].to_string();
            let removed = self.text.len() - prefix - suffix;
            if removed > 0 {
                // The IME replaced the selection to begin composing, and
                // `set_composition` inserts at the caret rather than replacing
                // it, so the selection has to go first.
                out.push(AgentEvent::DeleteSurrounding {
                    before: removed,
                    after: 0,
                });
            }
            let cursor = active_range(editor, &value, prefix, &preedit);
            out.push(AgentEvent::Ime(ImeEvent::Update {
                text: preedit,
                cursor,
            }));
            self.preedit_sent = true;
            self.text = value[..prefix].to_string();
            return;
        }
        self.composing = false;

        let served = self
            .served
            .take()
            .filter(|served| *served == input_type.as_str());

        if input_type == "insertLineBreak" {
            // The runtime owns Enter: newline for a multiline field, submit for
            // a single-line one, which `insert_text_into_focused` would not do.
            if served.is_none() {
                push_key_chord(out, KeyCode::Enter);
            }
            self.text = value;
            return;
        }

        let prefix = common_prefix_bytes(&value, &self.text);
        let suffix = common_suffix_bytes(&value, &self.text, prefix);
        let inserted = &value[prefix..value.len() - suffix];
        let removed = self.text.len() - prefix - suffix;
        // Typing over a selection is one edit in the runtime, which replaces the
        // selection as it inserts. Deleting first would split it into two undo
        // steps, so the removal is left to the insert when it covers exactly
        // the selection the runtime still holds.
        let replaces_selection = !inserted.is_empty()
            && self.selection
                == Range {
                    start: prefix,
                    end: prefix + removed,
                };

        if removed > 0 && served.is_none() && !replaces_selection {
            let forward = input_type.ends_with("Forward");
            out.push(AgentEvent::DeleteSurrounding {
                before: if forward { 0 } else { removed },
                after: if forward { removed } else { 0 },
            });
        }
        if !inserted.is_empty() {
            out.push(AgentEvent::Insert(inserted.to_string()));
        }
        self.text = value;
    }

    fn handle_composition_end(&mut self, editor: &HtmlTextAreaElement, out: &mut Vec<AgentEvent>) {
        self.composing = false;
        let value = editor.value();
        let prefix = common_prefix_bytes(&value, &self.text);
        let commit = value[prefix..].to_string();
        // A commit is what tells the runtime the preedit is over, and an IME
        // can end without committing anything: without one the field would
        // refuse every edit until focus moved. Only after a preedit was
        // actually delivered, so that an untouched composition stays silent.
        if !commit.is_empty() || self.preedit_sent {
            out.push(AgentEvent::Ime(ImeEvent::Commit(commit)));
        }
        self.preedit_sent = false;
        self.text = value;
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
            self.served = None;
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

        // Only meaningful for the keydown immediately below: a later soft
        // keyboard backspace must not be mistaken for this key's own input
        // event, which `prevent_default` normally suppresses altogether.
        self.served = None;

        self.special = match key_code {
            229 => KeydownSpecialCase::AndroidKeycode229,
            0 => KeydownSpecialCase::IosKeycode0,
            _ => KeydownSpecialCase::None,
        };
        if event.is_composing() || self.special != KeydownSpecialCase::None {
            return;
        }
        // AltGr plus a letter is character input, not a shortcut.
        if event.get_modifier_state("AltGraph") {
            return;
        }

        let modifiers = dom_modifiers(event);
        // "Dead" and "Unidentified" are character input too: a dead key arms
        // the accent that the next keystroke composes into one character.
        let character = event.key().chars().count() == 1
            || matches!(event.key().as_str(), "Dead" | "Unidentified");
        if character && !modifiers.intersects(MOD_KEYS) {
            // Printable characters reach the runtime through the following
            // `input` event, so forwarding the key too would insert them
            // twice. Their default must also stay enabled, because that event
            // only fires if the browser is allowed to insert them.
            return;
        }
        if code == "KeyV" && !modifiers.contains(ModifiersState::ALT) {
            // Let the element paste. Its `insertFromPaste` reaches the runtime
            // through `input`, lands at the caret, and needs no clipboard
            // permission, which is what the runtime's own paste path needs on
            // mobile and does not have.
            return;
        }

        // The runtime acts on this key, so the element must not also act on it:
        // letting the browser delete a character would land a second deletion
        // on the field once the key chord has already run.
        let key = dom_key_code(&code);
        if !key.is_some_and(|key| browser_owned(key, modifiers)) {
            event.prevent_default();
        }
        self.served = match code.as_str() {
            "Backspace" => Some("deleteContentBackward"),
            "Delete" => Some("deleteContentForward"),
            "Enter" | "NumpadEnter" => Some("insertLineBreak"),
            _ => None,
        };
        out.push(AgentEvent::Key(DomKey {
            physical: dom_physical_key(&code),
            pressed: true,
            repeat: event.repeat(),
            modifiers,
        }));
    }
}

/// Chords the browser keeps: the runtime has no use for reload, devtools or tab
/// switching, so `prevent_default` on these would only take them away.
fn browser_owned(key: KeyCode, modifiers: ModifiersState) -> bool {
    let shortcut = modifiers.intersects(MOD_KEYS);
    match key {
        KeyCode::F1
        | KeyCode::F2
        | KeyCode::F3
        | KeyCode::F4
        | KeyCode::F5
        | KeyCode::F6
        | KeyCode::F7
        | KeyCode::F8
        | KeyCode::F9
        | KeyCode::F10
        | KeyCode::F11
        | KeyCode::F12 => true,
        // Tab would blur the editor, which dismisses the keyboard. Ctrl+Tab is
        // the browser's, though.
        KeyCode::Tab => shortcut,
        _ => {
            shortcut
                && matches!(
                    key,
                    KeyCode::KeyR
                        | KeyCode::KeyT
                        | KeyCode::KeyW
                        | KeyCode::KeyN
                        | KeyCode::KeyP
                        | KeyCode::KeyS
                )
        }
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
    editor: &HtmlTextAreaElement,
    text: &str,
    prefix_bytes: usize,
    preedit: &str,
) -> Option<(usize, usize)> {
    let start = editor.selection_start().ok().flatten()? as usize;
    let end = editor.selection_end().ok().flatten()? as usize;
    let utf16 = text.encode_utf16().collect::<Vec<_>>();
    if start > utf16.len() || end > utf16.len() {
        // Android Chrome can report selections past the value's length here.
        return None;
    }
    let prefix_chars = text[..prefix_bytes].chars().count();
    let chars_before = String::from_utf16_lossy(&utf16[..start]).chars().count();
    let chars_in = String::from_utf16_lossy(&utf16[start..end]).chars().count();
    let index = chars_before.saturating_sub(prefix_chars);
    Some((
        char_byte_offset(preedit, index),
        char_byte_offset(preedit, index + chars_in),
    ))
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

fn has_focus(editor: &HtmlTextAreaElement) -> bool {
    let element: &web_sys::Element = editor.as_ref();
    let active: Option<web_sys::Element> = element
        .owner_document()
        .and_then(|document| document.active_element());
    active.as_ref() == Some(element)
}

fn focus_without_scroll(editor: &HtmlTextAreaElement) {
    let options = web_sys::FocusOptions::new();
    options.set_prevent_scroll(true);
    let _ = editor.focus_with_options(&options);
}

fn utf16_len(text: &str) -> u32 {
    text.chars().map(char::len_utf16).sum::<usize>() as u32
}

/// A byte offset the runtime's selection can safely be sliced at. The selection
/// is a public field, so an app or input transformation can leave it inside a
/// multi-byte character.
fn char_boundary(text: &str, index: usize) -> usize {
    let index = index.min(text.len());
    if text.is_char_boundary(index) {
        index
    } else {
        text.floor_char_boundary(index)
    }
}

fn common_prefix_bytes(a: &str, b: &str) -> usize {
    let mut len = 0usize;
    let mut ac = a.chars();
    let mut bc = b.chars();
    loop {
        match (ac.next(), bc.next()) {
            (Some(x), Some(y)) if x == y => len += x.len_utf8(),
            _ => return len,
        }
    }
}

/// Common trailing bytes, capped so it cannot overlap a common prefix of
/// `prefix` bytes: what is left between them is what each side replaced.
fn common_suffix_bytes(a: &str, b: &str, prefix: usize) -> usize {
    let room = a.len().min(b.len()) - prefix;
    let mut len = 0usize;
    for (x, y) in a.chars().rev().zip(b.chars().rev()) {
        if x != y || len + x.len_utf8() > room {
            break;
        }
        len += x.len_utf8();
    }
    len
}
