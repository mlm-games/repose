use crate::Vec2;

/// Y′CbCr or RGB signal range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorRange {
    /// Studio swing: Y 16–235, Cb/Cr 16–240 (the standard for broadcast video).
    #[default]
    Limited,
    /// Full swing: 0–255 for all channels (JPEG, computer graphics).
    Full,
}

/// Matrix coefficients for luma/chroma-to-RGB conversion (H.273 Table 4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatrixCoeffs {
    /// Identity / GBR -> the "matrix" is the identity, meaning the
    /// signal *is* RGB (or GBR order with different primaries).
    Identity,
    /// BT.601 525-line (Kr=0.299, Kb=0.114).
    Bt601,
    /// BT.709 (Kr=0.2126, Kb=0.0722).
    #[default]
    Bt709,
    /// BT.2020 non-constant luminance (Kr=0.2627, Kb=0.0593).
    Bt2020Ncl,
}

/// Color primaries (H.273 Table 2 / CICP ColourPrimaries).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Primaries {
    /// BT.709 / sRGB.
    #[default]
    Bt709,
    /// BT.601 625-line (EBU Tech 3213).
    Bt601_625,
    /// BT.601 525-line (SMPTE C).
    Bt601_525,
    /// BT.2020.
    Bt2020,
    /// DCI-P3 (SMPTE ST 428-1 / Display P3).
    DciP3,
}

/// Transfer characteristics (H.273 Table 3 / CICP TransferCharacteristics).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Transfer {
    /// BT.709 / BT.601 (gamma ≈ 2.2, same curve as BT.1886).
    Bt709,
    /// sRGB (IEC 61966-2-1) -> piecewise 2.4 gamma with linear toe.
    #[default]
    Srgb,
    /// SMPTE ST 2084 (Perceptual Quantizer) for HDR10.
    Pq,
    /// ITU-R BT.2100 Hybrid Log-Gamma for HDR broadcast.
    Hlg,
    /// Linear light (no gamma).
    Linear,
}

/// Chroma sample siting (horizontal and vertical alignment).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChromaSiting {
    /// "Left" / MPEG-2 / H.264 / AV1 default: chroma samples aligned
    /// with the left edge of luma samples.
    #[default]
    Left,
    /// "Center" / MPEG-4: chroma samples aligned with luma sample centers.
    Center,
    /// Top-left: chroma aligned with top-left corner (rare, some JPEG).
    TopLeft,
}

/// Describes the memory layout of a planar (or packed) video frame.
///
/// Maps 1:1 to common video pixel formats. Each variant implies the
/// number of planes, their texture formats, and subsampling factors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit YUV 4:2:0, 2 planes: Y (R8, full res), interleaved UV (Rg8, half res).
    #[default]
    Nv12,
    /// 8-bit YUV 4:2:0, 3 planes: Y (R8), U (R8), V (R8), all half res.
    I420,
    /// 8-bit YUV 4:4:4, 3 planes: Y (R8), U (R8), V (R8), full res.
    I444,
    /// 10-bit YUV 4:2:0, 2 planes: Y (R16Unorm, high bits), UV (Rg16Unorm, high bits), half res.
    P010,
    /// Packed 8-bit RGBA (single Rgba8Unorm plane, full res).
    Rgba,
}

impl PixelFormat {
    pub fn num_planes(&self) -> u32 {
        match self {
            PixelFormat::Nv12 | PixelFormat::P010 => 2,
            PixelFormat::I420 | PixelFormat::I444 => 3,
            PixelFormat::Rgba => 1,
        }
    }

    /// Chroma width/height for a given luma resolution.
    /// Returns `(0, 0)` for RGB-like formats.
    pub fn chroma_size(&self, luma_w: u32, luma_h: u32) -> (u32, u32) {
        match self {
            PixelFormat::Nv12 | PixelFormat::I420 | PixelFormat::P010 => {
                (luma_w.div_ceil(2), luma_h.div_ceil(2))
            }
            PixelFormat::I444 => (luma_w, luma_h),
            PixelFormat::Rgba => (0, 0),
        }
    }
}

