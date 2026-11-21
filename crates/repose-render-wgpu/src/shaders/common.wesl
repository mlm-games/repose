struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> G: Globals;

const MAX_GRADIENT_STOPS: u32 = 8u;

// Gradient stop tables are stored in a 2D texture, one table per row:
//   x 0..7  : rgba color for stops 0..7
//   x 8..15 : r channel holds the offset for stops 0..7
//   x 16    : r channel holds the stop count
// A texture (rather than a storage/uniform buffer) keeps the shader
// translatable to GLSL ES on WebGL backends.
@group(1) @binding(0) var SHAPE_STOPS: texture_2d<f32>;

fn stop_color_at(table: u32, i: u32) -> vec4<f32> {
    return textureLoad(SHAPE_STOPS, vec2<i32>(i32(i), i32(table)), 0);
}

fn stop_offset_at(table: u32, i: u32) -> f32 {
    return textureLoad(SHAPE_STOPS, vec2<i32>(8i + i32(i), i32(table)), 0).r;
}

fn stop_count(table: u32) -> u32 {
    return u32(textureLoad(SHAPE_STOPS, vec2<i32>(16, i32(table)), 0).r + 0.5);
}

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

fn stop_table_of(bits: u32) -> u32 {
    return (bits >> 8u) & 0xFFFu;
}

fn eval_shape_stops(t: f32, table: u32) -> vec4<f32> {
    let count = stop_count(table);
    if count == 0u {
        return vec4<f32>(0.0);
    }
    if count == 1u || t <= stop_offset_at(table, 0u) {
        return stop_color_at(table, 0u);
    }
    let last = count - 1u;
    if t >= stop_offset_at(table, last) {
        return stop_color_at(table, last);
    }
    for (var i = 0u; i < last; i = i + 1u) {
        if t < stop_offset_at(table, i + 1u) {
            let lo = stop_offset_at(table, i);
            let span = max(stop_offset_at(table, i + 1u) - lo, 1e-6);
            let f = clamp((t - lo) / span, 0.0, 1.0);
            return mix(stop_color_at(table, i), stop_color_at(table, i + 1u), f);
        }
    }
    return stop_color_at(table, last);
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
    let kind = grad_kind_of(bits);
    var t: f32;
    if kind == 1u {
        let d = distance(local_px, grad_p0);
        let radius = max(grad_p1.x, 1e-3);
        t = d / radius;
    } else if kind == 2u {
        let rel = local_px - grad_p0;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if frac < 0.0 {
            frac += 1.0;
        }
        t = frac;
    } else {
        let dir = grad_p1 - grad_p0;
        let len2 = max(dot(dir, dir), 1e-6);
        t = dot(local_px - grad_p0, dir) / len2;
    }
    t = apply_tile(t, tile_mode);
    if brush_type_of(bits) == 2u {
        return eval_shape_stops(t, stop_table_of(bits));
    }
    return mix(color0, color1, t);
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