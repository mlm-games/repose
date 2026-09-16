//! Contract tests for [`OffscreenRenderer`](repose_render_wgpu::offscreen::OffscreenRenderer).
//!
//! Locks the documented guarantees: empty scenes read back the clear color,
//! `clear` overrides it, `ensure_size` resizes (no-op when unchanged),
//! zero sizes clamp to 1, and output is premultiplied sRGB.
//! Skips (does not fail) without a WGPU adapter.

use repose_core::{Brush, Color, Px, Rect, Scene, SceneNode};
use repose_render_wgpu::offscreen::OffscreenRenderer;

fn try_offscreen(w: u32, h: u32) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new_blocking(w, h, 1) {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIP: no WGPU adapter ({e:#})");
            None
        }
    }
}

fn empty_scene() -> Scene {
    Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: Vec::new(),
    }
}

#[test]
fn empty_scene_reads_back_clear_color() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    // Transparent clear: all bytes zero.
    let px = off.render_rgba(&empty_scene(), None).expect("render");
    assert_eq!(px.len(), 16 * 16 * 4);
    assert!(px.iter().all(|&b| b == 0));

    // Opaque clear from the scene.
    let scene = Scene {
        clear_color: Color::from_rgba(10, 20, 30, 255),
        nodes: Vec::new(),
    };
    let px = off.render_rgba(&scene, None).expect("render");
    assert_eq!(&px[0..4], &[10, 20, 30, 255]);
}

#[test]
fn clear_override_replaces_scene_clear() {
    let Some(mut off) = try_offscreen(8, 8) else {
        return;
    };
    let px = off
        .render_rgba(&empty_scene(), Some([0.0, 1.0, 0.0, 1.0]))
        .expect("render");
    assert_eq!(&px[0..4], &[0, 255, 0, 255]);
}

#[test]
fn ensure_size_resizes_and_noops() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    assert_eq!((off.width(), off.height()), (16, 16));
    off.ensure_size(16, 16).expect("noop resize");
    assert_eq!((off.width(), off.height()), (16, 16));
    off.ensure_size(32, 8).expect("resize");
    assert_eq!((off.width(), off.height()), (32, 8));
    let px = off.render_rgba(&empty_scene(), None).expect("render");
    assert_eq!(px.len(), 32 * 8 * 4);
}

#[test]
fn zero_size_clamps_to_one() {
    let Some(off) = try_offscreen(0, 0) else {
        return;
    };
    assert_eq!((off.width(), off.height()), (1, 1));
}

#[test]
fn opaque_content_reads_back_exact() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Rect {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 16.0,
                h: 16.0,
            },
            brush: Brush::Solid(Color::from_rgba(255, 0, 0, 255)),
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    assert_eq!(&px[0..4], &[255, 0, 0, 255]);
}

#[test]
fn translucent_content_is_premultiplied() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    // 50% white over transparent: premultiplied linear 0.5 encodes to
    // sRGB ~188, alpha 128.
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Rect {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                w: 16.0,
                h: 16.0,
            },
            brush: Brush::Solid(Color::from_rgba(255, 255, 255, 128)),
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    assert_eq!(px[3], 128);
    assert!(
        (180..=196).contains(&px[0]),
        "premultiplied white ~188, got {}",
        px[0]
    );
}

#[test]
fn brush_border_matches_solid_border() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let rect = Rect {
        x: 2.0,
        y: 2.0,
        w: 12.0,
        h: 12.0,
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Border {
            rect,
            brush: Brush::Solid(Color::from_rgba(255, 0, 0, 255)),
            width: Px(2.0),
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let at = |x: usize, y: usize| (y * 16 + x) * 4;
    // Top edge of the ring is red (AA may soften the outermost row).
    let i = at(8, 3);
    assert!(px[i] > 200 && px[i + 3] > 200, "top edge should be red, got {:?}", &px[i..i + 4]);
    assert_eq!(px[i + 1], 0);
    assert_eq!(px[i + 2], 0);
    // Interior of the ring is untouched.
    let i = at(8, 8);
    assert_eq!(&px[i..i + 4], &[0, 0, 0, 0]);
}

#[test]
fn linear_border_blends_endpoint_colors() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let rect = Rect {
        x: 2.0,
        y: 2.0,
        w: 12.0,
        h: 12.0,
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Border {
            rect,
            brush: Brush::Linear {
                start: Vec2 { x: 0.0, y: 0.0 },
                end: Vec2 { x: 12.0, y: 0.0 },
                start_color: Color::from_rgba(255, 0, 0, 255),
                end_color: Color::from_rgba(0, 0, 255, 255),
            },
            width: Px(2.0),
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let at = |x: usize, y: usize| (y * 16 + x) * 4;
    // Left edge leans red, right edge leans blue.
    let l = at(2, 8);
    let r = at(13, 8);
    assert!(px[l] > 128, "left edge should be red, got {:?}", &px[l..l + 4]);
    assert!(px[l + 2] < 128);
    assert!(px[r + 2] > 128, "right edge should be blue, got {:?}", &px[r..r + 4]);
    assert!(px[r] < 128);
}

#[test]
fn radial_border_center_matches_start_color() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::EllipseBorder {
            rect: Rect {
                x: 2.0,
                y: 2.0,
                w: 12.0,
                h: 12.0,
            },
            brush: Brush::Radial {
                center: Vec2 { x: 6.0, y: 6.0 },
                radius: 24.0,
                start_color: Color::from_rgba(255, 0, 0, 255),
                end_color: Color::from_rgba(0, 0, 255, 255),
            },
            width: Px(2.0),
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    // Any strongly-painted ring pixel near the center-x column should read
    // red-dominant: with radius 24 the whole 12px shape sits at t < 0.35.
    let mut found = false;
    for y in 2..14 {
        let i = (y * 16 + 8) * 4;
        if px[i + 3] > 200 {
            assert!(
                px[i] > px[i + 2],
                "expected red-dominant at y={y}, got {:?}",
                &px[i..i + 4]
            );
            found = true;
        }
    }
    assert!(found, "expected opaque ring pixels in the center column");
}

#[test]
fn sweep_arc_renders_without_panic() {
    use repose_core::{StrokeCap, Vec2};
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Arc {
            rect: Rect {
                x: 2.0,
                y: 2.0,
                w: 12.0,
                h: 12.0,
            },
            start_angle: 0.0,
            sweep_angle: std::f32::consts::TAU,
            stroke_width: Px(2.0),
            brush: Brush::Sweep {
                center: Vec2 { x: 6.0, y: 6.0 },
                start_color: Color::from_rgba(255, 0, 0, 255),
                end_color: Color::from_rgba(0, 0, 255, 255),
            },
            cap: StrokeCap::Butt,
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    assert!(px.iter().any(|&b| b != 0), "sweep arc should paint pixels");
}
