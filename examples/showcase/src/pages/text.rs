use crate::ui::Section;
use anyhow::Ok;
use repose_core::prelude::*;
use repose_ui::*;

pub fn screen() -> View {
    Column(Modifier::new().padding(8.0)).child((
        Section("TextField (IME, selection, caret scroll)",
            TextField("Type here",
                Modifier::new()
                    .height(36.0).fill_max_width()
                    .background(Color::from_hex("#1E1E1E"))
                    .border(1.0, Color::from_hex("#444"), 6.0), Some(|_| {}), Some(|_| {})
            )
        ),
        Section("Multiline note",
            Text("➤ This TextField demo supports:\n- Mouse selection/drag\n- IME preedit/commit\n- Clipboard (Ctrl/Cmd C/X/V)\n- Caret scroll keeping.")
        ),
    ))
}
