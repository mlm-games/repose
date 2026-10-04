//! A text node whose alpha reaches 0 must leave no pixels behind, outline
//! included. The outline colour rides in `DrawStyle` rather than in the
//! node's `color`, so the draw path has to fold the text's alpha into it —
//! otherwise a label faded out with `Modifier::alpha(0.0)` keeps a solid
//! halo of glyph outlines.
//!
//! Skips (does not fail) without a WGPU adapter.

use repose_core::{Color, DrawStyle, Px, Rect, Scene, SceneNode, TextPaintStyle};
use repose_render_wgpu::offscreen::OffscreenRenderer;

fn fill() -> Color {
    Color::from_rgba(255, 210, 60, 255)
}

fn outline() -> Color {
    Color::from_rgba(20, 24, 74, 255)
}

fn try_offscreen(w: u32, h: u32) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new_blocking(w, h, 1) {
        Ok(o) => Some(o),
        Err(e) => {
            eprintln!("SKIP: no WGPU adapter ({e:#})");
            None
        }
    }
}

fn outlined_text(fill_alpha: u8) -> Scene {
    Scene {
        clear_color: Color::from_rgba(0, 0, 0, 0),
        nodes: vec![SceneNode::Text {
            rect: Rect {
                x: 2.0,
                y: 2.0,
                w: 80.0,
                h: 24.0,
            },
            text: "QUIET".into(),
            color: fill().with_alpha(fill_alpha),
            size: Px(20.0),
            style: TextPaintStyle {
                draw_style: DrawStyle::fill_and_stroke_with_outline(0.4, outline()),
                ..Default::default()
            },
        }],
    }
}

#[test]
fn zero_alpha_outlined_text_draws_nothing() {
    let Some(mut off) = try_offscreen(96, 32) else {
        return;
    };
    repose_text::load_font_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../repose-text/src/assets/OpenSans-Regular.ttf"
    ))
    .expect("register test font");

    let opaque = off.render_rgba(&outlined_text(255), None).expect("render");
    assert!(
        opaque.iter().any(|&b| b != 0),
        "control: an opaque outlined label must draw something"
    );

    let faded = off.render_rgba(&outlined_text(0), None).expect("render");
    assert!(
        faded.iter().all(|&b| b == 0),
        "a label faded to alpha 0 must not leave its outline behind"
    );
}
