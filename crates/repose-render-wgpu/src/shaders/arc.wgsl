struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) xywh: vec4<f32>,
    @location(2) start_angle: f32,
    @location(3) sweep_angle: f32,
    @location(4) stroke_ndc: f32,
    @location(5) pos_ndc: vec2<f32>,
    @location(6) sin_cos: vec2<f32>,
    @location(7) @interpolate(flat) start_endpoint: vec2<f32>,
    @location(8) @interpolate(flat) end_endpoint: vec2<f32>,
    @location(9) @interpolate(flat) cap: f32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) start_angle: f32,
    @location(2) sweep_angle: f32,
    @location(3) stroke_ndc: f32,
    @location(4) pad: f32,
    @location(5) color: vec4<f32>,
    @location(6) sin_cos: vec2<f32>,
    @location(7) cap: f32,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let half = 0.5 * xywh.zw;
    // Expand quad to accommodate the full stroke width + AA
    let quad_half = half + pad;
    let corner = (p * 2.0 - 1.0) * quad_half;
    let rotated = vec2(corner.x * sin_cos.x - corner.y * sin_cos.y, corner.x * sin_cos.y + corner.y * sin_cos.x);
    let pos_ndc = xywh.xy + rotated;

    let center = xywh.xy;
    // Compute arc endpoints in NDC (on the ellipse boundary)
    // The local_angle fn returns -atan2(dy,dx), so (dx,dy) at angle theta = (R*cos(theta), -R*sin(theta))
    let start_local = vec2(half.x * cos(start_angle), -half.y * sin(start_angle));
    let end_local = vec2(half.x * cos(start_angle + sweep_angle), -half.y * sin(start_angle + sweep_angle));
    let start_endpoint = center + vec2(
        start_local.x * sin_cos.x - start_local.y * sin_cos.y,
        start_local.x * sin_cos.y + start_local.y * sin_cos.x
    );
    let end_endpoint = center + vec2(
        end_local.x * sin_cos.x - end_local.y * sin_cos.y,
        end_local.x * sin_cos.y + end_local.y * sin_cos.x
    );

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.xywh = xywh;
    out.start_angle = start_angle;
    out.sweep_angle = sweep_angle;
    out.stroke_ndc = stroke_ndc;
    out.color = color;
    out.pos_ndc = pos_ndc;
    out.sin_cos = sin_cos;
    out.start_endpoint = start_endpoint;
    out.end_endpoint = end_endpoint;
    out.cap = cap;
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

// Round cap coverage: semicircle of radius = half_px centered at the endpoint.
fn round_cap_coverage(pos_ndc: vec2<f32>, endpoint: vec2<f32>, half_px: f32) -> f32 {
    let delta = (pos_ndc - endpoint) * G.ndc_to_px;
    let dist = length(delta);
    return 1.0 - smoothstep(max(half_px - 1.0, 0.0), half_px + 1.0, dist);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.sin_cos);
    let grad = vec2(dpdx(d), dpdy(d));
    let w = max(length(grad), 1e-5);
    let half_px = 0.5 * in.stroke_ndc;
    let half = half_px * w;
    let stroke_cov = 1.0 - smoothstep(-w, w, abs(d) - half);

    let angle = local_angle(in.pos_ndc, in.xywh, in.sin_cos);
    let angle_w = max(length(fwidth(in.pos_ndc)) / length(in.xywh.zw), 1e-4);
    let angle_cov = arc_coverage(angle, in.start_angle, in.sweep_angle, angle_w * 2.0);

    var cap_cov = 0.0;
    if in.cap >= 0.5 {
        let start_cap = round_cap_coverage(in.pos_ndc, in.start_endpoint, half_px);
        let end_cap = round_cap_coverage(in.pos_ndc, in.end_endpoint, half_px);
        cap_cov = max(start_cap, end_cap);
    }

    let alpha_cov = stroke_cov * max(angle_cov, cap_cov);
    let a = in.color.a * alpha_cov;
    return vec4(in.color.rgb * a, a);
}
