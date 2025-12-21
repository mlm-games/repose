struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) brush_type: u32,
    @location(1) color0: vec4<f32>,
    @location(2) color1: vec4<f32>,
    @location(3) xywh: vec4<f32>,      // NDC rect
    @location(4) radius: f32,
    @location(5) grad_start: vec2<f32>,
    @location(6) grad_end: vec2<f32>,
    @location(7) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radius: f32,
    @location(2) @interpolate(flat) brush_type: u32,
    @location(3) color0: vec4<f32>,
    @location(4) color1: vec4<f32>,
    @location(5) grad_start: vec2<f32>,
    @location(6) grad_end: vec2<f32>,
    @builtin(vertex_index) v: u32,
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
    out.brush_type = brush_type;
    out.color0 = color0;
    out.color1 = color1;
    out.grad_start = grad_start;
    out.grad_end = grad_end;
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

fn eval_brush(in: VSOut) -> vec4<f32> {
    if (in.brush_type == 0u) {
        return in.color0;
    }

    // Map pos_ndc back into [0,1] local rect space
    let rect_min = in.xywh.xy;
    let rect_size = in.xywh.zw;
    let local = (in.pos_ndc - rect_min) / rect_size; // 0..1 in both axes

    let dir = in.grad_end - in.grad_start;
    let len2 = max(dot(dir, dir), 1e-6);
    let t = clamp(dot(local - in.grad_start, dir) / len2, 0.0, 1.0);
    return mix(in.color0, in.color1, t);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_round_box(in.pos_ndc, in.xywh, in.radius);
    let aa = length(fwidth(in.pos_ndc));
    let alpha = clamp(0.5 - d / aa, 0.0, 1.0);
    let base = eval_brush(in);
    return vec4(base.rgb, base.a * alpha);
}