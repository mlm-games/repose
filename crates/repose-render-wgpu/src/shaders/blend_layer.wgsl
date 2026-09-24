// `mode` values mirror `repose_core::BlendMode::shader_mode`: 4 = Overlay,
// 7 = ColorDodge, 8 = ColorBurn, 9 = HardLight, 10 = SoftLight,
// 11 = Difference, 12 = Exclusion, 13 = Hue, 14 = Saturation, 15 = Color,
// 16 = Luminosity.
struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) uv_rect: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) fwd_mat: vec4<f32>,
    @location(3) @interpolate(flat) mode: u32,
    @location(4) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) mode: u32,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );

    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y,
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.uv_rect = uv_rect;
    out.xywh = xywh;
    out.fwd_mat = fwd_mat;
    out.mode = mode;
    out.pos_ndc = pos_ndc;
    return out;
}

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var src_samp: sampler;
@group(2) @binding(0) var dst_tex: texture_2d<f32>;
@group(2) @binding(1) var dst_samp: sampler;

fn unpremult(c: vec4<f32>) -> vec4<f32> {
    if (c.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(c.rgb / c.a, c.a);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.30, 0.59, 0.11));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var r = c;
    if (n < 0.0) {
        r = l + (((r - l) * l) / (l - n));
    }
    if (x > 1.0) {
        r = l + (((r - l) * (1.0 - l)) / (x - l));
    }
    return r;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let d = l - lum(c);
    return clip_color(c + vec3(d));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat_inner(c: vec3<f32>, s: f32) -> vec3<f32> {
    var r = c;
    let cmax = max(r.r, max(r.g, r.b));
    let cmin = min(r.r, min(r.g, r.b));
    // All channels equal: hue is undefined; spread from the max channel.
    if (cmax <= cmin) {
        if (cmax >= 0.5) {
            r = vec3(cmax, cmax - s, cmax - s);
        } else {
            r = vec3(cmax + s, cmax, cmax);
        }
        return r;
    }
    // Scale each channel around the minimum so max - min == s.
    let scale = s / (cmax - cmin);
    r = (r - vec3(cmin)) * scale;
    // Re-anchor: the old minimum maps to 0, shift so the old maximum
    // keeps its value and the result stays in gamut via clip_color.
    r = r + vec3(cmax - max(r.r, max(r.g, r.b)));
    return clip_color(r);
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    return set_sat_inner(c, clamp(s, 0.0, 1.0));
}

fn overlay_channel(s: f32, d: f32) -> f32 {
    if (d <= 0.5) {
        return 2.0 * s * d;
    }
    return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
}

fn blend_channel(mode: u32, s: f32, d: f32) -> f32 {
    switch (mode) {
        case 1u: { // Add
            return s + d;
        }
        case 2u: { // Multiply
            return s * d;
        }
        case 3u: { // Screen
            return s + d - s * d;
        }
        case 4u: { // Overlay
            return overlay_channel(s, d);
        }
        case 5u: { // Darken
            return min(s, d);
        }
        case 6u: { // Lighten
            return max(s, d);
        }
        case 7u: { // ColorDodge
            if (d <= 0.0) {
                return 0.0;
            }
            if (s >= 1.0) {
                return 1.0;
            }
            return clamp(d / (1.0 - s), 0.0, 1.0);
        }
        case 8u: { // ColorBurn
            if (d >= 1.0) {
                return 1.0;
            }
            if (s <= 0.0) {
                return 0.0;
            }
            return clamp(1.0 - (1.0 - d) / s, 0.0, 1.0);
        }
        case 9u: { // HardLight (overlay with swapped inputs)
            return overlay_channel(d, s);
        }
        case 10u: { // SoftLight (Pegtop approximation)
            if (s <= 0.5) {
                return d - (1.0 - 2.0 * s) * d * (1.0 - d);
            }
            let d2 = d * d;
            var g: f32;
            if (d <= 0.25) {
                g = ((16.0 * d - 12.0) * d + 4.0) * d;
            } else {
                g = sqrt(max(d, 0.0));
            }
            return d + (2.0 * s - 1.0) * (g - d);
        }
        case 11u: { // Difference
            return abs(s - d);
        }
        case 12u: { // Exclusion
            return d + s - 2.0 * d * s;
        }
        default: {
            return s;
        }
    }
}

fn blend_separable(mode: u32, s: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    return vec3(
        blend_channel(mode, s.r, d.r),
        blend_channel(mode, s.g, d.g),
        blend_channel(mode, s.b, d.b),
    );
}

fn blend_nonseparable(mode: u32, s: vec3<f32>, d: vec3<f32>) -> vec3<f32> {
    switch (mode) {
        case 13u: { // Hue
            return set_lum(set_sat(s, sat(d)), lum(d));
        }
        case 14u: { // Saturation
            return set_lum(set_sat(d, sat(s)), lum(d));
        }
        case 15u: { // Color
            return set_lum(s, lum(d));
        }
        case 16u: { // Luminosity
            return set_lum(d, lum(s));
        }
        default: {
            return s;
        }
    }
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let half = 0.5 * in.xywh.zw;
    let rel = (in.pos_ndc - in.xywh.xy) / half;
    let det = max(
        in.fwd_mat.x * in.fwd_mat.w - in.fwd_mat.y * in.fwd_mat.z,
        1e-6,
    );
    let unrotated_rel = vec2(
        (in.fwd_mat.w * rel.x - in.fwd_mat.y * rel.y) / det,
        (-in.fwd_mat.z * rel.x + in.fwd_mat.x * rel.y) / det,
    );
    let norm_uv = (unrotated_rel + 1.0) * 0.5;
    if (any(norm_uv < vec2(0.0)) || any(norm_uv > vec2(1.0))) {
        discard;
    }
    let uv = mix(in.uv_rect.xy, in.uv_rect.zw, norm_uv);
    // The snapshot owns exactly the composite region, so the source and
    // backdrop share UV space: no screen-space mapping needed.
    let dst_uv = uv;

    let src = unpremult(textureSample(src_tex, src_samp, uv));
    let dst = unpremult(textureSample(dst_tex, dst_samp, dst_uv));

    var blended: vec3<f32>;
    if (in.mode >= 13u) {
        blended = blend_nonseparable(in.mode, src.rgb, dst.rgb);
    } else {
        blended = blend_separable(in.mode, src.rgb, dst.rgb);
    }
    let src_a = src.a;
    let dst_a = dst.a;
    let out_a = src_a + dst_a * (1.0 - src_a);
    let composited = src.rgb * (src_a * (1.0 - dst_a))
        + blended * (src_a * dst_a)
        + dst.rgb * ((1.0 - src_a) * dst_a);
    return vec4(composited, out_a);
}
