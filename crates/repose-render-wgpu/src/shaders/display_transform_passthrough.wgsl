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
        vec2(0.0, 1.0), vec2(2.0, 1.0), vec2(0.0, -1.0),
    );
    var o: VSOut;
    o.pos = vec4(pos[v], 0.0, 1.0);
    o.uv = uv[v];
    return o;
}

@fragment
fn fs_main(i: VSOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, i.uv);
}
