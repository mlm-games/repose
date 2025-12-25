use repose_canvas::Canvas;
use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Section(
        "Canvas",
        Column(Modifier::new().padding(12.0)).child((
            Text("Immediate-mode drawing recorded into SceneNodes.")
                .size(14.0)
                .color(Color::from_hex("#999999")),
            Box(Modifier::new().height(12.0).width(1.0)),
            Canvas(
                Modifier::new()
                    .size(520.0, 240.0)
                    .background(theme().surface)
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                |ds| {
                    ds.draw_rect(
                        Rect {
                            x: 16.0,
                            y: 16.0,
                            w: 140.0,
                            h: 80.0,
                        },
                        theme().primary,
                        12.0,
                    );
                    ds.draw_rect_stroke(
                        Rect {
                            x: 180.0,
                            y: 22.0,
                            w: 120.0,
                            h: 120.0,
                        },
                        theme().outline,
                        16.0,
                        2.0,
                    );
                    ds.draw_circle(Vec2 { x: 380.0, y: 80.0 }, 36.0, theme().on_surface);
                    ds.draw_text(
                        "Hello Canvas",
                        Vec2 { x: 18.0, y: 140.0 },
                        theme().on_surface,
                        20.0,
                    );
                },
            ),
        )),
    )
}
