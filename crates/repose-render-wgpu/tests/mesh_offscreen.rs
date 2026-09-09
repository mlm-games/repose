//! Headless offscreen probes for `VectorMesh` rendering.
//!
//! Skips (does not fail) without a WGPU adapter, so adapter-less CI stays
//! green. Catches silent mesh-pipeline regressions: a mesh scene must produce
//! covered pixels.

use repose_core::{
    BlendMode, Brush, Color, PaintDesc, Px, Rect, Scene, SceneNode, Transform, VectorMeshData,
    VectorVertex,
};
use repose_render_wgpu::offscreen::OffscreenRenderer;
use std::sync::Arc;

fn quad(x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]) -> Arc<VectorMeshData> {
    let vertices: Arc<[VectorVertex]> = [
        ([x0, y0], [0.0, 0.0]),
        ([x1, y0], [1.0, 0.0]),
        ([x1, y1], [1.0, 1.0]),
        ([x0, y1], [0.0, 1.0]),
    ]
    .iter()
    .map(|(pos, uv)| VectorVertex {
        pos: *pos,
        color,
        uv: *uv,
    })
    .collect();
    Arc::new(VectorMeshData {
        vertices,
        indices: [0u32, 1, 2, 0, 2, 3].into(),
    })
}

fn try_offscreen(w: u32, h: u32) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new_blocking(w, h, 1) {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIP: no WGPU adapter for mesh offscreen probe ({e:#})");
            None
        }
    }
}

fn covered_of(nodes: Vec<SceneNode>) -> Option<Vec<u8>> {
    let Some(mut off) = try_offscreen(64, 64) else {
        return None;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes,
    };
    Some(off.render_rgba(&scene, None).expect("render"))
}

fn ink_bbox(px: &[u8]) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (64u32, 64u32, 0u32, 0u32);
    for (i, p) in px.chunks_exact(4).enumerate() {
        if p[3] > 8 {
            let (x, y) = ((i % 64) as u32, (i / 64) as u32);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x1 >= x0).then_some((x0, y0, x1, y1))
}

fn white_rect(x: f32, y: f32, w: f32, h: f32) -> SceneNode {
    SceneNode::Rect {
        rect: Rect { x, y, w, h },
        brush: repose_core::Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
        radius: [Px::ZERO; 4],
    }
}

#[test]
fn bisect_layer_coordinate_space() {
    // Layers expect layer-local children (the established producer
    // contract: repose-ui pushes a `-rect` shift after `BeginLayer`).
    // A world-space child double-offsets to (20,20); a layer-local child
    // composites exactly at the layer origin (10,10).
    for (label, cx, cy, expect) in [
        ("world-child", 10.0f32, 10.0f32, (20, 20, 39, 39)),
        ("local-child", 0.0f32, 0.0f32, (10, 10, 29, 29)),
    ] {
        let px = covered_of(vec![
            SceneNode::BeginLayer {
                rect: Rect {
                    x: 10.0,
                    y: 10.0,
                    w: 44.0,
                    h: 44.0,
                },
                layer_id: 7,
                alpha: 1.0,
                blur_radius_x: Px(0.0),
                blur_radius_y: Px(0.0),
                rectangle_edge: true,
            },
            white_rect(cx, cy, 20.0, 20.0),
            SceneNode::EndLayer { layer_id: 7 },
        ]);
        let bbox = px.as_deref().and_then(ink_bbox);
        assert_eq!(bbox, Some(expect), "{label} landed wrong");
    }
}

#[test]
fn vector_mesh_covers_pixels_offscreen() {
    let Some(mut off) = try_offscreen(64, 64) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::VectorMesh {
            mesh: quad(10.0, 10.0, 54.0, 54.0, [1.0, 1.0, 1.0, 1.0]),
            // Documented 2x3 identity `[m00, m01, m10, m11, tx, ty]`; the
            // transposed `[1, 0, 0, 0, 1, 0]` collapses to a line (zero
            // pixels) and must never be emitted by producers.
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            paint: PaintDesc::Solid,
            clip: None,
            blend: BlendMode::Alpha,
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let covered = px.chunks_exact(4).filter(|p| p[3] > 8).count();
    assert!(covered > 1000, "mesh must cover pixels, got {covered}");
    // Centre of the quad must be opaque white.
    let c = (32 * 64 + 32) * 4;
    assert!(px[c + 3] > 200, "centre alpha {}", px[c + 3]);
}

/// Tilt about the horizontal axis through the rect centre (focal 200px),
/// matching the hand computation in `geometry::projective_tests`.
fn tilt_about_y32() -> Transform {
    let (cy, f) = (32.0f32, 200.0f32);
    let th = 0.3f32;
    let (s, c) = (th.sin(), th.cos());
    Transform::from_projective_rows(
        [1.0, 0.0, 0.0],
        [0.0, c, cy * (1.0 - c)],
        [0.0, s / f, 1.0 - cy * s / f],
    )
}

fn tilt_scene() -> Vec<SceneNode> {
    vec![
        SceneNode::PushTransform {
            transform: tilt_about_y32(),
        },
        SceneNode::Rect {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                w: 44.0,
                h: 44.0,
            },
            brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
            radius: [Px::ZERO; 4],
        },
        SceneNode::PopTransform,
    ]
}

