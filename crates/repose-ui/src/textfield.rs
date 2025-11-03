use repose_core::*;
use std::ops::Range;
use std::rc::Rc;
use std::time::Duration;
use std::{cell::RefCell, time::Instant};

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use fontdb::Database;
use std::sync::OnceLock;

pub const TF_FONT_PX: f32 = 16.0;
pub const TF_PADDING_X: f32 = 8.0;

use unicode_segmentation::UnicodeSegmentation;

pub struct TextMetrics {
    /// positions[i] = advance up to the i-th grapheme (len == graphemes + 1)
    pub positions: Vec<f32>,
    /// byte_offsets[i] = byte index of the i-th grapheme (last == text.len())
    pub byte_offsets: Vec<usize>,
}

pub fn measure_text(text: &str, px: u32) -> TextMetrics {
    let m = repose_text::metrics_for_textfield(text, px as f32);
    TextMetrics {
        positions: m.positions,
        byte_offsets: m.byte_offsets,
    }
}

pub fn byte_to_char_index(m: &TextMetrics, byte: usize) -> usize {
    // Now returns grapheme index for a byte position
    match m.byte_offsets.binary_search(&byte) {
        Ok(i) | Err(i) => i,
    }
}

pub fn index_for_x_bytes(text: &str, px: u32, x: f32) -> usize {
    let m = measure_text(text, px);
    // nearest grapheme boundary -> byte index
    let mut best_i = 0usize;
    let mut best_d = f32::INFINITY;
    for i in 0..m.positions.len() {
        let d = (m.positions[i] - x).abs();
        if d < best_d {
            best_d = d;
            best_i = i;
        }
    }
    m.byte_offsets[best_i]
}

/// find prev/next grapheme boundaries around a byte index
fn prev_grapheme_boundary(text: &str, byte: usize) -> usize {
    let mut last = 0usize;
    for (i, _) in text.grapheme_indices(true) {
        if i >= byte {
            break;
        }
        last = i;
    }
    last
}

fn next_grapheme_boundary(text: &str, byte: usize) -> usize {
    for (i, _) in text.grapheme_indices(true) {
        if i > byte {
            return i;
        }
    }
    text.len()
}

// Returns cumulative X positions per codepoint boundary (len+1 entries; pos[0] = 0, pos[i] = advance up to char i)
pub fn positions_for(text: &str, px: u32) -> Vec<f32> {
    repose_text::metrics_for_textfield(text, px as f32).positions
}

// Clamp to [0..=len], nearest insertion point for a given x (content coords, before scroll)
pub fn index_for_x(text: &str, px: u32, x: f32) -> usize {
    let m = repose_text::metrics_for_textfield(text, px as f32);
    // nearest boundary
    let mut best = 0usize;
    let mut dmin = f32::INFINITY;
    for (i, &xx) in m.positions.iter().enumerate() {
        let d = (xx - x).abs();
        if d < dmin {
            dmin = d;
            best = i;
        }
    }
    best
}

#[derive(Clone, Debug)]
pub struct TextFieldState {
    pub text: String,
    pub selection: Range<usize>,
    pub composition: Option<Range<usize>>, // IME composition range
    pub scroll_offset: f32,
    pub drag_anchor: Option<usize>, // caret index where drag began
    pub blink_start: Instant,       // caret's
    pub inner_width: f32,
}

