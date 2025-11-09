struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,     // outer rect in NDC (xy = min corner, wh = extents)
    @location(2) radius: f32,         // corner radius in NDC
    @location(3) stroke_ndc: f32,     // stroke width (screen-space) in NDC
    @location(4) pos_ndc: vec2<f32>,  // interpolated NDC position
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radius: f32,
    @location(2) stroke_ndc: f32,
    @location(3) color: vec4<f32>,
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
    out.radius = radius;
    out.stroke_ndc = stroke_ndc;
    out.color = color;
    out.pos_ndc = pos_ndc;
    return out;
}

// SDF for rounded rectangle, based on iq's round-box
fn sdf_round_box(pos_ndc: vec2<f32>, xywh: vec4<f32>, r: f32) -> f32 {
    let half = 0.5 * xywh.zw;
    let center = xywh.xy + half;
    let p = pos_ndc - center;
    let q = abs(p) - (half - vec2<f32>(r, r));
    let outside = max(q, vec2<f32>(0.0, 0.0));
    let inside = min(max(q.x, q.y), 0.0);
    return length(outside) + inside - r;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // Signed distance to rounded rectangle
    let d = sdf_round_box(in.pos_ndc, in.xywh, in.radius);
    // Screen-space antialias (NDC derivatives)
    let aa = length(fwidth(in.pos_ndc)) + 1e-6;
    // Centered ring: |d| < stroke/2
    let half = 0.5 * in.stroke_ndc;
    let alpha = clamp(0.5 - (abs(d) - half) / aa, 0.0, 1.0);
    return vec4(in.color.rgb, in.color.a * alpha);
}