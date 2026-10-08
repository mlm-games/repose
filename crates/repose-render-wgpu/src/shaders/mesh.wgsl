struct Globals {
    ndc_to_px: vec2f,
    _pad: vec2f,
};
@group(0) @binding(0) var<uniform> G: Globals;

// Field order and member widths mirror `MeshUniform` in src/lib.rs; every
// member is a `vec4` because the uniform address space forces 16-byte
// alignment. `paint.z` is the live stop count (0..=8).
struct MeshUniform {
    m0: vec4f,
    m1: vec4f,
    paint: vec4u,
    color0: vec4f,
    color1: vec4f,
    grad_start: vec4f,
    grad_end: vec4f,
    stop_colors: array<vec4f, 8>,
    stop_offsets: array<vec4f, 2>,
};
@group(1) @binding(0) var<uniform> U: MeshUniform;

struct VSOut {
    @builtin(position) pos: vec4f,
    @location(0) @interpolate(flat) paint_type: u32,
    @location(1) @interpolate(flat) paint_kind: u32,
    @location(2) color: vec4f,
    @location(3) local: vec2f,
    @location(4) color0: vec4f,
    @location(5) color1: vec4f,
    @location(6) grad: vec4f,
};

@vertex
fn vs_main(
    @location(0) pos: vec2f,
    @location(1) color: vec4f,
    @location(2) uv: vec2f,
) -> VSOut {
    // world_px = affine(local), rows in U.m0 / U.m1.
    let world = vec2f(
        U.m0.x * pos.x + U.m0.y * pos.y + U.m0.z,
        U.m1.x * pos.x + U.m1.y * pos.y + U.m1.z,
    );
    var out: VSOut;
    out.pos = vec4f(
        world.x / G.ndc_to_px.x - 1.0,
        1.0 - world.y / G.ndc_to_px.y,
        0.0,
        1.0,
    );
    out.paint_type = U.paint.x;
    out.paint_kind = U.paint.y;
    out.color = color;
    out.local = pos;
    out.color0 = U.color0;
    out.color1 = U.color1;
    out.grad = vec4f(U.grad_start.xy, U.grad_end.xy);
    return out;
}

const MAX_STOPS: u32 = 8u;

fn stop_color(i: u32) -> vec4f {
    return U.stop_colors[min(i, MAX_STOPS - 1u)];
}

// Four offsets share one `vec4`; the lane is a branch on a uniform value, not a
// dynamic vector index, so no backend has to lower a component read from
// storage.
fn stop_offset(i: u32) -> f32 {
    let lane = i % 4u;
    let packed = U.stop_offsets[min(i / 4u, MAX_STOPS / 4u - 1u)];
    if (lane == 0u) {
        return packed.x;
    }
    if (lane == 1u) {
        return packed.y;
    }
    if (lane == 2u) {
        return packed.z;
    }
    return packed.w;
}

// Multi-stop ramp at parametric position `p`. Offsets arrive clamped to
// 0..=1 and non-decreasing (Rust forces that when packing), so one forward
// scan finds the bracketing pair; `p` outside the first/last offset clamps to
// that stop's colour (TileMode::Clamp), and repeated offsets are a hard stop
// at the shared position (the later stop wins). `count == 0` paints nothing,
// `count == 1` paints the single stop.
fn eval_stops(p: f32, count: u32) -> vec4f {
    if (count == 0u) {
        return vec4f(0.0);
    }
    let last = min(count, MAX_STOPS) - 1u;
    if (last == 0u) {
        return stop_color(0u);
    }
    if (p <= stop_offset(0u)) {
        return stop_color(0u);
    }
    if (p >= stop_offset(last)) {
        return stop_color(last);
    }
    var hi = last;
    for (var i = 1u; i < last; i++) {
        if (p < stop_offset(i)) {
            hi = i;
            break;
        }
    }
    let lo = hi - 1u;
    let span = stop_offset(hi) - stop_offset(lo);
    var f = 1.0;
    if (span > 0.0) {
        f = clamp((p - stop_offset(lo)) / span, 0.0, 1.0);
    }
    return mix(stop_color(lo), stop_color(hi), f);
}

fn radial_pos(local: vec2f, center: vec2f, radius: f32) -> f32 {
    return clamp(distance(local, center) / max(radius, 1e-3), 0.0, 1.0);
}

fn linear_pos(local: vec2f, p0: vec2f, p1: vec2f) -> f32 {
    let dir = p1 - p0;
    let len2 = max(dot(dir, dir), 1e-6);
    return clamp(dot(local - p0, dir) / len2, 0.0, 1.0);
}

fn eval_paint(in: VSOut) -> vec4f {
    if (in.paint_type == 0u) {
        return vec4f(in.color.rgb * in.color.a, in.color.a);
    }
    if (in.paint_kind == 1u) {
        let c = mix(in.color0, in.color1, radial_pos(in.local, in.grad.xy, in.grad.z));
        return vec4f(c.rgb * c.a, c.a);
    }
    if (in.paint_kind == 2u) {
        let rel = in.local - in.grad.xy;
        var frac = atan2(rel.y, rel.x) / 6.2831853;
        if (frac < 0.0) {
            frac += 1.0;
        }
        let c = mix(in.color0, in.color1, clamp(frac, 0.0, 1.0));
        return vec4f(c.rgb * c.a, c.a);
    }
    // Same positions as the two-stop ramps above, so a two-stop table
    // reproduces `Linear`/`Radial` exactly.
    if (in.paint_kind == 3u) {
        let c = eval_stops(linear_pos(in.local, in.grad.xy, in.grad.zw), U.paint.z);
        return vec4f(c.rgb * c.a, c.a);
    }
    if (in.paint_kind == 4u) {
        let c = eval_stops(radial_pos(in.local, in.grad.xy, in.grad.z), U.paint.z);
        return vec4f(c.rgb * c.a, c.a);
    }
    let c = mix(in.color0, in.color1, linear_pos(in.local, in.grad.xy, in.grad.zw));
    return vec4f(c.rgb * c.a, c.a);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4f {
    return eval_paint(in);
}
