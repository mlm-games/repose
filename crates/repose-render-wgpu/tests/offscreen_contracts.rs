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
