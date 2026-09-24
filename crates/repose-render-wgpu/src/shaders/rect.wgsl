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
    @location(5) radii: vec4<f32>,
    @location(6) grad_p0: vec2<f32>,
    @location(7) grad_p1: vec2<f32>,
    @location(8) @interpolate(flat) tile_mode: u32,
    @location(9) pos_ndc: vec2<f32>,
    @location(10) fwd_mat: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) @interpolate(flat) brush_type: u32,
    @location(3) @interpolate(flat) grad_kind: u32,
    @location(4) color0: vec4<f32>,
    @location(5) color1: vec4<f32>,
    @location(6) grad_p0: vec2<f32>,
    @location(7) grad_p1: vec2<f32>,
    @location(8) @interpolate(flat) tile_mode: u32,
    @location(9) fwd_mat: vec4<f32>,
    @builtin(vertex_index) v: u32,
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y,
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.xywh = xywh;
    out.radii = radii;
    out.brush_type = brush_type;
    out.grad_kind = grad_kind;
    out.color0 = color0;
    out.color1 = color1;
    out.grad_p0 = grad_p0;
    out.grad_p1 = grad_p1;
    out.tile_mode = tile_mode;
    out.pos_ndc = pos_ndc;
    out.fwd_mat = fwd_mat;
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

fn eval_brush(in: VSOut, local_px: vec2<f32>) -> vec4<f32> {
    if (in.brush_type == 0u) {
        return in.color0;
    }
    if (in.grad_kind == 1u) {
        let d = distance(local_px, in.grad_p0);
        let radius = max(in.grad_p1.x, 1e-3);
        return mix(in.color0, in.color1, apply_tile(d / radius, in.tile_mode));
    }
    if (in.grad_kind == 2u) {
        let rel = local_px - in.grad_p0;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if (frac < 0.0) {
            frac += 1.0;
        }
        return mix(in.color0, in.color1, apply_tile(frac, in.tile_mode));
    }
    let dir = in.grad_p1 - in.grad_p0;
    let len2 = max(dot(dir, dir), 1e-6);
    let t = apply_tile(dot(local_px - in.grad_p0, dir) / len2, in.tile_mode);
    return mix(in.color0, in.color1, t);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let center_ndc = in.xywh.xy;
    let p_px = (in.pos_ndc - center_ndc) * G.ndc_to_px;
    let half_px = 0.5 * in.xywh.zw * G.ndc_to_px;

    let det = max(
        in.fwd_mat.x * in.fwd_mat.w - in.fwd_mat.y * in.fwd_mat.z,
        1e-6,
    );
    let unrotated_px = vec2(
        (in.fwd_mat.w * p_px.x - in.fwd_mat.y * p_px.y) / det,
        (-in.fwd_mat.z * p_px.x + in.fwd_mat.x * p_px.y) / det,
    );

    let d = sdf_round_box_px(unrotated_px, half_px, in.radii);

    let w = max(fwidth(d), 1e-4);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    // Brush coordinates are shape-local px with (0,0) at the top-left,
    // matching `brush_to_shape_fields` and the border shader: recenter
    // `unrotated_px` (rotation-free, un-scaled) and re-apply the scale
    // magnitude so both live in the same space.
    let scale_px = vec2(length(vec2(in.fwd_mat.x, in.fwd_mat.z)), length(vec2(in.fwd_mat.y, in.fwd_mat.w)));
    let local_px = vec2(
        unrotated_px.x * scale_px.x,
        -unrotated_px.y * scale_px.y,
    ) + half_px;
    let base = eval_brush(in, local_px);
    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}
