use repose_canvas::Canvas;
use repose_core::prelude::*;
use repose_ui::anim::animate_f32_from;
use repose_ui::*;
use web_time::Duration;

use crate::ui::{Hint, Page, Section, sp};

pub fn screen() -> View {
    let pulse = animate_f32_from(
        "canvas_pulse",
        0.0,
        1.0,
        AnimationSpec::tween(Duration::from_millis(1400), Easing::EaseInOut)
            .repeated(RepeatableSpec::infinite().reverse()),
    );

    Page(vec![
        Section(
            "Primitives",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Filled and stroked rects, circles, and text recorded into SceneNodes."),
                Canvas(
                    Modifier::new()
                        .size(560.0, 200.0)
                        .background(theme().surface)
                        .border(1.0, theme().outline, 16.0)
                        .clip_rounded(16.0),
                    |ds| {
                        ds.draw_rect(Rect { x: 20.0, y: 24.0, w: 150.0, h: 96.0 }, theme().primary, 16.0);
                        ds.draw_rect_stroke(Rect { x: 200.0, y: 24.0, w: 130.0, h: 130.0 }, theme().outline, 18.0, 2.0);
                        ds.draw_circle(Vec2 { x: 430.0, y: 88.0 }, 44.0, theme().tertiary);
                        ds.draw_text("Fill · Stroke · Circle", Vec2 { x: 22.0, y: 160.0 }, theme().on_surface, 18.0);
                    },
                ),
            )),
        ),
        Section(
            "Animated bar chart",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("The animation system feeds values straight into the draw closure each frame."),
                Canvas(
                    Modifier::new()
                        .size(560.0, 220.0)
                        .background(theme().surface_container_low)
                        .border(1.0, theme().outline_variant, 16.0)
                        .clip_rounded(16.0),
                    move |ds| {
                        let th = theme();
                        let base_y = 190.0;
                        let colors = [th.primary, th.secondary, th.tertiary, th.error];
                        for i in 0..8 {
                            let phase = (i as f32 * 0.5).sin() * 0.5 + 0.5;
                            let t = (pulse + phase).fract();
                            let h = 30.0 + t * 130.0;
                            let x = 24.0 + i as f32 * 64.0;
                            ds.draw_rect(
                                Rect { x, y: base_y - h, w: 44.0, h },
                                colors[i % colors.len()],
                                8.0,
                            );
                        }
                        ds.draw_text("live values", Vec2 { x: 24.0, y: 208.0 }, th.on_surface_variant, 12.0);
                    },
                ),
            )),
        ),
    ])
}