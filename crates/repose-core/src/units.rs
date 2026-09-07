//! Density-independent (`Dp`), physical pixel (`Px`), and font-scaled (`Sp`) units.
//!
//! - `Dp` is a transparent value-class wrapper around `f32`, constructed via
//!   the [`UnitExt::dp`] extension (`16.0.dp()`), like Compose's `Float.dp`.
//! - `Sp` mirrors Compose's `TextUnit` Sp type (`10.0.sp()`). Em is omitted:
//!   repose has no relative-em usage (future addition if needed).
//! - There is **no `Px` scalar type in Compose** (pixels are bare `Float`);
//!   `Px` is added here per repo decision so paint/input boundaries are
//!   explicit instead of unitless `f32`.
//! - Conversions are `Density`-scoped in Compose (`Density.Dp.toPx()`).
//!   The ambient `to_px()` helpers below use the thread-local
//!   `Density * UiScale` (and `* TextScale` for `Sp`), matching repose's
//!   existing `effective_density_scale()` behavior; explicit
//!   [`Density`](crate::locals::Density) overloads take an exact scale.
//! - Compound geometry is split like Compose: dp-space
//!   [`DpOffset`]/[`DpSize`]/[`DpRect`] vs px-space `Vec2`/`Size`/`Rect`
//!   (Compose `Offset`/`Size`/`Rect` are px floats).
//! - `Duration` is intentionally **not** wrapped: like Compose (stdlib
//!   `kotlin.time.Duration`), repose uses `std`/`web_time::Duration`
//!   (see `MotionScheme`/`AnimationSpec`).
//! - `Velocity` is intentionally **not** wrapped either: like Compose's
//!   px-per-second `Velocity`, repose expresses velocities as `f32` px/s
//!   (see `gesture`/`scroll` physics, documented at each site). A newtype
//!   can be added if velocity/dp confusion ever arises; today the `/s`
//!   dimension makes misuse obvious.

use std::fmt;
use std::ops::{Add, Div, Mul, Neg, Sub};

use crate::geometry::{Rect, Size, Vec2};

/// Density-independent pixels. Authoring APIs (Modifier, layout) take this.
///
/// Compose parity (`Dp.kt`): value class, `Hairline`/`Infinity`/`Unspecified`,
/// `isSpecified`, arithmetic, `coerce*`, `lerp`.
#[derive(Clone, Copy, Default, PartialEq)]
#[repr(transparent)]
pub struct Dp(pub f32);

impl Dp {
    pub const ZERO: Dp = Dp(0.0);
    /// Compose `Dp.Hairline` (0.dp: takes no space, draws 1 px).
    pub const HAIRLINE: Dp = Dp(0.0);
    pub const INFINITY: Dp = Dp(f32::INFINITY);
    /// Compose `Dp.Unspecified` (NaN sentinel).
    pub const UNSPECIFIED: Dp = Dp(f32::NAN);

    #[inline]
    pub const fn from_px_raw(v: f32) -> Dp {
        Dp(v)
    }

    /// Raw value in dp.
    #[inline]
    pub const fn value(self) -> f32 {
        self.0
    }

