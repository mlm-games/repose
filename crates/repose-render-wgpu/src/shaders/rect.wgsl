struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) brush_type: u32,
    @location(1) color0: vec4<f32>,
    @location(2) color1: vec4<f32>,
    @location(3) xywh: vec4<f32>,
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

// Pixel-space SDF for true circular corners
fn sdf_round_box_px(p_px: vec2<f32>, half_px: vec2<f32>, r_px: f32) -> f32 {
    let r = max(r_px, 0.0);
    let q = abs(p_px) - (half_px - vec2<f32>(r, r));
    let outside = max(q, vec2<f32>(0.0, 0.0));
    let inside = min(max(q.x, q.y), 0.0);
    return length(outside) + inside - r;
}

fn eval_brush(in: VSOut) -> vec4<f32> {
    if (in.brush_type == 0u) {
        return in.color0;
    }

    let rect_min = in.xywh.xy;
    let rect_size = in.xywh.zw;
    let local = (in.pos_ndc - rect_min) / rect_size;

    let dir = in.grad_end - in.grad_start;
    let len2 = max(dot(dir, dir), 1e-6);
    let t = clamp(dot(local - in.grad_start, dir) / len2, 0.0, 1.0);
    return mix(in.color0, in.color1, t);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let center_ndc = in.xywh.xy + 0.5 * in.xywh.zw;

    let p_px = (in.pos_ndc - center_ndc) * G.ndc_to_px;
    let half_px = 0.5 * in.xywh.zw * G.ndc_to_px;

    let d = sdf_round_box_px(p_px, half_px, in.radius);

    // d is in pixels, so fwidth(d) ≈ 1 at edges
    let w = max(fwidth(d), 1e-4);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let base = eval_brush(in);
    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}