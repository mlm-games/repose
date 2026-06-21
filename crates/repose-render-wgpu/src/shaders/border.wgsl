struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) radius: f32,
    @location(3) stroke_px: f32,
    @location(4) pos_ndc: vec2<f32>,
    @location(5) sin_cos: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radius: f32,
    @location(2) stroke_px: f32,
    @location(3) color: vec4<f32>,
    @location(4) sin_cos: vec2<f32>,
    @builtin(vertex_index) v: u32
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
    out.stroke_px = stroke_px;
    out.color = color;
    out.pos_ndc = pos_ndc;
    out.sin_cos = sin_cos;
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

    let unrotated_px = vec2(
        p_px.x * in.sin_cos.x + p_px.y * in.sin_cos.y,
        -p_px.x * in.sin_cos.y + p_px.y * in.sin_cos.x
    );

    let stroke = max(in.stroke_px, 0.0);
    let r_outer = max(in.radius, 0.0);

    let d_outer = sdf_round_box_px(unrotated_px, half_px, r_outer);

    let half_inner = max(half_px - vec2<f32>(stroke, stroke), vec2<f32>(0.0, 0.0));
    let r_inner = max(r_outer - stroke, 0.0);
    let d_inner = sdf_round_box_px(unrotated_px, half_inner, r_inner);

    let w0 = max(fwidth(d_outer), 1e-4);
    let w1 = max(fwidth(d_inner), 1e-4);

    let cov_outer = 1.0 - smoothstep(-w0, w0, d_outer);
    let cov_inner = 1.0 - smoothstep(-w1, w1, d_inner);

    let ring = max(cov_outer - cov_inner, 0.0);

    let a = in.color.a * ring;
    return vec4(in.color.rgb * a, a);
}
