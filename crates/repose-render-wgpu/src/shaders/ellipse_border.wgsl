struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color0: vec4<f32>,
    @location(1) color1: vec4<f32>,
    @location(2) xywh: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) grad: vec4<f32>,
    @location(5) pos_ndc: vec2<f32>,
    @location(6) @interpolate(flat) stroke_ndc: f32,
    @location(7) @interpolate(flat) bits: u32,
};

fn brush_type_of(bits: u32) -> u32 {
    return bits & 3u;
}

fn grad_kind_of(bits: u32) -> u32 {
    return (bits >> 2u) & 3u;
}

fn tile_mode_of(bits: u32) -> u32 {
    return (bits >> 4u) & 3u;
}

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) color0: vec4<f32>,
    @location(2) color1: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) grad: vec4<f32>,
    @location(5) bits: u32,
    @location(6) stroke_ndc: f32,
    @location(7) pad: f32,
    @builtin(vertex_index) v: u32,
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
    out.color0 = color0;
    out.color1 = color1;
    out.xywh = xywh;
    out.fwd_mat = fwd_mat;
    out.grad = grad;
    out.pos_ndc = pos_ndc;
    out.stroke_ndc = stroke_ndc;
    out.bits = bits;
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
    let grad_p0 = in.grad.xy;
    let grad_p1 = in.grad.zw;
    let tile_mode = tile_mode_of(in.bits);
    if brush_type_of(in.bits) == 0u {
        return in.color0;
    }
    // Shape-local px with (0,0) at the shape top-left. `unrotated_rel` is in
    // NDC, `xywh.zw` is the shape size in px: convert the offset, recenter.
    let local_px = vec2(
        unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat).x,
        -unrotated_rel(in.pos_ndc, in.xywh, in.fwd_mat).y,
    ) * G.ndc_to_px + 0.5 * in.xywh.zw * G.ndc_to_px;
    if grad_kind_of(in.bits) == 1u {
        let d = distance(local_px, grad_p0);
        let radius = max(grad_p1.x, 1e-3);
        return mix(in.color0, in.color1, apply_tile(d / radius, tile_mode));
    }
    if grad_kind_of(in.bits) == 2u {
        let rel = local_px - grad_p0;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if frac < 0.0 {
            frac += 1.0;
        }
        return mix(in.color0, in.color1, apply_tile(frac, tile_mode));
    }
    let dir = grad_p1 - grad_p0;
    let len2 = max(dot(dir, dir), 1e-6);
    return mix(in.color0, in.color1, apply_tile(dot(local_px - grad_p0, dir) / len2, tile_mode));
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
