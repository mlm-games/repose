// CPU-computed Y′CbCr -> R′G′B′ affine transform.
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

fn catmull_rom_weights(t: f32) -> vec4<f32> {
    let t2 = t * t;
    let t3 = t2 * t;
    return vec4(
        -0.5 * t3 + t2 - 0.5 * t,
         1.5 * t3 - 2.5 * t2 + 1.0,
        -1.5 * t3 + 2.0 * t2 + 0.5 * t,
         0.5 * t3 - 0.5 * t2,
    );
}

fn sample_bicubic_uv(uv: vec2<f32>) -> vec2<f32> {
    let uv_size = vec2<f32>(textureDimensions(tex_uv));
    let coord = uv * uv_size - 0.5;
    let px = floor(coord);
    let f = coord - px;

    let wx = catmull_rom_weights(f.x);
    let wy = catmull_rom_weights(f.y);

    var result = vec4(0.0);
    for (var row = 0; row < 4; row++) {
        let wy_r = wy[row];
        for (var col = 0; col < 4; col++) {
            let p = vec2(f32(col) - 1.0, f32(row) - 1.0);
            let tap_uv = (px + p + 0.5) / uv_size;
            let s = textureSampleLevel(tex_uv, samp, tap_uv, 0.0);
            result += s * wx[col] * wy_r;
        }
    }
    return result.rg;
}

fn srgb_eotf_vec3(c: vec3<f32>) -> vec3<f32> {
    return vec3(srgb_eotf(c.r), srgb_eotf(c.g), srgb_eotf(c.b));
}

@fragment
fn fs_main(i: VSOut) -> @location(0) vec4<f32> {
    let uv_off = vec2(i.uv_x_offset, 0.0);
    let y = textureSample(tex_y, samp, i.uv).r;
    let uv = sample_bicubic_uv(i.uv + uv_off);
    let rgb_gamma = clamp(apply_yuv(yuv_transform, vec3(y, uv.r, uv.g)), vec3(0.0), vec3(1.0));
    let rgb_lin = srgb_eotf_vec3(rgb_gamma) * i.tint.rgb;
    let a = i.tint.a;
    return vec4(rgb_lin * a, a);
}
