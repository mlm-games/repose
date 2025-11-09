struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>, // NDC bounding rect
    @location(2) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) color: vec4<f32>,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let pos_ndc = xywh.xy + p * xywh.zw;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.xywh = xywh;
    out.color = color;
    out.pos_ndc = pos_ndc;
    return out;
}

// SDF for ellipse: transform into unit circle space and measure radius-1
fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>) -> f32 {
    let center = xywh.xy + 0.5 * xywh.zw;
    let radii = 0.5 * xywh.zw;
    let p = (pos_ndc - center) / radii;
    return length(p) - 1.0;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh);
    // derivative in ellipse-space (approx): scale fwidth with reciprocal radii
    let center = in.xywh.xy + 0.5 * in.xywh.zw;
    let radii = 0.5 * in.xywh.zw;
    let p = (in.pos_ndc - center) / radii;
    let aa = length(fwidth(p)) + 1e-6;
    let alpha = clamp(0.5 - d / aa, 0.0, 1.0);
    return vec4(in.color.rgb, in.color.a * alpha);
}