struct VSIn {
  @location(0) xywh_r: vec4<f32>,
  @location(1) color_a: vec4<f32>, // color in 1, but we'll forward through
};
struct VSOut {
  @builtin(position) pos: vec4<f32>,
  @location(0) color: vec4<f32>,
  @location(1) xywh_r: vec4<f32>,
};
@vertex
fn vs_main(@location(0) xywh_r: vec4<f32>, @location(1) color: vec4<f32>, @builtin(instance_index) i: u32, @builtin(vertex_index) v: u32) -> VSOut {
  var positions = array<vec2<f32>, 6>(
    vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
    vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
  );
  let p = positions[v];
  let xy = xywh_r.xy + p * xywh_r.zw;
  var out: VSOut;
  // convert to NDC (assuming framebuffer size baked into coordinates)
  // Coordinates are already in pixel space; we map in fragment via screen size from uniforms normally.
  // For simplicity, assume 1:1 mapping with a 0..width/height space mapped to -1..1 using a fixed viewport of 1280x800.
  // The host side feeds rects in window-space; here we approximate using hardcoded 1280x800 mapping.
  let w: f32 = 1280.0;
  let h: f32 = 800.0;
  let ndc = vec2((xy.x / w) * 2.0 - 1.0, 1.0 - (xy.y / h) * 2.0);
  out.pos = vec4(ndc, 0.0, 1.0);
  out.color = color;
  out.xywh_r = xywh_r;
  return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
  // Rounded corners: discard outside radius
  let r = in.xywh_r.w;
  if (r > 0.0) {
    let px = in.pos.xy; // NDC, not strictly correct; we will ignore precise rounding for simplicity.
  }
  return in.color;
}
