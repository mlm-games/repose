use crate::Color;
use std::fmt::Debug;
use std::rc::Rc;
use std::sync::Arc;

/// A range of text measured in byte offsets, matching Compose's `TextRange`.
///
/// When `start == end`, the range is collapsed (cursor position).
/// When `start > end`, the range is reversed (selection direction matters).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    pub const ZERO: TextRange = TextRange { start: 0, end: 0 };

    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn collapsed(at: usize) -> Self {
        Self { start: at, end: at }
    }

    pub fn min(self) -> usize {
        self.start.min(self.end)
    }

    pub fn max(self) -> usize {
        self.start.max(self.end)
    }

    pub fn is_collapsed(self) -> bool {
        self.start == self.end
    }

    pub fn reversed(self) -> bool {
        self.start > self.end
    }

    pub fn length(self) -> usize {
        self.max() - self.min()
    }

    pub fn intersects(self, other: TextRange) -> bool {
        self.min() < other.max() && other.min() < self.max()
    }

    pub fn contains(self, offset: usize) -> bool {
        self.min() <= offset && offset < self.max()
    }

    pub fn coerce_in(self, min: usize, max: usize) -> Self {
        Self {
            start: self.start.clamp(min, max),
            end: self.end.clamp(min, max),
        }
    }
}

impl From<(usize, usize)> for TextRange {
    fn from((start, end): (usize, usize)) -> Self {
        Self { start, end }
    }
}

/// Snapshot of a text field's editing state including text, selection, and
/// IME composition range. Corresponds to Compose's `TextFieldValue`.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFieldValue {
    /// The annotated text. Plain text is accessible via `text()` or `annotated_string.text`.
    pub annotated_string: AnnotatedString,
    /// Selection range in byte offsets. Collapsed range (start == end) = cursor.
    pub selection: TextRange,
    /// Active IME composition range in byte offsets, or None.
    pub composition: Option<TextRange>,
}

impl TextFieldValue {
    /// Create from plain text (no annotations).
    pub fn new(text: impl Into<String>) -> Self {
        let annotated = AnnotatedString::from(text.into());
        let len = annotated.text.len();
        Self {
            selection: TextRange::collapsed(len),
            annotated_string: annotated,
            composition: None,
        }
    }

    /// Create from an `AnnotatedString` preserving all annotations.
    pub fn from_annotated(annotated: AnnotatedString) -> Self {
        let len = annotated.text.len();
        Self {
            selection: TextRange::collapsed(len),
            annotated_string: annotated,
            composition: None,
        }
    }

    /// Convenience: plain text content (delegates to `annotated_string.text`).
    pub fn text(&self) -> &str {
        &self.annotated_string.text
    }

    pub fn with_selection(mut self, start: usize, end: usize) -> Self {
        let len = self.annotated_string.text.len();
        self.selection = TextRange::new(start.min(len), end.min(len));
        self
    }

    pub fn get_text_before_selection(&self, max_chars: usize) -> AnnotatedString {
        let sel_min = self.selection.min();
        let start = sel_min.saturating_sub(max_chars);
        let text = self.annotated_string.text[start..sel_min].to_string();
        // Preserve spans that intersect the sub-range
        let spans: Vec<TextSpan> = self
            .annotated_string
            .spans
            .iter()
            .filter(|s| s.start >= start && s.end <= sel_min)
            .map(|s| TextSpan {
                start: s.start - start,
                end: s.end - start,
                style: s.style.clone(),
                url: s.url.clone(),
            })
            .collect();
        AnnotatedString::new(text, spans)
    }

    pub fn get_text_after_selection(&self, max_chars: usize) -> AnnotatedString {
        let sel_max = self.selection.max();
        let end = (sel_max + max_chars).min(self.annotated_string.text.len());
        let text = self.annotated_string.text[sel_max..end].to_string();
        let spans: Vec<TextSpan> = self
            .annotated_string
            .spans
            .iter()
            .filter(|s| s.start >= sel_max && s.end <= end)
            .map(|s| TextSpan {
                start: s.start - sel_max,
                end: s.end - sel_max,
                style: s.style.clone(),
                url: s.url.clone(),
            })
            .collect();
        AnnotatedString::new(text, spans)
    }

    pub fn get_selected_text(&self) -> AnnotatedString {
        let r = self.selection.min()..self.selection.max();
        let text = self.annotated_string.text[r.clone()].to_string();
        let spans: Vec<TextSpan> = self
            .annotated_string
            .spans
            .iter()
            .filter(|s| s.start >= r.start && s.end <= r.end)
            .map(|s| TextSpan {
                start: s.start - r.start,
                end: s.end - r.start,
                style: s.style.clone(),
                url: s.url.clone(),
            })
            .collect();
        AnnotatedString::new(text, spans)
    }

    /// Returns a copy with the given annotated string.
    pub fn copy(&self, annotated_string: AnnotatedString) -> Self {
        TextFieldValue {
            annotated_string,
            selection: self.selection,
            composition: self.composition,
        }
    }

