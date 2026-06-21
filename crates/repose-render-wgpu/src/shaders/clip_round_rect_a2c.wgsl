struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) xywh: vec4<f32>,
    @location(1) radius: f32,
    @location(2) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radius: f32,
    @location(2) sin_cos: vec2<f32>,
    @builtin(vertex_index) v: u32,
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );

    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2(corner.x * sin_cos.x - corner.y * sin_cos.y, corner.x * sin_cos.y + corner.y * sin_cos.x);
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.xywh = xywh;
    out.radius = radius;
    out.pos_ndc = pos_ndc;
    return out;
}

fn sdf_round_box_px(p_px: vec2<f32>, half_px: vec2<f32>, r_px: f32) -> f32 {
    let r = max(r_px, 0.0);
    let q = abs(p_px) - (half_px - vec2<f32>(r, r));
    let outside = max(q, vec2<f32>(0.0, 0.0));
    let inside = min(max(q.x, q.y), 0.0);
    return length(outside) + inside - r;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let center_ndc = in.xywh.xy;

    let p_px = (in.pos_ndc - center_ndc) * G.ndc_to_px;
    let half_px = 0.5 * in.xywh.zw * G.ndc_to_px;

    let d = sdf_round_box_px(p_px, half_px, in.radius);
    let w = max(fwidth(d), 1e-4);
    let alpha = 1.0 - smoothstep(-w, w, d);
    return vec4(0.0, 0.0, 0.0, alpha);
}
