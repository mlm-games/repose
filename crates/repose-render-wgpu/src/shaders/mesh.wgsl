struct Globals {
    ndc_to_px: vec2f,
    _pad: vec2f,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct MeshUniform {
    m0: vec4f,
    m1: vec4f,
    paint: vec4u,
    color0: vec4f,
    color1: vec4f,
    grad_start: vec4f,
    grad_end: vec4f,
};
@group(1) @binding(0) var<uniform> U: MeshUniform;

struct VSOut {
    @builtin(position) pos: vec4f,
    @location(0) @interpolate(flat) paint_type: u32,
    @location(1) color: vec4f,
    @location(2) local: vec2f,
    @location(3) color0: vec4f,
    @location(4) color1: vec4f,
    @location(5) grad: vec4f,
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
    out.color = color;
    out.local = pos;
    out.color0 = U.color0;
    out.color1 = U.color1;
    out.grad = vec4f(U.grad_start.xy, U.grad_end.xy);
    return out;
}

fn eval_paint(in: VSOut) -> vec4f {
    if (in.paint_type == 0u) {
        return vec4f(in.color.rgb * in.color.a, in.color.a);
    }
    let dir = in.grad.zw - in.grad.xy;
    let len2 = max(dot(dir, dir), 1e-6);
    let t = clamp(dot(in.local - in.grad.xy, dir) / len2, 0.0, 1.0);
    let c = mix(in.color0, in.color1, t);
    return vec4f(c.rgb * c.a, c.a);
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4f {
    return eval_paint(in);
}