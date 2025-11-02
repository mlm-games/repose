use crate::ui::Section;
use compose_core::prelude::*;
use compose_ui::*;

pub fn screen() -> View {
    Column(Modifier::new().padding(8.0)).child((
        Section("TextField (IME, selection, caret scroll)",
            TextField("Type here",
                Modifier::new()
                    .size(360.0, 36.0)
                    .background(Color::from_hex("#1E1E1E"))
                    .border(1.0, Color::from_hex("#444"), 6.0)
            )
        ),
        Section("Multiline note",
            Text("This TextField demo supports:\n- Mouse selection/drag\n- IME preedit/commit\n- Clipboard (Ctrl/Cmd C/X/V)\n- Caret scroll keeping.")
        ),
    ))
}
