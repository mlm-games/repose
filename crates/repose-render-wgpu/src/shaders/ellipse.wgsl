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
    @location(6) @interpolate(flat) bits: u32,
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
    out.bits = bits;
    return out;
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = max(fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z, 1e-6);
    let unrotated = center + vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
    let radii = 0.5 * xywh.zw;
    let p = (unrotated - center) / radii;
    return length(p) - 1.0;
}

// Shape-local px with (0,0) at the shape top-left. `xywh.zw` is the shape
// size in px (rect_to_instance_ndc scales, never rotates, so the NDC span
// converts back with ndc_to_px); no extra normalization needed. Scaled by
// the caller's `G.ndc_to_px`, matching the px space of `half_px`/`stroke_px`.
fn ellipse_local_px(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> vec2<f32> {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = max(fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z, 1e-6);
    let unrotated_ndc = center + vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
    return vec2(
        unrotated_ndc.x - center.x,
        -(unrotated_ndc.y - center.y),
    ) * G.ndc_to_px + 0.5 * xywh.zw * G.ndc_to_px;
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

// Ellipse fill supports linear, radial and sweep gradients (same
// `grad_kind` encoding as the border shader).
fn eval_ellipse_brush(in: VSOut, local_px: vec2<f32>) -> vec4<f32> {
    let grad_p0 = in.grad.xy;
    let grad_p1 = in.grad.zw;
    let tile_mode = tile_mode_of(in.bits);
    if brush_type_of(in.bits) == 0u {
        return in.color0;
    }
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
    let w = max(fwidth(d), 1e-5);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let local_px = ellipse_local_px(in.pos_ndc, in.xywh, in.fwd_mat);
    let base = eval_ellipse_brush(in, local_px);

    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}