#[test]
fn perspective_flatten_projects_rect() {
    let Some(px) = covered_of(tilt_scene()) else {
        return;
    };
    let (x0, y0, x1, y1) = ink_bbox(&px).expect("tilted rect must cover pixels");
    // Hand-computed projection (see `tilt_about_y32`): x spans ~9.7..55.8
    // (x scales with the y-dependent w), y spans ~11.4..51.4. ±2px for
    // rasterization.
    assert!(
        (8..=12).contains(&x0) && (54..=58).contains(&x1),
        "projected x-range wrong: {x0}..{x1}"
    );
    assert!(
        (9..=13).contains(&y0) && (49..=53).contains(&y1),
        "projected y-range wrong: {y0}..{y1}"
    );
}

#[test]
fn affine_push_transform_is_unaffected() {
    // Control: the same rect under a plain translate lands exactly.
    let px = covered_of(vec![
        SceneNode::PushTransform {
            transform: Transform::translate(5.0, 7.0),
        },
        SceneNode::Rect {
            rect: Rect {
                x: 10.0,
                y: 10.0,
                w: 44.0,
                h: 44.0,
            },
            brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
            radius: [Px::ZERO; 4],
        },
        SceneNode::PopTransform,
    ])
    .expect("adapter");
    assert_eq!(
        ink_bbox(&px),
        Some((15, 17, 58, 60)),
        "affine path must stay exact"
    );
}

fn clip_scene(op: repose_core::ClipOp) -> Vec<SceneNode> {
    vec![
        SceneNode::PushVectorClip {
            mesh: quad(20.0, 20.0, 44.0, 44.0, [1.0, 1.0, 1.0, 1.0]),
            op,
        },
        white_rect(0.0, 0.0, 64.0, 64.0),
        SceneNode::PopVectorClip,
    ]
}

#[test]
fn vector_clip_intersect_masks_rect() {
    let px = covered_of(clip_scene(repose_core::ClipOp::Intersect)).expect("adapter");
    // Only the 24x24 mask area survives.
    assert_eq!(ink_bbox(&px), Some((20, 20, 43, 43)));
}

#[test]
fn vector_clip_difference_cuts_rect() {
    let px = covered_of(clip_scene(repose_core::ClipOp::Difference)).expect("adapter");
    let ink = ink_bbox(&px).expect("inverse clip must leave content");
    // Full frame minus the mask: edges survive, centre is empty.
    assert_eq!((ink.0, ink.1), (0, 0));
    assert_eq!((ink.2, ink.3), (63, 63));
    let centre = (32 * 64 + 32) * 4;
    assert_eq!(px[centre + 3], 0, "mask interior must be cut out");
    assert!(px[3] > 200, "frame corner must survive");
}

