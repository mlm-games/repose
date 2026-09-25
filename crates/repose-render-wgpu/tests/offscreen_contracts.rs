//! Contract tests for [`OffscreenRenderer`](repose_render_wgpu::offscreen::OffscreenRenderer).
//!
//! Locks the documented guarantees: empty scenes read back the clear color,
//! `clear` overrides it, `ensure_size` resizes (no-op when unchanged),
//! zero sizes clamp to 1, and output is premultiplied sRGB.
//! Skips (does not fail) without a WGPU adapter.

use repose_core::{
    Brush, ClipOp, Color, ImageFilter, ImageFit, ImageSourceRect, Px, Rect, Scene, SceneNode,
    Transform,
};
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
fn transformed_graphics_layer_replays_clips_in_layer_coordinates() {
    fn red_bounds(px: &[u8], width: usize, height: usize) -> Option<(usize, usize, usize, usize)> {
        let mut bounds = (width, height, 0, 0);
        let mut found = false;
        for y in 0..height {
            for x in 0..width {
                let i = (y * width + x) * 4;
                if px[i] > 180 && px[i] > px[i + 1] * 2 && px[i] > px[i + 2] * 2 {
                    found = true;
                    bounds.0 = bounds.0.min(x);
                    bounds.1 = bounds.1.min(y);
                    bounds.2 = bounds.2.max(x);
                    bounds.3 = bounds.3.max(y);
                }
            }
        }
        found.then_some(bounds)
    }

    let parent_transform = Transform {
        translate_x: 60.0,
        translate_y: 40.0,
        scale_x: 0.8,
        scale_y: 0.8,
        rotate: 0.0,
        shear_x: 0.0,
        shear_y: 0.0,
        origin_x: 0.0,
        origin_y: 0.0,
        perspective: [0.0, 0.0, 1.0],
    };
    let rect = Rect {
        x: 300.0,
        y: 200.0,
        w: 200.0,
        h: 120.0,
    };
    let brush = Brush::Solid(Color::from_rgb(255, 0, 0));
    let base_nodes = vec![
        SceneNode::PushTransform {
            transform: parent_transform,
        },
        SceneNode::PushClip {
            rect,
            radius: [Px(8.0); 4],
            op: ClipOp::Intersect,
        },
        SceneNode::Rect {
            rect,
            brush,
            radius: [Px(8.0); 4],
        },
        SceneNode::PopClip,
        SceneNode::PopTransform,
    ];
    let mut layer_nodes = base_nodes.clone();
    layer_nodes.insert(
        2,
        SceneNode::BeginLayer {
            rect,
            layer_id: 0,
            alpha: 1.0,
            blur_radius_x: Px::ZERO,
            blur_radius_y: Px::ZERO,
            rectangle_edge: true,
        },
    );
    layer_nodes.insert(
        3,
        SceneNode::PushTransform {
            transform: Transform::translate(-rect.x, -rect.y),
        },
    );
    layer_nodes.insert(5, SceneNode::PopTransform);
    layer_nodes.insert(6, SceneNode::EndLayer { layer_id: 0 });
    let base = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: base_nodes,
    };
    let layered = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: layer_nodes,
    };
    let Some(mut off) = try_offscreen(800, 600) else {
        return;
    };
    let base_px = off.render_rgba(&base, None).expect("render base");
    let layered_px = off.render_rgba(&layered, None).expect("render layer");
    assert_eq!(
        red_bounds(&layered_px, 800, 600),
        red_bounds(&base_px, 800, 600)
    );
}

#[test]
fn graphics_layer_shadow_draws_before_the_layer_composite() {
    let rect = Rect {
        x: 300.0,
        y: 200.0,
        w: 200.0,
        h: 120.0,
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![
            SceneNode::PushClip {
                rect,
                radius: [Px(8.0); 4],
                op: ClipOp::Intersect,
            },
            SceneNode::BeginLayer {
                rect,
                layer_id: 0,
                alpha: 1.0,
                blur_radius_x: Px::ZERO,
                blur_radius_y: Px::ZERO,
                rectangle_edge: true,
            },
            SceneNode::PushTransform {
                transform: Transform::translate(-rect.x, -rect.y),
            },
            SceneNode::Rect {
                rect,
                brush: Brush::Solid(Color::from_rgb(255, 0, 0)),
                radius: [Px(8.0); 4],
            },
            SceneNode::PopTransform,
            SceneNode::EndLayer { layer_id: 0 },
            SceneNode::PopClip,
            SceneNode::CompositeShadow {
                layer_id: 0,
                blur_px: Px(8.0),
                offset_px: (Px::ZERO, Px(4.0)),
                color: Color::from_rgba(0, 0, 0, 255),
            },
            SceneNode::PopTransform,
        ],
    };
    let Some(mut off) = try_offscreen(800, 600) else {
        return;
    };
    let pixels = off.render_rgba(&scene, None).expect("render");
    let center = (260 * 800 + 400) * 4;
    let outside = (260 * 800 + 295) * 4;
    assert_eq!(&pixels[center..center + 4], &[255, 0, 0, 255]);
    assert!(pixels[outside + 3] > 0);
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
    assert!(
        px[i] > 200 && px[i + 3] > 200,
        "top edge should be red, got {:?}",
        &px[i..i + 4]
    );
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
    assert!(
        px[l] > 128,
        "left edge should be red, got {:?}",
        &px[l..l + 4]
    );
    assert!(px[l + 2] < 128);
    assert!(
        px[r + 2] > 128,
        "right edge should be blue, got {:?}",
        &px[r..r + 4]
    );
    assert!(px[r] < 128);
}

