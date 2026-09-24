struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) @interpolate(flat) brush_type: u32,
    @location(1) @interpolate(flat) grad_kind: u32,
    @location(2) color0: vec4<f32>,
    @location(3) color1: vec4<f32>,
    @location(4) xywh: vec4<f32>,
    @location(5) start_angle: f32,
    @location(6) sweep_angle: f32,
    @location(7) stroke_px: f32,
    @location(8) grad_p0: vec2<f32>,
    @location(9) grad_p1: vec2<f32>,
    @location(10) @interpolate(flat) tile_mode: u32,
    @location(11) pos_ndc: vec2<f32>,
    @location(12) fwd_mat: vec4<f32>,
    @location(13) @interpolate(flat) cap: f32,
};

fn ellipse_pt_at_angle(center: vec2<f32>, half: vec2<f32>, angle: f32, fwd: vec4<f32>) -> vec2<f32> {
    let c = cos(angle);
    let s = sin(angle);
    let denom = sqrt(max(half.y * half.y * c * c + half.x * half.x * s * s, 1e-6));
    let local = vec2(half.x * half.y * c / denom, -half.x * half.y * s / denom);
    return center + vec2(
        fwd.x * local.x + fwd.y * local.y,
        fwd.z * local.x + fwd.w * local.y
    );
}

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) start_angle: f32,
    @location(2) sweep_angle: f32,
    @location(3) stroke_px: f32,
    @location(4) pad: f32,
    @location(5) @interpolate(flat) brush_type: u32,
    @location(6) @interpolate(flat) grad_kind: u32,
    @location(7) color0: vec4<f32>,
    @location(8) color1: vec4<f32>,
    @location(9) grad_p0: vec2<f32>,
    @location(10) grad_p1: vec2<f32>,
    @location(11) @interpolate(flat) tile_mode: u32,
    @location(12) cap: f32,
    @location(13) fwd_mat: vec4<f32>,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let pad_px = pad * G.ndc_to_px.x;
    let pad_ndc = vec2<f32>(
        pad_px / max(G.ndc_to_px.x, 1.0),
        pad_px / max(G.ndc_to_px.y, 1.0),
    );
    let quad_half = half + pad_ndc;
    let corner = (p * 2.0 - 1.0) * quad_half;
    let rotated = vec2(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y
    );
    let pos_ndc = xywh.xy + rotated;

    let center = xywh.xy;
    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.brush_type = brush_type;
    out.grad_kind = grad_kind;
    out.color0 = color0;
    out.color1 = color1;
    out.xywh = xywh;
    out.start_angle = start_angle;
    out.sweep_angle = sweep_angle;
    out.stroke_px = stroke_px;
    out.grad_p0 = grad_p0;
    out.grad_p1 = grad_p1;
    out.tile_mode = tile_mode;
    out.pos_ndc = pos_ndc;
    out.fwd_mat = fwd_mat;
    out.cap = cap;
    return out;
}

