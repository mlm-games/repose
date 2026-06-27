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
    @location(4) radii: vec4<f32>,
    @location(5) grad_start: vec2<f32>,
    @location(6) grad_end: vec2<f32>,
    @location(7) pos_ndc: vec2<f32>,
    @location(8) sin_cos: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) @interpolate(flat) brush_type: u32,
    @location(3) color0: vec4<f32>,
    @location(4) color1: vec4<f32>,
    @location(5) grad_start: vec2<f32>,
    @location(6) grad_end: vec2<f32>,
    @location(7) sin_cos: vec2<f32>,
    @builtin(vertex_index) v: u32,
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
    out.radii = radii;
    out.brush_type = brush_type;
    out.color0 = color0;
    out.color1 = color1;
    out.grad_start = grad_start;
    out.grad_end = grad_end;
    out.pos_ndc = pos_ndc;
    out.sin_cos = sin_cos;
    return out;
}

fn corner_radius(p: vec2<f32>, r: vec4<f32>) -> f32 {
    return select(
        select(r[3], r[2], p.x >= 0.0),
        select(r[0], r[1], p.x >= 0.0),
        p.y >= 0.0
    );
}

fn sdf_round_box_px(p_px: vec2<f32>, half_px: vec2<f32>, r: vec4<f32>) -> f32 {
    let ri = corner_radius(p_px, r);
    let ri_clamped = max(ri, 0.0);
    let q = abs(p_px) - (half_px - vec2<f32>(ri_clamped, ri_clamped));
    let outside = max(q, vec2<f32>(0.0));
    let inside = min(max(q.x, q.y), 0.0);
    return length(outside) + inside - ri_clamped;
}

fn eval_brush(in: VSOut) -> vec4<f32> {
    if (in.brush_type == 0u) {
        return in.color0;
    }

    let center_ndc = in.xywh.xy;
    let half = 0.5 * in.xywh.zw;
    let rect_min = center_ndc - half;
    let rect_size = in.xywh.zw;
    let unrotated_ndc = center_ndc + vec2(
        (in.pos_ndc.x - center_ndc.x) * in.sin_cos.x + (in.pos_ndc.y - center_ndc.y) * in.sin_cos.y,
        -(in.pos_ndc.x - center_ndc.x) * in.sin_cos.y + (in.pos_ndc.y - center_ndc.y) * in.sin_cos.x
    );
    let local = (unrotated_ndc - rect_min) / rect_size;

    let dir = in.grad_end - in.grad_start;
    let len2 = max(dot(dir, dir), 1e-6);
    let t = clamp(dot(local - in.grad_start, dir) / len2, 0.0, 1.0);
    return mix(in.color0, in.color1, t);
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

    let d = sdf_round_box_px(unrotated_px, half_px, in.radii);

    let w = max(fwidth(d), 1e-4);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let base = eval_brush(in);
    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}