    /// `false` for [`Dp::UNSPECIFIED`].
    #[inline]
    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }

    /// `true` for [`Dp::UNSPECIFIED`].
    #[inline]
    pub fn is_unspecified(self) -> bool {
        self.0.is_nan()
    }

    /// Return self if specified, else `block()`. (Compose `takeOrElse`.)
    #[inline]
    pub fn take_or_else(self, block: impl FnOnce() -> Dp) -> Dp {
        if self.is_specified() { self } else { block() }
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// Absolute value (mirrors `f32::abs`).
    #[inline]
    pub fn abs(self) -> Dp {
        Dp(self.0.abs())
    }

    /// Minimum of two `Dp`s (mirrors `f32::min`).
    #[inline]
    pub fn min(self, other: Dp) -> Dp {
        Dp(self.0.min(other.0))
    }

    /// Maximum of two `Dp`s (mirrors `f32::max`).
    #[inline]
    pub fn max(self, other: Dp) -> Dp {
        Dp(self.0.max(other.0))
    }

    /// Clamp within `[min, max]` (mirrors `f32::clamp`).
    #[inline]
    pub fn clamp(self, min: Dp, max: Dp) -> Dp {
        Dp(self.0.clamp(min.0, max.0))
    }

    #[inline]
    pub fn coerce_in(self, min: Dp, max: Dp) -> Dp {
        Dp(self.0.clamp(min.0, max.0))
    }

    #[inline]
    pub fn coerce_at_least(self, min: Dp) -> Dp {
        Dp(self.0.max(min.0))
    }

    #[inline]
    pub fn coerce_at_most(self, max: Dp) -> Dp {
        Dp(self.0.min(max.0))
    }

    /// Convert to physical pixels with the ambient `Density * UiScale`.
    #[inline]
    pub fn to_px(self) -> Px {
        Px(self.0 * crate::locals::effective_density_scale())
    }

    /// Convert to physical pixels with an explicit scale.
    #[inline]
    pub fn to_px_with_scale(self, scale: f32) -> Px {
        Px(self.0 * scale)
    }

    /// Round to whole pixels with the ambient scale (Compose `roundToPx`).
    #[inline]
    pub fn round_to_px(self) -> i32 {
        self.to_px().0.round() as i32
    }

    /// Compose `FontScalingLinear.Dp.toSp`: `value / fontScale`.
    #[inline]
    pub fn to_sp(self) -> Sp {
        let fs = crate::locals::text_scale().0.max(0.0001);
        Sp(self.0 / fs)
    }

    /// Normalized bits for hashing / `scope!` inputs
    /// (like `f32::to_bits`, with `-0.0`/`NaN` normalized).
    #[inline]
    pub fn to_bits(self) -> u32 {
        hash_f32_bits(self.0)
    }
}

impl fmt::Debug for Dp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unspecified() {
            write!(f, "Dp.Unspecified")
        } else {
            write!(f, "{}dp", self.0)
        }
    }
}

impl fmt::Display for Dp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unspecified() {
            write!(f, "Dp.Unspecified")
        } else {
            write!(f, "{}dp", self.0)
        }
    }
}

impl std::hash::Hash for Dp {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // `Eq` is intentionally NOT implemented (NaN `Unspecified` never
        // equals itself, like Compose); hashing still works via normalized bits.
        hash_f32_bits(self.0).hash(state);
    }
}

/// Bit hash of an `f32` with `-0.0` and NaN normalized
/// (same normalization as `repose-tree` content hashing).
#[inline]
fn hash_f32_bits(v: f32) -> u32 {
    let mut bits = v.to_bits();
    if bits == 0x8000_0000 {
        bits = 0;
    }
    if v.is_nan() {
        bits = 0x7FC0_0000;
    }
    bits
}

impl Add for Dp {
    type Output = Dp;
    #[inline]
    fn add(self, other: Dp) -> Dp {
        Dp(self.0 + other.0)
    }
}

impl Sub for Dp {
    type Output = Dp;
    #[inline]
    fn sub(self, other: Dp) -> Dp {
        Dp(self.0 - other.0)
    }
}

impl Neg for Dp {
    type Output = Dp;
    #[inline]
    fn neg(self) -> Dp {
        Dp(-self.0)
    }
}

impl Mul<f32> for Dp {
    type Output = Dp;
    #[inline]
    fn mul(self, other: f32) -> Dp {
        Dp(self.0 * other)
    }
}

impl Mul<Dp> for f32 {
    type Output = Dp;
    #[inline]
    fn mul(self, other: Dp) -> Dp {
        Dp(self * other.0)
    }
}

impl Div<f32> for Dp {
    type Output = Dp;
    #[inline]
    fn div(self, other: f32) -> Dp {
        Dp(self.0 / other)
    }
}

/// Divide by another `Dp` to get a scalar (Compose `Dp.div(Dp): Float`).
impl Div<Dp> for Dp {
    type Output = f32;
    #[inline]
    fn div(self, other: Dp) -> f32 {
        self.0 / other.0
    }
}

impl PartialOrd for Dp {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Compose: unspecified compares == 0 but never equals.
        if self.0.is_nan() || other.0.is_nan() {
            Some(std::cmp::Ordering::Equal)
        } else {
            self.0.partial_cmp(&other.0)
        }
    }
}

/// Linear interpolation between two `Dp`s (Compose `lerp`).
#[inline]
pub fn lerp_dp(start: Dp, stop: Dp, fraction: f32) -> Dp {
    Dp(start.0 + (stop.0 - start.0) * fraction)
}

#[inline]
pub fn min_dp(a: Dp, b: Dp) -> Dp {
    Dp(a.0.min(b.0))
}

#[inline]
pub fn max_dp(a: Dp, b: Dp) -> Dp {
    Dp(a.0.max(b.0))
}

