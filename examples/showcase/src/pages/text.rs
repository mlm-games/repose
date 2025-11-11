use crate::ui::Section;
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
    Column(Modifier::new().padding(8.0)).child((
        Section(
            "TextField (IME, selection, caret scroll)",
            TextField(
                "Type here",
                Modifier::new()
                    .height(36.0)
                    .fill_max_width()
                    .background(theme().surface)
                    .border(1.0, theme().outline, 6.0),
                Some(|_s| {}),
                Some(|_s| {}),
            ),
        ),
        Section(
            "Text wrapping / ellipsis",
            Column(Modifier::new().padding(8.0)).child((
                Text("This is a single-line ellipsized label that won’t overflow")
                    .single_line()
                    .overflow_ellipsize()
                    .modifier(Modifier::new().fill_max_width()),
                Text("This paragraph demonstrates wrapping in a constrained box. Lorem ipsum dolor sit amet, consectetur adipiscing elit.")
                    .size(16.0)
                    .modifier(Modifier::new().width(360.0)),
            )),
        ),
    ))
}
