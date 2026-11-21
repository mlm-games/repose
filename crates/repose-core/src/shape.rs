//! Compose-style shapes and their resolution against a layout [`Rect`].
//!
//! A [`Shape`] is authored in density-independent pixels (or as a relative
//! percentage) and is resolved once layout has produced a px-space [`Rect`].
//! The resolved form, [`ResolvedShape`], is what the paint layer consumes.

use std::sync::Arc;

use crate::geometry::Rect;
use crate::units::{Dp, Px};

/// Corner radii order used by [`ResolvedShape::Rect`] and [`Shape::Rounded`]:
/// `[top_left, top_right, bottom_right, bottom_left]` (CSS order).
pub type CornerRadii = [Dp; 4];

/// A single command of a [`ShapePath`], in Dp-local coordinates relative to the
/// element's top-left corner. The paint layer converts these to px with the
/// ambient density and adds the resolved offset.
#[derive(Clone, Debug, PartialEq)]
pub enum PathCommand {
    /// Start a new subpath at `(x, y)`.
    MoveTo(Dp, Dp),
    /// Append a straight line to `(x, y)`.
    LineTo(Dp, Dp),
    /// Append a cubic Bezier with control points `c1`, `c2` and endpoint `end`.
    CubicTo {
        c1: (Dp, Dp),
        c2: (Dp, Dp),
        end: (Dp, Dp),
    },
    /// Close the current subpath back to its starting point.
    Close,
}

/// An arbitrary shape described by path commands in Dp-local space.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapePath {
    pub commands: Vec<PathCommand>,
}

impl ShapePath {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn move_to(mut self, x: Dp, y: Dp) -> Self {
        self.commands.push(PathCommand::MoveTo(x, y));
        self
    }

    pub fn line_to(mut self, x: Dp, y: Dp) -> Self {
        self.commands.push(PathCommand::LineTo(x, y));
        self
    }

    pub fn cubic_to(mut self, c1: (Dp, Dp), c2: (Dp, Dp), end: (Dp, Dp)) -> Self {
        self.commands.push(PathCommand::CubicTo { c1, c2, end });
        self
    }

    pub fn close(mut self) -> Self {
        self.commands.push(PathCommand::Close);
        self
    }
}

/// A Compose-style shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Shape {
    /// A sharp-cornered rectangle filling the bounds.
    #[default]
    Rectangle,
    /// A rectangle with independent per-corner radii, in [`Dp`].
    Rounded { radii: CornerRadii },
    /// A rounded shape whose radius is `percent / 100` of half the rect's
    /// smaller side (0 = sharp rectangle, 100 = same radius as [`Shape::Circle`]).
    /// The value is clamped to `0.0..=100.0`.
    Percent(f32),
    /// A circle/ellipse inscribed in the bounds (Compose's `CircleShape`).
    Circle,
    /// An arbitrary path in Dp-local coordinates, offset to the rect origin.
    Path(Arc<ShapePath>),
}

impl Shape {
    /// Uniform-radius rounded rectangle.
    pub fn rounded(radius: Dp) -> Self {
        Shape::Rounded { radii: [radius; 4] }
    }

    /// Arbitrary path shape.
    pub fn path(path: ShapePath) -> Self {
        Shape::Path(Arc::new(path))
    }

    /// Resolve this shape against a px-space layout rect.
    pub fn resolve(&self, rect: Rect) -> ResolvedShape {
        match self {
            Shape::Rectangle => ResolvedShape::Rect([Px::ZERO; 4]),
            Shape::Rounded { radii } => ResolvedShape::Rect([
                radii[0].to_px(),
                radii[1].to_px(),
                radii[2].to_px(),
                radii[3].to_px(),
            ]),
            Shape::Percent(percent) => {
                let half_min = (rect.w.min(rect.h) * 0.5).max(0.0);
                let radius = Px(half_min * (percent.clamp(0.0, 100.0) / 100.0));
                ResolvedShape::Rect([radius; 4])
            }
            Shape::Circle => ResolvedShape::Ellipse,
            Shape::Path(path) => ResolvedShape::Path {
                path: path.clone(),
                offset: (Px(rect.x), Px(rect.y)),
            },
        }
    }
}

/// A [`Shape`] resolved to px-space against a concrete layout [`Rect`].
#[derive(Clone, Debug, PartialEq)]
pub enum ResolvedShape {
    /// Corner radii in px, ordered `[top_left, top_right, bottom_right,
    /// bottom_left]`. Zero radii mean a sharp rectangle.
    Rect([Px; 4]),
    /// An ellipse inscribed in the scene node's rect.
    Ellipse,
    /// A Dp-local path plus its px-space translation (the rect's origin).
    Path {
        path: Arc<ShapePath>,
        offset: (Px, Px),
    },
}
