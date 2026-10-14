fn unrotated_rel(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> vec2<f32> {
    let center = xywh.xy;
    let rel = pos_ndc - center;
    let det = max(fwd_mat.x * fwd_mat.w - fwd_mat.y * fwd_mat.z, 1e-6);
    return vec2(
        (fwd_mat.w * rel.x - fwd_mat.y * rel.y) / det,
        (-fwd_mat.z * rel.x + fwd_mat.x * rel.y) / det,
    );
}

fn sdf_ellipse(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>) -> f32 {
    let radii = 0.5 * xywh.zw;
    let p = unrotated_rel(pos_ndc, xywh, fwd_mat) / radii;
    return length(p) - 1.0;
}

// Shape-local px with (0,0) at the shape top-left. `unrotated_rel` is in
// NDC, `xywh.zw` is the shape size in px: convert the offset, recenter.
fn local_px(pos_ndc: vec2<f32>, xywh: vec4<f32>, fwd_mat: vec4<f32>, ndc_to_px: vec2<f32>) -> vec2<f32> {
    let rel = unrotated_rel(pos_ndc, xywh, fwd_mat);
    return vec2(rel.x, -rel.y) * ndc_to_px + 0.5 * xywh.zw * ndc_to_px;
}