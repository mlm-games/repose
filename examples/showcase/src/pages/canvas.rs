use repose_canvas::{Canvas, ShapeStyle};
use repose_core::{StrokeCap, prelude::*};
use repose_ui::anim::animate_f32_from;
use repose_ui::*;
use web_time::Duration;

use crate::ui::{Hint, Page, Section, dp_px, sp};

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
                        .fill_max_width()
                        .max_width(Dp(560.0))
                        .height(Dp(200.0))
                        .background(theme().surface)
                        .border(Dp(1.0), theme().outline, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
                    |ds| {
                        ds.draw_rect(
                            Rect {
                                x: dp_px(20.0),
                                y: dp_px(24.0),
                                w: dp_px(150.0),
                                h: dp_px(96.0),
                            },
                            theme().primary,
                            Px(dp_px(16.0)),
                        );
                        ds.draw_rect_stroke(
                            Rect {
                                x: dp_px(200.0),
                                y: dp_px(24.0),
                                w: dp_px(130.0),
                                h: dp_px(130.0),
                            },
                            theme().outline,
                            Px(dp_px(18.0)),
                            Px(dp_px(2.0)),
                        );
                        ds.draw_circle(
                            Vec2 {
                                x: dp_px(430.0),
                                y: dp_px(88.0),
                            },
                            dp_px(44.0),
                            theme().tertiary,
                        );
                        ds.draw_text(
                            "Fill · Stroke · Circle",
                            Vec2 {
                                x: dp_px(22.0),
                                y: dp_px(160.0),
                            },
                            theme().on_surface,
                            Px(dp_px(18.0)),
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
                        .fill_max_width()
                        .max_width(Dp(560.0))
                        .height(Dp(200.0))
                        .background(theme().surface)
                        .border(Dp(1.0), theme().outline, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
                    |ds| {
                        let th = theme();
                        ds.draw_rect_brush(
                            Rect {
                                x: dp_px(20.0),
                                y: dp_px(24.0),
                                w: dp_px(150.0),
                                h: dp_px(96.0),
                            },
                            Brush::Linear {
                                start: Vec2 { x: 0.0, y: 0.0 },
                                end: Vec2 {
                                    x: dp_px(150.0),
                                    y: dp_px(96.0),
                                },
                                start_color: th.primary,
                                end_color: th.tertiary,
                            },
                            Px(dp_px(16.0)),
                        );
                        ds.draw_rect_style(
                            Rect {
                                x: dp_px(200.0),
                                y: dp_px(24.0),
                                w: dp_px(130.0),
                                h: dp_px(130.0),
                            },
                            Brush::Radial {
                                center: Vec2 {
                                    x: dp_px(65.0),
                                    y: dp_px(65.0),
                                },
                                radius: dp_px(90.0),
                                start_color: th.secondary,
                                end_color: th.primary,
                            },
                            Px(dp_px(18.0)),
                            ShapeStyle::stroke(Px(dp_px(6.0))),
                        );
                        ds.draw_line(
                            Vec2 {
                                x: dp_px(360.0),
                                y: dp_px(40.0),
                            },
                            Vec2 {
                                x: dp_px(520.0),
                                y: dp_px(120.0),
                            },
                            th.error,
                            Px(dp_px(4.0)),
                            StrokeCap::Round,
                        );
                        ds.draw_arc(
                            Rect {
                                x: dp_px(380.0),
                                y: dp_px(60.0),
                                w: dp_px(120.0),
                                h: dp_px(120.0),
                            },
                            0.0,
                            std::f32::consts::TAU * 0.75,
                            false,
                            Brush::Sweep {
                                center: Vec2 {
                                    x: dp_px(60.0),
                                    y: dp_px(60.0),
                                },
                                start_color: th.primary,
                                end_color: th.tertiary,
                            },
                            Px(dp_px(8.0)),
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
                        .fill_max_width()
                        .max_width(Dp(560.0))
                        .height(Dp(220.0))
                        .background(theme().surface_container_low)
                        .border(Dp(1.0), theme().outline_variant, Dp(16.0))
                        .clip_rounded(Dp(16.0)),
                    move |ds| {
                        let th = theme();
                        let base_y = dp_px(190.0);
                        let colors = [th.primary, th.secondary, th.tertiary, th.error];
                        for i in 0..8 {
                            let phase = (i as f32 * 0.5).sin() * 0.5 + 0.5;
                            let t = (pulse + phase).fract();
                            let h = dp_px(30.0 + t * 130.0);
                            let x = dp_px(24.0) + i as f32 * dp_px(64.0);
                            ds.draw_rect(
                                Rect {
                                    x,
                                    y: base_y - h,
                                    w: dp_px(44.0),
                                    h,
                                },
                                colors[i % colors.len()],
                                Px(dp_px(8.0)),
                            );
                        }
                        ds.draw_text(
                            "live values",
                            Vec2 {
                                x: dp_px(24.0),
                                y: dp_px(208.0),
                            },
                            th.on_surface_variant,
                            Px(dp_px(12.0)),
                        );
                    },
                ),
            )),
        ),
    ])
}
