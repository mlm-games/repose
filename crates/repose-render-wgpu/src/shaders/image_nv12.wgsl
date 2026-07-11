// CPU-computed Y′CbCr → R′G′B′ affine transform.
// Folds range expansion + matrix coefficients into a single M/b.
struct YuvTransform {
    row0: vec4<f32>, // m[0][0], m[0][1], m[0][2], 0
    row1: vec4<f32>, // m[1][0], m[1][1], m[1][2], 0
    row2: vec4<f32>, // m[2][0], m[2][1], m[2][2], 0
    b:    vec4<f32>, // b[0], b[1], b[2], 0
}

@group(1) @binding(0) var tex_y:  texture_2d<f32>;
@group(1) @binding(1) var tex_uv: texture_2d<f32>;
@group(1) @binding(2) var samp:   sampler;
@group(1) @binding(3) var<uniform> yuv_transform: YuvTransform;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) tint: vec4<f32>,
    @location(2) uv_x_offset: f32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) tint: vec4<f32>,
    @location(3) uv_x_offset: f32,
    @location(4) sin_cos: vec2<f32>,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    var uvs = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let uv_lerp = uvs[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2(corner.x * sin_cos.x - corner.y * sin_cos.y, corner.x * sin_cos.y + corner.y * sin_cos.x);
    var o: VSOut;
    o.pos = vec4(xywh.xy + rotated, 0.0, 1.0);
    o.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
    o.tint = tint;
    o.uv_x_offset = uv_x_offset;
    return o;
}

fn apply_yuv(t: YuvTransform, yuv: vec3<f32>) -> vec3<f32> {
    return vec3(
        dot(t.row0.xyz, yuv) + t.b.x,
        dot(t.row1.xyz, yuv) + t.b.y,
        dot(t.row2.xyz, yuv) + t.b.z,
    );
}

fn srgb_eotf(c: f32) -> f32 {
    if c <= 0.04045 {
        return c / 12.92;
    }
    return pow((c + 0.055) / 1.055, 2.4);
}

@fragment
fn fs_main(i: VSOut) -> @location(0) vec4<f32> {
    let uv_off = vec2(i.uv_x_offset, 0.0);
    let y = textureSample(tex_y, samp, i.uv).r;
    let uv = textureSample(tex_uv, samp, i.uv + uv_off).rg;
    let rgb_gamma = clamp(apply_yuv(yuv_transform, vec3(y, uv.r, uv.g)), vec3(0.0), vec3(1.0));

    let r_lin = srgb_eotf(rgb_gamma.r);
    let g_lin = srgb_eotf(rgb_gamma.g);
    let b_lin = srgb_eotf(rgb_gamma.b);

    let a = i.tint.a;
    let out_rgb = vec3(r_lin, g_lin, b_lin) * i.tint.rgb * a;
    return vec4(out_rgb, a);
}
