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
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;
use web_time::Duration;
use web_time::Instant;

use crate::layout::mul_alpha_color;

thread_local! {
    static TEXTFIELD_STATES: RefCell<HashMap<u64, Rc<RefCell<TextFieldState>>>> = RefCell::new(HashMap::new());
}

pub fn set_textfield_state(key: u64, state: Rc<RefCell<TextFieldState>>) {
    TEXTFIELD_STATES.with(|m| m.borrow_mut().insert(key, state));
}

pub fn get_textfield_state(key: u64) -> Option<Rc<RefCell<TextFieldState>>> {
    TEXTFIELD_STATES.with(|m| m.borrow().get(&key).cloned())
}

pub fn ensure_caret_visible(state: &mut TextFieldState, multiline: bool) {
    let font_px = repose_core::dp_to_px(TF_FONT_DP) * repose_core::locals::text_scale().0;
    let wrap_width = state.inner_width;
    if multiline {
        let (cx, cy, _) = crate::textfield::caret_xy_for_byte(
            &state.text,
            font_px,
            wrap_width,
            state.caret_index(),
        );
        let iw = state.inner_width;
        let ih = state.inner_height;
        state.ensure_caret_visible_xy(cx, cy, iw, ih, repose_core::dp_to_px(2.0));
    } else {
        let caret_idx = state.caret_index();
        let (display, caret_display_off) = if let Some(vt) = &state.visual_transformation {
            let annotated = repose_core::AnnotatedString::new(state.text.clone(), vec![]);
            let tfmd = vt.filter(&annotated);
            let off =
                repose_core::original_offset_to_display(&state.text, tfmd.text.as_str(), caret_idx);
            (tfmd.text.text, off)
        } else {
            (state.text.clone(), caret_idx)
        };
        let m = crate::textfield::measure_text(&display, font_px, TextMeasureConfig::default());
        let caret_x = m.positions.get(caret_display_off).copied().unwrap_or(0.0);
        state.ensure_caret_visible(caret_x, wrap_width, repose_core::dp_to_px(2.0));
    }
}

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
    Start, // backspace: cursor moving towards start
    End,   // delete forward: cursor moving towards end
    Inner, // selection removed
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
                if self_del == next_del
                    && (self_del == TextDeleteType::Start || self_del == TextDeleteType::End)
                {
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
        if self.post_selection.start != self.post_selection.end {
            return TextDeleteType::NotByUser;
        }
        if self.pre_selection.start == self.pre_selection.end {
            // Collapsed selection before delete: cursor moved
            if self.pre_selection.start > self.post_selection.start {
                TextDeleteType::Start // backspace
            } else {
                TextDeleteType::End // delete forward
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
    pub capitalization: repose_core::KeyboardCapitalization,
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self {
            keyboard_type: repose_core::KeyboardType::default(),
            autocorrect: true,
            capitalization: repose_core::KeyboardCapitalization::default(),
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

pub struct TextMeasureConfig {
    pub font_family: Option<&'static str>,
    pub font_weight: u16,
    pub font_style: u8,
    pub letter_spacing: f32,
    pub font_variation_settings: Option<String>,
}

impl Default for TextMeasureConfig {
    fn default() -> Self {
        Self {
            font_family: None,
            font_weight: 400,
            font_style: 0,
            letter_spacing: 0.0,
            font_variation_settings: None,
        }
    }
}

/// Measure caret positions for a single-line textfield using shaping.
/// `font_px` must match the px size used for rendering the text.
/// `font_family` optionally overrides the default font (e.g. for icons).
pub fn measure_text(text: &str, font_px: f32, config: TextMeasureConfig) -> TextMetrics {
    let m = repose_text::metrics_for_textfield(
        text,
        font_px,
        config.font_family,
        config.font_weight,
        config.font_style,
        config.letter_spacing,
        config.font_variation_settings.as_deref(),
    );
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
pub fn index_for_x_bytes(
    text: &str,
    font_px: f32,
    x_px: f32,
    font_weight: u16,
    font_style: u8,
) -> usize {
    let m = measure_text(
        text,
        font_px,
        TextMeasureConfig {
            font_weight,
            font_style,
            ..Default::default()
        },
    );

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
pub(crate) fn prev_grapheme_boundary(text: &str, byte: usize) -> usize {
    let mut last = 0usize;
    for (i, _) in text.grapheme_indices(true) {
        if i >= byte {
            break;
        }
        last = i;
    }
    last
}

pub(crate) fn next_grapheme_boundary(text: &str, byte: usize) -> usize {
    for (i, _) in text.grapheme_indices(true) {
        if i > byte {
            return i;
        }
    }
    text.len()
}

/// Find the word boundaries around the given byte index.
/// Selects alphanumeric+underscore runs; falls back to the grapheme cluster.
pub(crate) fn word_range(text: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(text.len());
    let is_word = |g: &str| g.chars().all(|c| c.is_alphanumeric() || c == '_');

    let mut start = byte;
    while start > 0 {
        let p = prev_grapheme_boundary(text, start);
        if is_word(&text[p..start]) {
            start = p;
        } else {
            break;
        }
    }
    let mut end = byte;
    while end < text.len() {
        let n = next_grapheme_boundary(text, end);
        if is_word(&text[end..n]) {
            end = n;
        } else {
            break;
        }
    }
    if start == end {
        let s = if byte == 0 {
            0
        } else {
            prev_grapheme_boundary(text, byte)
        };
        let e = next_grapheme_boundary(text, byte);
        (s, e.max(s))
    } else {
        (start, end)
    }
}

pub struct TextFieldState {
    pub text: String,
    pub selection: Range<usize>,
    pub composition: Option<Range<usize>>, // IME composition range (byte offsets)
    pub scroll_offset: f32,                // px (x) - current animated display value
    pub scroll_offset_y: f32,              // px (y) for multiline - current animated display value
    pub drag_anchor: Option<usize>,        // byte index where drag began

    // Double/triple-tap tracking
    pub(crate) last_tap_time: Option<Instant>,
    pub(crate) last_tap_pos: Option<(f32, f32)>,
    pub(crate) tap_count: u8,

    pub blink_start: Instant,        // caret blink timer
    pub inner_width: f32,            // px
    pub inner_height: f32,           // px
    pub preferred_x_px: Option<f32>, // for Up/Down caret movement in multiline
    /// When a visual transformation is active, this maps offsets in the
    /// display text back to offsets in the original text.
    pub offset_map: Option<Box<dyn OffsetMapping>>,
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

    // Undo/Redo
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
            .field(
                "offset_map",
                &self.offset_map.as_ref().map(|_| "<offset_mapping>"),
            )
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

impl Clone for TextFieldState {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            selection: self.selection.clone(),
            composition: self.composition.clone(),
            scroll_offset: self.scroll_offset,
            scroll_offset_y: self.scroll_offset_y,
            drag_anchor: self.drag_anchor,
            last_tap_time: self.last_tap_time,
            last_tap_pos: self.last_tap_pos,
            tap_count: self.tap_count,
            blink_start: self.blink_start,
            inner_width: self.inner_width,
            inner_height: self.inner_height,
            preferred_x_px: self.preferred_x_px,
            offset_map: self.offset_map.as_ref().map(|m| m.clone_box()),
            visual_transformation: self.visual_transformation.clone(),
            scroll_target: self.scroll_target,
            scroll_target_y: self.scroll_target_y,
            scroll_vel: self.scroll_vel,
            scroll_vel_y: self.scroll_vel_y,
            last_scroll_tick: self.last_scroll_tick,
            undo_stack: self.undo_stack.clone(),
            redo_stack: self.redo_stack.clone(),
            staging_undo: self.staging_undo.clone(),
        }
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
            last_tap_time: None,
            last_tap_pos: None,
            tap_count: 0,
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

    // Undo/Redo

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
        if self.selection.start < self.selection.end {
            repose_core::clipboard::set_primary_selection(
                &self.text[self.selection.start..self.selection.end],
            );
        }
    }
    pub fn end_drag(&mut self) {
        self.drag_anchor = None;
        if self.selection.start < self.selection.end {
            repose_core::clipboard::set_primary_selection(
                &self.text[self.selection.start..self.selection.end],
            );
        }
    }

    pub fn handle_pointer_down(&mut self, idx_byte: usize, pos_px: (f32, f32), shift: bool) {
        const DOUBLE_TAP_MS: u64 = 300;
        const TAP_SLOP_PX: f32 = 12.0;

        let now = Instant::now();
        let mut count = self.tap_count;
        if let (Some(t), Some(p)) = (self.last_tap_time, self.last_tap_pos) {
            let dt = now.saturating_duration_since(t);
            let dist = ((pos_px.0 - p.0).powi(2) + (pos_px.1 - p.1).powi(2)).sqrt();
            if dt < Duration::from_millis(DOUBLE_TAP_MS) && dist < TAP_SLOP_PX {
                count = count.saturating_add(1);
            } else {
                count = 1;
            }
        } else {
            count = 1;
        }
        self.tap_count = count;
        self.last_tap_time = Some(now);
        self.last_tap_pos = Some(pos_px);

        let idx = idx_byte.min(self.text.len());

        if count >= 3 {
            // Triple-tap: select all
            self.selection = 0..self.text.len();
            self.drag_anchor = None;
            self.preferred_x_px = None;
            self.reset_caret_blink();
            if self.selection.end > 0 {
                repose_core::clipboard::set_primary_selection(&self.text);
            }
            return;
        }

        if count == 2 {
            // Double-tap: select word
            let (s, e) = word_range(&self.text, idx);
            self.selection = s..e;
            self.drag_anchor = Some(s);
            self.preferred_x_px = None;
            self.reset_caret_blink();
            if e > s {
                repose_core::clipboard::set_primary_selection(&self.text[s..e]);
            }
            return;
        }

        // Single tap
        self.begin_drag(idx, shift);
    }

    /// Select the word at the given byte index.
    pub fn select_word_at(&mut self, byte: usize) {
        let (s, e) = word_range(&self.text, byte.min(self.text.len()));
        self.selection = s..e;
        self.drag_anchor = Some(s);
        self.preferred_x_px = None;
        self.reset_caret_blink();
    }

    /// Select all text.
    pub fn select_all(&mut self) {
        self.selection = 0..self.text.len();
        self.drag_anchor = None;
        self.preferred_x_px = None;
        self.reset_caret_blink();
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

    /// If the selection is collapsed (caret is visible), return the [`Instant`]
    /// of the next 500 ms blink boundary.
    pub fn next_blink_deadline(&self) -> Option<Instant> {
        if self.selection.start != self.selection.end {
            return None;
        }
        const PERIOD_MS: u128 = 500;
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.blink_start).as_millis();
        let next_tick = (elapsed / PERIOD_MS) + 1;
        Some(self.blink_start + Duration::from_millis((next_tick * PERIOD_MS) as u64))
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

/// Configuration for `BasicTextField` / `BasicSecureTextField`.
///
/// Use `..Default::default()` for unset fields:
/// ```ignore
/// BasicTextField(state, modifier, "Hint", TextFieldConfig {
///     enabled: false,
///     ..Default::default()
/// })
/// ```
#[derive(Clone)]
pub struct TextFieldConfig {
    /// When false, the text field is not editable, not focusable, and input is not selectable (-> `enabled`).
    pub enabled: bool,
    /// When true, the text field can be focused and text can be selected/copied, but not modified (-> `readOnly`).
    pub read_only: bool,
    /// Input transformation (-> `inputTransformation`). Transforms text before it is applied.
    pub input_transformation: Option<Rc<dyn repose_core::InputTransformation>>,
    /// Style for the text content (-> `textStyle`).
    pub text_style: repose_core::TextStyle,
    /// Platform keyboard configuration hints (-> `keyboardOptions`).
    pub keyboard_options: repose_core::KeyboardOptions,
    /// Per-action IME callback (-> `onKeyboardAction`).
    pub on_keyboard_action: Option<Rc<dyn repose_core::KeyboardActionHandler>>,
    /// Line limits (-> `TextFieldLineLimits`).
    pub line_limits: repose_core::TextFieldLineLimits,
    /// Callback invoked after each text layout computation (-> `onTextLayout`).
    pub on_text_layout: Option<Rc<dyn Fn(&repose_core::TextLayoutResult)>>,
    /// Interaction source for tracking focus/press/hover state.
    pub interaction_source: Option<repose_core::MutableInteractionSource>,
    /// Tracks focus state during layout. The cell is set to `true` while this
    /// field is the focused text input, `false` otherwise.
    pub focus_tracker: Option<Rc<Cell<bool>>>,
    /// Cursor brush (-> `cursorBrush`). `None` -> theme default (`on_surface`).
    pub cursor_brush: Option<repose_core::Brush>,
    /// Output transformation (-> `outputTransformation`). Transforms text for display only.
    pub output_transformation: Option<Rc<dyn repose_core::OutputTransformation>>,
    /// Decorator (-> `decorator`). Wraps the inner text field with custom decorations.
    pub decorator: Option<Rc<dyn repose_core::TextFieldDecorator>>,
    /// Internal codepoint transformation for password obfuscation (-> `codepointTransformation`).
    pub codepoint_transformation: Option<repose_core::CodepointTransformation>,
    /// Text obfuscation mode (-> `textObfuscationMode`). Used by `BasicSecureTextField`.
    pub text_obfuscation_mode: repose_core::TextObfuscationMode,
    /// Character used for text obfuscation (-> `textObfuscationCharacter`). Used by `BasicSecureTextField`.
    pub text_obfuscation_character: char,

    // Legacy / reposé-specific (for migration convenience, kept in config)
    pub on_change: Option<Rc<dyn Fn(String)>>,
    pub on_submit: Option<Rc<dyn Fn(String)>>,
    pub visual_transformation: Option<Rc<dyn repose_core::VisualTransformation>>,
    pub decoration_box: Option<Rc<dyn Fn(repose_core::View) -> repose_core::View>>,
}

impl Default for TextFieldConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            read_only: false,
            input_transformation: None,
            text_style: Default::default(),
            keyboard_options: repose_core::KeyboardOptions::DEFAULT,
            on_keyboard_action: None,
            line_limits: repose_core::TextFieldLineLimits::MultiLine {
                min_height_in_lines: 1,
                max_height_in_lines: usize::MAX,
            },
            on_text_layout: None,
            interaction_source: None,
            focus_tracker: None,
            cursor_brush: None,
            output_transformation: None,
            decorator: None,
            codepoint_transformation: None,
            text_obfuscation_mode: repose_core::TextObfuscationMode::System,
            text_obfuscation_character: '\u{2022}',
            on_change: None,
            on_submit: None,
            visual_transformation: None,
            decoration_box: None,
        }
    }
}

/// State-based text field. Corresponds to Compose's `BasicTextField(state: TextFieldState, ...)`.
///
/// The state is managed externally and all editing is reflected in the `TextFieldState`
/// object passed to the platform runner via `set_textfield_state`.
///
/// # Example
/// ```ignore
/// let state = Rc::new(RefCell::new(TextFieldState::new("")));
/// BasicTextField(state.clone(), Modifier::new(), "Hint", TextFieldConfig {
///     enabled: false,
///     ..Default::default()
/// })
/// ```
pub fn BasicTextField(
    state: Rc<RefCell<TextFieldState>>,
    modifier: repose_core::Modifier,
    hint: impl Into<String>,
    config: TextFieldConfig,
) -> repose_core::View {
    let (single_line, max_lines, min_lines) = match config.line_limits {
        repose_core::TextFieldLineLimits::SingleLine => (true, 1, 1),
        repose_core::TextFieldLineLimits::MultiLine {
            min_height_in_lines,
            max_height_in_lines,
        } => (false, max_height_in_lines, min_height_in_lines),
    };

    let ka = if let Some(ref handler) = config.on_keyboard_action {
        let handler = handler.clone();
        repose_core::KeyboardActions {
            on_done: Some({
                let h = handler.clone();
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    h.on_keyboard_action(&|| {})
                })
            }),
            on_go: Some({
                let h = handler.clone();
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    h.on_keyboard_action(&|| {})
                })
            }),
            on_next: Some({
                let h = handler.clone();
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    h.on_keyboard_action(&|| {})
                })
            }),
            on_previous: Some({
                let h = handler.clone();
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    h.on_keyboard_action(&|| {})
                })
            }),
            on_search: Some({
                let h = handler.clone();
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    h.on_keyboard_action(&|| {})
                })
            }),
            on_send: Some({
                Rc::new(move |_: &dyn repose_core::KeyboardActionScope| {
                    handler.on_keyboard_action(&|| {})
                })
            }),
        }
    } else {
        repose_core::KeyboardActions::default()
    };

    let decoration_box = config
        .decorator
        .map(|d| Rc::new(move |inner: repose_core::View| d.decorate(inner)) as Rc<dyn Fn(_) -> _>);

    let cursor_color = config.cursor_brush.and_then(|b| match b {
        repose_core::Brush::Solid(c) => Some(c),
        _ => None,
    });

    let value = state.borrow().text.clone();
    let key = state.as_ptr() as u64;
    set_textfield_state(key, state.clone());

    let state_on_change = {
        let s = state.clone();
        move |new_value: String| {
            s.borrow_mut().text = new_value;
        }
    };

    let merged_on_change: Option<Rc<dyn Fn(String)>> =
        if let Some(ref cfg_on_change) = config.on_change {
            let a = Rc::new(state_on_change) as Rc<dyn Fn(String)>;
            let b = cfg_on_change.clone();
            Some(Rc::new(move |v: String| {
                a(v.clone());
                b(v);
            }) as Rc<dyn Fn(String)>)
        } else {
            Some(Rc::new(state_on_change) as Rc<dyn Fn(String)>)
        };

    text_field_view(
        modifier,
        hint.into(),
        value,
        !single_line,
        merged_on_change,
        config.on_submit,
        config.visual_transformation,
        config.keyboard_options.keyboard_type,
        config.keyboard_options.capitalization,
        config.keyboard_options.ime_action,
        config.keyboard_options.auto_correct_enabled,
        config.enabled,
        config.read_only,
        Some(max_lines),
        min_lines,
        cursor_color,
        config.on_text_layout,
        config.text_style,
        ka,
        config.interaction_source,
        config.focus_tracker,
        Some(config.line_limits),
        config.input_transformation,
        config.output_transformation,
        decoration_box,
        config.codepoint_transformation,
    )
}