/// Minimum viable color metadata for a video frame or stream.
///
/// Maps 1:1 to the values carried in H.264 VUI
/// (`matrix_coefficients`, `transfer_characteristics`, `colour_primaries`,
/// `video_full_range_flag`) and the AV1 sequence header `color_config`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorInfo {
    pub range: ColorRange,
    pub matrix: MatrixCoeffs,
    pub primaries: Primaries,
    pub transfer: Transfer,
    pub chroma_siting: ChromaSiting,
}

impl Default for ColorInfo {
    fn default() -> Self {
        Self {
            range: ColorRange::Limited,
            matrix: MatrixCoeffs::Bt709,
            primaries: Primaries::Bt709,
            transfer: Transfer::Bt709,
            chroma_siting: ChromaSiting::Left,
        }
    }
}

/// Pre-computed affine transform from Y′CbCr (raw texel values 0..1) to
/// gamma-encoded R′G′B′ (0..1).
///
/// Folded in:
/// - Range expansion (16/219, 128/224 offsets for limited range)
/// - Matrix coefficients (601 / 709 / 2020 / Identity)
///
/// The shader applies: `rgb = M * yuv + b`, then clamps and applies the
/// sRGB EOTF before writing to the srgb render target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct YuvTransform {
    /// 3×3 matrix applied to (Y, U, V).
    pub m: [[f32; 3]; 3],
    /// 3-element bias (added after the matrix multiply).
    pub b: [f32; 3],
}