impl TextFieldState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            selection: 0..0,
            composition: None,
            scroll_offset: 0.0,
            drag_anchor: None,
            blink_start: Instant::now(),
            inner_width: 0.0,
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        let start = self.selection.start.min(self.text.len());
        let end = self.selection.end.min(self.text.len());

        self.text.replace_range(start..end, text);
        let new_pos = start + text.len();
        self.selection = new_pos..new_pos;
        self.reset_caret_blink();
    }

    pub fn delete_backward(&mut self) {
        if self.selection.start == self.selection.end {
            let pos = self.selection.start.min(self.text.len());
            if pos > 0 {
                let prev = prev_grapheme_boundary(&self.text, pos);
                self.text.replace_range(prev..pos, "");
                self.selection = prev..prev;
            }
        } else {
            self.insert_text("");
        }
        self.reset_caret_blink();
    }

    pub fn delete_forward(&mut self) {
        if self.selection.start == self.selection.end {
            let pos = self.selection.start.min(self.text.len());
            if pos < self.text.len() {
                let next = next_grapheme_boundary(&self.text, pos);
                self.text.replace_range(pos..next, "");
            }
        } else {
            self.insert_text("");
        }
        self.reset_caret_blink();
    }

    pub fn move_cursor(&mut self, delta: isize, extend_selection: bool) {
        let mut pos = self.selection.end.min(self.text.len());
        if delta < 0 {
            for _ in 0..delta.unsigned_abs() {
                pos = prev_grapheme_boundary(&self.text, pos);
            }
        } else if delta > 0 {
            for _ in 0..(delta as usize) {
                pos = next_grapheme_boundary(&self.text, pos);
            }
        }
        if extend_selection {
            self.selection.end = pos;
        } else {
            self.selection = pos..pos;
        }
        self.reset_caret_blink();
    }

    pub fn selected_text(&self) -> String {
        if self.selection.start == self.selection.end {
            String::new()
        } else {
            self.text[self.selection.clone()].to_string()
        }
    }

    pub fn set_composition(&mut self, text: String, cursor: Option<(usize, usize)>) {
        if text.is_empty() {
            if let Some(range) = self.composition.take() {
                let s = clamp_to_char_boundary(&self.text, range.start.min(self.text.len()));
                let e = clamp_to_char_boundary(&self.text, range.end.min(self.text.len()));
                if s <= e {
                    self.text.replace_range(s..e, "");
                    self.selection = s..s;
                }
            }
            self.reset_caret_blink();
            return;
        }

        let anchor_start;
        if let Some(r) = self.composition.take() {
            // Clamp to current text and char boundaries
            let mut s = clamp_to_char_boundary(&self.text, r.start.min(self.text.len()));
            let mut e = clamp_to_char_boundary(&self.text, r.end.min(self.text.len()));
            if e < s {
                std::mem::swap(&mut s, &mut e);
            }
            self.text.replace_range(s..e, &text);
            anchor_start = s;
        } else {
            // Insert at caret (snap to boundary)
            let pos = clamp_to_char_boundary(&self.text, self.selection.start.min(self.text.len()));
            self.text.insert_str(pos, &text);
            anchor_start = pos;
        }

        // Update composition range to the new inserted/replaced span
        self.composition = Some(anchor_start..(anchor_start + text.len()));

        // Map IME cursor (char indices in `text`) to byte offsets relative to anchor_start
        if let Some((c0, c1)) = cursor {
            let b0 = char_to_byte(&text, c0);
            let b1 = char_to_byte(&text, c1);
            self.selection = (anchor_start + b0)..(anchor_start + b1);
        } else {
            let end = anchor_start + text.len();
            self.selection = end..end;
        }

        self.reset_caret_blink();
    }

    pub fn commit_composition(&mut self, text: String) {
        if let Some(r) = self.composition.take() {
            let s = clamp_to_char_boundary(&self.text, r.start.min(self.text.len()));
            let e = clamp_to_char_boundary(&self.text, r.end.min(self.text.len()));
            self.text.replace_range(s..e, &text);
            let new_pos = s + text.len();
            self.selection = new_pos..new_pos;
        } else {
            // No active composition: insert at caret
            let pos = clamp_to_char_boundary(&self.text, self.selection.end.min(self.text.len()));
            self.text.insert_str(pos, &text);
            let new_pos = pos + text.len();
            self.selection = new_pos..new_pos;
        }
        self.reset_caret_blink();
    }

    pub fn cancel_composition(&mut self) {
        if let Some(r) = self.composition.take() {
            let s = clamp_to_char_boundary(&self.text, r.start.min(self.text.len()));
            let e = clamp_to_char_boundary(&self.text, r.end.min(self.text.len()));
            if s <= e {
                self.text.replace_range(s..e, "");
                self.selection = s..s;
            }
        }
        self.reset_caret_blink();
    }

    pub fn delete_surrounding(&mut self, before_bytes: usize, after_bytes: usize) {
        if self.selection.start != self.selection.end {
            let start = self.selection.start.min(self.text.len());
            let end = self.selection.end.min(self.text.len());
            self.text.replace_range(start..end, "");
            self.selection = start..start;
            self.reset_caret_blink();
            return;
        }

        let caret = self.selection.end.min(self.text.len());
        let start_raw = caret.saturating_sub(before_bytes);
        let end_raw = (caret + after_bytes).min(self.text.len());
        // Snap to nearest safe boundaries
        let start = prev_grapheme_boundary(&self.text, start_raw);
        let end = next_grapheme_boundary(&self.text, end_raw);
        if start < end {
            self.text.replace_range(start..end, "");
            self.selection = start..start;
        }
        self.reset_caret_blink();
    }

    // Begin a selection on press; if extend==true, keep existing anchor; else set new anchor
    pub fn begin_drag(&mut self, idx_byte: usize, extend: bool) {
        let idx = idx_byte.min(self.text.len());
        if extend {
            let anchor = self.selection.start;
            self.selection = anchor.min(idx)..anchor.max(idx);
            self.drag_anchor = Some(anchor);
        } else {
            self.selection = idx..idx;
            self.drag_anchor = Some(idx);
        }
        self.reset_caret_blink();
    }

    pub fn drag_to(&mut self, idx_byte: usize) {
        if let Some(anchor) = self.drag_anchor {
            let i = idx_byte.min(self.text.len());
            self.selection = anchor.min(i)..anchor.max(i);
        }
        self.reset_caret_blink();
    }
    pub fn end_drag(&mut self) {
        self.drag_anchor = None;
    }

    pub fn caret_index(&self) -> usize {
        self.selection.end
    }

    // Keep caret visible inside inner content width
    pub fn ensure_caret_visible(&mut self, caret_x: f32, inner_width: f32) {
        // small 2px inset
        let inset = 2.0;
        let left = self.scroll_offset + inset;
        let right = self.scroll_offset + inner_width - inset;
        if caret_x < left {
            self.scroll_offset = (caret_x - inset).max(0.0);
        } else if caret_x > right {
            self.scroll_offset = (caret_x - inner_width + inset).max(0.0);
        }
    }

    pub fn reset_caret_blink(&mut self) {
        self.blink_start = Instant::now();
    }
    pub fn caret_visible(&self) -> bool {
        const PERIOD: Duration = Duration::from_millis(500);
        ((Instant::now() - self.blink_start).as_millis() / PERIOD.as_millis() as u128) % 2 == 0
    }

    pub fn set_inner_width(&mut self, w: f32) {
        self.inner_width = w.max(0.0);
    }
}