/// Physical pixels. Paint/input/raster boundaries take this.
///
/// NOTE: Compose has no `Px` scalar type (px are bare `Float`); this newtype
/// exists so `f32` return/param positions are unambiguous in repose.
#[derive(Clone, Copy, Default, PartialEq)]
#[repr(transparent)]
pub struct Px(pub f32);

impl Px {
    pub const ZERO: Px = Px(0.0);
    pub const INFINITY: Px = Px(f32::INFINITY);
    pub const UNSPECIFIED: Px = Px(f32::NAN);

    #[inline]
    pub const fn value(self) -> f32 {
        self.0
    }

    #[inline]
    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }

    #[inline]
    pub fn is_finite(self) -> bool {
        self.0.is_finite()
    }

    /// Absolute value (mirrors `f32::abs`).
    #[inline]
    pub fn abs(self) -> Px {
        Px(self.0.abs())
    }

    /// Minimum of two `Px`s (mirrors `f32::min`).
    #[inline]
    pub fn min(self, other: Px) -> Px {
        Px(self.0.min(other.0))
    }

    /// Maximum of two `Px`s (mirrors `f32::max`).
    #[inline]
    pub fn max(self, other: Px) -> Px {
        Px(self.0.max(other.0))
    }

    /// Convert to dp with the ambient `Density * UiScale`.
    #[inline]
    pub fn to_dp(self) -> Dp {
        let scale = crate::locals::effective_density_scale();
        if scale <= 0.0001 {
            Dp(0.0)
        } else {
            Dp(self.0 / scale)
        }
    }

    /// Convert to dp with an explicit scale.
    #[inline]
    pub fn to_dp_with_scale(self, scale: f32) -> Dp {
        if scale <= 0.0001 {
            Dp(0.0)
        } else {
            Dp(self.0 / scale)
        }
    }

    /// Convert a px value to `Sp` (px → dp → sp).
    #[inline]
    pub fn to_sp(self) -> Sp {
        self.to_dp().to_sp()
    }
}

impl fmt::Debug for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}px", self.0)
    }
}

impl fmt::Display for Px {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}px", self.0)
    }
}

impl std::hash::Hash for Px {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        hash_f32_bits(self.0).hash(state);
    }
}

impl Add for Px {
    type Output = Px;
    #[inline]
    fn add(self, other: Px) -> Px {
        Px(self.0 + other.0)
    }
}

impl Sub for Px {
    type Output = Px;
    #[inline]
    fn sub(self, other: Px) -> Px {
        Px(self.0 - other.0)
    }
}

impl Neg for Px {
    type Output = Px;
    #[inline]
    fn neg(self) -> Px {
        Px(-self.0)
    }
}

impl Mul<f32> for Px {
    type Output = Px;
    #[inline]
    fn mul(self, other: f32) -> Px {
        Px(self.0 * other)
    }
}

impl Mul<Px> for f32 {
    type Output = Px;
    #[inline]
    fn mul(self, other: Px) -> Px {
        Px(self * other.0)
    }
}

impl Div<f32> for Px {
    type Output = Px;
    #[inline]
    fn div(self, other: f32) -> Px {
        Px(self.0 / other)
    }
}

impl Div<Px> for Px {
    type Output = f32;
    #[inline]
    fn div(self, other: Px) -> f32 {
        self.0 / other.0
    }
}

impl PartialOrd for Px {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[inline]
pub fn lerp_px(start: Px, stop: Px, fraction: f32) -> Px {
    Px(start.0 + (stop.0 - start.0) * fraction)
}

/// Scaled pixels for text. Mirrors Compose `TextUnit` Sp type.
///
/// Converted to px with `Density * UiScale * TextScale`
/// (Compose `Density.TextUnit.toPx` via `toDp().toPx()`).
#[derive(Clone, Copy, Default, PartialEq)]
#[repr(transparent)]
pub struct Sp(pub f32);

impl Sp {
    pub const ZERO: Sp = Sp(0.0);
    pub const UNSPECIFIED: Sp = Sp(f32::NAN);

    #[inline]
    pub const fn value(self) -> f32 {
        self.0
    }

    #[inline]
    pub fn is_specified(self) -> bool {
        !self.0.is_nan()
    }

    /// Absolute value (mirrors `f32::abs`).
    #[inline]
    pub fn abs(self) -> Sp {
        Sp(self.0.abs())
    }

