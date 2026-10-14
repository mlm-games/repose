import super::common::{G, eval_brush_px, sdf_round_box_px};

struct VSOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color0: vec4<f32>,
    @location(1) color1: vec4<f32>,
    @location(2) xywh: vec4<f32>,
    @location(3) radii: vec4<f32>,
    @location(4) fwd_mat: vec4<f32>,
    @location(5) grad: vec4<f32>,
    @location(6) pos_ndc: vec2<f32>,
    @location(7) @interpolate(flat) stroke_px: f32,
    @location(8) @interpolate(flat) bits: u32,
};

@vertex
fn vs_main(
    @location(0) xywh: vec4<f32>,
    @location(1) radii: vec4<f32>,
    @location(2) color0: vec4<f32>,
    @location(3) color1: vec4<f32>,
    @location(4) fwd_mat: vec4<f32>,
    @location(5) grad: vec4<f32>,
    @location(6) bits: u32,
    @location(7) stroke_px: f32,
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
    out.radii = radii;
    out.fwd_mat = fwd_mat;
    out.grad = grad;
    out.pos_ndc = pos_ndc;
    out.bits = bits;
    out.stroke_px = stroke_px;
    return out;
}

@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let center_ndc = in.xywh.xy;
    let p_px = (in.pos_ndc - center_ndc) * G.ndc_to_px;
    let half_px = 0.5 * in.xywh.zw * G.ndc_to_px;

    let det = max(
        in.fwd_mat.x * in.fwd_mat.w - in.fwd_mat.y * in.fwd_mat.z,
        1e-6,
    );
    let unrotated_px = vec2(
        (in.fwd_mat.w * p_px.x - in.fwd_mat.y * p_px.y) / det,
        (-in.fwd_mat.z * p_px.x + in.fwd_mat.x * p_px.y) / det,
    );

    let stroke = max(in.stroke_px, 0.0);
    let radii_clamped = max(in.radii, vec4<f32>(0.0));

    let d_outer = sdf_round_box_px(unrotated_px, half_px, radii_clamped);

    let half_inner = max(half_px - vec2<f32>(stroke, stroke), vec2<f32>(0.0));
    let r_inner = max(radii_clamped - vec4<f32>(stroke), vec4<f32>(0.0));
    let d_inner = sdf_round_box_px(unrotated_px, half_inner, r_inner);

    let w0 = max(fwidth(d_outer), 1e-4);
    let w1 = max(fwidth(d_inner), 1e-4);

    let taps = array<vec2<f32>, 4>(
        vec2(-0.25, -0.25),
        vec2(0.25, -0.25),
        vec2(-0.25, 0.25),
        vec2(0.25, 0.25),
    );
    var ring_sum = 0.0;
    for (var i = 0; i < 4; i++) {
        let p_tap = unrotated_px + taps[i] * vec2(w0, w1);
        let d_o = sdf_round_box_px(p_tap, half_px, radii_clamped);
        let d_i = sdf_round_box_px(p_tap, half_inner, r_inner);
        let c_o = 1.0 - smoothstep(-w0, w0, d_o);
        let c_i = 1.0 - smoothstep(-w1, w1, d_i);
        ring_sum += max(c_o - c_i, 0.0);
    }
    let ring = ring_sum * 0.25;

    // Brush coordinates are shape-local: recenter so (0,0) is top-left.
    // `half_px` is scaled but `unrotated_px` is un-scaled by the inverse
    // fwd_mat: re-apply the scale magnitude so both live in the same space.
    let scale_px = vec2(length(vec2(in.fwd_mat.x, in.fwd_mat.z)), length(vec2(in.fwd_mat.y, in.fwd_mat.w)));
    let local_px = vec2(
        unrotated_px.x * scale_px.x,
        -unrotated_px.y * scale_px.y,
    ) + half_px;
    let base = eval_brush_px(in.color0, in.color1, in.grad, in.bits, local_px);

    let a = base.a * ring;
    return vec4(base.rgb * a, a);
}