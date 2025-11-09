struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>, // NDC bounding rect
    @location(2) stroke_ndc: f32, // stroke width (screen-space) in NDC
    @location(3) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) stroke_ndc: f32,
    @location(2) color: vec4<f32>,
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
    out.stroke_ndc = stroke_ndc;
    out.color = color;
    out.pos_ndc = pos_ndc;
    return out;
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>) -> f32 {
    let center = xywh.xy + 0.5 * xywh.zw;
    let radii = 0.5 * xywh.zw;
    let p = (pos_ndc - center) / radii;
    return length(p) - 1.0;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh);
    // AA based on ellipse-space derivatives
    let center = in.xywh.xy + 0.5 * in.xywh.zw;
    let radii = 0.5 * in.xywh.zw;
    let p = (in.pos_ndc - center) / radii;
    let aa = length(fwidth(p)) + 1e-6;
    let half = 0.5 * in.stroke_ndc;
    let alpha = clamp(0.5 - (abs(d) - half) / aa, 0.0, 1.0);
    return vec4(in.color.rgb, in.color.a * alpha);
}