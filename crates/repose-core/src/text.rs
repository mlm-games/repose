use crate::Color;
use std::rc::Rc;
use std::sync::Arc;

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

/// Transforms the visual representation of a text field's text without changing
/// the underlying value. For example, password masking.
pub trait VisualTransformation: std::fmt::Debug + Send + Sync + 'static {
    /// Transform the text for display. Returns the transformed text and an
    /// offset-translation function that maps offsets in the display text back
    /// to the original text.
    fn filter(&self, text: &str) -> TransformedText;
}

/// The result of applying a `VisualTransformation`.
pub struct TransformedText {
    /// The text to display (e.g., "•••••" for a password).
    pub text: String,
    /// Maps an offset in `text` back to the original offset.
    pub offset_map: Rc<dyn Fn(usize) -> usize>,
}

impl Clone for TransformedText {
    fn clone(&self) -> Self {
        Self {
            text: self.text.clone(),
            offset_map: self.offset_map.clone(),
        }
    }
}

impl std::fmt::Debug for TransformedText {
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
        let len = text.len();
        TransformedText {
            text: text.to_string(),
            offset_map: Rc::new(move |offset| offset.min(len)),
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
        let len = text.len();
        TransformedText {
            text: masked,
            offset_map: Rc::new(move |offset| offset.min(len)),
        }
    }
}

/// Convert a byte offset in the original text to the corresponding byte offset
/// in the visually-transformed display text, assuming each original character
/// maps to one or more display characters starting at a predictable position.
pub fn original_offset_to_display(original: &str, display: &str, original_byte: usize) -> usize {
    let char_idx = original[..original_byte.min(original.len())]
        .chars()
        .count();
    display
        .char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(display.len())
}

/// Hints the platform about the type of keyboard to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum KeyboardType {
    #[default]
    Text,
    Ascii,
    Number,
    Phone,
    Email,
    Uri,
    Decimal,
}

/// The action button on the IME (soft keyboard).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ImeAction {
    #[default]
    Unspecified,
    None,
    Go,
    Search,
    Send,
    Next,
    Done,
    Previous,
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
