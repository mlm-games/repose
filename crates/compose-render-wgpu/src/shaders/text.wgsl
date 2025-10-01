struct VSOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) color: vec4<f32>,
  @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(
  @location(0) xywh: vec4<f32>,
  @location(1) uv_rect: vec4<f32>,
  @location(2) color: vec4<f32>,
  @builtin(vertex_index) v: u32
) -> VSOut {
  var positions = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
  );
  var uvs = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
  );

  let p = positions[v];
  let uv_lerp = uvs[v];

  let pos = xywh.xy + p * xywh.zw;

  // Same simplistic 1280x800 mapping
  let w: f32 = 1280.0;
  let h: f32 = 800.0;
  let ndc = vec2((pos.x / w) * 2.0 - 1.0, 1.0 - (pos.y / h) * 2.0);

  var out: VSOut;
  out.pos = vec4(ndc, 0.0, 1.0);
  out.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
  out.color = color;
  return out;
}

@group(0) @binding(0) var glyph_tex: texture_2d<f32>;
@group(0) @binding(1) var glyph_sampler: sampler;

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
  let a = textureSample(glyph_tex, glyph_sampler, in.uv).r;
  return vec4(in.color.rgb, a * in.color.a);
}
