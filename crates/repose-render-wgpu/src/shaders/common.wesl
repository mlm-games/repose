struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> G: Globals;

fn brush_type_of(bits: u32) -> u32 {
    return bits & 3u;
}

fn grad_kind_of(bits: u32) -> u32 {
    return (bits >> 2u) & 3u;
}

fn tile_mode_of(bits: u32) -> u32 {
    return (bits >> 4u) & 3u;
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

fn eval_brush_px(
    color0: vec4<f32>,
    color1: vec4<f32>,
    grad: vec4<f32>,
    bits: u32,
    local_px: vec2<f32>,
) -> vec4<f32> {
    if brush_type_of(bits) == 0u {
        return color0;
    }
    let tile_mode = tile_mode_of(bits);
    let grad_p0 = grad.xy;
    let grad_p1 = grad.zw;
    if grad_kind_of(bits) == 1u {
        let d = distance(local_px, grad_p0);
        let radius = max(grad_p1.x, 1e-3);
        return mix(color0, color1, apply_tile(d / radius, tile_mode));
    }
    if grad_kind_of(bits) == 2u {
        let rel = local_px - grad_p0;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if frac < 0.0 {
            frac += 1.0;
        }
        return mix(color0, color1, apply_tile(frac, tile_mode));
    }
    let dir = grad_p1 - grad_p0;
    let len2 = max(dot(dir, dir), 1e-6);
    return mix(color0, color1, apply_tile(dot(local_px - grad_p0, dir) / len2, tile_mode));
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