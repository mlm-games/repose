//! Contract tests for how `SceneNode::Image` maps a view rect onto source
//! pixels: scale mode, alignment, clipping to the view rect, and tiling.
//! Skips (does not fail) without a WGPU adapter.
//!
//! The scale factors are Compose's `ContentScale.computeScaleFactor`; clipping
//! stands in for Compose's `clipToBounds` and Godot's `clip_contents`.

use repose_core::{Color, ImageAlignment, ImageFilter, ImageFit, Rect, Scene, SceneNode};
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

fn alpha_at(buf: &[u8], w: usize, x: usize, y: usize) -> u8 {
    buf[(y * w + x) * 4 + 3]
}

fn draw(
    off: &mut OffscreenRenderer,
    handle: u64,
    rect: Rect,
    fit: ImageFit,
    alignment: ImageAlignment,
) -> Vec<u8> {
    off.render_rgba(
        &Scene {
            clear_color: Color::from_rgba(0, 0, 0, 0),
            nodes: vec![SceneNode::Image {
                rect,
                handle,
                tint: Color::WHITE,
                fit,
                filter: ImageFilter::Nearest,
                source_rect: None,
                alignment,
            }],
        },
        None,
    )
    .expect("render")
}

fn solid(off: &mut OffscreenRenderer, w: u32, h: u32, rgba: [u8; 4]) -> u64 {
    let px: Vec<u8> = (0..w * h).flat_map(|_| rgba).collect();
    off.renderer_mut().register_image_rgba8(w, h, &px, true)
}

#[test]
fn alignment_places_fitted_content_within_the_view_rect() {
    let Some(mut off) = try_offscreen(32, 32) else {
        return;
    };
    let handle = solid(&mut off, 2, 2, [255, 0, 0, 255]);
    // A 2x2 source contained in a 32x16 box is 16x16, leaving 16px of slack
    // horizontally to distribute by alignment.
    let wide = Rect {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 16.0,
    };

    for (alignment, first, last) in [
        (ImageAlignment::Begin, 0usize, 15usize),
        (ImageAlignment::Center, 8, 23),
        (ImageAlignment::End, 16, 31),
    ] {
        let buf = draw(&mut off, handle, wide, ImageFit::Contain, alignment);
        let cols: Vec<usize> = (0..32)
            .filter(|&x| (0..32).any(|y| alpha_at(&buf, 32, x, y) != 0))
            .collect();
        assert_eq!(
            (cols.first().copied(), cols.last().copied()),
            (Some(first), Some(last)),
            "{alignment:?} should span columns {first}..={last}, got {cols:?}"
        );
    }
}

/// Modes that scale past an edge (`Cover`, `FitWidth`, `FitHeight`, `None`) must
/// stay inside the view rect. Compose relies on `clipToBounds` and Godot on
/// `clip_contents`; this clips by remapping UVs, with no extra pass.
#[test]
fn no_scale_mode_draws_outside_the_view_rect() {
    let Some(mut off) = try_offscreen(32, 32) else {
        return;
    };
    let handle = solid(&mut off, 4, 4, [255, 0, 0, 255]);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 8.0,
    };

    for fit in [
        ImageFit::Contain,
        ImageFit::Cover,
        ImageFit::FitWidth,
        ImageFit::FitHeight,
        ImageFit::FillBounds,
        ImageFit::Inside,
        ImageFit::None,
    ] {
        let buf = draw(&mut off, handle, rect, fit, ImageAlignment::Center);
        let outside = (0..32)
            .flat_map(|y| (0..32).map(move |x| (x, y)))
            .filter(|&(x, y)| alpha_at(&buf, 32, x, y) != 0 && y >= 8)
            .count();
        assert_eq!(outside, 0, "{fit:?} drew below the 8px-tall view rect");
        let drew = (0..32).any(|y| (0..32).any(|x| alpha_at(&buf, 32, x, y) != 0));
        assert!(drew, "{fit:?} drew nothing");
    }
}

/// `None` is a true 1:1 draw; placement comes from `alignment`, not from an
/// implicit top-left.
#[test]
fn none_draws_one_to_one_at_the_aligned_corner() {
    let Some(mut off) = try_offscreen(32, 32) else {
        return;
    };
    let handle = solid(&mut off, 4, 4, [255, 0, 0, 255]);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 32.0,
        h: 32.0,
    };

    let rows = |buf: &[u8]| -> Vec<usize> {
        (0..32)
            .filter(|&y| (0..32).any(|x| alpha_at(buf, 32, x, y) != 0))
            .collect()
    };

    let buf = draw(
        &mut off,
        handle,
        rect,
        ImageFit::None,
        ImageAlignment::Center,
    );
    assert_eq!(
        rows(&buf),
        vec![14, 15, 16, 17],
        "4px tall, centered in 32px"
    );

    let buf = draw(
        &mut off,
        handle,
        rect,
        ImageFit::None,
        ImageAlignment::Begin,
    );
    assert_eq!(rows(&buf), vec![0, 1, 2, 3], "4px tall, at the top");
}

/// `Tile` covers the view rect by repeating the source, relying on wrapping
/// samplers rather than extra quads.
#[test]
fn tile_repeats_the_source_across_the_view_rect() {
    let Some(mut off) = try_offscreen(16, 16) else {
        return;
    };
    let flat = solid(&mut off, 1, 1, [255, 0, 0, 255]);
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 16.0,
        h: 16.0,
    };
    let buf = draw(&mut off, flat, rect, ImageFit::Tile, ImageAlignment::Center);
    let not_red = (0..16)
        .flat_map(|y| (0..16).map(move |x| (x, y)))
        .filter(|&(x, y)| alpha_at(&buf, 16, x, y) == 0)
        .count();
    assert_eq!(not_red, 0, "a 1x1 tile must cover every pixel");

    // A 4x4 source of 2x2 blocks: the pattern repeats on the tile boundary.
    let mut px = Vec::new();
    for y in 0..4u8 {
        for x in 0..4u8 {
            let v = if (x / 2 + y / 2) % 2 == 0 { 255 } else { 0 };
            px.extend_from_slice(&[v, v, v, 255]);
        }
    }
    let blocks = off.renderer_mut().register_image_rgba8(4, 4, &px, true);
    let buf = draw(
        &mut off,
        blocks,
        rect,
        ImageFit::Tile,
        ImageAlignment::Center,
    );
    let at = |x: usize, y: usize| &buf[(y * 16 + x) * 4..(y * 16 + x) * 4 + 4];
    assert_eq!(at(0, 0), at(4, 0), "tile repeats horizontally");
    assert_eq!(at(0, 0), at(0, 4), "tile repeats vertically");
    assert_ne!(at(0, 0), at(2, 0), "source blocks alternate");
    assert_eq!(at(2, 0), at(6, 0), "the alternation repeats too");
}
