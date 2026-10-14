import super::common::{G, eval_brush_px};
import super::ellipse_sdf::{local_px, sdf_ellipse};

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color0: vec4<f32>,
    @location(1) color1: vec4<f32>,
    @location(2) xywh: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) grad: vec4<f32>,
    @location(5) pos_ndc: vec2<f32>,
    @location(6) @interpolate(flat) bits: u32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) color0: vec4<f32>,
    @location(2) color1: vec4<f32>,
    @location(3) fwd_mat: vec4<f32>,
    @location(4) grad: vec4<f32>,
    @location(5) bits: u32,
    @builtin(vertex_index) v: u32,
) -> VSOut {
    var positions = array<vec2<f32>, 6>(
        vec2(0.0, 0.0), vec2(1.0, 0.0), vec2(1.0, 1.0),
        vec2(0.0, 0.0), vec2(1.0, 1.0), vec2(0.0, 1.0)
    );
    let p = positions[v];
    let half = 0.5 * xywh.zw;
    let corner = (p * 2.0 - 1.0) * half;
    let rotated = vec2(
        fwd_mat.x * corner.x + fwd_mat.y * corner.y,
        fwd_mat.z * corner.x + fwd_mat.w * corner.y
    );
    let pos_ndc = xywh.xy + rotated;

    var out: VSOut;
    out.pos = vec4(pos_ndc, 0.0, 1.0);
    out.color0 = color0;
    out.color1 = color1;
    out.xywh = xywh;
    out.fwd_mat = fwd_mat;
    out.grad = grad;
    out.pos_ndc = pos_ndc;
    out.bits = bits;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let d = sdf_ellipse(in.pos_ndc, in.xywh, in.fwd_mat);
    let w = max(fwidth(d), 1e-5);
    let alpha_cov = 1.0 - smoothstep(-w, w, d);

    let local = local_px(in.pos_ndc, in.xywh, in.fwd_mat, G.ndc_to_px);
    let base = eval_brush_px(in.color0, in.color1, in.grad, in.bits, local);

    let a = base.a * alpha_cov;
    return vec4(base.rgb * a, a);
}