    /// Returns a copy with the given plain text.
    pub fn copy_text(&self, text: String) -> Self {
        TextFieldValue {
            annotated_string: AnnotatedString::from(text),
            selection: self.selection,
            composition: self.composition,
        }
    }
}

/// Result of text layout computation, provided to the `on_text_layout` callback.
/// Exposes key information about the rendered text layout.
#[derive(Clone, Debug)]
pub struct TextLayoutResult {
    /// Number of visual lines in the layout.
    pub line_count: usize,
    /// Total content width in px.
    pub width_px: f32,
    /// Total content height in px.
    pub height_px: f32,
    /// First baseline position in px.
    pub first_baseline: f32,
    /// Last baseline position in px.
    pub last_baseline: f32,
    /// Whether text overflows the available width.
    pub did_overflow_width: bool,
    /// Whether text overflows the available height.
    pub did_overflow_height: bool,
    /// Per-line layout information.
    pub lines: Vec<TextLineInfo>,
}

/// Determines how text is obfuscated in a secure text field.
/// Corresponds to Compose's `TextObfuscationMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextObfuscationMode {
    /// Text is visible, no obfuscation.
    Visible,
    /// Reveal the last typed character briefly, then hide.
    RevealLastTyped,
    /// All characters are obfuscated.
    Hidden,
    /// Uses the platform's default obfuscation behavior.
    #[default]
    System,
}

/// Layout information for a single line of text.
#[derive(Clone, Debug)]
pub struct TextLineInfo {
    /// Byte offset of the line start in the text.
    pub start: usize,
    /// Byte offset of the line end (exclusive) in the text.
    pub end: usize,
    /// Top y position in px relative to the text field content area.
    pub top: f32,
    /// Baseline y position in px relative to the text field content area.
    pub baseline: f32,
    /// Bottom y position in px relative to the text field content area.
    pub bottom: f32,
    /// Left x position in px relative to the text field content area.
    pub left: f32,
    /// Right x position in px relative to the text field content area.
    pub right: f32,
    /// Width of this line in px.
    pub width: f32,
}

impl TextLayoutResult {
    pub fn get_line_start(&self, line_index: usize) -> Option<usize> {
        self.lines.get(line_index).map(|l| l.start)
    }

    pub fn get_line_end(&self, line_index: usize) -> Option<usize> {
        self.lines.get(line_index).map(|l| l.end)
    }

    pub fn get_line_end_visible(&self, line_index: usize) -> Option<usize> {
        self.lines.get(line_index).map(|l| l.end)
    }

    pub fn is_line_ellipsized(&self, _line_index: usize) -> bool {
        false
    }

    pub fn get_line_top(&self, line_index: usize) -> Option<f32> {
        self.lines.get(line_index).map(|l| l.top)
    }

    pub fn get_line_baseline(&self, line_index: usize) -> Option<f32> {
        self.lines.get(line_index).map(|l| l.baseline)
    }

    pub fn get_line_bottom(&self, line_index: usize) -> Option<f32> {
        self.lines.get(line_index).map(|l| l.bottom)
    }

    pub fn get_line_left(&self, line_index: usize) -> Option<f32> {
        self.lines.get(line_index).map(|l| l.left)
    }

    pub fn get_line_right(&self, line_index: usize) -> Option<f32> {
        self.lines.get(line_index).map(|l| l.right)
    }

    pub fn get_line_for_offset(&self, offset: usize) -> usize {
        for (i, line) in self.lines.iter().enumerate() {
            if offset >= line.start && offset < line.end {
                return i;
            }
        }
        self.line_count.saturating_sub(1)
    }

    pub fn get_line_for_vertical_position(&self, vertical: f32) -> usize {
        for (i, line) in self.lines.iter().enumerate() {
            if vertical >= line.top && vertical < line.bottom {
                return i;
            }
        }
        self.line_count.saturating_sub(1)
    }

    pub fn get_horizontal_position(&self, offset: usize, _use_primary_direction: bool) -> f32 {
        self.lines
            .iter()
            .find(|l| offset >= l.start && offset <= l.end)
            .map(|l| l.left)
            .unwrap_or(0.0)
    }

    /// Returns true if the text has visual overflow in either direction.
    pub fn has_visual_overflow(&self) -> bool {
        self.did_overflow_width || self.did_overflow_height
    }

