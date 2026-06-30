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
    pub text: String,
    /// Selection range in byte offsets. Collapsed range (start == end) = cursor.
    pub selection: TextRange,
    /// Active IME composition range in byte offsets, or None.
    pub composition: Option<TextRange>,
}

impl TextFieldValue {
    pub fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            selection: TextRange::collapsed(len),
            text,
            composition: None,
        }
    }

    pub fn with_selection(mut self, start: usize, end: usize) -> Self {
        let len = self.text.len();
        self.selection = TextRange::new(start.min(len), end.min(len));
        self
    }

    pub fn text_before_selection(&self, max_chars: usize) -> String {
        let start = self.selection.start.saturating_sub(max_chars);
        self.text[start..self.selection.start].to_string()
    }

    pub fn text_after_selection(&self, max_chars: usize) -> String {
        let end = (self.selection.end + max_chars).min(self.text.len());
        self.text[self.selection.end..end].to_string()
    }

    pub fn selected_text(&self) -> String {
        let r = self.selection.min()..self.selection.max();
        self.text[r].to_string()
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
}

/// Bidirectional offset mapping between original and transformed text.
/// Corresponds to Compose's `OffsetMapping` interface.
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
    /// Transform the text for display. Returns the transformed text and an
    /// offset-translation function that maps offsets in the display text back
    /// to the original text.
    fn filter(&self, text: &str) -> TransformedText;
}

/// The result of applying a `VisualTransformation`.
pub struct TransformedText {
    /// The text to display (e.g., "•••••" for a password).
    pub text: String,
    /// Maps offsets between original and transformed text.
    pub offset_mapping: Box<dyn OffsetMapping>,
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
            .field("text", &self.text)
            .finish()
    }
}

/// No visual transformation - text is displayed as-is.
#[derive(Clone, Copy, Debug)]
pub struct NoVisualTransformation;

impl VisualTransformation for NoVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        TransformedText {
            text: text.to_string(),
            offset_mapping: Box::new(IdentityOffsetMapping),
        }
    }
}

/// A `VisualTransformation` that masks all characters with `*`.
#[derive(Clone, Copy, Debug)]
pub struct PasswordVisualTransformation {
    /// The replacement character (default `*`).
    pub mask_char: char,
}

impl Default for PasswordVisualTransformation {
    fn default() -> Self {
        Self { mask_char: '*' }
    }
}

impl VisualTransformation for PasswordVisualTransformation {
    fn filter(&self, text: &str) -> TransformedText {
        let masked: String = text.chars().map(|_| self.mask_char).collect();
        let src = text.to_string();
        TransformedText {
            text: masked,
            offset_mapping: Box::new(PasswordOffsetMapping { original: src }),
        }
    }
}

#[derive(Clone, Debug)]
struct PasswordOffsetMapping {
    original: String,
}

impl OffsetMapping for PasswordOffsetMapping {
    fn original_to_transformed(&self, offset: usize) -> usize {
        let char_idx = self.original[..offset.min(self.original.len())]
            .chars()
            .count();
        char_idx
    }
    fn transformed_to_original(&self, offset: usize) -> usize {
        self.original
            .char_indices()
            .nth(offset)
            .map(|(i, _)| i)
            .unwrap_or(self.original.len())
    }
    fn clone_box(&self) -> Box<dyn OffsetMapping> {
        Box::new(self.clone())
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

/// Style configuration for text displayed in a text field.
/// Corresponds to a subset of Compose's `TextStyle`.
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    /// Font size in dp. 0 = use default (16dp for TextField).
    pub font_size: f32,
    /// Text color. None = use theme default.
    pub color: Option<Color>,
    /// Font weight. None = NORMAL.
    pub font_weight: Option<u16>,
    /// Font family. None = use default.
    pub font_family: Option<&'static str>,
    /// Font style. None = Normal.
    pub font_style: Option<u8>,
    /// Text alignment. Unspecified = inherit.
    pub text_align: crate::TextAlign,
    /// Letter spacing in dp. 0 = no extra spacing.
    pub letter_spacing: f32,
    /// Line height in dp. 0 = default (font_size * 1.2).
    pub line_height: f32,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_size: 0.0,
            color: None,
            font_weight: None,
            font_family: None,
            font_style: None,
            text_align: crate::TextAlign::Unspecified,
            letter_spacing: 0.0,
            line_height: 0.0,
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
/// Corresponds to Compose's `ImeAction` with all 9 variants.
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

/// Callbacks for IME action button presses on the soft keyboard.
/// Corresponds to Compose's `KeyboardActions`.
#[derive(Clone, Default)]
pub struct KeyboardActions {
    pub on_done: Option<Rc<dyn Fn()>>,
    pub on_go: Option<Rc<dyn Fn()>>,
    pub on_next: Option<Rc<dyn Fn()>>,
    pub on_previous: Option<Rc<dyn Fn()>>,
    pub on_search: Option<Rc<dyn Fn()>>,
    pub on_send: Option<Rc<dyn Fn()>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpanStyle {
    pub color: Option<Color>,
    pub font_size: Option<f32>,
}

impl SpanStyle {
    pub const fn default() -> Self {
        Self {
            color: None,
            font_size: None,
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
}

/// Text with multiple styled spans.
///
/// Analogous to Compose's `AnnotatedString`.
#[derive(Debug, Clone)]
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
            self.spans.push(TextSpan { start, end, style });
        }
        self
    }

    /// Append text in a specific color.
    pub fn push_color(&mut self, text: &str, color: Color) -> &mut Self {
        self.push_with_style(text, SpanStyle::default().color(color))
    }

    /// Apply a style to a range of already-appended text.
    pub fn add_style(&mut self, start: usize, end: usize, style: SpanStyle) -> &mut Self {
        if start < end && end <= self.text.len() {
            self.spans.push(TextSpan { start, end, style });
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
