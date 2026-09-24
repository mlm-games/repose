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
    @location(3) @interpolate(flat) edge_mode: u32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) blur_uv: vec2<f32>,
    @location(4) fwd_mat: vec4<f32>,
    @location(5) edge_mode: u32,
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
    let rotated = vec2(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
    out.color = color;
    out.blur_uv = blur_uv;
    out.edge_mode = edge_mode;
    return out;
}

@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var src_smp: sampler;

fn weighted_tap(uv: vec2<f32>, weight: f32, edge_mode: u32) -> vec4<f32> {
    if (edge_mode != 0u && (any(uv < vec2<f32>(0.0)) || any(uv > vec2<f32>(1.0)))) {
        return vec4<f32>(0.0);
    }
    let sample = textureSample(src_tex, src_smp, clamp(uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    return vec4<f32>(sample.rgb * weight, weight);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let bu = in.blur_uv;
    var blurred = vec4<f32>(0.0);
    var uv = in.uv + vec2<f32>(-bu.x, -bu.y);
    blurred += weighted_tap(uv, 1.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(0.0, -bu.y);
    blurred += weighted_tap(uv, 2.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(bu.x, -bu.y);
    blurred += weighted_tap(uv, 1.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(-bu.x, 0.0);
    blurred += weighted_tap(uv, 2.0 / 16.0, in.edge_mode);
    blurred += weighted_tap(in.uv, 4.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(bu.x, 0.0);
    blurred += weighted_tap(uv, 2.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(-bu.x, bu.y);
    blurred += weighted_tap(uv, 1.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(0.0, bu.y);
    blurred += weighted_tap(uv, 2.0 / 16.0, in.edge_mode);
    uv = in.uv + vec2<f32>(bu.x, bu.y);
    blurred += weighted_tap(uv, 1.0 / 16.0, in.edge_mode);
    let total_weight = max(blurred.a, 1.0 / 16.0);
    return vec4<f32>(
        blurred.rgb / total_weight * in.color.a,
        blurred.a / total_weight * in.color.a,
    );
}