    /// Returns the bounding box of the character at the given offset.
    /// Returns a zero rect if the offset is out of range.
    pub fn get_bounding_box(&self, offset: usize) -> crate::Rect {
        if self.lines.is_empty() || offset > self.lines.last().unwrap().end {
            return crate::Rect::default();
        }
        let line = self
            .lines
            .iter()
            .find(|l| offset >= l.start && offset < l.end)
            .unwrap_or_else(|| {
                if offset >= self.lines.last().unwrap().end {
                    self.lines.last().unwrap()
                } else {
                    &self.lines[0]
                }
            });
        let _line_idx = self.get_line_for_offset(offset);
        let char_in_line = offset - line.start;
        let pos_in_line = if char_in_line == 0 {
            line.left
        } else {
            // Approximate position
            let chars = line.end - line.start;
            if chars == 0 {
                line.left
            } else {
                line.left + (line.width * (char_in_line as f32) / (chars as f32))
            }
        };
        crate::Rect {
            x: pos_in_line,
            y: line.top,
            w: if char_in_line < (line.end - line.start) {
                line.width / (line.end - line.start).max(1) as f32
            } else {
                1.0
            },
            h: line.bottom - line.top,
        }
    }

    /// Returns the cursor rectangle at the given offset.
    pub fn get_cursor_rect(&self, offset: usize) -> crate::Rect {
        let line = self
            .lines
            .iter()
            .find(|l| offset >= l.start && offset <= l.end)
            .unwrap_or_else(|| {
                if offset >= self.lines.last().map(|l| l.end).unwrap_or(0) {
                    self.lines.last().unwrap()
                } else {
                    &self.lines[0]
                }
            });
        let char_in_line = offset.saturating_sub(line.start);
        let chars = (line.end - line.start).max(1);
        let x = line.left + (line.width * (char_in_line as f32) / (chars as f32));
        crate::Rect {
            x,
            y: line.top,
            w: 1.0,
            h: line.bottom - line.top,
        }
    }

    /// Returns the offset closest to the given position.
    pub fn get_offset_for_position(&self, position: (f32, f32)) -> Option<usize> {
        let line_idx = self.get_line_for_vertical_position(position.1);
        let line = self.lines.get(line_idx)?;
        if position.0 <= line.left {
            return Some(line.start);
        }
        if position.0 >= line.right {
            return Some(line.end);
        }
        let fraction = (position.0 - line.left) / line.width.max(1.0);
        let offset_in_line = ((line.end - line.start) as f32 * fraction).round() as usize;
        Some((line.start + offset_in_line).min(line.end))
    }

    /// Returns the text range of the word at the given offset.
    pub fn get_word_boundary(&self, _offset: usize) -> super::TextRange {
        // Simple word boundary: extend to spaces or line boundaries
        let text = ""; // We don't store the full text in layout result
        let start = if text.is_empty() { 0 } else { 0 };
        let end = if text.is_empty() { 0 } else { 0 };
        super::TextRange::new(start, end)
    }

    /// Returns the paragraph direction at the given offset.
    pub fn get_paragraph_direction(&self, _offset: usize) -> u8 {
        0 // LTR
    }

    /// Returns the BiDi run direction at the given offset.
    pub fn get_bidi_run_direction(&self, _offset: usize) -> u8 {
        0 // LTR
    }
}

/// Bidirectional offset mapping between original and transformed text.
pub trait OffsetMapping: Debug + Send + Sync + 'static {
    fn original_to_transformed(&self, offset: usize) -> usize;
    fn transformed_to_original(&self, offset: usize) -> usize;
    fn clone_box(&self) -> Box<dyn OffsetMapping>;
}

/// Identity offset mapping: original and transformed offsets are the same.
#[derive(Clone, Copy, Debug)]
pub struct IdentityOffsetMapping;

impl OffsetMapping for IdentityOffsetMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        offset
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        offset
    }
    fn clone_box(&self) -> Box<dyn OffsetMapping> {
        Box::new(*self)
    }
}

/// Transforms the visual representation of a text field's text without changing
/// the underlying value. For example, password masking.
pub trait VisualTransformation: Debug + Send + Sync + 'static {
    /// Transform the text for display. Takes the original `AnnotatedString` and returns
    /// the transformed `TransformedText` with an offset mapping.
    fn filter(&self, text: &AnnotatedString) -> TransformedText;
}

/// The result of applying a `VisualTransformation`.
pub struct TransformedText {
    /// The transformed text (annotated).
    pub text: AnnotatedString,
    /// Maps offsets between original and transformed text.
    pub offset_mapping: Box<dyn OffsetMapping>,
}

impl TransformedText {
    pub fn new(text: AnnotatedString, offset_mapping: Box<dyn OffsetMapping>) -> Self {
        TransformedText {
            text,
            offset_mapping,
        }
    }
}

impl Clone for TransformedText {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            offset_mapping: self.offset_mapping.clone_box(),
        }
    }
}

impl Debug for TransformedText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TransformedText")
            .field("text", &self.text.text)
            .finish()
    }
}

/// A `VisualTransformation` that displays text as-is (identity).
/// Equivalent to Compose's `VisualTransformation.None`.
#[derive(Clone, Copy, Debug)]
pub struct IdentityVisualTransformation;

impl VisualTransformation for IdentityVisualTransformation {
    fn filter(&self, text: &AnnotatedString) -> TransformedText {
        TransformedText {
            text: text.clone(),
            offset_mapping: Box::new(IdentityOffsetMapping),
        }
    }
}