impl ColorInfo {
    /// Compute the single affine transform that the fragment shader applies.
    ///
    /// The shader samples raw texel values `y`, `u`, `v` ∈ [0..1] and does:
    /// ```ignore
    /// let rgb = clamp(transform.m * vec3(y, u, v) + transform.b, 0, 1);
    /// ```
    pub fn to_yuv_transform(&self) -> YuvTransform {
        let (y_scale, y_off, uv_scale, uv_off) = match self.range {
            ColorRange::Limited => (255.0 / 219.0, 16.0 / 255.0, 255.0 / 224.0, 128.0 / 255.0),
            ColorRange::Full => (1.0, 0.0, 1.0, 0.5),
        };

        if self.matrix == MatrixCoeffs::Identity {
            return YuvTransform {
                m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
                b: [0.0; 3],
            };
        }

        let (kr, kb) = match self.matrix {
            MatrixCoeffs::Bt601 => (0.299_f32, 0.114_f32),
            MatrixCoeffs::Bt709 => (0.2126_f32, 0.0722_f32),
            MatrixCoeffs::Bt2020Ncl => (0.2627_f32, 0.0593_f32),
            _ => unreachable!(), // Identity already handled above.
        };
        let kg = 1.0 - kr - kb;

        // G′ coefficient for Cb and Cr
        let cb_factor = 2.0 * kb * (1.0 - kb) / (1.0 - kg);
        let cr_factor = 2.0 * kr * (1.0 - kr) / (1.0 - kg);

        // Combined matrix: [M] * [range_expansion]
        let m00 = y_scale;
        let m01 = 0.0;
        let m02 = 2.0 * (1.0 - kr) * uv_scale;

        let m10 = y_scale;
        let m11 = -cb_factor * uv_scale;
        let m12 = -cr_factor * uv_scale;

        let m20 = y_scale;
        let m21 = 2.0 * (1.0 - kb) * uv_scale;
        let m22 = 0.0;

        // Combined bias: [M] * [-range_offset]
        let b0 = -y_scale * y_off - 2.0 * (1.0 - kr) * uv_scale * uv_off;
        let b1 = -y_scale * y_off + cb_factor * uv_scale * uv_off + cr_factor * uv_scale * uv_off;
        let b2 = -y_scale * y_off - 2.0 * (1.0 - kb) * uv_scale * uv_off;

        YuvTransform {
            m: [[m00, m01, m02], [m10, m11, m12], [m20, m21, m22]],
            b: [b0, b1, b2],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color(pub u8, pub u8, pub u8, pub u8);

impl Color {
    pub const TRANSPARENT: Color = Color(0, 0, 0, 0);
    pub const BLACK: Color = Color(0, 0, 0, 255);
    pub const WHITE: Color = Color(255, 255, 255, 255);

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Color(r, g, b, 255)
    }
    pub fn from_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Color(r, g, b, a)
    }
    pub fn from_hex(hex: &str) -> Self {
        let s = hex.trim_start_matches('#');
        let (r, g, b, a) = match s.len() {
            6 => (
                u8::from_str_radix(&s[0..2], 16).unwrap_or(0),
                u8::from_str_radix(&s[2..4], 16).unwrap_or(0),
                u8::from_str_radix(&s[4..6], 16).unwrap_or(0),
                255,
            ),
            8 => (
                u8::from_str_radix(&s[0..2], 16).unwrap_or(0),
                u8::from_str_radix(&s[2..4], 16).unwrap_or(0),
                u8::from_str_radix(&s[4..6], 16).unwrap_or(0),
                u8::from_str_radix(&s[6..8], 16).unwrap_or(255),
            ),
            _ => (0, 0, 0, 255),
        };
        Color(r, g, b, a)
    }
    pub fn with_alpha(self, a: u8) -> Self {
        Color(self.0, self.1, self.2, a)
    }

    pub fn with_alpha_f32(self, a: f32) -> Self {
        Color(
            self.0,
            self.1,
            self.2,
            (a * 255.0).round().clamp(0.0, 255.0) as u8,
        )
    }

    /// Composite `self` (as a foreground with alpha) over a `background` color.
    /// Result has same alpha as background (treats background as opaque).
    pub fn composite_over(&self, background: Color) -> Color {
        let a = self.3 as f32 / 255.0;
        let inv_a = 1.0 - a;
        Color(
            (self.0 as f32 * a + background.0 as f32 * inv_a)
                .round()
                .clamp(0.0, 255.0) as u8,
            (self.1 as f32 * a + background.1 as f32 * inv_a)
                .round()
                .clamp(0.0, 255.0) as u8,
            (self.2 as f32 * a + background.2 as f32 * inv_a)
                .round()
                .clamp(0.0, 255.0) as u8,
            background.3,
        )
    }

    pub fn to_linear(self) -> [f32; 4] {
        fn srgb_to_linear(c: f32) -> f32 {
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        let r = srgb_to_linear(self.0 as f32 / 255.0);
        let g = srgb_to_linear(self.1 as f32 / 255.0);
        let b = srgb_to_linear(self.2 as f32 / 255.0);
        let a = self.3 as f32 / 255.0;
        [r, g, b, a]
    }
}

/// Brush for filling shapes.
///
/// This can be a solid color or a gradient. Higher‑level APIs (Modifier,
/// widgets) should talk in terms of `Brush` rather than raw `Color` so that
/// gradients and future brush types (radial, image) can share the same path.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Brush {
    /// Solid color fill
    Solid(Color),

    /// Linear gradient from `start` to `end` in local coordinates.
    ///
    /// The gradient is defined in the local space of the node being drawn
    /// (e.g. Rect's top‑left is (0,0), bottom‑right is (w,h)).
    Linear {
        start: Vec2,
        end: Vec2,
        start_color: Color,
        end_color: Color,
    },
    // Later can add Radial, Image, etc...
}

impl From<Color> for Brush {
    fn from(c: Color) -> Self {
        Brush::Solid(c)
    }
}

pub struct LinearGradient {
    pub start: Vec2,
    pub end: Vec2,
    pub start_color: Color,
    pub end_color: Color,
}

impl LinearGradient {
    pub fn vertical(top: Color, bottom: Color) -> Brush {
        Brush::Linear {
            start: Vec2 { x: 0.0, y: 0.0 },
            end: Vec2 { x: 0.0, y: 1.0 }, // normalized; interpreted in rect size
            start_color: top,
            end_color: bottom,
        }
    }

    pub fn horizontal(left: Color, right: Color) -> Brush {
        Brush::Linear {
            start: Vec2 { x: 0.0, y: 0.0 },
            end: Vec2 { x: 1.0, y: 0.0 },
            start_color: left,
            end_color: right,
        }
    }
}
