struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) blur_uv: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) blur_uv: vec2<f32>,
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
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
    out.color = color;
    out.blur_uv = blur_uv;
    return out;
}

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var src_smp: sampler;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let bu = in.blur_uv;
    let c0 = textureSample(src_tex, src_smp, in.uv + vec2(-bu.x, -bu.y)) * (1.0 / 16.0);
    let c1 = textureSample(src_tex, src_smp, in.uv + vec2( 0.0,   -bu.y)) * (2.0 / 16.0);
    let c2 = textureSample(src_tex, src_smp, in.uv + vec2( bu.x,  -bu.y)) * (1.0 / 16.0);
    let c3 = textureSample(src_tex, src_smp, in.uv + vec2(-bu.x,  0.0  )) * (2.0 / 16.0);
    let c4 = textureSample(src_tex, src_smp, in.uv                       ) * (4.0 / 16.0);
    let c5 = textureSample(src_tex, src_smp, in.uv + vec2( bu.x,  0.0  )) * (2.0 / 16.0);
    let c6 = textureSample(src_tex, src_smp, in.uv + vec2(-bu.x,  bu.y )) * (1.0 / 16.0);
    let c7 = textureSample(src_tex, src_smp, in.uv + vec2( 0.0,   bu.y )) * (2.0 / 16.0);
    let c8 = textureSample(src_tex, src_smp, in.uv + vec2( bu.x,  bu.y )) * (1.0 / 16.0);
    let blurred = c0 + c1 + c2 + c3 + c4 + c5 + c6 + c7 + c8;
    return vec4(blurred.rgb * in.color.a, blurred.a * in.color.a);
}