/// A `VisualTransformation` that masks all characters with a given character.
/// Matches Compose's `PasswordVisualTransformation`.
#[derive(Clone, Copy, Debug)]
pub struct PasswordVisualTransformation {
    /// The replacement character (default `•` U+2022, to match compose, was *).
    pub mask: char,
}

impl Default for PasswordVisualTransformation {
    fn default() -> Self {
        Self { mask: '\u{2022}' }
    }
}

impl VisualTransformation for PasswordVisualTransformation {
    fn filter(&self, text: &AnnotatedString) -> TransformedText {
        let masked_text: String = text.text.chars().map(|_| self.mask).collect();
        TransformedText {
            text: AnnotatedString::new(masked_text, vec![]),
            offset_mapping: Box::new(IdentityOffsetMapping),
        }
    }
}

/// Convert a byte offset in the original text to the corresponding byte offset
/// in the visually-transformed display text.
pub fn original_offset_to_display(original: &str, display: &str, original_byte: usize) -> usize {
    original_offset_to_display_with_mapping(original, display, original_byte, None)
}

/// Convert a byte offset in the original text to the corresponding byte offset
/// in the visually-transformed display text, using the provided `OffsetMapping` if available.
pub fn original_offset_to_display_with_mapping(
    original: &str,
    display: &str,
    original_byte: usize,
    offset_mapping: Option<&dyn OffsetMapping>,
) -> usize {
    if let Some(om) = offset_mapping {
        om.original_to_transformed(original_byte)
    } else {
        let char_idx = original[..original_byte.min(original.len())]
            .chars()
            .count();
        display
            .char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or(display.len())
    }
}

/// Configures automatic capitalization behavior for the keyboard.
/// Corresponds to Compose's `KeyboardCapitalization`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardCapitalization {
    #[default]
    Unspecified,
    None,
    Characters,
    Words,
    Sentences,
}

/// Text shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    pub color: Color,
    /// Horizontal offset in dp.
    pub offset_x: f32,
    /// Vertical offset in dp.
    pub offset_y: f32,
    /// Blur radius in dp.
    pub blur_radius: f32,
}

/// Font synthesis controls whether the font renderer may synthesize
/// bold, italic, or small-caps variants when the font lacks them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FontSynthesis {
    #[default]
    Unspecified,
    None,
    Weight,
    Style,
    SmallCaps,
    All,
}

/// Baseline shift for subscript/superscript.
/// Wraps a multiplier applied against font_size for glyph y-offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaselineShift(pub f32);

impl BaselineShift {
    /// No baseline shift (zero offset).
    pub const Unspecified: BaselineShift = BaselineShift(0.0);
    /// Default superscript offset: shift up by 33.3% of font_size (CSS standard).
    pub const Superscript: BaselineShift = BaselineShift(-0.333);
    /// Default subscript offset: shift down by 20% of font_size (CSS standard).
    pub const Subscript: BaselineShift = BaselineShift(0.2);
}

impl Default for BaselineShift {
    fn default() -> Self {
        BaselineShift::Unspecified
    }
}

/// Hyphenation behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Hyphens {
    #[default]
    Unspecified,
    None,
    Auto,
}

/// Line break behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineBreak {
    #[default]
    Unspecified,
    Simple,
    Heading,
    Paragraph,
}

/// First-line and rest-line indent in dp.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextIndent {
    pub first_line: f32,
    pub rest_lines: f32,
}

impl Default for TextIndent {
    fn default() -> Self {
        Self {
            first_line: 0.0,
            rest_lines: 0.0,
        }
    }
}

/// Path effect applied to a stroked path.
/// Mirrors Compose's `PathEffect`.
#[derive(Clone, Debug, PartialEq)]
pub enum PathEffect {
    /// Replace sharp corners with rounded arcs of the given radius.
    Corner {
        /// Corner radius in em-units.
        radius: f32,
    },
    /// Draw the path as a dashed line.
    Dash {
        /// Interleaved on/off lengths in em-units.
        intervals: Vec<f32>,
        /// Starting phase offset in em-units.
        phase: f32,
    },
}

/// Draw style for text (fill or stroke).
#[derive(Clone, Debug, PartialEq)]
pub enum DrawStyle {
    Fill,
    Stroke {
        /// Stroke width in em-units (fraction of font size). 0.05 = 5% of em.
        width: f32,
        /// Line cap style for stroke endpoints.
        cap: crate::StrokeCap,
        /// Line join style for stroke segment joins.
        join: crate::StrokeJoin,
        /// Miter limit for miter joins.
        miter: f32,
        /// Optional path effect (dash, corner rounding, etc.).
        path_effect: Option<PathEffect>,
    },
}

