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
    @location(5) stroke_ndc: f32,
    @location(6) grad_p0: vec2<f32>,
    @location(7) grad_p1: vec2<f32>,
    @location(8) @interpolate(flat) tile_mode: u32,
    @location(9) pos_ndc: vec2<f32>,
    @location(10) fwd_mat: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) stroke_ndc: f32,
    @location(2) pad: f32,
    @location(3) @interpolate(flat) brush_type: u32,
    @location(4) @interpolate(flat) grad_kind: u32,
    @location(5) color0: vec4<f32>,
    @location(6) color1: vec4<f32>,
    @location(7) grad_p0: vec2<f32>,
    @location(8) grad_p1: vec2<f32>,
    @location(9) @interpolate(flat) tile_mode: u32,
    @location(10) fwd_mat: vec4<f32>,
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

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.brush_type = brush_type;
    out.grad_kind = grad_kind;
    out.color0 = color0;
    out.color1 = color1;
    out.xywh = xywh;
    out.stroke_ndc = stroke_ndc;
    out.grad_p0 = grad_p0;
    out.grad_p1 = grad_p1;
    out.tile_mode = tile_mode;
    out.pos_ndc = pos_ndc;
    out.fwd_mat = fwd_mat;
    return out;
}

fn unrotated_rel(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> vec2<f32> {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = max(fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z, 1e-6);
    return vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let radii = 0.5 * xywh.zw;
    let p = unrotated_rel(pos_ndc, xywh, fwd_mat) / radii;
    return length(p) - 1.0;
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

fn eval_ring_brush(in: VSOut) -> vec4<f32> {
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

    let half_px = 0.5 * in.stroke_ndc;
    let half = half_px * w;
    let alpha_cov = 1.0 - smoothstep(-w, w, abs(d) - half);

    let base = eval_ring_brush(in);
    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}
