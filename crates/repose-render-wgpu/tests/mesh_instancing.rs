//! Headless probes for `VectorMeshInstanced`.
//!
//! Skips (does not fail) without a WGPU adapter. The contract these pin down:
//! one instanced node must be pixel-identical to the same placements issued as
//! separate `VectorMesh` nodes, and each instance must use its OWN transform.
//! The failure that matters is silent: a shared or mis-strided instance buffer
//! collapses every placement onto instance 0, which still "renders" and still
//! covers pixels, so a mere coverage assertion would pass.

use repose_core::{BlendMode, Color, PaintDesc, Scene, SceneNode, VectorMeshData, VectorVertex};
use repose_render_wgpu::offscreen::OffscreenRenderer;
use std::sync::Arc;

const SIZE: u32 = 64;

fn quad(size: f32) -> Arc<VectorMeshData> {
    let vertices: Arc<[VectorVertex]> = [
        ([0.0, 0.0], [0.0, 0.0]),
        ([size, 0.0], [1.0, 0.0]),
        ([size, size], [1.0, 1.0]),
        ([0.0, size], [0.0, 1.0]),
    ]
    .iter()
    .map(|(pos, uv)| VectorVertex {
        pos: *pos,
        color: [1.0, 1.0, 1.0, 1.0],
        uv: *uv,
    })
    .collect();
    Arc::new(VectorMeshData {
        vertices,
        indices: [0u32, 1, 2, 0, 2, 3].into(),
    })
}

fn render(nodes: Vec<SceneNode>) -> Option<Vec<u8>> {
    let mut off = match OffscreenRenderer::new_blocking(SIZE, SIZE, 1) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("SKIP: no WGPU adapter for mesh instancing probe ({e:#})");
            return None;
        }
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes,
    };
    Some(off.render_rgba(&scene, None).expect("render"))
}

/// Distinct placements, including one that is entirely off-screen so the
/// per-instance cull has something to drop. Node transforms are 2x3
/// `[m00, m01, m10, m11, tx, ty]`.
fn placements() -> Vec<[f32; 6]> {
    vec![
        [1.0, 0.0, 0.0, 1.0, 2.0, 2.0],
        [1.0, 0.0, 0.0, 1.0, 2.0, 20.0],
        [1.0, 0.0, 0.0, 1.0, 2.0, 38.0],
        [0.5, 0.0, 0.0, 0.5, 30.0, 4.0],
        [1.0, 0.0, 0.0, 1.0, 2.0, 900.0],
    ]
}

fn ink_pixels(px: &[u8]) -> usize {
    px.as_chunks::<4>().0.iter().filter(|p| p[3] > 8).count()
}

#[test]
fn instanced_matches_the_same_placements_drawn_one_by_one() {
    let mesh = quad(8.0);
    let paint = PaintDesc::Solid;

    let batched = render(vec![SceneNode::VectorMeshInstanced {
        mesh: mesh.clone(),
        transforms: placements(),
        paint,
        blend: BlendMode::Alpha,
    }]);
    let Some(batched) = batched else { return };

    let one_by_one: Vec<SceneNode> = placements()
        .into_iter()
        .map(|transform| SceneNode::VectorMesh {
            mesh: mesh.clone(),
            transform,
            paint,
            clip: None,
            blend: BlendMode::Alpha,
        })
        .collect();
    let Some(reference) = render(one_by_one) else {
        return;
    };

    // Guard the comparison itself: an all-transparent frame would satisfy
    // `assert_eq!` between two broken paths.
    assert!(
        ink_pixels(&reference) > 100,
        "reference scene must actually draw, else this comparison is vacuous"
    );
    assert_eq!(
        batched, reference,
        "one instanced node must be pixel-identical to the same placements issued separately"
    );
}

#[test]
fn every_instance_uses_its_own_transform() {
    let mesh = quad(8.0);
    let Some(px) = render(vec![SceneNode::VectorMeshInstanced {
        mesh,
        transforms: placements(),
        paint: PaintDesc::Solid,
        blend: BlendMode::Alpha,
    }]) else {
        return;
    };

    // Four placements are on-screen and none overlap: three 8x8 and one
    // half-scale 4x4. The fifth is at y=900 and must be culled. If instances
    // shared a slot they would all land on one another and this would
    // collapse to a single 8x8.
    assert_eq!(
        ink_pixels(&px),
        3 * 8 * 8 + 4 * 4,
        "each instance must draw at its own transform, and off-screen ones must be culled"
    );
}

#[test]
fn off_screen_instances_are_dropped_rather_than_drawn() {
    let mesh = quad(8.0);
    let visible = [1.0, 0.0, 0.0, 1.0, 2.0, 2.0];
    let offscreen = [1.0, 0.0, 0.0, 1.0, 2.0, 900.0];

    let only_visible = render(vec![SceneNode::VectorMeshInstanced {
        mesh: mesh.clone(),
        transforms: vec![visible],
        paint: PaintDesc::Solid,
        blend: BlendMode::Alpha,
    }]);
    let Some(only_visible) = only_visible else {
        return;
    };
    let with_offscreen = render(vec![SceneNode::VectorMeshInstanced {
        mesh,
        transforms: vec![visible, offscreen],
        paint: PaintDesc::Solid,
        blend: BlendMode::Alpha,
    }]);
    let Some(with_offscreen) = with_offscreen else {
        return;
    };

    assert_eq!(
        only_visible, with_offscreen,
        "an off-screen instance must not reach the frame at all"
    );
}