impl DrawStyle {
    /// Create a `Stroke` variant with default cap, join, miter, and no path effect.
    pub const fn stroke(width: f32) -> Self {
        Self::Stroke {
            width,
            cap: crate::StrokeCap::Butt,
            join: crate::StrokeJoin::Miter,
            miter: 4.0,
            path_effect: None,
        }
    }
}

impl Default for DrawStyle {
    fn default() -> Self {
        Self::Fill
    }
}

/// Style configuration for text displayed in a text field.
/// Corresponds to Compose's `TextStyle`.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Font size in dp. 0 = use default (16dp for TextField).
    pub font_size: f32,
    /// Text color. None = use theme default.
    pub color: Option<Color>,
    /// Font weight. None = NORMAL.
    pub font_weight: Option<u16>,
    /// Font family. None = use default (sans-serif).
    pub font_family: Option<&'static str>,
    /// Font style. None = Normal.
    pub font_style: Option<u8>,
    /// Text alignment. Unspecified = inherit.
    pub text_align: crate::TextAlign,
    /// Letter spacing in dp. 0 = no extra spacing.
    pub letter_spacing: f32,
    /// Line height in dp. 0 = default (font_size * 1.2).
    pub line_height: f32,
    /// Text background color. None = transparent.
    pub background: Option<Color>,
    /// Text decoration (underline, strikethrough). None = no decoration.
    pub text_decoration: Option<crate::TextDecoration>,
    /// Text shadow. None = no shadow.
    pub shadow: Option<Shadow>,
    /// Text direction. None = inherit from thread-local default (usually LTR).
    pub text_direction: Option<crate::TextDirection>,
    /// Font synthesis policy (synthesize missing bold/italic/small-caps).
    pub font_synthesis: FontSynthesis,
    /// Baseline shift (superscript/subscript).
    pub baseline_shift: BaselineShift,
    /// Hyphenation behavior.
    pub hyphens: Hyphens,
    /// Line break behavior.
    pub line_break: LineBreak,
    /// First-line and rest-line indent in dp.
    pub text_indent: Option<TextIndent>,
    /// Draw style (fill or stroke).
    pub draw_style: DrawStyle,
    /// Text opacity (0.0-1.0). 0.0 = use default (fully opaque).
    pub alpha: f32,
    /// Locale hint for text shaping. Empty = use default.
    pub locale_list: Option<String>,
    /// OpenType font feature settings (e.g. "liga", "kern").
    pub font_feature_settings: Option<String>,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 0.0,
            color: None,
            font_weight: None,
            font_family: Some("sans-serif"),
            font_style: None,
            text_align: crate::TextAlign::Unspecified,
            letter_spacing: 0.0,
            line_height: 0.0,
            background: None,
            text_decoration: None,
            shadow: None,
            text_direction: None,
            font_synthesis: FontSynthesis::Unspecified,
            baseline_shift: BaselineShift::Unspecified,
            hyphens: Hyphens::Unspecified,
            line_break: LineBreak::Unspecified,
            text_indent: None,
            draw_style: DrawStyle::Fill,
            alpha: 0.0,
            locale_list: None,
            font_feature_settings: None,
        }
    }
}

/// Hints the platform about the type of keyboard to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardType {
    #[default]
    Unspecified,
    Text,
    Ascii,
    Number,
    Phone,
    Uri,
    Email,
    Password,
    NumberPassword,
    Decimal,
    PasswordVisible,
    PostalAddress,
    PersonName,
    EmailSubject,
    ShortMessage,
    LongMessage,
    Filter,
    Phonetic,
    DateTime,
    Date,
    Time,
    NumberSigned,
    DecimalSigned,
    DecimalPassword,
    NumberPasswordSigned,
    DecimalPasswordSigned,
}

/// The action button on the IME (soft keyboard).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImeAction {
    #[default]
    Unspecified,
    None,
    Default,
    Go,
    Search,
    Send,
    Previous,
    Next,
    Done,
}

/// Scope provided to `KeyboardActions` callbacks, allowing fallback to the
/// platform's default IME action behavior. Corresponds to Compose's `KeyboardActionScope`.
pub trait KeyboardActionScope {
    fn default_keyboard_action(&self, action: ImeAction);
}

/// Callbacks for IME action button presses on the soft keyboard.
/// Corresponds to Compose's legacy `KeyboardActions`.
#[derive(Clone)]
pub struct KeyboardActions {
    pub on_done: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
    pub on_go: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
    pub on_next: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
    pub on_previous: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
    pub on_search: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
    pub on_send: Option<Rc<dyn Fn(&dyn KeyboardActionScope)>>,
}

