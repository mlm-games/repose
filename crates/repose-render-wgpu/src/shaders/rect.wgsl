struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>, // NDC rect: xy = min corner, wh = extents
    @location(2) radius: f32,     // NDC radius (min of x/y scale)
    @location(3) pos_ndc: vec2<f32>, // interpolated NDC position
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,   // already in NDC
    @location(1) radius: f32,
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
    out.radius = radius;
    out.color = color;
    out.pos_ndc = pos_ndc;
    return out;
}

// Signed distance to rounded rectangle in NDC, using iq's round-box SDF.
// We treat xywh in NDC; radius is in NDC too.
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
  // Compute smooth alpha based on distance and derivatives
    let d = sdf_round_box(in.pos_ndc, in.xywh, in.radius);
    let aa = length(fwidth(in.pos_ndc)); // screen-space derivative
    let alpha = clamp(0.5 - d / aa, 0.0, 1.0);
    return vec4(in.color.rgb, in.color.a * alpha);
}
