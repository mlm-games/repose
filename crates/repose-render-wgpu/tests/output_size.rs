//! The framebuffer has to stay on a single uniform scale relative to the
//! viewport, because that scale is the only thing mapping input coordinates
//! onto layout coordinates.
//!
//! Clamping each axis with `min(max)` still satisfies every per-axis limit
//! check, so it slips through review, but it leaves width and height on
//! different factors: the scene projects against one aspect ratio while a
//! touch is resolved against another, which lands proportionally further from
//! the origin the further out you press.

use repose_render_wgpu::clamp_output_size;

#[test]
fn sizes_within_the_limit_are_untouched() {
    assert_eq!(clamp_output_size(1080, 2296, 4096), (1080, 2296));
    assert_eq!(clamp_output_size(2048, 2048, 2048), (2048, 2048));
    assert_eq!(clamp_output_size(0, 0, 2048), (0, 0));
    assert_eq!(clamp_output_size(0, 2296, 2048), (0, 0));
}

#[test]
fn oversized_axes_shrink_by_one_shared_factor() {
    for &(w, h, max) in &[
        (1080u32, 2296u32, 2048u32),
        (3840, 2160, 2048),
        (1000, 4000, 1024),
    ] {
        let (cw, ch) = clamp_output_size(w, h, max);
        assert!(cw <= max && ch <= max, "{cw}x{ch} exceeds {max}");
        let fx = cw as f64 / w as f64;
        let fy = ch as f64 / h as f64;
        assert!(
            (fx - fy).abs() < 1.0 / max as f64,
            "{w}x{h} clamped to {cw}x{ch}: x scaled by {fx}, y by {fy}"
        );
    }
}

#[test]
fn aspect_ratio_survives() {
    for &(w, h) in &[(1080u32, 2296u32), (3840, 2160), (1000, 1), (5, 4093)] {
        let (cw, ch) = clamp_output_size(w, h, 2048);
        let before = w as f64 / h as f64;
        let after = cw as f64 / ch as f64;
        assert!(
            (before - after).abs() < 0.01,
            "{w}x{h} clamped to {cw}x{ch}"
        );
    }
}