impl KeyboardActions {
    pub fn on_any(f: impl Fn(ImeAction, &dyn KeyboardActionScope) + 'static) -> Self {
        let f = Rc::new(f);
        KeyboardActions {
            on_done: Some({
                let f = f.clone();
                Rc::new(move |scope| f(ImeAction::Done, scope))
            }),
            on_go: Some({
                let f = f.clone();
                Rc::new(move |scope| f(ImeAction::Go, scope))
            }),
            on_next: Some({
                let f = f.clone();
                Rc::new(move |scope| f(ImeAction::Next, scope))
            }),
            on_previous: Some({
                let f = f.clone();
                Rc::new(move |scope| f(ImeAction::Previous, scope))
            }),
            on_search: Some({
                let f = f.clone();
                Rc::new(move |scope| f(ImeAction::Search, scope))
            }),
            on_send: Some({ Rc::new(move |scope| f(ImeAction::Send, scope)) }),
        }
    }
}

impl Default for KeyboardActions {
    fn default() -> Self {
        KeyboardActions {
            on_done: None,
            on_go: None,
            on_next: None,
            on_previous: None,
            on_search: None,
            on_send: None,
        }
    }
}

struct NoopKeyboardActionScope;
impl KeyboardActionScope for NoopKeyboardActionScope {
    fn default_keyboard_action(&self, _action: ImeAction) {}
}

/// Handles IME action button presses. Single-callback interface used by the
/// new `BasicTextField(state, ...)` API. Corresponds to Compose's `KeyboardActionHandler`.
pub trait KeyboardActionHandler: Debug + 'static {
    fn on_keyboard_action(&self, perform_default: &dyn Fn());
}

/// A simple `KeyboardActionHandler` that only executes the default behavior.
#[derive(Clone, Copy, Debug)]
pub struct DefaultKeyboardActionHandler;

impl KeyboardActionHandler for DefaultKeyboardActionHandler {
    fn on_keyboard_action(&self, perform_default: &dyn Fn()) {
        perform_default();
    }
}

/// Mutable text buffer used as the scope for `InputTransformation` and
/// `OutputTransformation`. Corresponds to Compose's `TextFieldBuffer`.
pub trait TextFieldBuffer {
    fn text(&self) -> &str;
    fn set_text(&mut self, text: &str);
    fn selection(&self) -> TextRange;
    fn set_selection(&mut self, sel: TextRange);
    fn length(&self) -> usize;
    fn replace(&mut self, start: usize, end: usize, text: &str);
    fn insert(&mut self, index: usize, text: &str);
    fn delete(&mut self, start: usize, end: usize);
    fn place_cursor_before_char_at(&mut self, index: usize);
    fn place_cursor_at_end(&mut self);
    fn select_all(&mut self);
    fn revert_all_changes(&mut self);
    fn original_text(&self) -> &str;
    fn original_selection(&self) -> TextRange;
    fn has_selection(&self) -> bool;
}

/// Limits on the number of visible lines in a text field.
/// Corresponds to Compose's `TextFieldLineLimits`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFieldLineLimits {
    SingleLine,
    MultiLine {
        min_height_in_lines: usize,
        max_height_in_lines: usize,
    },
}

impl TextFieldLineLimits {
    pub fn default() -> Self {
        TextFieldLineLimits::MultiLine {
            min_height_in_lines: 1,
            max_height_in_lines: usize::MAX,
        }
    }
}

/// Wraps the inner text field with custom decorations.
/// Corresponds to Compose's `TextFieldDecorator`.
pub trait TextFieldDecorator: Debug + 'static {
    fn decorate(&self, inner: crate::View) -> crate::View;
}

/// A `TextFieldDecorator` that passes through the inner text field unchanged.
#[derive(Clone, Copy, Debug)]
pub struct DefaultTextFieldDecorator;

impl TextFieldDecorator for DefaultTextFieldDecorator {
    fn decorate(&self, inner: crate::View) -> crate::View {
        inner
    }
}

/// Transforms user input before it is applied to the text field.
/// Corresponds to Compose's `InputTransformation`.
pub trait InputTransformation: Debug + 'static {
    fn keyboard_options(&self) -> Option<KeyboardOptions> {
        None
    }
    fn transform_input(&self, buffer: &mut dyn TextFieldBuffer);
}

/// Transforms text output for display.
/// Corresponds to Compose's `OutputTransformation`.
pub trait OutputTransformation: Debug + 'static {
    fn transform_output(&self, buffer: &mut dyn TextFieldBuffer);
}

/// Internal 1-to-1 codepoint transformation for password obfuscation.
/// Corresponds to Compose's `CodepointTransformation`.
pub struct CodepointTransformation {
    pub transform: Box<dyn Fn(usize, char) -> char>,
}

impl Clone for CodepointTransformation {
    fn clone(&self) -> Self {
        // Cannot clone Box<dyn Fn>; this is for internal use only.
        // In practice, CodepointTransformation is passed as an Option and
        // constructed fresh each time. If clone is needed, wrap the Fn in Rc.
        panic!("CodepointTransformation::clone() is not supported -> use Rc instead");
    }
}

impl CodepointTransformation {
    pub fn new(transform: impl Fn(usize, char) -> char + 'static) -> Self {
        CodepointTransformation {
            transform: Box::new(transform),
        }
    }

