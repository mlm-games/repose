use repose_core::prelude::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    Column(Modifier::new().fill_max_width()).child((
        Section(
            "TextField",
            Column(Modifier::new().padding(12.0)).child((
                TextField(
                    "Type here",
                    Modifier::new()
                        .height(40.0)
                        .fill_max_width()
                        .background(theme().surface)
                        .border(1.0, theme().outline, 10.0)
                        .clip_rounded(10.0),
                    Some(|_s| {}),
                    Some(|_s| {}),
                ),
                Box(Modifier::new().height(12.0).width(1.0)),
                Text("Selection, IME composition underline, and caret scrolling are supported.")
                    .size(14.0)
                    .color(Color::from_hex("#999999")),
            )),
        ),
        Section(
            "Wrapping + Ellipsis",
            Column(Modifier::new().padding(12.0)).child((
                Text("Single-line label that ellipsizes when it runs out of space.")
                    .single_line()
                    .overflow_ellipsize()
                    .modifier(Modifier::new().fill_max_width()),
                Box(Modifier::new().height(12.0).width(1.0)),
                Text("This paragraph demonstrates wrapping in a constrained box. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Vestibulum at arcu sed justo viverra posuere.")
                    .size(16.0)
                    .modifier(Modifier::new().width(420.0)),
            )),
        ),
    ))
}
