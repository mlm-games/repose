struct Globals {
    ndc_to_px: vec2<f32>,
    _pad: vec2<f32>,
};
@group(0) @binding(0) var<uniform> G: Globals;

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) @interpolate(flat) uv_rect: vec4<f32>,
    @location(2) xywh: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) pos_ndc: vec2<f32>,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) uv_rect: vec4<f32>,
    @location(2) color: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @builtin(vertex_index) v: u32
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );

    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    // Axis-aligned quad - no vertex rotation.
    // Rotation is handled in the fragment shader via UV rotation.
    let pos_ndc = xywh.xy + corner;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.uv_rect = uv_rect;
    out.xywh = xywh;
    out.fwd_mat = fwd_mat;
    out.color = color;
    out.pos_ndc = pos_ndc;
    return out;
}

@group(1) @binding(0) var glyph_tex: texture_2d<f32>;
@group(1) @binding(1) var glyph_sampler: sampler;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let half = 0.5 * in.xywh.zw;
    let rel = (in.pos_ndc - in.xywh.xy) / half;
    let det = max(
        in.fwd_mat.x * in.fwd_mat.w - in.fwd_mat.y * in.fwd_mat.z,
        1e-6,
    );
    let unrotated_rel = vec2(
        (in.fwd_mat.w * rel.x - in.fwd_mat.y * rel.y) / det,
        (-in.fwd_mat.z * rel.x + in.fwd_mat.x * rel.y) / det,
    );
    let norm_uv = (unrotated_rel + 1.0) * 0.5;
    if any(norm_uv < vec2(0.0)) || any(norm_uv > vec2(1.0)) {
        discard;
    }
    let atlas_uv = mix(in.uv_rect.xy, in.uv_rect.zw, norm_uv);
    let cov = textureSample(glyph_tex, glyph_sampler, atlas_uv).r;
    let a = cov * in.color.a;
    return vec4(in.color.rgb * a, a);
}
