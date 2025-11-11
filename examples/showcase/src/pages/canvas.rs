use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::canvas::*;

pub fn screen() -> View {
    Section(
        "Canvas (rect/circle/text)",
        Canvas(
            Modifier::new()
                .size(320.0, 200.0)
                .background(theme().surface),
            |ds| {
                ds.draw_rect(
                    Rect {
                        x: 12.0,
                        y: 12.0,
                        w: 100.0,
                        h: 60.0,
                    },
                    theme().primary,
                    8.0,
                );
                ds.draw_rect_stroke(
                    Rect {
                        x: 130.0,
                        y: 20.0,
                        w: 80.0,
                        h: 80.0,
                    },
                    theme().outline,
                    12.0,
                    2.0,
                );
                ds.draw_circle(Vec2 { x: 240.0, y: 60.0 }, 28.0, theme().on_surface);
                ds.draw_text(
                    "Hello Canvas",
                    Vec2 { x: 16.0, y: 120.0 },
                    theme().on_surface,
                    18.0,
                );
            },
        ), // .modifier(Modifier::new().translate(8.0, 8.0).scale(1.0)),
    )
}
