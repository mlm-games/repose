use lyon_path::Path;
use lyon_path::math::Point;

use repose_text::Command;

/// Build a lyon Path from glyph outline commands in em-space (font Y-up).
/// Returns None if the path has no geometry.
pub fn commands_to_path(commands: &[Command], _font_size: f32) -> Option<Path> {
    if commands.is_empty() {
        return None;
    }
    let mut builder = Path::builder();
    let mut cur = Point::new(0.0, 0.0);
    let mut contour_start = Point::new(0.0, 0.0);

    for cmd in commands {
        match *cmd {
            Command::MoveTo(x, y) => {
                cur = Point::new(x, y);
                contour_start = cur;
                builder.begin(cur);
            }
            Command::LineTo(x, y) => {
                cur = Point::new(x, y);
                builder.line_to(cur);
            }
            Command::QuadTo(cx, cy, x, y) => {
                let c = Point::new(cx, cy);
                let p = Point::new(x, y);
                builder.quadratic_bezier_to(c, p);
                cur = p;
            }
            Command::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
                let c1 = Point::new(c1x, c1y);
                let c2 = Point::new(c2x, c2y);
                let p = Point::new(x, y);
                builder.cubic_bezier_to(c1, c2, p);
                cur = p;
            }
            Command::Close => {
                builder.close();
                cur = contour_start;
            }
        }
    }

    let path = builder.build();
    if path.iter().next().is_none() {
        None
    } else {
        Some(path)
    }
}
