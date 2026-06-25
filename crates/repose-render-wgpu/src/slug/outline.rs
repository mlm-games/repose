use lyon_path::Path;
use lyon_path::math::Point;

use cosmic_text::Command;

/// Build a lyon Path from glyph outline commands in em-space (font Y-up).
/// Returns None if the path has no geometry.
pub fn commands_to_path(commands: &[Command], font_size: f32) -> Option<Path> {
    if commands.is_empty() {
        return None;
    }
    let inv_size = 1.0 / font_size;
    let mut builder = Path::builder();
    let mut cur = Point::new(0.0, 0.0);
    let mut contour_start = Point::new(0.0, 0.0);

    for cmd in commands {
        match *cmd {
            Command::MoveTo(p) => {
                cur = Point::new(p.x * inv_size, p.y * inv_size);
                contour_start = cur;
                builder.begin(cur);
            }
            Command::LineTo(p) => {
                cur = Point::new(p.x * inv_size, p.y * inv_size);
                builder.line_to(cur);
            }
            Command::QuadTo(c, p) => {
                let c = Point::new(c.x * inv_size, c.y * inv_size);
                let p = Point::new(p.x * inv_size, p.y * inv_size);
                builder.quadratic_bezier_to(c, p);
                cur = p;
            }
            Command::CurveTo(c1, c2, p) => {
                let c1 = Point::new(c1.x * inv_size, c1.y * inv_size);
                let c2 = Point::new(c2.x * inv_size, c2.y * inv_size);
                let p = Point::new(p.x * inv_size, p.y * inv_size);
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
