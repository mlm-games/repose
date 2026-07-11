@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) v: u32) -> VSOut {
    let pos = array(
        vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0),
    );
    let uv = array(
        vec2(0.0, 0.0), vec2(2.0, 0.0), vec2(0.0, 2.0),
    );
    var o: VSOut;
    o.pos = vec4(pos[v], 0.0, 1.0);
    o.uv = uv[v];
    return o;
}

fn srgb_oetf(c: f32) -> f32 {
    if c <= 0.0031308 {
        return c * 12.92;
    }
    return 1.055 * pow(c, 1.0 / 2.4) - 0.055;
}

@fragment
fn fs_main(i: VSOut) -> @location(0) vec4<f32> {
    let linear = textureSample(tex, samp, i.uv);
    return vec4(
        srgb_oetf(linear.r),
        srgb_oetf(linear.g),
        srgb_oetf(linear.b),
        linear.a,
    );
}