fn unrotated_rel(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> vec2<f32> {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z;
    if abs(det) < 1e-6 {
        return rel;
    }
    return vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let radii = 0.5 * xywh.zw;
    if any(radii <= vec2<f32>(0.0)) {
        return 1.0;
    }
    let p = unrotated_rel(pos_ndc, xywh, fwd_mat) / radii;
    return length(p) - 1.0;
}

fn local_angle(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let rel = unrotated_rel(pos_ndc, xywh, fwd_mat);
    return -atan2(rel.y, rel.x);
}

// Arc coverage: 1.0 inside the sweep, smoothly fading to 0.0 at the boundaries.
fn arc_coverage(angle: f32, start_arg: f32, sweep_arg: f32, half_px: f32) -> f32 {
    if abs(sweep_arg) <= 1e-6 {
        return 0.0;
    }
    var start = start_arg;
    var sweep = sweep_arg;
    if sweep < 0.0 {
        start = start + sweep;
        sweep = -sweep;
    }
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

fn cap_coverage(
    pos_ndc: vec2<f32>,
    xywh: vec4<f32>,
    fwd_mat: vec4<f32>,
    start_angle: f32,
    end_angle: f32,
    half_width_px: f32,
    aa: f32,
    cap: f32,
) -> f32 {
    if (cap < 0.5 || abs(end_angle - start_angle) >= 6.2831853) {
        return 0.0;
    }
    let radii = 0.5 * xywh.zw * G.ndc_to_px;
    let pos = unrotated_rel(pos_ndc, xywh, fwd_mat) * G.ndc_to_px;
    let start = vec2<f32>(radii.x * cos(start_angle), -radii.y * sin(start_angle));
    let end = vec2<f32>(radii.x * cos(end_angle), -radii.y * sin(end_angle));
    if (cap < 1.5) {
        let distance = length(pos - start);
        let end_distance = length(pos - end);
        return 1.0 - smoothstep(
            max(half_width_px - aa, 0.0),
            half_width_px + aa,
            min(distance, end_distance),
        );
    }
    let start_tangent = normalize(vec2<f32>(-radii.x * sin(start_angle), -radii.y * cos(start_angle)));
    let end_tangent = normalize(vec2<f32>(-radii.x * sin(end_angle), -radii.y * cos(end_angle)));
    let start_delta = pos - (start - start_tangent * half_width_px);
    let end_delta = pos - (end + end_tangent * half_width_px);
    let start_q = abs(vec2<f32>(dot(start_delta, start_tangent), start_delta.x * -start_tangent.y + start_delta.y * start_tangent.x)) - vec2<f32>(half_width_px, half_width_px);
    let end_q = abs(vec2<f32>(dot(end_delta, end_tangent), end_delta.x * -end_tangent.y + end_delta.y * end_tangent.x)) - vec2<f32>(half_width_px, half_width_px);
    let start_distance = length(max(start_q, vec2<f32>(0.0))) + min(max(start_q.x, start_q.y), 0.0);
    let end_distance = length(max(end_q, vec2<f32>(0.0))) + min(max(end_q.x, end_q.y), 0.0);
    return 1.0 - smoothstep(-aa, aa, min(start_distance, end_distance));
}

fn apply_tile(t: f32, tile_mode: u32) -> f32 {
    if tile_mode == 1u {
        return t - floor(t);
    }
    if tile_mode == 2u {
        let m = t - floor(t * 0.5) * 2.0;
        return select(m, 2.0 - m, m > 1.0);
    }
    return clamp(t, 0.0, 1.0);
}

fn eval_arc_brush(in: VSOut) -> vec4<f32> {
    if in.brush_type == 0u {
        return in.color0;
    }
    // Shape-local px with (0,0) at the shape top-left. `unrotated_rel` is in
    // NDC, `xywh.zw` is the shape size in px: convert the offset, recenter.
    let local_px = unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat) * G.ndc_to_px
        + 0.5 * in.xywh.zw * G.ndc_to_px;
    if in.grad_kind == 1u {
        let d = distance(local_px, in.grad_p0);
        let radius = max(in.grad_p1.x, 1e-3);
        return mix(in.color0, in.color1, apply_tile(d / radius, in.tile_mode));
    }
    if in.grad_kind == 2u {
        let rel = local_px - in.grad_p0;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if frac < 0.0 {
            frac += 1.0;
        }
        return mix(in.color0, in.color1, apply_tile(frac, in.tile_mode));
    }
    let dir = in.grad_p1 - in.grad_p0;
    let len2 = max(dot(dir, dir), 1e-6);
    return mix(in.color0, in.color1, apply_tile(dot(local_px - in.grad_p0, dir) / len2, in.tile_mode));
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.fwd_mat);
    let grad = vec2(dpdx(d), dpdy(d));
    let w = max(length(grad), 1e-5);
    let half_width_px = 0.5 * in.stroke_px;
    let half = half_width_px * w;
    let stroke_cov = 1.0 - smoothstep(-w, w, abs(d) - half);

    let angle = local_angle(in.pos_ndc, in.xywh, in.fwd_mat);
    let rel = unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat);
    let angle_w = max(length(vec2<f32>(
        fwidth(rel.x) / max(0.5 * in.xywh.z, 1e-3),
        fwidth(rel.y) / max(0.5 * in.xywh.w, 1e-3),
    )), 1e-4);
    let angle_cov = arc_coverage(angle, in.start_angle, in.sweep_angle, angle_w * 2.0);

    let cap_aa = max(
        fwidth(unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat).x * G.ndc_to_px.x)
        + fwidth(unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat).y * G.ndc_to_px.y),
        0.25,
    );
    let cap_cov = cap_coverage(
        in.pos_ndc,
        in.xywh,
        in.fwd_mat,
        in.start_angle,
        in.start_angle + in.sweep_angle,
        half_width_px,
        cap_aa,
        in.cap,
    );

    let alpha_cov = max(stroke_cov * angle_cov, cap_cov);
    let base = eval_arc_brush(in);
    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}