#[test]
fn coverage_tile_composites_tinted() {
    let Some(mut off) = try_offscreen(64, 64) else {
        return;
    };
    // 10x10 full-coverage tile composited red at (5,5).
    let data = vec![255u8; 10 * 10];
    let h = off.renderer_mut().register_coverage_a8(10, 10, &data);
    assert_ne!(h, 0, "registration must return a handle");
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Coverage {
            rect: Rect {
                x: 5.0,
                y: 5.0,
                w: 10.0,
                h: 10.0,
            },
            handle: h,
            color: Color::from_rgba(255, 0, 0, 255),
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let at = |x: usize, y: usize| (y * 64 + x) * 4;
    let c = at(10, 10);
    assert!(px[c + 3] > 200, "tile interior alpha {}", px[c + 3]);
    assert!(px[c] > 200, "tile interior red {}", px[c]);
    assert_eq!(px[at(0, 0) + 3], 0, "outside tile must be empty");
    assert_eq!(px[at(20, 20) + 3], 0, "outside tile must be empty");

    // Half-coverage tile: alpha must track coverage (~128).
    let half = vec![128u8; 8 * 8];
    let h2 = off.renderer_mut().register_coverage_a8(8, 8, &half);
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Coverage {
            rect: Rect {
                x: 30.0,
                y: 30.0,
                w: 8.0,
                h: 8.0,
            },
            handle: h2,
            color: Color::from_rgba(255, 255, 255, 255),
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let c = at(34, 34);
    assert!(
        (100..=160).contains(&px[c + 3]),
        "half coverage alpha {}",
        px[c + 3]
    );

    // Unknown handles are skipped, not fatal.
    off.renderer_mut().remove_coverage(h2);
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Coverage {
            rect: Rect {
                x: 30.0,
                y: 30.0,
                w: 8.0,
                h: 8.0,
            },
            handle: h2,
            color: Color::from_rgba(255, 255, 255, 255),
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    assert_eq!(px[at(34, 34) + 3], 0, "removed tile must not draw");
}

fn covered_of_sized(w: u32, h: u32, nodes: Vec<SceneNode>) -> Option<Vec<u8>> {
    let Some(mut off) = try_offscreen(w, h) else {
        return None;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes,
    };
    Some(off.render_rgba(&scene, None).expect("render"))
}

fn ink_bbox_wh(px: &[u8], w: usize) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w as u32, 100000u32, 0u32, 0u32);
    for (i, p) in px.chunks_exact(4).enumerate() {
        if p[3] > 8 {
            let (x, y) = ((i % w) as u32, (i / w) as u32);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    (x1 >= x0).then_some((x0, y0, x1, y1))
}

#[test]
fn flatten_with_tiny_tilt() {
    // Near-identity perspective: output must sit approximately where the
    // rect is (systematic placement check, independent of map math).
    let mut t = Transform::identity();
    t.perspective = [0.0, 0.0001, 1.0];
    let px = covered_of_sized(
        640,
        360,
        vec![
            SceneNode::PushTransform { transform: t },
            SceneNode::Rect {
                rect: Rect {
                    x: 214.0,
                    y: 142.0,
                    w: 211.0,
                    h: 75.0,
                },
                brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
                radius: [Px::ZERO; 4],
            },
            SceneNode::PopTransform,
        ],
    );
    eprintln!(
        "tiny-tilt bbox={:?}",
        px.as_deref().and_then(|p| ink_bbox_wh(p, 640))
    );
}

#[test]
fn flatten_places_content_by_map() {
    // Near-affine perspective: the map rules placement. With an (almost)
    // identity map the rect lands exactly; with a gentle tilt it lands on
    // the hand-computed projection (±2px rasterization).
    let mut t = Transform::identity();
    t.perspective = [0.0, 1e-9, 1.0];
    let px = covered_of_sized(
        640,
        360,
        vec![
            SceneNode::PushTransform { transform: t },
            SceneNode::Rect {
                rect: Rect {
                    x: 214.0,
                    y: 142.0,
                    w: 211.0,
                    h: 75.0,
                },
                brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
                radius: [Px::ZERO; 4],
            },
            SceneNode::PopTransform,
        ],
    );
    assert_eq!(
        px.as_deref().and_then(|p| ink_bbox_wh(p, 640)),
        Some((214, 142, 424, 216))
    );

    let mut t = Transform::identity();
    t.perspective = [0.0, 0.0001, 1.0];
    for (rect, expect) in [
        (
            Rect {
                x: 10.0,
                y: 10.0,
                w: 44.0,
                h: 44.0,
            },
            (10, 10, 53, 53),
        ),
        (
            Rect {
                x: 500.0,
                y: 280.0,
                w: 60.0,
                h: 40.0,
            },
            (485, 272, 544, 309),
        ),
    ] {
        let px = covered_of_sized(
            640,
            360,
            vec![
                SceneNode::PushTransform { transform: t },
                SceneNode::Rect {
                    rect,
                    brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
                    radius: [Px::ZERO; 4],
                },
                SceneNode::PopTransform,
            ],
        );
        let got = px.as_deref().and_then(|p| ink_bbox_wh(p, 640));
        let (x0, y0, x1, y1) = got.expect("tilted rect must cover pixels");
        let (ex0, ey0, ex1, ey1) = expect;
        assert!(
            x0.abs_diff(ex0) <= 2
                && y0.abs_diff(ey0) <= 2
                && x1.abs_diff(ex1) <= 2
                && y1.abs_diff(ey1) <= 2,
            "tilted rect at {got:?}, want {expect:?}"
        );
    }
}

#[test]
fn flatten_with_ass_frx45_map() {
    // Exact transform ass-rs emits for `\frx45` (from the parity debug
    // dump): the rect must land where the map says, center ~(320,179).
    let t = Transform {
        translate_x: 130.33391,
        translate_y: 126.03361,
        scale_x: 0.87359774,
        scale_y: 0.61428744,
        rotate: 0.5082492,
        shear_x: -0.79221636,
        shear_y: -0.5570625,
        origin_x: 0.5,
        origin_y: 0.5,
        perspective: [0.0, -0.0022627416, 1.4072934],
    };
    let px = covered_of_sized(
        640,
        360,
        vec![
            SceneNode::PushTransform { transform: t },
            SceneNode::Rect {
                rect: Rect {
                    x: 214.0,
                    y: 142.0,
                    w: 211.0,
                    h: 75.0,
                },
                brush: Brush::Solid(Color::from_rgba(255, 255, 255, 255)),
                radius: [Px::ZERO; 4],
            },
            SceneNode::PopTransform,
        ],
    );
    // Hand-computed projection of the rect through the map. Note the
    // left edge slants (204 at the bottom vs 222 at the top): the libass
    // `offs` coupling shears x with y, exactly as measured on libass
    // output itself.
    assert_eq!(
        px.as_deref().and_then(|p| ink_bbox_wh(p, 640)),
        Some((204, 155, 434, 208))
    );
}
