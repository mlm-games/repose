//! Text selection highlight geometry.
//!
//! Compose fills the path from `TextLayoutResult.getPathForRange(start, end)`,
//! which on Skia is `getRectsForRange(start, end, RectHeightMode.MAX,
//! RectWidthMode.TIGHT)`: one rect per line the range touches, spanning the full
//! line height and horizontally tight to the selected glyphs, never stretched
//! to the line or node width.

use std::ops::Range;

use repose_core::{Brush, Color, Px, Rect, Scene, SceneNode};

/// Compose `TextSelectionBackgroundOpacity` from `MaterialTheme.kt`.
pub(crate) const SELECTION_BACKGROUND_ALPHA: u8 = (0.4 * 255.0) as u8;

/// `colorScheme.primary.copy(alpha = 0.4f)`.
pub(crate) fn selection_brush(primary: Color) -> Brush {
    Brush::Solid(primary.with_alpha(SELECTION_BACKGROUND_ALPHA))
}

/// A laid-out line: the byte range it covers and its box within the text block.
pub(crate) struct SelectionLine {
    pub start: usize,
    pub end: usize,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub height: f32,
}

/// One rect per touched line. `x_for` maps a byte offset inside `line` to its x
/// position; the returned rects are offset by `origin`.
pub(crate) fn selection_rects(
    lines: &[SelectionLine],
    range: &Range<usize>,
    x_for: &dyn Fn(&SelectionLine, usize) -> f32,
    origin: (f32, f32),
) -> Vec<Rect> {
    if range.start >= range.end || lines.is_empty() {
        return Vec::new();
    }
    let first = match lines.iter().position(|l| range.start < l.end) {
        Some(i) => i,
        None => return Vec::new(),
    };
    let last = match lines.iter().rposition(|l| range.end > l.start) {
        Some(i) => i,
        None => return Vec::new(),
    };

    let mut rects = Vec::with_capacity(last - first + 1);
    for (i, line) in lines.iter().enumerate().take(last + 1).skip(first) {
        let sel_start = if i == first { range.start } else { line.start };
        let sel_end = if i == last { range.end } else { line.end };
        let x0 = x_for(line, sel_start).max(line.left);
        let x1 = x_for(line, sel_end).min(line.right);
        let w = x1 - x0;
        if w <= 0.0 {
            continue;
        }
        rects.push(Rect {
            x: origin.0 + x0,
            y: origin.1 + line.top,
            w,
            h: line.height,
        });
    }
    rects
}

pub(crate) fn push_selection(scene: &mut Scene, rects: Vec<Rect>, brush: Brush) {
    for rect in rects {
        scene.nodes.push(SceneNode::Rect {
            rect,
            brush,
            radius: [Px::ZERO; 4],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "abc\nde\nf", 10px per character, 20px lines.
    fn lines() -> Vec<SelectionLine> {
        let line = |start, end, width, top| SelectionLine {
            start,
            end,
            left: 0.0,
            right: width as f32,
            top: top as f32,
            height: 20.0,
        };
        vec![line(0, 3, 30, 0), line(4, 6, 20, 20), line(7, 8, 10, 40)]
    }

    fn x_for(line: &SelectionLine, byte: usize) -> f32 {
        (byte - line.start) as f32 * 10.0
    }

    /// Rects as (x, y, w, h), with the text block placed at (100, 5).
    fn rects(range: Range<usize>) -> Vec<(f32, f32, f32, f32)> {
        selection_rects(&lines(), &range, &x_for, (100.0, 5.0))
            .into_iter()
            .map(|r| (r.x, r.y, r.w, r.h))
            .collect()
    }

    #[test]
    fn within_one_line_the_rect_is_tight() {
        assert_eq!(rects(1..3), [(110.0, 5.0, 20.0, 20.0)]);
    }

    /// A line the selection runs past ends at its own text end, not at the wrap
    /// width: the old behavior painted full-width boxes here.
    #[test]
    fn line_the_selection_runs_past_ends_at_its_own_text_end() {
        assert_eq!(
            rects(1..5),
            [(110.0, 5.0, 20.0, 20.0), (100.0, 25.0, 10.0, 20.0)]
        );
    }

    #[test]
    fn lines_fully_covered_span_their_own_width() {
        assert_eq!(
            rects(1..8),
            [
                (110.0, 5.0, 20.0, 20.0),
                (100.0, 25.0, 20.0, 20.0),
                (100.0, 45.0, 10.0, 20.0)
            ]
        );
    }

    /// "abc\n" covers line 0 up to the wrap point; the newline itself has no
    /// glyph on line 1 to highlight.
    #[test]
    fn selection_ending_on_a_newline_stops_at_the_line_end() {
        assert_eq!(rects(0..4), [(100.0, 5.0, 30.0, 20.0)]);
    }

    #[test]
    fn collapsed_and_empty_selections_draw_nothing() {
        assert!(rects(2..2).is_empty());
        assert!(selection_rects(&[], &(0..1), &x_for, (0.0, 0.0)).is_empty());
    }
}