/// Secure text field for password entry. Corresponds to Compose's `BasicSecureTextField`.
///
/// Wraps `BasicTextField` with secure defaults: single-line, password keyboard,
/// text obfuscation, and disabled cut/copy.
pub fn BasicSecureTextField(
    state: Rc<RefCell<TextFieldState>>,
    modifier: repose_core::Modifier,
    config: TextFieldConfig,
) -> repose_core::View {
    let mask = config.text_obfuscation_character;
    let secure_config = TextFieldConfig {
        line_limits: repose_core::TextFieldLineLimits::SingleLine,
        keyboard_options: repose_core::KeyboardOptions::SECURE_TEXT_FIELD,
        visual_transformation: match config.text_obfuscation_mode {
            repose_core::TextObfuscationMode::Visible => None,
            _ => Some(Rc::new(repose_core::PasswordVisualTransformation { mask })
                as Rc<dyn repose_core::VisualTransformation>),
        },
        ..config
    };
    BasicTextField(state, modifier, "", secure_config)
}

#[derive(Clone, Debug)]
pub struct TextAreaLayout {
    pub ranges: Vec<(usize, usize)>,
    pub line_h_px: f32,
}

pub fn layout_text_area(
    text: &str,
    font_px: f32,
    wrap_w_px: f32,
    font_weight: u16,
    font_style: u8,
    letter_spacing: f32,
    font_variation_settings: Option<&str>,
) -> TextAreaLayout {
    let line_h = font_px;
    let (ranges, _) = repose_text::wrap_line_ranges(
        text,
        font_px,
        wrap_w_px.max(1.0),
        None,
        true,
        font_weight,
        font_style,
        letter_spacing,
        font_variation_settings,
    );
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
    let layout = layout_text_area(text, font_px, wrap_w_px, 400, 0, 0.0, None);
    let (ranges, line_h) = (&layout.ranges, layout.line_h_px);
    let (li, local, _) = locate_byte_in_ranges(ranges, byte);
    let (s, e) = ranges.get(li).copied().unwrap_or((0, 0));
    let line = &text[s..e];
    let m = measure_text(line, font_px, TextMeasureConfig::default());
    let ci = byte_to_char_index(&m, local);
    let x = m.positions.get(ci).copied().unwrap_or(0.0);
    let y = (li as f32) * line_h;
    (x, y, li)
}

