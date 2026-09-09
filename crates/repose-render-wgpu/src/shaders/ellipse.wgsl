struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) pos_ndc: vec2<f32>,
    @location(3) fwd_mat: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) fwd_mat: vec4<f32>,
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
        fwd_mat.z * corner.x + fwd_mat.w * corner.y
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.xywh = xywh;
    out.color = color;
    out.pos_ndc = pos_ndc;
    out.fwd_mat = fwd_mat;
    return out;
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = max(fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z, 1e-6);
    let unrotated = center + vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
    let radii = 0.5 * xywh.zw;
    let p = (unrotated - center) / radii;
    return length(p) - 1.0;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.fwd_mat);
    let w = max(fwidth(d), 1e-5);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let a = in.color.a * alpha_cov;
    return vec4(in.color.rgb * a, a);
}
