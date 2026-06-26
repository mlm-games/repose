struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) start_angle: f32,
    @location(3) sweep_angle: f32,
    @location(4) stroke_ndc: f32,
    @location(5) pos_ndc: vec2<f32>,
    @location(6) sin_cos: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) start_angle: f32,
    @location(2) sweep_angle: f32,
    @location(3) stroke_ndc: f32,
    @location(4) color: vec4<f32>,
    @location(5) sin_cos: vec2<f32>,
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
    out.start_angle = start_angle;
    out.sweep_angle = sweep_angle;
    out.stroke_ndc = stroke_ndc;
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

fn local_angle(pos_ndc: vec2<f32>, xywh: vec4<f32>, sin_cos: vec2<f32>) -> f32 {
    let center = xywh.xy;
    let unrotated = center + vec2(
        (pos_ndc.x - center.x) * sin_cos.x + (pos_ndc.y - center.y) * sin_cos.y,
        -(pos_ndc.x - center.x) * sin_cos.y + (pos_ndc.y - center.y) * sin_cos.x
    );
    let dx = unrotated.x - center.x;
    let dy = unrotated.y - center.y;
    return -atan2(dy, dx);
}

// Arc coverage: 1.0 inside the sweep, smoothly fading to 0.0 at the boundaries.
// Angles follow the convention: 0 = 3 o'clock, positive = clockwise.
fn arc_coverage(angle: f32, start: f32, sweep: f32, half_px: f32) -> f32 {
    if sweep >= 6.2831853 {
        return 1.0;
    }
    let end = start + sweep;
    let a = (angle % 6.2831853 + 6.2831853) % 6.2831853;
    let s = (start % 6.2831853 + 6.2831853) % 6.2831853;
    let e = (end % 6.2831853 + 6.2831853) % 6.2831853;

    var da: f32;
    if s <= e {
        if a < s || a > e {
            da = min((s - a + 6.2831853) % 6.2831853, (a - e + 6.2831853) % 6.2831853);
        } else {
            return 1.0;
        }
    } else {
        if a > e && a < s {
            da = min((a - e + 6.2831853) % 6.2831853, (s - a + 6.2831853) % 6.2831853);
        } else {
            return 1.0;
        }
    }
    return 1.0 - smoothstep(0.0, half_px, da);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.sin_cos);
    let w = max(fwidth(d), 1e-5);
    let half_px = 0.5 * in.stroke_ndc;
    let half = half_px * w;
    let stroke_cov = 1.0 - smoothstep(-w, w, abs(d) - half);

    let angle = local_angle(in.pos_ndc, in.xywh, in.sin_cos);
    let angle_w = max(length(fwidth(in.pos_ndc)) / length(in.xywh.zw), 1e-4);
    let angle_cov = arc_coverage(angle, in.start_angle, in.sweep_angle, angle_w * 0.5);

    let alpha_cov = stroke_cov * angle_cov;
    let a = in.color.a * alpha_cov;
    return vec4(in.color.rgb * a, a);
}
