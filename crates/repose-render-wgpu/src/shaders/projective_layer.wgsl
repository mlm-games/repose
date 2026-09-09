// Projective layer composite: samples an offscreen graphics-layer texture
// through a general 2D projective map (perspective flattening, CSS style).
//
// The subtree renders flat into the layer; the four layer-rect corners are
// projected on the CPU to NDC (`c0..c3`, counter-clockwise from top-left)
// with their homogeneous `w`. The `w` rides in CLIP SPACE
// (`out.pos = vec4(corner * w, w)`), so the GPU's perspective-correct
// interpolation reconstructs the projective map exactly; the fragment
// shader samples the plain interpolated uv (a separate `w` varying would
// interpolate linearly in screen space, which is wrong). Input and output
// are premultiplied alpha, like the sharp `CompositeLayer` path.
struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) alpha: f32,
};

@vertex
fn vs_main(
    @location(0) c0: vec2<f32>,
    @location(1) c1: vec2<f32>,
    @location(2) c2: vec2<f32>,
    @location(3) c3: vec2<f32>,
    @location(4) uv_bounds: vec4<f32>,
    @location(5) w_row: vec4<f32>,
    @location(6) alpha: f32,
    @builtin(vertex_index) v: u32,
) -> VSOut {
    // 6 vertices -> 4 corners: 0,1,2, 0,2,3.
    var corner = c0;
    var uv = uv_bounds.xy;
    var w = w_row.x;
    if (v == 1u) {
        corner = c1;
        uv = vec2(uv_bounds.z, uv_bounds.y);
        w = w_row.y;
    } else if (v == 2u || v == 4u) {
        corner = c2;
        uv = uv_bounds.zw;
        w = w_row.z;
    } else if (v == 5u) {
        corner = c3;
        uv = vec2(uv_bounds.x, uv_bounds.w);
        w = w_row.w;
    }

    var out: VSOut;
    out.pos = vec4(corner * w, 0.0, w);
    out.uv = uv;
    out.alpha = alpha;
    return out;
}

@group(1) @binding(0) var layer_tex: texture_2d<f32>;
@group(1) @binding(1) var layer_samp: sampler;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    // `in.uv` is already perspective-correct (divided by clip-space `w` in
    // hardware). The layer sampler clamps, like the sharp composite path.
    let c = textureSample(layer_tex, layer_samp, in.uv);
    let a = c.a * in.alpha;
    return vec4(c.rgb * in.alpha, a);
}
