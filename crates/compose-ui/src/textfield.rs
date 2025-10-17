use compose_core::*;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct TextFieldState {
    pub text: String,
    pub selection: Range<usize>,
    pub composition: Option<Range<usize>>, // IME composition range
    pub scroll_offset: f32,
}

impl TextFieldState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            selection: 0..0,
            composition: None,
            scroll_offset: 0.0,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        let start = self.selection.start.min(self.text.len());
        let end = self.selection.end.min(self.text.len());

        self.text.replace_range(start..end, text);
        let new_pos = start + text.len();
        self.selection = new_pos..new_pos;
    }

    pub fn delete_backward(&mut self) {
        if self.selection.start == self.selection.end {
            if self.selection.start > 0 {
                let pos = self.selection.start - 1;
                self.text.remove(pos);
                self.selection = pos..pos;
            }
        } else {
            self.insert_text("");
        }
    }

    pub fn delete_forward(&mut self) {
        if self.selection.start == self.selection.end {
            if self.selection.start < self.text.len() {
                self.text.remove(self.selection.start);
            }
        } else {
            self.insert_text("");
        }
    }

    pub fn move_cursor(&mut self, delta: isize, extend_selection: bool) {
        let pos = if delta < 0 {
            self.selection.end.saturating_sub(delta.unsigned_abs())
        } else {
            (self.selection.end + delta as usize).min(self.text.len())
        };

        if extend_selection {
            self.selection.end = pos;
        } else {
            self.selection = pos..pos;
        }
    }

    pub fn set_composition(&mut self, text: String, cursor: Option<(usize, usize)>) {
        if let Some(range) = &self.composition {
            // Replace existing composition
            self.text.replace_range(range.clone(), &text);
        } else {
            // Start new composition
            let pos = self.selection.start;
            self.text.insert_str(pos, &text);
        }

        let start = self.selection.start;
        self.composition = Some(start..start + text.len());

        if let Some((cursor_start, cursor_end)) = cursor {
            self.selection = (start + cursor_start)..(start + cursor_end);
        }
    }

    pub fn commit_composition(&mut self, text: String) {
        if let Some(range) = self.composition.take() {
            self.text.replace_range(range.clone(), &text);
            let new_pos = range.start + text.len();
            self.selection = new_pos..new_pos;
        }
    }

    pub fn cancel_composition(&mut self) {
        if let Some(range) = self.composition.take() {
            self.text.replace_range(range, "");
        }
    }

    pub fn delete_surrounding(&mut self, before_bytes: usize, after_bytes: usize) {
        // Simplified: delete around current caret end, respecting selection first
        if self.selection.start != self.selection.end {
            // If selection active, delete it
            let start = self.selection.start.min(self.text.len());
            let end = self.selection.end.min(self.text.len());
            self.text.replace_range(start..end, "");
            self.selection = start..start;
            return;
        }

        let caret = self.selection.end.min(self.text.len());
        let start = caret.saturating_sub(before_bytes);
        let end = (caret + after_bytes).min(self.text.len());
        if start < end {
            self.text.replace_range(start..end, "");
            self.selection = start..start;
        }
    }
}

// Platform-managed state: no Rc in builder, hint only.
pub fn TextField(hint: impl Into<String>, modifier: Modifier) -> View {
    let display_text = hint.into();

    View::new(
        0,
        ViewKind::TextField {
            state_key: 0, // assigned during composition
            hint: display_text.clone(),
        },
    )
    .modifier(modifier)
    .semantics(Semantics {
        role: Role::TextField,
        label: Some(display_text),
        focused: false,
        enabled: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_textfield_insert() {
        let mut state = TextFieldState::new();
        state.insert_text("Hello");
        assert_eq!(state.text, "Hello");
        assert_eq!(state.selection, 5..5);
    }

    #[test]
    fn test_textfield_delete_backward() {
        let mut state = TextFieldState::new();
        state.insert_text("Hello");
        state.delete_backward();
        assert_eq!(state.text, "Hell");
        assert_eq!(state.selection, 4..4);
    }

    #[test]
    fn test_textfield_selection() {
        let mut state = TextFieldState::new();
        state.insert_text("Hello World");
        state.selection = 0..5; // Select "Hello"
        state.insert_text("Hi");
        assert_eq!(state.text, "Hi World");
        assert_eq!(state.selection, 2..2);
    }

    #[test]
    fn test_textfield_ime_composition() {
        let mut state = TextFieldState::new();
        state.insert_text("Test ");
        state.set_composition("日本".to_string(), Some((0, 2)));
        assert_eq!(state.text, "Test 日本");
        assert!(state.composition.is_some());

        state.commit_composition("日本語".to_string());
        assert_eq!(state.text, "Test 日本語");
        assert!(state.composition.is_none());
    }

    #[test]
    fn test_textfield_cursor_movement() {
        let mut state = TextFieldState::new();
        state.insert_text("Hello");
        state.move_cursor(-2, false);
        assert_eq!(state.selection, 3..3);

        state.move_cursor(1, false);
        assert_eq!(state.selection, 4..4);
    }

    #[test]
    fn test_delete_surrounding() {
        let mut state = TextFieldState::new();
        state.insert_text("Hello");
        // caret at 5
        state.delete_surrounding(2, 1); // delete "lo"
        assert_eq!(state.text, "Hel");
        assert_eq!(state.selection, 3..3);
    }
}
