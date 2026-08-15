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

    for cmd in commands {
        match *cmd {
            Command::MoveTo(x, y) => {
                builder.begin(Point::new(x, y));
            }
            Command::LineTo(x, y) => {
                builder.line_to(Point::new(x, y));
            }
            Command::QuadTo(cx, cy, x, y) => {
                builder.quadratic_bezier_to(Point::new(cx, cy), Point::new(x, y));
            }
            Command::CurveTo(c1x, c1y, c2x, c2y, x, y) => {
                builder.cubic_bezier_to(
                    Point::new(c1x, c1y),
                    Point::new(c2x, c2y),
                    Point::new(x, y),
                );
            }
            Command::Close => {
                builder.close();
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