    pub fn transform(&self, codepoint_index: usize, codepoint: char) -> char {
        (self.transform)(codepoint_index, codepoint)
    }
}

/// Input transformation settings: keyboard type, capitalization, and IME action.
/// Corresponds to Compose's `KeyboardOptions`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardOptions {
    pub keyboard_type: KeyboardType,
    pub capitalization: KeyboardCapitalization,
    pub ime_action: ImeAction,
    pub auto_correct_enabled: Option<bool>,
    pub show_keyboard_on_focus: Option<bool>,
    pub platform_ime_options: Option<&'static str>,
    pub hint_locales: Option<&'static str>,
}

impl KeyboardOptions {
    pub const DEFAULT: KeyboardOptions = KeyboardOptions {
        keyboard_type: KeyboardType::Text,
        capitalization: KeyboardCapitalization::Unspecified,
        ime_action: ImeAction::Unspecified,
        auto_correct_enabled: None,
        show_keyboard_on_focus: None,
        platform_ime_options: None,
        hint_locales: None,
    };

    pub const SECURE_TEXT_FIELD: KeyboardOptions = KeyboardOptions {
        keyboard_type: KeyboardType::Password,
        capitalization: KeyboardCapitalization::Unspecified,
        ime_action: ImeAction::Unspecified,
        auto_correct_enabled: Some(false),
        show_keyboard_on_focus: None,
        platform_ime_options: None,
        hint_locales: None,
    };

    pub fn fill_unspecified_values_with(&self, other: Option<&KeyboardOptions>) -> KeyboardOptions {
        let other = match other {
            Some(o) => o,
            None => return *self,
        };
        KeyboardOptions {
            keyboard_type: if self.keyboard_type == KeyboardType::Unspecified {
                other.keyboard_type
            } else {
                self.keyboard_type
            },
            capitalization: if self.capitalization == KeyboardCapitalization::Unspecified {
                other.capitalization
            } else {
                self.capitalization
            },
            ime_action: if self.ime_action == ImeAction::Unspecified {
                other.ime_action
            } else {
                self.ime_action
            },
            auto_correct_enabled: self.auto_correct_enabled.or(other.auto_correct_enabled),
            show_keyboard_on_focus: self.show_keyboard_on_focus.or(other.show_keyboard_on_focus),
            platform_ime_options: self.platform_ime_options.or(other.platform_ime_options),
            hint_locales: self.hint_locales.or(other.hint_locales),
        }
    }

    /// Returns a new [KeyboardOptions] that merges this with [other].
    /// [other]'s null or Unspecified values are replaced with this object's values.
    /// Corresponds to Compose's `KeyboardOptions.merge()`.
    pub fn merge(&self, other: Option<&KeyboardOptions>) -> KeyboardOptions {
        other.map_or(*self, |o| o.fill_unspecified_values_with(Some(self)))
    }
}

impl Default for KeyboardOptions {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpanStyle {
    pub color: Option<Color>,
    pub font_size: Option<f32>,
    pub font_weight: Option<u16>,
    pub font_family: Option<&'static str>,
    pub font_style: Option<u8>,
    pub text_align: Option<crate::TextAlign>,
    pub letter_spacing: Option<f32>,
    pub line_height: Option<f32>,
    pub background: Option<Color>,
    pub text_decoration: Option<crate::TextDecoration>,
    pub text_direction: Option<crate::TextDirection>,
    pub font_synthesis: Option<FontSynthesis>,
    pub baseline_shift: Option<BaselineShift>,
    pub hyphens: Option<Hyphens>,
    pub line_break: Option<LineBreak>,
    pub text_indent: Option<TextIndent>,
    pub draw_style: Option<DrawStyle>,
    pub alpha: f32,
}

impl SpanStyle {
    pub const fn default() -> Self {
        Self {
            color: None,
            font_size: None,
            font_weight: None,
            font_family: None,
            font_style: None,
            text_align: None,
            letter_spacing: None,
            line_height: None,
            background: None,
            text_decoration: None,
            text_direction: None,
            font_synthesis: None,
            baseline_shift: None,
            hyphens: None,
            line_break: None,
            text_indent: None,
            draw_style: None,
            alpha: 0.0,
        }
    }

    pub fn color(mut self, c: Color) -> Self {
        self.color = Some(c);
        self
    }

    pub fn font_size(mut self, px: f32) -> Self {
        self.font_size = Some(px);
        self
    }

    pub fn text_decoration(mut self, d: TextDecoration) -> Self {
        self.text_decoration = Some(d);
        self
    }

    pub fn font_weight(mut self, w: u16) -> Self {
        self.font_weight = Some(w);
        self
    }

    pub fn font_style(mut self, s: u8) -> Self {
        self.font_style = Some(s);
        self
    }

    pub fn draw_style(mut self, s: DrawStyle) -> Self {
        self.draw_style = Some(s);
        self
    }

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }

    pub fn baseline_shift(mut self, s: BaselineShift) -> Self {
        self.baseline_shift = Some(s);
        self
    }
}

