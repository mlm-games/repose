use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_docking::*;
use repose_ui::*;

use crate::ui::{Hint, Section, sp};

fn panel(id: u64, title: &str, body: &'static str) -> DockPanel {
    DockPanel {
        id,
        title: title.to_string(),
        content: Rc::new(move || Text(body).size(16.0)),
    }
}

pub fn screen() -> View {
    let state = remember_with_key("dock:state", || {
        RefCell::new(DockState::new_with_tabs(vec![1, 2, 3]))
    });

    let panels = vec![
        panel(1, "Inspector", "Inspector panel"),
        panel(2, "Assets", "Assets panel"),
        panel(3, "Scene", "Scene panel"),
    ];

    Section(
        "Docking",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            Hint("Drag tabs to edges/center to dock. Drag splitters to resize."),
            DockArea(
                "showcase_dock",
                Modifier::new()
                    .height(420.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                state,
                panels,
                DockCallbacks::default(),
            ),
        )),
    )
}
