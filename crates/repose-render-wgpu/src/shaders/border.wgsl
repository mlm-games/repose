struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,     // outer rect in NDC (xy = min corner, wh = extents)
    @location(2) r_outer: f32,        // outer radius in NDC
    @location(3) stroke_ndc: f32,     // stroke width in NDC
    @location(4) pos_ndc: vec2<f32>,  // interpolated NDC position
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radius_outer: f32,
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
    out.r_outer = radius_outer;
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
  // Outer/inner SDFs with smooth AA
    let aa = length(fwidth(in.pos_ndc)) + 1e-6;

    let d_outer = sdf_round_box(in.pos_ndc, in.xywh, in.r_outer);

  // Inner rect: shrink by stroke on all edges, reduce radius accordingly
    let inner_xywh = vec4<f32>(
        in.xywh.x + in.stroke_ndc,
        in.xywh.y + in.stroke_ndc,
        in.xywh.z - 2.0 * in.stroke_ndc,
        in.xywh.w - 2.0 * in.stroke_ndc
    );
  // Clamp inner radius to non-negative
    let r_inner = max(in.r_outer - in.stroke_ndc, 0.0);
    let d_inner = sdf_round_box(in.pos_ndc, inner_xywh, r_inner);

  // Ring coverage: outer filled minus inner filled
    let cov_outer = clamp(0.5 - d_outer / aa, 0.0, 1.0);
    let cov_inner = clamp(0.5 - d_inner / aa, 0.0, 1.0);
    let alpha = max(cov_outer - cov_inner, 0.0);

    return vec4(in.color.rgb, in.color.a * alpha);
}