// Platform-managed state: no Rc in builder, hint only.
pub fn TextField(
    hint: impl Into<String>,
    modifier: Modifier,
    on_change: impl Fn(String) + 'static,
) -> View {
    View::new(
        0,
        ViewKind::TextField {
            state_key: 0,
            hint: hint.into(),
            on_change: Some(Rc::new(on_change)),
        },
    )
    .modifier(modifier)
    .semantics(Semantics {
        role: Role::TextField,
        label: None,
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

    #[test]
    fn test_grapheme_delete_and_move() {
        // "👍🏽" is a grapheme cluster (thumbs up + skin tone)
        let mut st = TextFieldState::new();
        st.insert_text("A👍🏽B");
        assert_eq!(st.text, "A👍🏽B");
        // Move left over 'B'
        st.move_cursor(-1, false);
        assert_eq!(st.selection.end, "A👍🏽".len());
        st.delete_backward();
        assert_eq!(st.text, "AB");
        assert_eq!(st.selection, "A".len().."A".len());
    }

    #[test]
    fn test_index_for_x_bytes_grapheme() {
        // Ensure we return boundaries consistent with graphemes
        let t = "A👍🏽B";
        let px = 16u32;
        let m = measure_text(t, px);
        // All byte_offsets must be grapheme boundaries
        for i in 0..m.byte_offsets.len() - 1 {
            let b = m.byte_offsets[i];
            // slicing at boundary must be OK
            let _ = &t[..b];
        }
    }
}

fn clamp_to_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(i) {
        return i;
    }
    // walk back to previous valid boundary
    let mut j = i;
    while j > 0 && !s.is_char_boundary(j) {
        j -= 1;
    }
    j
}

fn char_to_byte(s: &str, ci: usize) -> usize {
    if ci == 0 {
        0
    } else {
        s.char_indices().nth(ci).map(|(i, _)| i).unwrap_or(s.len())
    }
}
