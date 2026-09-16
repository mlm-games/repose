use repose_canvas::{Canvas, ShapeStyle};
use repose_core::{StrokeCap, prelude::*};
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
                        .size(Dp(560.0), Dp(200.0))
                        .background(theme().surface)
                        .border(Dp(1.0), theme().outline, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
                    |ds| {
                        ds.draw_rect(
                            Rect {
                                x: 20.0,
                                y: 24.0,
                                w: 150.0,
                                h: 96.0,
                            },
                            theme().primary,
                            Px(16.0),
                        );
                        ds.draw_rect_stroke(
                            Rect {
                                x: 200.0,
                                y: 24.0,
                                w: 130.0,
                                h: 130.0,
                            },
                            theme().outline,
                            Px(18.0),
                            Px(2.0),
                        );
                        ds.draw_circle(Vec2 { x: 430.0, y: 88.0 }, 44.0, theme().tertiary);
                        ds.draw_text(
                            "Fill · Stroke · Circle",
                            Vec2 { x: 22.0, y: 160.0 },
                            theme().on_surface,
                            Px(18.0),
                        );
                    },
                ),
            )),
        ),
        Section(
            "Brushes & strokes",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint("Gradient fills, brush borders, lines, and arcs share the Compose DrawScope model."),
                Canvas(
                    Modifier::new()
                        .size(Dp(560.0), Dp(200.0))
                        .background(theme().surface)
                        .border(Dp(1.0), theme().outline, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
                    |ds| {
                        let th = theme();
                        ds.draw_rect_brush(
                            Rect {
                                x: 20.0,
                                y: 24.0,
                                w: 150.0,
                                h: 96.0,
                            },
                            Brush::Linear {
                                start: Vec2 { x: 0.0, y: 0.0 },
                                end: Vec2 { x: 150.0, y: 96.0 },
                                start_color: th.primary,
                                end_color: th.tertiary,
                            },
                            Px(16.0),
                        );
                        ds.draw_rect_style(
                            Rect {
                                x: 200.0,
                                y: 24.0,
                                w: 130.0,
                                h: 130.0,
                            },
                            Brush::Radial {
                                center: Vec2 { x: 65.0, y: 65.0 },
                                radius: 90.0,
                                start_color: th.secondary,
                                end_color: th.primary,
                            },
                            Px(18.0),
                            ShapeStyle::stroke(Px(6.0)),
                        );
                        ds.draw_line(
                            Vec2 { x: 360.0, y: 40.0 },
                            Vec2 { x: 520.0, y: 120.0 },
                            th.error,
                            Px(4.0),
                            StrokeCap::Round,
                        );
                        ds.draw_arc(
                            Rect {
                                x: 380.0,
                                y: 60.0,
                                w: 120.0,
                                h: 120.0,
                            },
                            0.0,
                            std::f32::consts::TAU * 0.75,
                            false,
                            Brush::Sweep {
                                center: Vec2 { x: 60.0, y: 60.0 },
                                start_color: th.primary,
                                end_color: th.tertiary,
                            },
                            Px(8.0),
                            StrokeCap::Round,
                        );
                    },
                ),
            )),
        ),
        Section(
            "Animated bar chart",
            Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
                Hint(
                    "The animation system feeds values straight into the draw closure each frame.",
                ),
                Canvas(
                    Modifier::new()
                        .size(Dp(560.0), Dp(220.0))
                        .background(theme().surface_container_low)
                        .border(Dp(1.0), theme().outline_variant, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
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
                                Rect {
                                    x,
                                    y: base_y - h,
                                    w: 44.0,
                                    h,
                                },
                                colors[i % colors.len()],
                                Px(8.0),
                            );
                        }
                        ds.draw_text(
                            "live values",
                            Vec2 { x: 24.0, y: 208.0 },
                            th.on_surface_variant,
                            Px(12.0),
                        );
                    },
                ),
            )),
        ),
    ])
}