impl Default for SpanStyle {
    fn default() -> Self {
        Self::default()
    }
}

/// A span of text with an associated style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpan {
    /// Byte offset start in the original text.
    pub start: usize,
    /// Byte offset end (exclusive) in the original text.
    pub end: usize,
    pub style: SpanStyle,
    /// URL for clickable links.
    pub url: Option<Arc<str>>,
}

/// Text with multiple styled spans.
///
/// Analogous to Compose's `AnnotatedString`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedString {
    pub text: String,
    pub spans: Arc<[TextSpan]>,
}

impl AnnotatedString {
    pub fn new(text: impl Into<String>, spans: Vec<TextSpan>) -> Self {
        let text = text.into();
        Self {
            text,
            spans: spans.into(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }
}

impl From<String> for AnnotatedString {
    fn from(text: String) -> Self {
        Self {
            text,
            spans: Arc::from([]),
        }
    }
}

impl From<&str> for AnnotatedString {
    fn from(text: &str) -> Self {
        Self {
            text: text.to_string(),
            spans: Arc::from([]),
        }
    }
}

/// Builder for constructing an `AnnotatedString`.
#[derive(Default)]
pub struct AnnotatedStringBuilder {
    text: String,
    spans: Vec<TextSpan>,
}

impl AnnotatedStringBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append plain text (inherits parent style, or default if at top level).
    pub fn push(&mut self, text: &str) -> &mut Self {
        self.text.push_str(text);
        self
    }

    /// Append text with a specific style.
    pub fn push_with_style(&mut self, text: &str, style: SpanStyle) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        if start < end {
            self.spans.push(TextSpan {
                start,
                end,
                style,
                url: None,
            });
        }
        self
    }

    /// Append text in a specific color.
    pub fn push_color(&mut self, text: &str, color: Color) -> &mut Self {
        self.push_with_style(text, SpanStyle::default().color(color))
    }

    /// Append text with a clickable link URL (auto-applies underline + blue color).
    pub fn push_link(&mut self, text: &str, url: impl Into<Arc<str>>) -> &mut Self {
        let start = self.text.len();
        self.text.push_str(text);
        let end = self.text.len();
        if start < end {
            self.spans.push(TextSpan {
                start,
                end,
                style: SpanStyle::default()
                    .color(Color::from_rgba(0x15, 0x76, 0xFF, 255))
                    .text_decoration(TextDecoration::UNDERLINE),
                url: Some(url.into()),
            });
        }
        self
    }

    /// Apply a style to a range of already-appended text.
    pub fn add_style(&mut self, start: usize, end: usize, style: SpanStyle) -> &mut Self {
        if start < end && end <= self.text.len() {
            self.spans.push(TextSpan {
                start,
                end,
                style,
                url: None,
            });
        }
        self
    }

    pub fn build(&mut self) -> AnnotatedString {
        let text = std::mem::take(&mut self.text);
        self.spans.sort_by_key(|s| s.start);
        // Merge overlapping/adjacent spans with same style
        let mut merged: Vec<TextSpan> = Vec::new();
        for span in std::mem::take(&mut self.spans) {
            if let Some(last) = merged.last_mut()
                && last.end == span.start
                && last.style == span.style
            {
                last.end = span.end;
                continue;
            }
            merged.push(span);
        }
        AnnotatedString {
            text,
            spans: merged.into(),
        }
    }
}

/// Horizontal text alignment.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
    Unspecified,
}

impl Default for TextAlign {
    fn default() -> Self {
        TextAlign::Unspecified
    }
}

/// Font weight as a numeric value 100-900, matching CSS `font-weight`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const SEMI_BOLD: FontWeight = FontWeight(600);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const EXTRA_BOLD: FontWeight = FontWeight(800);
    pub const BLACK: FontWeight = FontWeight(900);
}

impl Default for FontWeight {
    fn default() -> Self {
        FontWeight::NORMAL
    }
}

/// Font style: normal or italic.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum FontStyle {
    Normal,
    Italic,
}

impl Default for FontStyle {
    fn default() -> Self {
        FontStyle::Normal
    }
}

/// Text decoration state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<Color>,
}

impl TextDecoration {
    pub const UNDERLINE: TextDecoration = TextDecoration {
        underline: true,
        strikethrough: false,
        color: None,
    };
    pub const STRIKETHROUGH: TextDecoration = TextDecoration {
        underline: false,
        strikethrough: true,
        color: None,
    };
}

impl Default for TextDecoration {
    fn default() -> Self {
        Self {
            underline: false,
            strikethrough: false,
            color: None,
        }
    }
}

/// Convenience function to build an `AnnotatedString`.
pub fn build_annotated_string(b: impl FnOnce(&mut AnnotatedStringBuilder)) -> AnnotatedString {
    let mut builder = AnnotatedStringBuilder::new();
    b(&mut builder);
    builder.build()
}
