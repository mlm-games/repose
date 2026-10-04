struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) sigma_px: vec2<f32>,
    @location(3) @interpolate(flat) axis: u32,
    @location(4) @interpolate(flat) edge_mode: u32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) sigma_px: vec2<f32>,
    @location(4) fwd_mat: vec4<f32>,
    @location(5) axis: u32,
    @location(6) edge_mode: u32,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0)
    );
    let p = positions[v];
    let uv_lerp = uvs[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2<f32>(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4<f32>(pos_ndc, 0.0, 1.0);
    out.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
    out.color = color;
    out.sigma_px = sigma_px;
    out.axis = axis;
    out.edge_mode = edge_mode;
    return out;
}

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var src_smp: sampler;

// Bilinear pairs, so 2 texels of kernel per fetch. Caps the kernel at
// 2 * MAX_TAPS - 1 texels, i.e. sigma ~= 43px (blur radius ~= 74px); past
// that the Gaussian is truncated rather than allowed to grow unbounded.
const MAX_TAPS: i32 = 65;

fn fetch(uv: vec2<f32>, edge_mode: u32) -> vec4<f32> {
    if (edge_mode != 0u && (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)))) {
        return vec4<f32>(0.0);
    }
    return textureSampleLevel(
        src_tex, src_smp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)), 0.0
    );
}

/// Separable Gaussian along `axis`. `sigma_px` is in source texels, so the
/// kernel support is `ceil(3 * sigma)` texels and tap offsets convert to uv by
/// dividing by the source extent. The kernel is packed into bilinear pairs:
/// pair `i` sits half a texel off `-support + 2i`, so one filtered fetch
/// reconstructs two adjacent texels, plus one exact tap at the far edge.
fn blur_axis(in: VSOut) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(src_tex, 0));
    let along_y = in.axis == 1u;
    let dir = select(vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), along_y);
    let extent = select(dims.x, dims.y, along_y);
    let sigma = select(in.sigma_px.x, in.sigma_px.y, along_y);

    if (sigma < 0.25 || extent <= 0.0) {
        return fetch(in.uv, in.edge_mode);
    }

    let support = min(i32(ceil(3.0 * sigma)), 2 * MAX_TAPS - 1);
    let inv_sigma = 1.0 / sigma;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = 0; i < support; i++) {
        let dist = f32(-support) + 0.5 + f32(2 * i);
        let w = exp(-0.5 * dist * dist * inv_sigma * inv_sigma);
        sum += fetch(in.uv + dir * (dist / extent), in.edge_mode) * w;
        total += w;
    }
    let tail = f32(support);
    let w_tail = exp(-0.5 * tail * tail * inv_sigma * inv_sigma);
    sum += fetch(in.uv + dir * (tail / extent), in.edge_mode) * w_tail;
    total += w_tail;
    return sum / max(total, 1e-6);
}

@fragment
fn fs_alpha(in: VSOut) -> @location(0) vec4<f32> {
    let alpha = blur_axis(in).a * in.color.a;
    return vec4<f32>(in.color.rgb * alpha, alpha);
}

@fragment
fn fs_color(in: VSOut) -> @location(0) vec4<f32> {
    let blurred = blur_axis(in);
    return vec4<f32>(blurred.rgb * in.color.a, blurred.a * in.color.a);
}
