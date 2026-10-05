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
    @location(5) @interpolate(flat) shape: vec4<f32>,
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
    @location(7) shape: vec4<f32>,
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
    out.shape = shape;
    return out;
}

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var src_smp: sampler;

// Kernel half-width cap in texels. Packed pair mode spans 2 * MAX_SUPPORT
// texels, i.e. sigma ~= 21px (blur radius ~= 37px); past that the Gaussian is
// truncated rather than allowed to grow unbounded.
const MAX_SUPPORT: i32 = 64;

/// Rounded-rect coverage for a shadow silhouette, evaluated in source texel
/// space (`uv * dims`, origin top-left, y down). `r` holds the four corner
/// radii as `[top-left, top-right, bottom-right, bottom-left]`, the same order
/// `corner_radius` in rect.wgsl uses. All zero disables the mask.
///
/// Only a corner arc ever cuts. The box edge is the layer's own edge, which
/// the layer alpha already defines, so ramping there would fade content the
/// source has already accounted for — and would make an unrounded corner
/// depend on this mask at all. The one-texel edge is fixed rather than
/// `fwidth` so it stays well-defined inside the blur's loop; the convolution
/// smooths it further regardless.
fn shape_mask(uv: vec2<f32>, dims: vec2<f32>, r: vec4<f32>) -> f32 {
    if (all(r <= vec4<f32>(0.0))) {
        return 1.0;
    }
    let c = uv * dims - 0.5 * dims;
    let top = c.y < 0.0;
    let left = c.x < 0.0;
    // `abs(c)` mirrors into the selected corner, so each corner only ever
    // sees its own radius.
    let rad = max(select(select(r.z, r.w, left), select(r.y, r.x, left), top), 0.0);
    if (rad <= 0.0) {
        return 1.0;
    }
    let inner = 0.5 * dims - vec2<f32>(rad, rad);
    if (any(abs(c) <= inner)) {
        return 1.0;
    }
    let d = length(abs(c) - inner) - rad;
    return smoothstep(0.5, -0.5, d);
}

fn fetch(
    uv: vec2<f32>,
    edge_mode: u32,
    dims: vec2<f32>,
    shape: vec4<f32>,
) -> vec4<f32> {
    if (edge_mode != 0u && (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)))) {
        return vec4<f32>(0.0);
    }
    let cuv = clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0));
    let s = textureSampleLevel(src_tex, src_smp, cuv, 0.0);
    return s * shape_mask(cuv, dims, shape);
}

/// Separable Gaussian along `axis`. `sigma_px` is in source texels, so the
/// kernel support is `ceil(3 * sigma)` texels and tap offsets convert to uv by
/// dividing by the source extent. Wide kernels are packed into bilinear pairs
/// (two texels of kernel per fetch); narrow ones tap texel centres directly,
/// since a pair is wider than the kernel itself and would alias.
fn blur_axis(in: VSOut) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(src_tex, 0));
    let along_y = in.axis == 1u;
    let dir = select(vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0), along_y);
    let extent = select(dims.x, dims.y, along_y);
    let sigma = select(in.sigma_px.x, in.sigma_px.y, along_y);

    if (sigma < 0.25 || extent <= 0.0) {
        return fetch(in.uv, in.edge_mode, dims, in.shape);
    }

    let support = min(i32(ceil(3.0 * sigma)), MAX_SUPPORT);
    let inv_sigma = 1.0 / sigma;
    var sum = vec4<f32>(0.0);
    var total = 0.0;
    if (sigma >= 1.5) {
        // Pair `i` sits half a texel off `-support + 2i`, so one filtered
        // fetch reconstructs two adjacent texels, plus one exact tap at the
        // far edge.
        for (var i = 0; i <= support; i++) {
            let dist = f32(-support) + 0.5 + f32(2 * i);
            let w = exp(-0.5 * dist * dist * inv_sigma * inv_sigma);
            sum += fetch(in.uv + dir * (dist / extent), in.edge_mode, dims, in.shape) * w;
            total += w;
        }
    } else {
        for (var i = -support; i <= support; i++) {
            let dist = f32(i);
            let w = exp(-0.5 * dist * dist * inv_sigma * inv_sigma);
            sum += fetch(in.uv + dir * (dist / extent), in.edge_mode, dims, in.shape) * w;
            total += w;
        }
    }
    // Normalize by the weights actually fetched, so a clamp-edge source whose
    // taps run off the texture keeps a solid interior instead of darkening.
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
