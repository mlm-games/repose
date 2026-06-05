//! # TextField model
//!
//! Repose TextFields are fully controlled widgets. The visual `View` only
//! describes *where* the field is and what its hint is; the *state* lives in
//! `TextFieldState`, which the platform runner owns.
//!
//! ```rust,ignore
//! pub struct TextFieldState {
//!     pub text: String,
//!     pub selection: Range<usize>,      // byte offsets
//!     pub composition: Option<Range<usize>>, // IME preedit range
//!     pub scroll_offset: f32,           // px, left edge of visible text
//!     pub drag_anchor: Option<usize>,   // selection start for drag
//!     pub blink_start: Instant,         // caret blink timer
//!     pub inner_width: f32,             // px, content box width
//! }
//! ```
//!
//! Key properties:
//!
//! - Grapheme‑safe editing: cursor movement, deletion, and selection operate
//!   on extended grapheme clusters (via `unicode-segmentation`), not raw bytes.
//! - IME support: `set_composition`, `commit_composition`, and
//!   `cancel_composition` integrate with platform IME events.
//! - Horizontal scrolling: `scroll_offset` plus `ensure_caret_visible` keep
//!   the caret within the visible inner rect.
//!
//! Platform runners (`repose-platform`) keep a `HashMap<u64, Rc<RefCell<TextFieldState>>>`
//! indexed by a stable `tf_state_key`. During layout/paint, this map is passed
//! into `layout_and_paint`, which renders:
//!
//! - Selection highlight
//! - Composition underline
//! - Text (value or hint)
//! - Caret (with blink)
//!
//! And exposes `on_text_change` / `on_text_submit` callbacks via `HitRegion`
//! so your app can react to edits.

use repose_core::*;
use std::ops::Range;
use std::rc::Rc;
use web_time::Duration;
use web_time::Instant;

use unicode_segmentation::UnicodeSegmentation;

/// Maximum number of undo/redo operations stored in history.
const TEXT_UNDO_CAPACITY: usize = 100;

/// Time window (ms) within which consecutive operations can be merged.
const SNAPSHOTS_INTERVAL_MILLIS: u128 = 5000;

/// Type of text edit operation.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TextEditType {
    Insert,
    Delete,
    Replace,
}

/// Direction of a deletion.
#[derive(Clone, Copy, Debug, PartialEq)]
enum TextDeleteType {
    Start,  // backspace: cursor moving towards start
    End,    // delete forward: cursor moving towards end
    Inner,  // selection removed
    NotByUser,
}

/// A single atomic text change that can be undone/redone.
#[derive(Clone, Debug)]
pub struct TextUndoOp {
    /// Start point of the change in the text.
    pub index: usize,
    /// Text that was present before the change (being replaced/deleted).
    pub pre_text: String,
    /// Text that was inserted (replacing pre_text).
    pub post_text: String,
    /// Selection before the change.
    pub pre_selection: Range<usize>,
    /// Selection after the change.
    pub post_selection: Range<usize>,
    /// When this change was first committed.
    pub time: Instant,
    /// Whether this change can merge with adjacent operations.
    pub can_merge: bool,
}

impl TextUndoOp {
    fn edit_type(&self) -> TextEditType {
        match (self.pre_text.is_empty(), self.post_text.is_empty()) {
            (true, true) => unreachable!("Both pre and post text cannot be empty"),
            (true, false) => TextEditType::Insert,
            (false, true) => TextEditType::Delete,
            (false, false) => TextEditType::Replace,
        }
    }

    fn is_newline(&self) -> bool {
        self.post_text == "\n" || self.post_text == "\r\n"
    }