    /// Minimum of two `Sp`s (mirrors `f32::min`).
    #[inline]
    pub fn min(self, other: Sp) -> Sp {
        Sp(self.0.min(other.0))
    }

    /// Maximum of two `Sp`s (mirrors `f32::max`).
    #[inline]
    pub fn max(self, other: Sp) -> Sp {
        Sp(self.0.max(other.0))
    }

    /// Pixels with ambient `Density * UiScale * TextScale`.
    #[inline]
    pub fn to_px(self) -> Px {
        Px(self.0
            * crate::locals::effective_density_scale()
            * crate::locals::text_scale().0.max(0.0))
    }

    /// Compose `TextUnit.toDp`: `value * fontScale`.
    #[inline]
    pub fn to_dp(self) -> Dp {
        Dp(self.0 * crate::locals::text_scale().0.max(0.0))
    }

    pub fn take_or_else(self, block: impl FnOnce() -> Sp) -> Sp {
        if self.is_specified() { self } else { block() }
    }
}

impl fmt::Debug for Sp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_specified() {
            write!(f, "{}sp", self.0)
        } else {
            write!(f, "Sp.Unspecified")
        }
    }
}

impl fmt::Display for Sp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_specified() {
            write!(f, "{}sp", self.0)
        } else {
            write!(f, "Sp.Unspecified")
        }
    }
}

impl std::hash::Hash for Sp {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        hash_f32_bits(self.0).hash(state);
    }
}

impl Add for Sp {
    type Output = Sp;
    #[inline]
    fn add(self, other: Sp) -> Sp {
        Sp(self.0 + other.0)
    }
}

impl Sub for Sp {
    type Output = Sp;
    #[inline]
    fn sub(self, other: Sp) -> Sp {
        Sp(self.0 - other.0)
    }
}

impl Neg for Sp {
    type Output = Sp;
    #[inline]
    fn neg(self) -> Sp {
        Sp(-self.0)
    }
}

impl Mul<f32> for Sp {
    type Output = Sp;
    #[inline]
    fn mul(self, other: f32) -> Sp {
        Sp(self.0 * other)
    }
}

impl Div<f32> for Sp {
    type Output = Sp;
    #[inline]
    fn div(self, other: f32) -> Sp {
        Sp(self.0 / other)
    }
}

impl PartialOrd for Sp {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

#[inline]
pub fn lerp_sp(start: Sp, stop: Sp, fraction: f32) -> Sp {
    Sp(start.0 + (stop.0 - start.0) * fraction)
}

/// Compose-style unit constructors: `16.0.dp()`, `10.px()`, `14.0.sp()`.
pub trait UnitExt {
    fn dp(self) -> Dp;
    fn px(self) -> Px;
    fn sp(self) -> Sp;
}

impl UnitExt for f32 {
    #[inline]
    fn dp(self) -> Dp {
        Dp(self)
    }
    #[inline]
    fn px(self) -> Px {
        Px(self)
    }
    #[inline]
    fn sp(self) -> Sp {
        Sp(self)
    }
}

impl UnitExt for f64 {
    #[inline]
    fn dp(self) -> Dp {
        Dp(self as f32)
    }
    #[inline]
    fn px(self) -> Px {
        Px(self as f32)
    }
    #[inline]
    fn sp(self) -> Sp {
        Sp(self as f32)
    }
}

impl UnitExt for i32 {
    #[inline]
    fn dp(self) -> Dp {
        Dp(self as f32)
    }
    #[inline]
    fn px(self) -> Px {
        Px(self as f32)
    }
    #[inline]
    fn sp(self) -> Sp {
        Sp(self as f32)
    }
}

impl UnitExt for u32 {
    #[inline]
    fn dp(self) -> Dp {
        Dp(self as f32)
    }
    #[inline]
    fn px(self) -> Px {
        Px(self as f32)
    }
    #[inline]
    fn sp(self) -> Sp {
        Sp(self as f32)
    }
}

/// Dp-space 2D offset (Compose `DpOffset`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DpOffset {
    pub x: Dp,
    pub y: Dp,
}

impl DpOffset {
    pub const ZERO: DpOffset = DpOffset {
        x: Dp::ZERO,
        y: Dp::ZERO,
    };
    pub const UNSPECIFIED: DpOffset = DpOffset {
        x: Dp::UNSPECIFIED,
        y: Dp::UNSPECIFIED,
    };

    #[inline]
    pub fn new(x: Dp, y: Dp) -> Self {
        Self { x, y }
    }