/// Given x/y (px) relative to inner content (not scrolled), return nearest grapheme boundary byte index.
pub fn index_for_xy_bytes(text: &str, font_px: f32, wrap_w_px: f32, x_px: f32, y_px: f32) -> usize {
    let layout = layout_text_area(text, font_px, wrap_w_px, 400, 0, 0.0, None);
    let li = ((y_px / layout.line_h_px).floor() as isize).max(0) as usize;
    let li = li.min(layout.ranges.len().saturating_sub(1));
    let (s, e) = layout.ranges.get(li).copied().unwrap_or((0, 0));
    let line = &text[s..e];
    let local = index_for_x_bytes(line, font_px, x_px.max(0.0), 400, 0);
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
    let layout = layout_text_area(text, font_px, wrap_w_px, 400, 0, 0.0, None);
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
    let local = index_for_x_bytes(line, font_px, px.max(0.0), 400, 0);
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
    let layout = layout_text_area(text, font_px, wrap_w_px, 400, 0, 0.0, None);
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

/// Paint a text field into the scene. Called by layout.rs when
/// `modifier.text_input.is_some()`. This is the Compose-equivalent of
/// `TextFieldCoreModifierNode.draw()` - the engine handles painting natively
/// when the text_input modifier is present (no caller-side painter needed).
///
/// Behavior per Compose BasicTextField:
/// - `text_input.enabled=false`: no cursor, no selection highlight, text rendered normally
/// - `text_input.read_only=true`: no cursor, selection highlight rendered
/// - `cursor_color`: overrides cursor brush
/// - `max_lines`: caps rendered lines (clip applied by container)
/// - `on_text_layout`: called after layout computation
pub(crate) fn paint_text_field(
    scene: &mut Scene,
    rect: repose_core::Rect,
    text_input: &TextInputConfig,
    state: Option<&Rc<RefCell<TextFieldState>>>,
    is_focused: bool,
    clip_rounded: Option<[f32; 4]>,
    alpha_accum: f32,
) {
    let ts = text_input.text_style.clone().unwrap_or_default();
    let font_size_dp = if ts.font_size != 0.0 {
        ts.font_size
    } else {
        TF_FONT_DP
    };
    let font_val = dp_to_px(font_size_dp) * locals::text_scale().0;
    let line_h = if ts.line_height != 0.0 {
        dp_to_px(ts.line_height) * locals::text_scale().0
    } else if text_input.multiline {
        0.0 // sentinel -> renderer uses Normal line height (font-metric-based)
    } else {
        font_val // single-line needs tp use font em-size for correct cursor–text alignment
    };
    let text_off_y = (rect.h - line_h.max(font_val)) / 2.0;

    let clip_radius = clip_rounded.unwrap_or([0.0; 4]).map(dp_to_px);
    scene.nodes.push(SceneNode::PushClip {
        rect,
        radius: clip_radius,
        op: repose_core::ClipOp::Intersect,
    });

    let th = locals::theme();
    let show_selection = text_input.enabled;
    let show_cursor = text_input.enabled && !text_input.read_only;
    let cursor_color = text_input.cursor_color.unwrap_or(th.on_surface);
    let rendered_by_vt = |original: &str| -> String {
        if let Some(ref vt) = text_input.visual_transformation {
            let annotated = repose_core::AnnotatedString::new(original.to_string(), vec![]);
            vt.filter(&annotated).text.text
        } else {
            original.to_string()
        }
    };

    if let Some(state_rc) = state {
        let st = state_rc.borrow();

        if !text_input.multiline {
            // Single-line
            let measure_for = if text_input.visual_transformation.is_some() && !st.text.is_empty() {
                rendered_by_vt(&st.text)
            } else {
                st.text.clone()
            };
            let has_vt = text_input.visual_transformation.is_some();
            let m = measure_text(
                &measure_for,
                font_val,
                TextMeasureConfig {
                    font_family: ts.font_family,
                    font_weight: ts.font_weight.unwrap_or(400),
                    font_style: ts.font_style.unwrap_or(0),
                    letter_spacing: ts.letter_spacing,
                    font_variation_settings: None,
                },
            );

            // Selection highlight
            if show_selection && st.selection.start != st.selection.end {
                let start_off = if has_vt {
                    original_offset_to_display(&st.text, &measure_for, st.selection.start)
                } else {
                    st.selection.start
                };
                let end_off = if has_vt {
                    original_offset_to_display(&st.text, &measure_for, st.selection.end)
                } else {
                    st.selection.end
                };
                let sx = m
                    .positions
                    .get(byte_to_char_index(&m, start_off))
                    .copied()
                    .unwrap_or(0.0)
                    - st.scroll_offset;
                let ex = m
                    .positions
                    .get(byte_to_char_index(&m, end_off))
                    .copied()
                    .unwrap_or(sx)
                    - st.scroll_offset;
                let selection = th.focus.with_alpha_f32(85.0 / 255.0);
                let vis_x = sx.max(0.0);
                let vis_ex = ex.max(0.0);
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x + vis_x,
                        y: rect.y + text_off_y,
                        w: (vis_ex - vis_x).max(0.0),
                        h: line_h.max(font_val),
                    },
                    brush: Brush::Solid(selection),
                    radius: [0.0; 4],
                });
            }

            // IME composition underline (visual feedback for an active preedit).
            if let Some(comp) = st.composition.clone() {
                let cs = if has_vt {
                    original_offset_to_display(&st.text, &measure_for, comp.start)
                } else {
                    comp.start
                };
                let ce = if has_vt {
                    original_offset_to_display(&st.text, &measure_for, comp.end)
                } else {
                    comp.end
                };
                let sx = m
                    .positions
                    .get(byte_to_char_index(&m, cs))
                    .copied()
                    .unwrap_or(0.0)
                    - st.scroll_offset;
                let ex = m
                    .positions
                    .get(byte_to_char_index(&m, ce))
                    .copied()
                    .unwrap_or(sx)
                    - st.scroll_offset;
                let y = rect.y + text_off_y + line_h.max(font_val) - dp_to_px(2.0);
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x + sx.max(0.0),
                        y,
                        w: (ex - sx).max(dp_to_px(2.0)),
                        h: dp_to_px(2.0),
                    },
                    brush: Brush::Solid(th.focus),
                    radius: [0.0; 4],
                });
            }

            // Text
            let txt_col = if st.text.is_empty() {
                ts.color.unwrap_or(th.on_surface_variant)
            } else {
                ts.color.unwrap_or(th.on_surface)
            };
            let render_txt = if st.text.is_empty() {
                text_input.hint.clone()
            } else {
                rendered_by_vt(&st.text)
            };
            scene.nodes.push(SceneNode::Text {
                rect: repose_core::Rect {
                    x: rect.x - st.scroll_offset,
                    y: rect.y + text_off_y,
                    w: rect.w,
                    h: line_h,
                },
                text: Arc::from(render_txt),
                color: mul_alpha_color(txt_col, alpha_accum),
                size: font_val,
                font_family: ts.font_family,
                text_align: ts.text_align,
                font_weight: FontWeight(ts.font_weight.unwrap_or(400)),
                font_style: match ts.font_style.unwrap_or(0) {
                    1 => FontStyle::Italic,
                    _ => FontStyle::Normal,
                },
                text_decoration: ts.text_decoration.unwrap_or_default(),
                letter_spacing: ts.letter_spacing,
                line_height: ts.line_height,
                extra_style: Default::default(),
                url: None,
                font_variation_settings: None,
            });

            // Caret (only when enabled && !readOnly)
            if show_cursor
                && is_focused
                && st.selection.start == st.selection.end
                && st.caret_visible()
            {
                let caret_off = if has_vt {
                    original_offset_to_display(&st.text, &measure_for, st.selection.end)
                } else {
                    st.selection.end
                };
                let cx = m
                    .positions
                    .get(byte_to_char_index(&m, caret_off))
                    .copied()
                    .unwrap_or(0.0)
                    - st.scroll_offset;
                let cursor_y = rect.y + text_off_y + (line_h.max(font_val) - font_val) / 2.0;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: rect.x + cx.max(0.0),
                        y: cursor_y,
                        w: dp_to_px(1.0),
                        h: font_val,
                    },
                    brush: Brush::Solid(cursor_color),
                    radius: [0.0; 4],
                });
            }
        } else {
            // Multi-line
            let render_text = if st.text.is_empty() {
                st.text.clone()
            } else if let Some(ref vt) = text_input.visual_transformation {
                let annotated = repose_core::AnnotatedString::new(st.text.clone(), vec![]);
                vt.filter(&annotated).text.text
            } else {
                st.text.clone()
            };
            let layout = layout_text_area(
                &render_text,
                font_val,
                rect.w.max(1.0),
                400,
                0,
                ts.letter_spacing,
                None,
            );
            let lh = layout.line_h_px;
            let max_line_count = text_input.max_lines.unwrap_or(usize::MAX);

            // Hint text (empty field)
            if st.text.is_empty() {
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: rect.y,
                        w: rect.w,
                        h: line_h,
                    },
                    text: Arc::from(text_input.hint.clone()),
                    color: mul_alpha_color(ts.color.unwrap_or(th.on_surface_variant), alpha_accum),
                    size: font_val,
                    font_family: ts.font_family,
                    text_align: ts.text_align,
                    font_weight: FontWeight(ts.font_weight.unwrap_or(400)),
                    font_style: match ts.font_style.unwrap_or(0) {
                        1 => FontStyle::Italic,
                        _ => FontStyle::Normal,
                    },
                    text_decoration: ts.text_decoration.unwrap_or_default(),
                    letter_spacing: ts.letter_spacing,
                    line_height: ts.line_height,
                    extra_style: Default::default(),
                    url: None,
                    font_variation_settings: None,
                });
            } else {
                for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                    if i >= max_line_count {
                        break;
                    }
                    let ln = render_text[s..e].to_string();
                    let draw_y = rect.y + (i as f32) * lh - st.scroll_offset_y;
                    if draw_y + lh < rect.y - 1.0 || draw_y > rect.y + rect.h + 1.0 {
                        continue;
                    }
                    scene.nodes.push(SceneNode::Text {
                        rect: repose_core::Rect {
                            x: rect.x,
                            y: draw_y,
                            w: rect.w,
                            h: lh,
                        },
                        text: Arc::<str>::from(ln),
                        color: mul_alpha_color(ts.color.unwrap_or(th.on_surface), alpha_accum),
                        size: font_val,
                        font_family: ts.font_family,
                        text_align: ts.text_align,
                        font_weight: FontWeight(ts.font_weight.unwrap_or(400)),
                        font_style: match ts.font_style.unwrap_or(0) {
                            1 => FontStyle::Italic,
                            _ => FontStyle::Normal,
                        },
                        text_decoration: ts.text_decoration.unwrap_or_default(),
                        letter_spacing: ts.letter_spacing,
                        line_height: ts.line_height,
                        extra_style: Default::default(),
                        url: None,
                        font_variation_settings: None,
                    });
                }
            }

            // Selection (multi-line)
            if show_selection && st.selection.start != st.selection.end {
                let sel_a_orig: usize = st.selection.start.min(st.selection.end);
                let sel_b_orig: usize = st.selection.start.max(st.selection.end);
                let has_vt = text_input.visual_transformation.is_some();
                let sel_a = if has_vt {
                    original_offset_to_display(&st.text, &render_text, sel_a_orig)
                } else {
                    sel_a_orig
                };
                let sel_b = if has_vt {
                    original_offset_to_display(&st.text, &render_text, sel_b_orig)
                } else {
                    sel_b_orig
                };
                let selection = th.focus.with_alpha_f32(85.0 / 255.0);
                for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                    if i >= max_line_count {
                        break;
                    }
                    let os = sel_a.max(s);
                    let oe = sel_b.min(e);
                    if os >= oe {
                        continue;
                    }
                    let ln = &render_text[s..e];
                    let m = measure_text(
                        ln,
                        font_val,
                        TextMeasureConfig {
                            font_family: ts.font_family,
                            font_weight: ts.font_weight.unwrap_or(400),
                            font_style: ts.font_style.unwrap_or(0),
                            letter_spacing: ts.letter_spacing,
                            font_variation_settings: None,
                        },
                    );
                    let ls = os - s;
                    let le = oe - s;
                    let sx = m
                        .positions
                        .get(byte_to_char_index(&m, ls))
                        .copied()
                        .unwrap_or(0.0);
                    let ex = m
                        .positions
                        .get(byte_to_char_index(&m, le))
                        .copied()
                        .unwrap_or(sx);
                    let draw_y = rect.y + (i as f32) * lh - st.scroll_offset_y;
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: rect.x + sx,
                            y: draw_y,
                            w: (ex - sx).max(0.0),
                            h: lh,
                        },
                        brush: Brush::Solid(selection),
                        radius: [0.0; 4],
                    });
                }
            }

            // IME composition underline (multi-line): intersect the preedit
            // range with each visible line and underline the overlapping span.
            if let Some(comp) = st.composition.clone() {
                let has_vt = text_input.visual_transformation.is_some();
                let comp_a = if has_vt {
                    original_offset_to_display(&st.text, &render_text, comp.start)
                } else {
                    comp.start
                };
                let comp_b = if has_vt {
                    original_offset_to_display(&st.text, &render_text, comp.end)
                } else {
                    comp.end
                };
                for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                    if i >= max_line_count {
                        break;
                    }
                    let os = comp_a.max(s);
                    let oe = comp_b.min(e);
                    if os >= oe {
                        continue;
                    }
                    let ln = &render_text[s..e];
                    let m = measure_text(
                        ln,
                        font_val,
                        TextMeasureConfig {
                            font_family: ts.font_family,
                            font_weight: ts.font_weight.unwrap_or(400),
                            font_style: ts.font_style.unwrap_or(0),
                            letter_spacing: ts.letter_spacing,
                            font_variation_settings: None,
                        },
                    );
                    let ls = os - s;
                    let le = oe - s;
                    let sx = m
                        .positions
                        .get(byte_to_char_index(&m, ls))
                        .copied()
                        .unwrap_or(0.0);
                    let ex = m
                        .positions
                        .get(byte_to_char_index(&m, le))
                        .copied()
                        .unwrap_or(sx);
                    let draw_y = rect.y + (i as f32) * lh - st.scroll_offset_y;
                    scene.nodes.push(SceneNode::Rect {
                        rect: repose_core::Rect {
                            x: rect.x + sx,
                            y: draw_y + lh - dp_to_px(2.0),
                            w: (ex - sx).max(dp_to_px(2.0)),
                            h: dp_to_px(2.0),
                        },
                        brush: Brush::Solid(th.focus),
                        radius: [0.0; 4],
                    });
                }
            }

            // Caret (multi-line) - only when enabled && !readOnly
            if show_cursor
                && is_focused
                && st.selection.start == st.selection.end
                && st.caret_visible()
            {
                let caret_orig = st.selection.end.min(st.text.len());
                let has_vt = text_input.visual_transformation.is_some();
                let caret = if has_vt {
                    original_offset_to_display(&st.text, &render_text, caret_orig)
                } else {
                    caret_orig
                };
                let (cx, cy, _li) =
                    caret_xy_for_byte(&render_text, font_val, rect.w.max(1.0), caret);
                let draw_x = rect.x + cx;
                let draw_y = rect.y + cy - st.scroll_offset_y;
                scene.nodes.push(SceneNode::Rect {
                    rect: repose_core::Rect {
                        x: draw_x,
                        y: draw_y + (lh - font_val) / 2.0,
                        w: dp_to_px(1.0),
                        h: font_val,
                    },
                    brush: Brush::Solid(cursor_color),
                    radius: [0.0; 4],
                });
            }
        }
    } else {
        // No state yet (unfocused) - render hint or raw value
        if text_input.value.is_empty() {
            let hint_y = if text_input.multiline {
                rect.y
            } else {
                rect.y + text_off_y
            };
            scene.nodes.push(SceneNode::Text {
                rect: repose_core::Rect {
                    x: rect.x,
                    y: hint_y,
                    w: rect.w,
                    h: line_h,
                },
                text: Arc::from(text_input.hint.clone()),
                color: mul_alpha_color(th.on_surface_variant, alpha_accum),
                size: font_val,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: ts.text_decoration.unwrap_or_default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
                font_variation_settings: None,
            });
        } else if text_input.multiline {
            let render_text = if text_input.value.is_empty() {
                text_input.value.clone()
            } else if let Some(ref vt) = text_input.visual_transformation {
                let annotated = repose_core::AnnotatedString::new(text_input.value.clone(), vec![]);
                vt.filter(&annotated).text.text
            } else {
                text_input.value.clone()
            };
            let layout = layout_text_area(
                &render_text,
                font_val,
                rect.w.max(1.0),
                400,
                0,
                ts.letter_spacing,
                None,
            );
            let lh = layout.line_h_px;
            for (i, (s, e)) in layout.ranges.iter().copied().enumerate() {
                let ln = render_text[s..e].to_string();
                let draw_y = rect.y + (i as f32) * lh;
                if draw_y + lh < rect.y - 1.0 || draw_y > rect.y + rect.h + 1.0 {
                    continue;
                }
                scene.nodes.push(SceneNode::Text {
                    rect: repose_core::Rect {
                        x: rect.x,
                        y: draw_y,
                        w: rect.w,
                        h: lh,
                    },
                    text: Arc::<str>::from(ln),
                    color: mul_alpha_color(th.on_surface, alpha_accum),
                    size: font_val,
                    font_family: None,
                    text_align: TextAlign::Unspecified,
                    font_weight: FontWeight::NORMAL,
                    font_style: FontStyle::Normal,
                    text_decoration: ts.text_decoration.unwrap_or_default(),
                    letter_spacing: 0.0,
                    line_height: 0.0,
                    extra_style: Default::default(),
                    url: None,
                    font_variation_settings: None,
                });
            }
        } else {
            scene.nodes.push(SceneNode::Text {
                rect: repose_core::Rect {
                    x: rect.x,
                    y: rect.y + text_off_y,
                    w: rect.w,
                    h: line_h,
                },
                text: Arc::from(rendered_by_vt(&text_input.value)),
                color: mul_alpha_color(th.on_surface, alpha_accum),
                size: font_val,
                font_family: None,
                text_align: TextAlign::Unspecified,
                font_weight: FontWeight::NORMAL,
                font_style: FontStyle::Normal,
                text_decoration: ts.text_decoration.unwrap_or_default(),
                letter_spacing: 0.0,
                line_height: 0.0,
                extra_style: Default::default(),
                url: None,
                font_variation_settings: None,
            });
        }
    }

    // Fire on_text_layout callback with computed layout info
    if let Some(ref cb) = text_input.on_text_layout {
        let (
            line_count,
            content_w,
            content_h,
            first_baseline,
            last_baseline,
            did_overflow_w,
            did_overflow_h,
            lines,
        ) = if let Some(state_rc) = state {
            let st = state_rc.borrow();
            let display = if st.text.is_empty() {
                text_input.hint.clone()
            } else if let Some(ref vt) = text_input.visual_transformation {
                let annotated = repose_core::AnnotatedString::new(st.text.clone(), vec![]);
                vt.filter(&annotated).text.text
            } else {
                st.text.clone()
            };
            if text_input.multiline {
                let l = layout_text_area(
                    &display,
                    font_val,
                    rect.w.max(1.0),
                    400,
                    0,
                    ts.letter_spacing,
                    None,
                );
                let lc = l.ranges.len();
                let cw = rect.w.max(0.0);
                let ch = (lc as f32 * l.line_h_px).max(0.0);
                let line_infos: Vec<_> = l
                    .ranges
                    .iter()
                    .enumerate()
                    .map(|(i, &(s, e))| {
                        let top = i as f32 * l.line_h_px;
                        let bottom = top + l.line_h_px;
                        let line_text = &display[s..e];
                        let m = measure_text(line_text, font_val, TextMeasureConfig::default());
                        let line_w = m.positions.last().copied().unwrap_or(0.0);
                        TextLineInfo {
                            start: s,
                            end: e,
                            top,
                            baseline: top + l.line_h_px * 0.8,
                            bottom,
                            left: 0.0,
                            right: line_w,
                            width: line_w,
                        }
                    })
                    .collect();
                let fb = line_infos.first().map(|l| l.baseline).unwrap_or(0.0);
                let lb = line_infos.last().map(|l| l.baseline).unwrap_or(0.0);
                (lc, cw, ch, fb, lb, cw > rect.w, ch > rect.h, line_infos)
            } else {
                let m = measure_text(&display, font_val, TextMeasureConfig::default());
                let w = m.positions.last().copied().unwrap_or(0.0);
                let top = 0.0;
                let bottom = line_h.max(font_val);
                let baseline = bottom * 0.8;
                let line_info = TextLineInfo {
                    start: 0,
                    end: display.len(),
                    top,
                    baseline,
                    bottom,
                    left: 0.0,
                    right: w,
                    width: w,
                };
                (
                    1,
                    w.max(0.0),
                    bottom,
                    baseline,
                    baseline,
                    w > rect.w,
                    bottom > rect.h,
                    vec![line_info],
                )
            }
        } else {
            (0, 0.0, 0.0, 0.0, 0.0, false, false, vec![])
        };
        cb(&repose_core::TextLayoutResult {
            line_count,
            width_px: content_w,
            height_px: content_h,
            first_baseline,
            last_baseline,
            did_overflow_width: did_overflow_w,
            did_overflow_height: did_overflow_h,
            lines,
        });
    }

    scene.nodes.push(SceneNode::PopClip);
}