    /// Try to merge `self` (earlier) with `next` (later). Returns merged op if merge is possible.
    fn try_merge(&self, next: &TextUndoOp) -> Option<TextUndoOp> {
        if !self.can_merge || !next.can_merge {
            return None;
        }

        let elapsed = next.time.saturating_duration_since(self.time);
        if elapsed.as_millis() >= SNAPSHOTS_INTERVAL_MILLIS {
            return None;
        }

        if self.is_newline() || next.is_newline() {
            return None;
        }

        let self_type = self.edit_type();
        if self_type != next.edit_type() {
            return None;
        }

        match self_type {
            TextEditType::Insert => {
                // Only merge if next insertion continues from the end of this one
                if self.index + self.post_text.len() == next.index {
                    Some(TextUndoOp {
                        index: self.index,
                        pre_text: String::new(),
                        post_text: format!("{}{}", self.post_text, next.post_text),
                        pre_selection: self.pre_selection.clone(),
                        post_selection: next.post_selection.clone(),
                        time: self.time,
                        can_merge: true,
                    })
                } else {
                    None
                }
            }
            TextEditType::Delete => {
                let self_del = self.deletion_type();
                let next_del = next.deletion_type();
                // Only merge consecutive deletions with same directionality
                if self_del == next_del && (self_del == TextDeleteType::Start || self_del == TextDeleteType::End) {
                    if self.index == next.index + next.pre_text.len() {
                        // This op is after next (backspace: deleting right-to-left)
                        Some(TextUndoOp {
                            index: next.index,
                            pre_text: format!("{}{}", next.pre_text, self.pre_text),
                            post_text: String::new(),
                            pre_selection: self.pre_selection.clone(),
                            post_selection: next.post_selection.clone(),
                            time: self.time,
                            can_merge: true,
                        })
                    } else if self.index == next.index {
                        // Same position (delete forward: deleting left-to-right)
                        Some(TextUndoOp {
                            index: self.index,
                            pre_text: format!("{}{}", self.pre_text, next.pre_text),
                            post_text: String::new(),
                            pre_selection: self.pre_selection.clone(),
                            post_selection: next.post_selection.clone(),
                            time: self.time,
                            can_merge: true,
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            TextEditType::Replace => None,
        }
    }

    /// Determine the deletion direction. Only meaningful when edit_type is Delete.
    fn deletion_type(&self) -> TextDeleteType {
        if self.edit_type() != TextEditType::Delete {
            return TextDeleteType::NotByUser;
        }
        if !self.post_selection.start == self.post_selection.end {
            return TextDeleteType::NotByUser;
        }
        if self.pre_selection.start == self.pre_selection.end {
            // Collapsed selection before delete: cursor moved
            if self.pre_selection.start > self.post_selection.start {
                TextDeleteType::Start    // backspace
            } else {
                TextDeleteType::End      // delete forward
            }
        } else if self.pre_selection.start == self.post_selection.start
            && self.pre_selection.start == self.index
        {
            TextDeleteType::Inner
        } else {
            TextDeleteType::NotByUser
        }
    }
}

/// Spring physics constants for smooth scroll animation.
const SCROLL_STIFFNESS: f32 = 300.0;
const SCROLL_DAMPING: f32 = 30.0;

/// Logical font size for TextField in dp (converted to px at measure/paint time).
pub const TF_FONT_DP: f32 = 16.0;

/// Configures the keyboard for a text field.
#[derive(Clone, Copy, Debug)]
pub struct KeyboardOptions {
    pub keyboard_type: repose_core::KeyboardType,
    pub autocorrect: bool,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            keyboard_type: repose_core::KeyboardType::default(),
            autocorrect: true,
        }
    }
}
/// Horizontal padding inside the TextField in dp.
pub const TF_PADDING_X_DP: f32 = 8.0;

pub struct TextMetrics {
    /// positions[i] = advance up to the i-th grapheme (len == graphemes + 1)
    pub positions: Vec<f32>, // px
    /// byte_offsets[i] = byte index of the i-th grapheme (last == text.len())
    pub byte_offsets: Vec<usize>,
}

/// Measure caret positions for a single-line textfield using shaping.
/// `font_px` must match the px size used for rendering the text.
/// `font_family` optionally overrides the default font (e.g. for icons).
pub fn measure_text(text: &str, font_px: f32, font_family: Option<&'static str>) -> TextMetrics {
    let m = repose_text::metrics_for_textfield(text, font_px, font_family);
    TextMetrics {
        positions: m.positions,
        byte_offsets: m.byte_offsets,
    }
}

pub fn byte_to_char_index(m: &TextMetrics, byte: usize) -> usize {
    match m.byte_offsets.binary_search(&byte) {
        Ok(i) | Err(i) => i,
    }
}

/// Given an x position (px), return the nearest grapheme boundary byte index.
pub fn index_for_x_bytes(text: &str, font_px: f32, x_px: f32) -> usize {
    let m = measure_text(text, font_px, None);

    let mut best_i = 0usize;
    let mut best_d = f32::INFINITY;
    for i in 0..m.positions.len() {
        let d = (m.positions[i] - x_px).abs();
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

#[derive(Clone)]
pub struct TextFieldState {
    pub text: String,
    pub selection: Range<usize>,
    pub composition: Option<Range<usize>>, // IME composition range (byte offsets)
    pub scroll_offset: f32,                // px (x) - current animated display value
    pub scroll_offset_y: f32,              // px (y) for multiline - current animated display value
    pub drag_anchor: Option<usize>,        // byte index where drag began
    pub blink_start: Instant,              // caret blink timer
    pub inner_width: f32,                  // px
    pub inner_height: f32,                 // px
    pub preferred_x_px: Option<f32>,       // for Up/Down caret movement in multiline
    /// When a visual transformation is active, this maps offsets in the
    /// display text back to offsets in the original text.
    pub offset_map: Option<Rc<dyn Fn(usize) -> usize>>,
    /// The active visual transformation, set during layout.
    pub visual_transformation: Option<Rc<dyn VisualTransformation>>,
    /// Target horizontal scroll offset (where we're animating toward).
    pub(crate) scroll_target: f32,
    /// Target vertical scroll offset.
    pub(crate) scroll_target_y: f32,
    /// Spring velocity for horizontal scroll animation.
    scroll_vel: f32,
    /// Spring velocity for vertical scroll animation.
    scroll_vel_y: f32,
    /// Last time tick_scroll_animation was called (for dt computation).
    last_scroll_tick: Option<Instant>,

    // --- Undo/Redo ---
    /// Stack of undo operations (most recent at end).
    undo_stack: Vec<TextUndoOp>,
    /// Stack of redo operations (most recent at end).
    redo_stack: Vec<TextUndoOp>,
    /// Staging area for the latest operation that may still merge.
    staging_undo: Option<TextUndoOp>,
}

impl std::fmt::Debug for TextFieldState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextFieldState")
            .field("text", &self.text)
            .field("selection", &self.selection)
            .field("composition", &self.composition)
            .field("scroll_offset", &self.scroll_offset)
            .field("scroll_offset_y", &self.scroll_offset_y)
            .field("drag_anchor", &self.drag_anchor)
            .field("blink_start", &self.blink_start)
            .field("inner_width", &self.inner_width)
            .field("inner_height", &self.inner_height)
            .field("preferred_x_px", &self.preferred_x_px)
            .field("offset_map", &self.offset_map.as_ref().map(|_| "<fn>"))
            .field(
                "visual_transformation",
                &self.visual_transformation.as_ref().map(|_| "<vt>"),
            )
            .field("scroll_target", &self.scroll_target)
            .field("scroll_target_y", &self.scroll_target_y)
            .field("can_undo", &self.can_undo())
            .field("can_redo", &self.can_redo())
            .field("undo_count", &self.undo_stack.len())
            .field("redo_count", &self.redo_stack.len())
            .finish()
    }
}

impl Default for TextFieldState {
    fn default() -> Self {
        Self::new()
    }
}

impl TextFieldState {
    pub fn new() -> Self {
        Self {
            text: String::new(),
            selection: 0..0,
            composition: None,
            scroll_offset: 0.0,
            scroll_offset_y: 0.0,
            drag_anchor: None,
            blink_start: Instant::now(),
            inner_width: 0.0,
            inner_height: 0.0,
            preferred_x_px: None,
            offset_map: None,
            visual_transformation: None,
            scroll_target: 0.0,
            scroll_target_y: 0.0,
            scroll_vel: 0.0,
            scroll_vel_y: 0.0,
            last_scroll_tick: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            staging_undo: None,
        }
    }

    // --- Undo/Redo ---

    /// Whether there is an action to undo.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || self.staging_undo.is_some()
    }

    /// Whether there is an action to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Revert the latest edit. Returns true if an undo was performed.
    pub fn undo(&mut self) -> bool {
        self.flush_undo();
        if let Some(op) = self.undo_stack.pop() {
            let end = (op.index + op.post_text.len()).min(self.text.len());
            self.text.replace_range(op.index..end, &op.pre_text);
            self.selection = op.pre_selection.clone();
            self.redo_stack.push(op);
            self.preferred_x_px = None;
            self.reset_caret_blink();
            true
        } else {
            false
        }
    }

    /// Re-apply a previously undone edit. Returns true if a redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(op) = self.redo_stack.pop() {
            let end = (op.index + op.pre_text.len()).min(self.text.len());
            self.text.replace_range(op.index..end, &op.post_text);
            self.selection = op.post_selection.clone();
            self.undo_stack.push(op);
            self.preferred_x_px = None;
            self.reset_caret_blink();
            true
        } else {
            false
        }
    }