    #[inline]
    pub fn is_specified(self) -> bool {
        self.x.is_specified() && self.y.is_specified()
    }

    /// To px-space offset with the ambient scale.
    #[inline]
    pub fn to_px(self) -> Vec2 {
        Vec2 {
            x: self.x.to_px().0,
            y: self.y.to_px().0,
        }
    }
}

impl Add for DpOffset {
    type Output = DpOffset;
    #[inline]
    fn add(self, other: DpOffset) -> DpOffset {
        DpOffset::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for DpOffset {
    type Output = DpOffset;
    #[inline]
    fn sub(self, other: DpOffset) -> DpOffset {
        DpOffset::new(self.x - other.x, self.y - other.y)
    }
}

/// Dp-space size (Compose `DpSize`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DpSize {
    pub width: Dp,
    pub height: Dp,
}

impl DpSize {
    pub const ZERO: DpSize = DpSize {
        width: Dp::ZERO,
        height: Dp::ZERO,
    };

    #[inline]
    pub fn new(width: Dp, height: Dp) -> Self {
        Self { width, height }
    }

    /// To px-space size with the ambient scale (Compose `DpSize.toSize`).
    #[inline]
    pub fn to_px(self) -> Size {
        Size {
            width: self.width.to_px().0,
            height: self.height.to_px().0,
        }
    }
}

/// Dp-space bounds (Compose `DpRect`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DpRect {
    pub left: Dp,
    pub top: Dp,
    pub right: Dp,
    pub bottom: Dp,
}

impl DpRect {
    #[inline]
    pub fn new(left: Dp, top: Dp, right: Dp, bottom: Dp) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    #[inline]
    pub fn width(self) -> Dp {
        self.right - self.left
    }

    #[inline]
    pub fn height(self) -> Dp {
        self.bottom - self.top
    }

    /// To px-space rect with the ambient scale (Compose `DpRect.toRect`).
    #[inline]
    pub fn to_px(self) -> Rect {
        Rect {
            x: self.left.to_px().0,
            y: self.top.to_px().0,
            w: (self.right - self.left).to_px().0,
            h: (self.bottom - self.top).to_px().0,
        }
    }
}

/// Px → dp-geometry helpers (Compose `Size.toDpSize`).
#[inline]
pub fn size_px_to_dp(size: Size) -> DpSize {
    DpSize::new(Px(size.width).to_dp(), Px(size.height).to_dp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dp_arithmetic_matches_compose() {
        assert_eq!(Dp(2.0) + Dp(3.0), Dp(5.0));
        assert_eq!(Dp(5.0) - Dp(3.0), Dp(2.0));
        assert_eq!(-Dp(2.0), Dp(-2.0));
        assert_eq!(Dp(2.0) * 3.0, Dp(6.0));
        assert_eq!(Dp(6.0) / 3.0, Dp(2.0));
        assert_eq!(Dp(6.0) / Dp(3.0), 2.0);
    }

    #[test]
    fn dp_unspecified_semantics() {
        assert!(!Dp::UNSPECIFIED.is_specified());
        assert!(Dp(1.0).is_specified());
        assert_eq!(Dp::UNSPECIFIED.take_or_else(|| Dp(4.0)), Dp(4.0));
        // Compose: unspecified compares Equal but never equals.
        assert!(Dp::UNSPECIFIED != Dp::UNSPECIFIED);
        assert_eq!(
            Dp::UNSPECIFIED.partial_cmp(&Dp(1.0)),
            Some(std::cmp::Ordering::Equal)
        );
    }

    #[test]
    fn unit_ext_constructors() {
        assert_eq!(16.0f32.dp(), Dp(16.0));
        assert_eq!(10i32.dp(), Dp(10.0));
        assert_eq!(14.0f32.sp(), Sp(14.0));
        assert_eq!(2.0f32.px(), Px(2.0));
    }

    #[test]
    fn dp_rect_dimensions() {
        let r = DpRect::new(Dp(0.0), Dp(0.0), Dp(10.0), Dp(20.0));
        assert_eq!(r.width(), Dp(10.0));
        assert_eq!(r.height(), Dp(20.0));
    }

    #[test]
    fn lerp_midpoint() {
        assert_eq!(lerp_dp(Dp(0.0), Dp(10.0), 0.5), Dp(5.0));
        assert_eq!(lerp_sp(Sp(0.0), Sp(10.0), 0.5), Sp(10.0 * 0.5));
    }
}