/// Shared view-builder for `BasicTextField`.
/// Creates the view with text_input modifier. Painting is handled natively
/// by layout.rs when it encounters `modifier.text_input` (Compose-aligned).
fn text_field_view(
    modifier: Modifier,
    hint: String,
    value: String,
    multiline: bool,
    on_change: Option<Rc<dyn Fn(String)>>,
    on_submit: Option<Rc<dyn Fn(String)>>,
    visual_transformation: Option<Rc<dyn repose_core::VisualTransformation>>,
    keyboard_type: repose_core::KeyboardType,
    capitalization: repose_core::KeyboardCapitalization,
    ime_action: repose_core::ImeAction,
    auto_correct_enabled: Option<bool>,
    enabled: bool,
    read_only: bool,
    max_lines: Option<usize>,
    min_lines: usize,
    cursor_color: Option<Color>,
    on_text_layout: Option<Rc<dyn Fn(&repose_core::TextLayoutResult)>>,
    text_style: repose_core::TextStyle,
    keyboard_actions: repose_core::KeyboardActions,
    interaction_source: Option<repose_core::MutableInteractionSource>,
    focus_tracker: Option<Rc<Cell<bool>>>,
    line_limits: Option<repose_core::TextFieldLineLimits>,
    _input_transformation: Option<Rc<dyn repose_core::InputTransformation>>,
    _output_transformation: Option<Rc<dyn repose_core::OutputTransformation>>,
    decoration_box: Option<Rc<dyn Fn(repose_core::View) -> repose_core::View>>,
    _codepoint_transformation: Option<repose_core::CodepointTransformation>,
) -> View {
    let mut modif = modifier.text_input(TextInputConfig {
        hint,
        multiline,
        on_change,
        on_submit,
        focus_tracker,
        value,
        visual_transformation,
        keyboard_type,
        capitalization,
        ime_action,
        auto_correct_enabled,
        enabled,
        read_only,
        max_lines,
        min_lines,
        cursor_color,
        on_text_layout,
        text_style: Some(text_style),
        keyboard_actions: Some(keyboard_actions),
        interaction_source: interaction_source.as_ref().map(|s| s.source()),
        line_limits,
    });

    // `enabled=false` => not focusable and inert;
    // `read_only` keeps focus for selection/copy but blocks mutation.
    if !enabled {
        modif = modif.focusable(false).enabled(false);
    } else if read_only {
        modif = modif.focusable(true);
    }

    let inner = View::new(0, ViewKind::Box)
        .modifier(modif)
        .semantics(Semantics {
            role: Role::TextField,
            enabled,
            ..Default::default()
        });

    // Compose `decorationBox`: wrap the field node when provided.
    if let Some(decorate) = decoration_box {
        decorate(inner)
    } else {
        inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_for_x_bytes_grapheme() {
        let t = "A👍🏽B";
        let font_px = 16.0; // in tests, exact px isn't important-boundaries are.
        let m = measure_text(t, font_px, TextMeasureConfig::default());
        for i in 0..m.byte_offsets.len() - 1 {
            let b = m.byte_offsets[i];
            let _ = &t[..b];
        }
    }

    fn delete_op(
        index: usize,
        pre_text: &str,
        pre_selection: Range<usize>,
        post_selection: Range<usize>,
    ) -> TextUndoOp {
        TextUndoOp {
            index,
            pre_text: pre_text.to_string(),
            post_text: String::new(),
            pre_selection,
            post_selection,
            time: Instant::now(),
            can_merge: true,
        }
    }

    #[test]
    fn deletion_type_collapsed_post_selection_is_backspace() {
        // Backspace on "abc" with cursor at 3 deletes 'c', cursor moves to 2.
        let op = delete_op(2, "c", 3..3, 2..2);
        assert_eq!(op.deletion_type(), TextDeleteType::Start);
    }

    #[test]
    fn deletion_type_collapsed_post_selection_is_delete_forward() {
        // Delete-forward at cursor 3 removes 'c' but the cursor stays put.
        let op = delete_op(3, "c", 3..3, 3..3);
        assert_eq!(op.deletion_type(), TextDeleteType::End);
    }

    #[test]
    fn deletion_type_range_post_selection_is_not_by_user() {
        // A deletion that leaves an expanded post-selection is not a plain
        // backspace/delete-forward and must never merge (regression for the
        // old `!start == end` precedence bug which compared bitwise-not of start).
        let op = delete_op(2, "de", 2..4, 3..5);
        assert_eq!(op.deletion_type(), TextDeleteType::NotByUser);
    }

    #[test]
    fn backspace_ops_merge() {
        // "abc": cursor 3 -> 2 -> 1 via two backspaces merges into one "bc" delete.
        let a = delete_op(2, "c", 3..3, 2..2);
        let b = delete_op(1, "b", 2..2, 1..1);
        let merged = a
            .try_merge(&b)
            .expect("consecutive backspaces should merge");
        assert_eq!(merged.index, 1);
        assert_eq!(merged.pre_text, "bc");
    }

    #[test]
    fn selection_delete_does_not_merge_with_backspace() {
        // Selection-delete classifies as Inner, so it never merges with a
        // Start/End backspace-merge even back-to-back.
        let backspace = delete_op(2, "c", 3..3, 2..2);
        let selection = delete_op(2, "de", 2..4, 2..2);
        assert_eq!(selection.deletion_type(), TextDeleteType::Inner);
        assert!(backspace.try_merge(&selection).is_none());
    }
}