    /// Clear all undo/redo history.
    pub fn clear_undo_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.staging_undo = None;
    }

    /// Push a [TextUndoOp] to the staging area, possibly merging with the
    /// previous staging operation. Flushes staging to the undo stack when
    /// merge is not possible.
    fn record_edit(&mut self, op: TextUndoOp) {
        if let Some(staging) = self.staging_undo.take() {
            if let Some(merged) = staging.try_merge(&op) {
                self.staging_undo = Some(merged);
                return;
            }
            // Can't merge: flush staging to undo stack
            self.undo_stack.push(staging);
            self.redo_stack.clear();
            // Enforce capacity: drop oldest entries
            while self.undo_stack.len() + 1 > TEXT_UNDO_CAPACITY {
                self.undo_stack.remove(0);
            }
        }
        self.staging_undo = Some(op);
    }

    /// Flush the staging operation into the undo stack.
    fn flush_undo(&mut self) {
        if let Some(op) = self.staging_undo.take() {
            self.undo_stack.push(op);
            self.redo_stack.clear();
            while self.undo_stack.len() > TEXT_UNDO_CAPACITY {
                self.undo_stack.remove(0);
            }
        }
    }

    fn insert_text_impl(&mut self, text: &str, can_merge: bool) {
        let start = self.selection.start.min(self.text.len());
        let end = self.selection.end.min(self.text.len());
        let pre_text = self.text[start..end].to_string();
        let pre_selection = self.selection.clone();

        self.text.replace_range(start..end, text);
        let new_pos = start + text.len();
        self.selection = new_pos..new_pos;
        self.preferred_x_px = None;
        self.reset_caret_blink();

        if !pre_text.is_empty() || !text.is_empty() {
            self.record_edit(TextUndoOp {
                index: start,
                pre_text,
                post_text: text.to_string(),
                pre_selection,
                post_selection: self.selection.clone(),
                time: Instant::now(),
                can_merge,
            });
        }
    }

    pub fn insert_text(&mut self, text: &str) {
        self.insert_text_impl(text, true);
    }

    /// Like `insert_text` but marks the operation as unmergeable (for cut/paste).
    pub fn insert_text_atomic(&mut self, text: &str) {
        self.insert_text_impl(text, false);
    }

    pub fn delete_backward(&mut self) {
        if self.selection.start == self.selection.end {
            let pos = self.selection.start.min(self.text.len());
            if pos > 0 {
                let prev = prev_grapheme_boundary(&self.text, pos);
                let pre_text = self.text[prev..pos].to_string();
                let pre_selection = self.selection.clone();
                self.text.replace_range(prev..pos, "");
                self.selection = prev..prev;
                self.preferred_x_px = None;
                self.reset_caret_blink();
                self.record_edit(TextUndoOp {
                    index: prev,
                    pre_text,
                    post_text: String::new(),
                    pre_selection,
                    post_selection: self.selection.clone(),
                    time: Instant::now(),
                    can_merge: true,
                });
            }
        } else {
            self.insert_text_impl("", true);
        }
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

    pub fn delete_forward(&mut self) {
        if self.selection.start == self.selection.end {
            let pos = self.selection.start.min(self.text.len());
            if pos < self.text.len() {
                let next = next_grapheme_boundary(&self.text, pos);
                let pre_text = self.text[pos..next].to_string();
                let pre_selection = self.selection.clone();
                self.text.replace_range(pos..next, "");
                self.preferred_x_px = None;
                self.reset_caret_blink();
                self.record_edit(TextUndoOp {
                    index: pos,
                    pre_text,
                    post_text: String::new(),
                    pre_selection,
                    post_selection: self.selection.clone(),
                    time: Instant::now(),
                    can_merge: true,
                });
            }
        } else {
            self.insert_text_impl("", true);
        }
        self.preferred_x_px = None;
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
        self.preferred_x_px = None;
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
            self.preferred_x_px = None;
            self.reset_caret_blink();
            return;
        }

        let anchor_start;
        if let Some(r) = self.composition.take() {
            let mut s = clamp_to_char_boundary(&self.text, r.start.min(self.text.len()));
            let mut e = clamp_to_char_boundary(&self.text, r.end.min(self.text.len()));
            if e < s {
                std::mem::swap(&mut s, &mut e);
            }
            self.text.replace_range(s..e, &text);
            anchor_start = s;
        } else {
            let pos = clamp_to_char_boundary(&self.text, self.selection.start.min(self.text.len()));
            self.text.insert_str(pos, &text);
            anchor_start = pos;
        }

        self.composition = Some(anchor_start..(anchor_start + text.len()));

        if let Some((c0, c1)) = cursor {
            let b0 = char_to_byte(&text, c0);
            let b1 = char_to_byte(&text, c1);
            self.selection = (anchor_start + b0)..(anchor_start + b1);
        } else {
            let end = anchor_start + text.len();
            self.selection = end..end;
        }

        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

    pub fn commit_composition(&mut self, text: String) {
        let pre_selection = self.selection.clone();
        if let Some(r) = self.composition.take() {
            let s = clamp_to_char_boundary(&self.text, r.start.min(self.text.len()));
            let e = clamp_to_char_boundary(&self.text, r.end.min(self.text.len()));
            let pre_text = self.text[s..e].to_string();
            self.text.replace_range(s..e, &text);
            let new_pos = s + text.len();
            self.selection = new_pos..new_pos;
            self.preferred_x_px = None;
            self.reset_caret_blink();
            if !pre_text.is_empty() || !text.is_empty() {
                self.record_edit(TextUndoOp {
                    index: s,
                    pre_text,
                    post_text: text,
                    pre_selection,
                    post_selection: self.selection.clone(),
                    time: Instant::now(),
                    can_merge: true,
                });
            }
        } else {
            let pos = clamp_to_char_boundary(&self.text, self.selection.end.min(self.text.len()));
            self.text.insert_str(pos, &text);
            let new_pos = pos + text.len();
            self.selection = new_pos..new_pos;
            self.preferred_x_px = None;
            self.reset_caret_blink();
            if !text.is_empty() {
                self.record_edit(TextUndoOp {
                    index: pos,
                    pre_text: String::new(),
                    post_text: text,
                    pre_selection,
                    post_selection: self.selection.clone(),
                    time: Instant::now(),
                    can_merge: true,
                });
            }
        }
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
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

    pub fn delete_surrounding(&mut self, before_bytes: usize, after_bytes: usize) {
        if self.selection.start != self.selection.end {
            let start = self.selection.start.min(self.text.len());
            let end = self.selection.end.min(self.text.len());
            self.text.replace_range(start..end, "");
            self.selection = start..start;
            self.preferred_x_px = None;
            self.reset_caret_blink();
            return;
        }

        let caret = self.selection.end.min(self.text.len());
        let start_raw = caret.saturating_sub(before_bytes);
        let end_raw = (caret + after_bytes).min(self.text.len());

        let start = prev_grapheme_boundary(&self.text, start_raw);
        let end = next_grapheme_boundary(&self.text, end_raw);
        if start < end {
            self.text.replace_range(start..end, "");
            self.selection = start..start;
        }
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

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
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

    pub fn drag_to(&mut self, idx_byte: usize) {
        if let Some(anchor) = self.drag_anchor {
            let i = idx_byte.min(self.text.len());
            self.selection = anchor.min(i)..anchor.max(i);
        }
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }
    pub fn end_drag(&mut self) {
        self.drag_anchor = None;
    }

    pub fn caret_index(&self) -> usize {
        self.selection.end
    }

    /// Keep caret visible inside inner content width (px).
    /// `inset_px` is a small padding (px) to avoid hugging edges.
    /// Sets the scroll target for smooth animated scrolling.
    pub fn ensure_caret_visible(&mut self, caret_x_px: f32, inner_width_px: f32, inset_px: f32) {
        self.ensure_caret_visible_xy(caret_x_px, 0.0, inner_width_px, 1.0, inset_px);
    }

    /// Keep caret visible inside an inner rect (for multiline).
    /// Sets the scroll target for smooth animated scrolling.
    pub fn ensure_caret_visible_xy(
        &mut self,
        caret_x_px: f32,
        caret_y_px: f32,
        inner_w_px: f32,
        inner_h_px: f32,
        inset_px: f32,
    ) {
        let inset_px = inset_px.max(0.0);

        // Compute target X scroll based on current display offset
        let left_px = self.scroll_offset + inset_px;
        let right_px = self.scroll_offset + inner_w_px - inset_px;
        if caret_x_px < left_px {
            self.scroll_target = (caret_x_px - inset_px).max(0.0);
        } else if caret_x_px > right_px {
            self.scroll_target = (caret_x_px - inner_w_px + inset_px).max(0.0);
        }

        // Compute target Y scroll based on current display offset
        let top_px = self.scroll_offset_y + inset_px;
        let bot_px = self.scroll_offset_y + inner_h_px - inset_px;
        if caret_y_px < top_px {
            self.scroll_target_y = (caret_y_px - inset_px).max(0.0);
        } else if caret_y_px > bot_px {
            self.scroll_target_y = (caret_y_px - inner_h_px + inset_px).max(0.0);
        }
    }

    pub fn clamp_scroll(&mut self, content_h_px: f32) {
        let max_y = (content_h_px - self.inner_height).max(0.0);
        self.scroll_target_y = self.scroll_target_y.clamp(0.0, max_y);
        if self.scroll_target_y.is_nan() {
            self.scroll_target_y = 0.0;
        }
    }

    pub fn reset_caret_blink(&mut self) {
        self.blink_start = Instant::now();
    }
    pub fn caret_visible(&self) -> bool {
        const PERIOD: Duration = Duration::from_millis(500);
        ((Instant::now() - self.blink_start).as_millis() / PERIOD.as_millis()).is_multiple_of(2)
    }

    pub fn set_inner_width(&mut self, w_px: f32) {
        self.inner_width = w_px.max(0.0);
        if self.scroll_offset.is_nan() {
            self.scroll_offset = 0.0;
        }
        if self.scroll_target.is_nan() {
            self.scroll_target = 0.0;
        }
    }
    pub fn set_inner_height(&mut self, h_px: f32) {
        self.inner_height = h_px.max(0.0);
        if self.scroll_offset_y.is_nan() {
            self.scroll_offset_y = 0.0;
        }
        if self.scroll_target_y.is_nan() {
            self.scroll_target_y = 0.0;
        }
    }

    /// Advance scroll animation by actual wall-clock dt using spring physics.
    /// Call this once per frame before reading [scroll_offset] / [scroll_offset_y].
    /// On the first call after a target change, snaps immediately to avoid 1-frame delay.
    pub fn tick_scroll_animation(&mut self) {
        let now = Instant::now();
        let dt = match self.last_scroll_tick {
            Some(prev) => {
                let d = now.saturating_duration_since(prev).as_secs_f32();
                d.min(0.05) // cap to 50ms to avoid jumps after pause
            }
            None => {
                // First tick: snap to target immediately, but record the time
                // so subsequent ticks produce a smooth spring.
                self.last_scroll_tick = Some(now);
                self.scroll_offset = self.scroll_target;
                self.scroll_vel = 0.0;
                self.scroll_offset_y = self.scroll_target_y;
                self.scroll_vel_y = 0.0;
                return;
            }
        };
        self.last_scroll_tick = Some(now);

        // X axis
        if dt > 0.0 {
            let dx = self.scroll_target - self.scroll_offset;
            let near_x = dx.abs() < 0.5 && self.scroll_vel.abs() < 0.5;
            if near_x {
                self.scroll_offset = self.scroll_target;
                self.scroll_vel = 0.0;
            } else {
                let force_x = SCROLL_STIFFNESS * dx - SCROLL_DAMPING * self.scroll_vel;
                self.scroll_vel += force_x * dt;
                self.scroll_offset += self.scroll_vel * dt;
                // Overshoot protection: clamp to target if we'd pass it this frame
                if (self.scroll_target - self.scroll_offset).signum() != dx.signum() && dx != 0.0 {
                    self.scroll_offset = self.scroll_target;
                    self.scroll_vel = 0.0;
                }
            }
        }

        // Y axis
        if dt > 0.0 {
            let dy = self.scroll_target_y - self.scroll_offset_y;
            let near_y = dy.abs() < 0.5 && self.scroll_vel_y.abs() < 0.5;
            if near_y {
                self.scroll_offset_y = self.scroll_target_y;
                self.scroll_vel_y = 0.0;
            } else {
                let force_y = SCROLL_STIFFNESS * dy - SCROLL_DAMPING * self.scroll_vel_y;
                self.scroll_vel_y += force_y * dt;
                self.scroll_offset_y += self.scroll_vel_y * dt;
                if (self.scroll_target_y - self.scroll_offset_y).signum() != dy.signum()
                    && dy != 0.0
                {
                    self.scroll_offset_y = self.scroll_target_y;
                    self.scroll_vel_y = 0.0;
                }
            }
        }
    }
}

// Platform-managed view: hint shown only when `value` is empty.
pub fn TextField(
    hint: impl Into<String>,
    value: String,
    modifier: repose_core::Modifier,
    on_change: Option<impl Fn(String) + 'static>,
    on_submit: Option<impl Fn(String) + 'static>,
) -> repose_core::View {
    TextFieldEx {
        hint: hint.into(),
        value,
        modifier,
        on_change: on_change.map(|f| Rc::new(f) as _),
        on_submit: on_submit.map(|f| Rc::new(f) as _),
        visual_transformation: None,
        keyboard_options: None,
        ime_action: None,
    }
    .build()
}

/// Extended TextField with optional visual transformation, keyboard options, and IME action.
pub struct TextFieldEx {
    pub hint: String,
    pub value: String,
    pub modifier: repose_core::Modifier,
    pub on_change: Option<Rc<dyn Fn(String)>>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    /// Optional visual transformation (e.g., password masking).
    pub visual_transformation: Option<Rc<dyn repose_core::VisualTransformation>>,
    /// Platform keyboard configuration hints.
    pub keyboard_options: Option<KeyboardOptions>,
    /// IME action button configuration.
    pub ime_action: Option<repose_core::ImeAction>,
}

impl TextFieldEx {
    pub fn new(hint: impl Into<String>, value: String, modifier: repose_core::Modifier) -> Self {
        Self {
            hint: hint.into(),
            value,
            modifier,
            on_change: None,
            on_submit: None,
            visual_transformation: None,
            keyboard_options: None,
            ime_action: None,
        }
    }

    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    pub fn on_submit(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    pub fn visual_transformation(mut self, vt: impl repose_core::VisualTransformation) -> Self {
        self.visual_transformation = Some(Rc::new(vt));
        self
    }

    pub fn keyboard_options(mut self, opts: KeyboardOptions) -> Self {
        self.keyboard_options = Some(opts);
        self
    }

    pub fn ime_action(mut self, action: repose_core::ImeAction) -> Self {
        self.ime_action = Some(action);
        self
    }

    pub fn password(mut self) -> Self {
        self.visual_transformation =
            Some(Rc::new(repose_core::PasswordVisualTransformation::default()));
        self
    }

    pub fn build(self) -> repose_core::View {
        repose_core::View::new(
            0,
            repose_core::ViewKind::TextField {
                state_key: 0,
                hint: self.hint,
                on_change: self.on_change,
                on_submit: self.on_submit,
                multiline: false,
                focus_tracker: None,
                value: self.value,
                visual_transformation: self.visual_transformation,
                keyboard_type: self.keyboard_options.map(|o| o.keyboard_type),
                ime_action: self.ime_action,
            },
        )
        .modifier(self.modifier)
        .semantics(repose_core::Semantics {
            role: repose_core::Role::TextField,
            label: None,
            focused: false,
            enabled: true,
        })
    }
}

/// Platform-managed view: multiline text input.
/// - Allows '\n' insertion
/// - Renders wrapped lines + vertical scrolling
pub fn TextArea(
    hint: impl Into<String>,
    value: String,
    modifier: repose_core::Modifier,
    on_change: Option<impl Fn(String) + 'static>,
    on_submit: Option<impl Fn(String) + 'static>,
) -> repose_core::View {
    TextAreaEx {
        hint: hint.into(),
        value,
        modifier,
        on_change: on_change.map(|f| Rc::new(f) as _),
        on_submit: on_submit.map(|f| Rc::new(f) as _),
        visual_transformation: None,
        keyboard_options: None,
        ime_action: None,
    }
    .build()
}

/// Extended TextArea with optional visual transformation, keyboard options, and IME action.
pub struct TextAreaEx {
    pub hint: String,
    pub value: String,
    pub modifier: repose_core::Modifier,
    pub on_change: Option<Rc<dyn Fn(String)>>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    pub visual_transformation: Option<Rc<dyn repose_core::VisualTransformation>>,
    pub keyboard_options: Option<KeyboardOptions>,
    pub ime_action: Option<repose_core::ImeAction>,
}

impl TextAreaEx {
    pub fn new(hint: impl Into<String>, value: String, modifier: repose_core::Modifier) -> Self {
        Self {
            hint: hint.into(),
            value,
            modifier,
            on_change: None,
            on_submit: None,
            visual_transformation: None,
            keyboard_options: None,
            ime_action: None,
        }
    }

    pub fn on_change(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    pub fn on_submit(mut self, f: impl Fn(String) + 'static) -> Self {
        self.on_submit = Some(Rc::new(f));
        self
    }

    pub fn visual_transformation(mut self, vt: impl repose_core::VisualTransformation) -> Self {
        self.visual_transformation = Some(Rc::new(vt));
        self
    }

    pub fn keyboard_options(mut self, opts: KeyboardOptions) -> Self {
        self.keyboard_options = Some(opts);
        self
    }

    pub fn ime_action(mut self, action: repose_core::ImeAction) -> Self {
        self.ime_action = Some(action);
        self
    }

    pub fn build(self) -> repose_core::View {
        repose_core::View::new(
            0,
            repose_core::ViewKind::TextField {
                state_key: 0,
                hint: self.hint,
                on_change: self.on_change,
                on_submit: self.on_submit,
                multiline: true,
                focus_tracker: None,
                value: self.value,
                visual_transformation: self.visual_transformation,
                keyboard_type: self.keyboard_options.map(|o| o.keyboard_type),
                ime_action: self.ime_action,
            },
        )
        .modifier(self.modifier)
        .semantics(repose_core::Semantics {
            role: repose_core::Role::TextField,
            label: None,
            focused: false,
            enabled: true,
        })
    }
}

#[derive(Clone, Debug)]
pub struct TextAreaLayout {
    pub ranges: Vec<(usize, usize)>,
    pub line_h_px: f32,
}

pub fn layout_text_area(text: &str, font_px: f32, wrap_w_px: f32) -> TextAreaLayout {
    let line_h = font_px * 1.3;
    let (ranges, _) = repose_text::wrap_line_ranges(text, font_px, wrap_w_px.max(1.0), None, true);
    TextAreaLayout {
        ranges,
        line_h_px: line_h,
    }
}

/// Return (line_index, local_byte, global_byte) for a global byte index.
fn locate_byte_in_ranges(ranges: &[(usize, usize)], b: usize) -> (usize, usize, usize) {
    if ranges.is_empty() {
        return (0, 0, b);
    }
    for (i, (s, e)) in ranges.iter().enumerate() {
        if b < *s {
            if i == 0 {
                return (0, 0, b);
            }
            let (ps, pe) = ranges[i - 1];
            let local = pe.saturating_sub(ps);
            return (i - 1, local, ps + local);
        }
        if b < *e {
            let local = b.saturating_sub(*s).min(e.saturating_sub(*s));
            return (i, local, *s + local);
        }
        if b == *e {
            if let Some((ns, _ne)) = ranges.get(i + 1)
                && *ns == b
            {
                return (i + 1, 0, b);
            }
            let local = e.saturating_sub(*s);
            return (i, local, *s + local);
        }
    }
    let (ls, le) = ranges[ranges.len() - 1];
    let local = le.saturating_sub(ls);
    (ranges.len() - 1, local, ls + local)
}

/// Compute caret (x, y) in px relative to the top-left of the inner content (not scrolled).
pub fn caret_xy_for_byte(
    text: &str,
    font_px: f32,
    wrap_w_px: f32,
    byte: usize,
) -> (f32, f32, usize) {
    let layout = layout_text_area(text, font_px, wrap_w_px);
    let (ranges, line_h) = (&layout.ranges, layout.line_h_px);
    let (li, local, _) = locate_byte_in_ranges(ranges, byte);
    let (s, e) = ranges.get(li).copied().unwrap_or((0, 0));
    let line = &text[s..e];
    let m = measure_text(line, font_px, None);
    let ci = byte_to_char_index(&m, local);
    let x = m.positions.get(ci).copied().unwrap_or(0.0);
    let y = (li as f32) * line_h;
    (x, y, li)
}

/// Given x/y (px) relative to inner content (not scrolled), return nearest grapheme boundary byte index.
pub fn index_for_xy_bytes(text: &str, font_px: f32, wrap_w_px: f32, x_px: f32, y_px: f32) -> usize {
    let layout = layout_text_area(text, font_px, wrap_w_px);
    let li = ((y_px / layout.line_h_px).floor() as isize).max(0) as usize;
    let li = li.min(layout.ranges.len().saturating_sub(1));
    let (s, e) = layout.ranges.get(li).copied().unwrap_or((0, 0));
    let line = &text[s..e];
    let local = index_for_x_bytes(line, font_px, x_px.max(0.0));
    (s + local).min(text.len())
}

/// Move caret up/down in wrapped multiline text, keeping a preferred x column.
pub fn move_caret_vertical(
    text: &str,
    font_px: f32,
    wrap_w_px: f32,
    cur_byte: usize,
    dir: i32, // -1 up, +1 down
    preferred_x: Option<f32>,
) -> (usize, f32) {
    let layout = layout_text_area(text, font_px, wrap_w_px);
    if layout.ranges.is_empty() {
        return (cur_byte, preferred_x.unwrap_or(0.0));
    }
    let (x, _y, li) = caret_xy_for_byte(text, font_px, wrap_w_px, cur_byte);
    let px = preferred_x.unwrap_or(x);
    let mut nli = li as i32 + dir;
    nli = nli.clamp(0, (layout.ranges.len().saturating_sub(1)) as i32);
    let nli = nli as usize;
    let (s, e) = layout.ranges[nli];
    let line = &text[s..e];
    let local = index_for_x_bytes(line, font_px, px.max(0.0));
    ((s + local).min(text.len()), px)
}

/// Move to start/end of current visual line.
pub fn line_home_end(
    text: &str,
    font_px: f32,
    wrap_w_px: f32,
    cur_byte: usize,
    to_end: bool,
) -> usize {
    let layout = layout_text_area(text, font_px, wrap_w_px);
    let (li, _local, _) = locate_byte_in_ranges(&layout.ranges, cur_byte);
    let (s, e) = layout.ranges.get(li).copied().unwrap_or((0, 0));
    if to_end { e } else { s }
}

fn clamp_to_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    if s.is_char_boundary(i) {
        return i;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_for_x_bytes_grapheme() {
        let t = "A👍🏽B";
        let font_px = 16.0; // in tests, exact px isn't important-boundaries are.
        let m = measure_text(t, font_px, None);
        for i in 0..m.byte_offsets.len() - 1 {
            let b = m.byte_offsets[i];
            let _ = &t[..b];
        }
    }
}
