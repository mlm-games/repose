struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) pos_ndc: vec2<f32>,
    @location(3) sin_cos: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) sin_cos: vec2<f32>,
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
    out.color = color;
    out.pos_ndc = pos_ndc;
    out.sin_cos = sin_cos;
    return out;
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, sin_cos: vec2<f32>) -> f32 {
    let center = xywh.xy;
    let unrotated = center + vec2(
        (pos_ndc.x - center.x) * sin_cos.x + (pos_ndc.y - center.y) * sin_cos.y,
        -(pos_ndc.x - center.x) * sin_cos.y + (pos_ndc.y - center.y) * sin_cos.x
    );
    let radii = 0.5 * xywh.zw;
    let p = (unrotated - center) / radii;
    return length(p) - 1.0;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.sin_cos);
    let w = max(fwidth(d), 1e-5);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let a = in.color.a * alpha_cov;
    return vec4(in.color.rgb * a, a);
}
