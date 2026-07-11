struct VSOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) uv: vec2<f32>,
  @location(1) tint: vec4<f32>,
  @location(2) full_range: f32,
};

@vertex
fn vs_main(
  @location(0) xywh: vec4<f32>,
  @location(1) uv_rect: vec4<f32>,
  @location(2) tint: vec4<f32>,
  @location(3) full_range: f32,
  @location(4) sin_cos: vec2<f32>,
  @builtin(vertex_index) v: u32
) -> VSOut {
  var positions = array<vec2<f32>, 6>(
    vec2(0.0,0.0), vec2(1.0,0.0), vec2(1.0,1.0),
    vec2(0.0,0.0), vec2(1.0,1.0), vec2(0.0,1.0)
  );
  var uvs = array<vec2<f32>, 6>(
    vec2(0.0,0.0), vec2(1.0,0.0), vec2(1.0,1.0),
    vec2(0.0,0.0), vec2(1.0,1.0), vec2(0.0,1.0)
  );
  let p = positions[v];
  let uv_lerp = uvs[v];
  let half = 0.5 * xywh.zw;
  let corner = (p * 2.0 - 1.0) * half;
  let rotated = vec2(corner.x * sin_cos.x - corner.y * sin_cos.y, corner.x * sin_cos.y + corner.y * sin_cos.x);
  var o: VSOut;
  o.pos = vec4(xywh.xy + rotated, 0.0, 1.0);
  o.uv = mix(uv_rect.xy, uv_rect.zw, uv_lerp);
  o.tint = tint;
  o.full_range = full_range;
  return o;
}

@group(1) @binding(0) var tex_y: texture_2d<f32>;
@group(1) @binding(1) var tex_uv: texture_2d<f32>;
@group(1) @binding(2) var samp: sampler;

fn yuv_to_rgb(y: f32, u: f32, v: f32, full_range: bool) -> vec3<f32> {
  var yy = y;
  var uu = u - 0.5;
  var vv = v - 0.5;

  if (!full_range) {
    yy = clamp((y - (16.0 / 255.0)) * (255.0 / 219.0), 0.0, 1.0);
    uu = (u - (128.0 / 255.0)) * (255.0 / 224.0);
    vv = (v - (128.0 / 255.0)) * (255.0 / 224.0);
  }

  let r = yy + 1.5748 * vv;
  let g = yy - 0.1873 * uu - 0.4681 * vv;
  let b = yy + 1.8556 * uu;
  return clamp(vec3(r,g,b), vec3(0.0), vec3(1.0));
}

@fragment
fn fs_main(i: VSOut) -> @location(0) vec4<f32> {
  let y = textureSample(tex_y, samp, i.uv).r;
  let uv = textureSample(tex_uv, samp, i.uv).rg;
  let rgb = yuv_to_rgb(y, uv.r, uv.g, i.full_range > 0.5);

  let a = i.tint.a;
  let out_rgb = rgb * i.tint.rgb * a;
  return vec4(out_rgb, a);
}