#[test]
fn normalized_linear_gradient_spans_shape_bounds() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(32, 32) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Rect {
            rect: Rect {
                x: 4.0,
                y: 4.0,
                w: 24.0,
                h: 24.0,
            },
            brush: Brush::LinearNormalized {
                start: Vec2 { x: 0.0, y: 0.0 },
                end: Vec2 { x: 0.0, y: 1.0 },
                start_color: Color::from_rgba(255, 0, 0, 255),
                end_color: Color::from_rgba(0, 0, 255, 255),
            },
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let at = |x: usize, y: usize| (y * 32 + x) * 4;
    let top = at(16, 5);
    let bottom = at(16, 26);
    assert!(px[top] > px[top + 2]);
    assert!(px[bottom + 2] > px[bottom]);
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
fn zero_arc_is_empty() {
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
            sweep_angle: 0.0,
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
    assert!(px.chunks_exact(4).all(|pixel| pixel[3] == 0));
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
    assert!(px.iter().any(|&byte| byte != 0));
}

#[test]
fn round_arc_uses_physical_angles_on_non_square_targets() {
    use repose_core::StrokeCap;
    let Some(mut off) = try_offscreen(64, 32) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Arc {
            rect: Rect {
                x: 16.0,
                y: 0.0,
                w: 32.0,
                h: 32.0,
            },
            start_angle: std::f32::consts::FRAC_PI_4,
            sweep_angle: std::f32::consts::FRAC_PI_2,
            stroke_width: Px(4.0),
            brush: Brush::Solid(Color::from_rgba(255, 0, 0, 255)),
            cap: StrokeCap::Round,
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let at = |x: usize, y: usize| (y * 64 + x) * 4;
    let interior = at(41, 29);
    assert!(
        px[interior + 3] > 180 && px[interior] > 180,
        "arc body should cover the physical interior point, got {:?}",
        &px[interior..interior + 4]
    );
    let outside = at(46, 24);
    assert!(
        px[outside + 3] < 20,
        "arc angular coverage should not extend outside its physical sweep, got {:?}",
        &px[outside..outside + 4]
    );
}

#[test]
fn radial_rect_fill_center_matches_start_color() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Rect {
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
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let i = (8 * 16 + 8) * 4;
    assert!(
        px[i + 3] > 200,
        "center should be opaque, got {:?}",
        &px[i..i + 4]
    );
    assert!(
        px[i] > px[i + 2],
        "center should be red-dominant, got {:?}",
        &px[i..i + 4]
    );
}

#[test]
fn sweep_rect_fill_paints_pixels() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Rect {
            rect: Rect {
                x: 2.0,
                y: 2.0,
                w: 12.0,
                h: 12.0,
            },
            brush: Brush::Sweep {
                center: Vec2 { x: 6.0, y: 6.0 },
                start_color: Color::from_rgba(255, 0, 0, 255),
                end_color: Color::from_rgba(0, 0, 255, 255),
            },
            radius: [Px::ZERO; 4],
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let i = (8 * 16 + 8) * 4;
    assert!(px[i + 3] > 200, "sweep rect centre should be opaque");
    assert!(
        px[i] > 20 && px[i + 2] > 20,
        "sweep rect centre should contain gradient color"
    );
}

#[test]
fn radial_ellipse_fill_center_matches_start_color() {
    use repose_core::Vec2;
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let scene = Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Ellipse {
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
        }],
    };
    let px = off.render_rgba(&scene, None).expect("render");
    let i = (8 * 16 + 8) * 4;
    assert!(
        px[i + 3] > 200,
        "center should be opaque, got {:?}",
        &px[i..i + 4]
    );
    assert!(
        px[i] > px[i + 2],
        "center should be red-dominant, got {:?}",
        &px[i..i + 4]
    );
}

#[test]
fn image_source_rect_and_filter_select_exact_atlas_frame() {
    let Some(mut off) = try_offscreen(8, 8) else {
        return;
    };
    let handle =
        off.renderer_mut()
            .register_image_rgba8(2, 1, &[255, 0, 0, 255, 0, 255, 0, 255], true);
    let mut render = |source_rect, filter| {
        let scene = Scene {
            clear_color: Color::from_rgba(0, 0, 0, 0),
            nodes: vec![SceneNode::Image {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    w: 8.0,
                    h: 8.0,
                },
                handle,
                tint: Color::WHITE,
                fit: ImageFit::FillBounds,
                filter,
                source_rect: Some(source_rect),
            }],
        };
        off.render_rgba(&scene, None).expect("render")
    };
    let left = render(ImageSourceRect::new(0, 0, 1, 1), ImageFilter::Linear);
    assert!(left[0] > 200 && left[1] < 20, "got {:?}", &left[..4]);
    let right = render(ImageSourceRect::new(1, 0, 1, 1), ImageFilter::Nearest);
    assert!(right[1] > 200 && right[0] < 20, "got {:?}", &right[..4]);
}
