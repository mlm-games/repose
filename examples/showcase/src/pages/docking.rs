use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_docking::*;
use repose_ui::*;

use crate::ui::Section;

pub fn screen() -> View {
    let state = remember_with_key("dock:state", || {
        RefCell::new(DockState::new_with_tabs(vec![1, 2, 3]))
    });

    let panels = vec![
        DockPanel {
            id: 1,
            title: "Inspector".to_string(),
            content: Rc::new(|| Text("Inspector panel").size(16.0)),
        },
        DockPanel {
            id: 2,
            title: "Assets".to_string(),
            content: Rc::new(|| Text("Assets panel").size(16.0)),
        },
        DockPanel {
            id: 3,
            title: "Scene".to_string(),
            content: Rc::new(|| Text("Scene panel").size(16.0)),
        },
    ];

    let callbacks = DockCallbacks::default();

    Section(
        "Docking",
        Column(Modifier::new().padding(12.0)).child((
            Text("Drag tabs to edges/center to dock. Drag splitters to resize.")
                .size(14.0)
                .color(Color::from_hex("#999999")),
            Box(Modifier::new().height(12.0).width(1.0)),
            DockArea(
                "showcase_dock",
                Modifier::new()
                    .height(420.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                state,
                panels,
                callbacks,
            ),
        )),
    )
}
