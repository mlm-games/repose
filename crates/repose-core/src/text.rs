use crate::Color;
use std::sync::Arc;

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
            if let Some(last) = merged.last_mut() {
                if last.end == span.start && last.style == span.style {
                    last.end = span.end;
                    continue;
                }
            }
            merged.push(span);
        }
        AnnotatedString {
            text,
            spans: merged.into(),
        }
    }
}

/// Convenience function to build an `AnnotatedString`.
pub fn build_annotated_string(b: impl FnOnce(&mut AnnotatedStringBuilder)) -> AnnotatedString {
    let mut builder = AnnotatedStringBuilder::new();
    b(&mut builder);
    builder.build()
}
