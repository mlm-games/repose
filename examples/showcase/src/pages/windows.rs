use std::cell::RefCell;
use std::rc::Rc;

use repose_core::prelude::*;
use repose_material::material3::{
    Button, ButtonConfig, OutlinedTextField, OutlinedTextFieldConfig, TextButton,
};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::windowing::{FloatingWindow, WindowAction, WindowHost, WindowManagerState};
use repose_ui::*;

use crate::ui::{Caption, Hint, Section, sp};

pub fn screen(global_windows: Rc<RefCell<WindowManagerState>>) -> View {
    let windows = remember_with_key("windows:state", || RefCell::new(WindowManagerState::new()));
    let list_state = remember_scroll_state("windows:list");

    let note_text = remember(|| signal("Detached note".to_string()));
    let log_lines = remember(|| signal(vec!["System ready".to_string()]));

    // ---- Shared window bodies (built ONCE, reused by startup + buttons) ----

    // Note editor body: identical for every note window (shared signal).
    let note_body: Rc<dyn Fn() -> View> = {
        let note_text = note_text.clone();
        Rc::new(move || {
            let hint = note_text.get();
            BasicTextField(
                hint,
                note_text.get(),
                Modifier::new().fill_max_size(),
                BasicTextFieldConfig {
                    single_line: false,
                    on_change: Some(Rc::new({
                        let t = note_text.clone();
                        move |v| t.set(v)
                    })),
                    on_submit: Some(Rc::new({
                        let t = note_text.clone();
                        move |v| t.set(v)
                    })),
                    ..Default::default()
                },
            )
        })
    };

    // Log viewer body factory: parameterized by window id (per-window scroll key).
    let log_body = {
        let log_lines = log_lines.clone();
        move |id| -> Rc<dyn Fn() -> View> {
            let log_lines = log_lines.clone();
            Rc::new(move || {
                let state = remember_scroll_state(format!("windows:log:{}", id));
                let lines = log_lines.get();
                ScrollArea(
                    Modifier::new().fill_max_size(),
                    state,
                    Column(Modifier::new().fill_max_width()).child(
                        lines
                            .iter()
                            .enumerate()
                            .map(|(i, line)| {
                                Text(format!("{}  {}", i + 1, line))
                                    .size(12.0)
                                    .color(theme().on_surface)
                                    .modifier(Modifier::new().padding(6.0))
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
            })
        }
    };

    // ---- Startup windows ----
    {
        let mut st = windows.borrow_mut();
        if st.windows.is_empty() {
            let note_id = st.alloc_id();
            st.open(
                FloatingWindow::new(note_id, "Notes", note_body.clone())
                    .position(80.0, 80.0)
                    .size(360.0, 220.0)
                    .min_size(260.0, 160.0),
            );

            let log_id = st.alloc_id();
            st.open(
                FloatingWindow::new(log_id, "Activity", log_body(log_id))
                    .position(480.0, 120.0)
                    .size(340.0, 240.0)
                    .min_size(240.0, 160.0)
                    .actions(vec![WindowAction {
                        label: "Add".to_string(),
                        on_click: {
                            let log_lines = log_lines.clone();
                            Rc::new(move || {
                                let stamp = web_time::Instant::now().elapsed().as_millis();
                                log_lines.update(|lines| {
                                    lines.push(format!("Log entry {}", stamp));
                                    if lines.len() > 200 {
                                        lines.remove(0);
                                    }
                                });
                            })
                        },
                    }]),
            );

            let inspector_id = st.alloc_id();
            st.open(
                FloatingWindow::new(
                    inspector_id,
                    "Inspector",
                    Rc::new(|| {
                        Column(Modifier::new().fill_max_size().gap(sp::SM)).child(vec![
                            Hint("Selection"),
                            Text("No selection").size(15.0).color(theme().on_surface),
                            Hint("Transform"),
                            Caption("Position: 0, 0"),
                            Caption("Rotation: 0 deg"),
                            Caption("Scale: 1.0"),
                        ])
                    }),
                )
                .position(200.0, 380.0)
                .size(300.0, 220.0)
                .min_size(220.0, 160.0)
                .resizable(false),
            );
        }
    }

    // ---- Spawn buttons (reuse the shared bodies) ----
    let open_note = {
        let windows = windows.clone();
        let note_body = note_body.clone();
        move || {
            let mut st = windows.borrow_mut();
            let id = st.alloc_id();
            st.open(
                FloatingWindow::new(id, format!("Note {}", id), note_body.clone())
                    .position(140.0, 140.0)
                    .size(320.0, 200.0)
                    .min_size(240.0, 160.0),
            );
        }
    };

    let open_log = {
        let windows = windows.clone();
        let log_body = log_body.clone();
        move || {
            let mut st = windows.borrow_mut();
            let id = st.alloc_id();
            st.open(
                FloatingWindow::new(id, format!("Log {}", id), log_body(id))
                    .position(520.0, 160.0)
                    .size(320.0, 220.0)
                    .min_size(240.0, 160.0),
            );
        }
    };

    let open_tools = {
        let windows = windows.clone();
        move || {
            let mut st = windows.borrow_mut();
            let id = st.alloc_id();
            st.open(
                FloatingWindow::new(
                    id,
                    "Tools",
                    Rc::new(|| {
                        Column(Modifier::new().fill_max_size().gap(sp::SM)).child((
                            Hint("Window Actions"),
                            TextButton(
                                Modifier::new().fill_max_width(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Focus Note"),
                            ),
                            TextButton(
                                Modifier::new().fill_max_width(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Spawn Task"),
                            ),
                            TextButton(
                                Modifier::new().fill_max_width(),
                                || {},
                                ButtonConfig::default(),
                                || Text("Clear Logs"),
                            ),
                        ))
                    }),
                )
                .position(260.0, 120.0)
                .size(260.0, 200.0)
                .min_size(220.0, 160.0)
                .resizable(false),
            );
        }
    };

    let palette_text = remember_with_key("palette_text", || signal(String::new()));
    let open_palette = {
        let windows = windows.clone();
        let palette_text = palette_text.clone();
        move || {
            let palette_text = palette_text.clone();
            let mut st = windows.borrow_mut();
            let id = st.alloc_id();
            st.open(
                FloatingWindow::new(
                    id,
                    "Palette",
                    Rc::new(move || {
                        Column(Modifier::new().fill_max_size().gap(6.0)).child((
                            Hint("Command Palette"),
                            OutlinedTextField(
                                Modifier::new().fill_max_width(),
                                palette_text.get(),
                                {
                                    let t = palette_text.clone();
                                    move |v| t.set(v)
                                },
                                OutlinedTextFieldConfig {
                                    placeholder: Some("Type a command".into()),
                                    ..Default::default()
                                },
                            ),
                            Column(Modifier::new().fill_max_width().gap(sp::XS)).child(
                                [
                                    "Open Layout",
                                    "Open Inspector",
                                    "Search Assets",
                                    "Open Logs",
                                ]
                                .iter()
                                .enumerate()
                                .map(|(i, label)| {
                                    Box(Modifier::new()
                                        .fill_max_width()
                                        .padding(6.0)
                                        .background(theme().surface_variant)
                                        .clip_rounded(6.0)
                                        .key(i as u64))
                                    .child(Text(*label).size(12.0).color(theme().on_surface))
                                })
                                .collect::<Vec<_>>(),
                            ),
                        ))
                    }),
                )
                .position(360.0, 220.0)
                .size(360.0, 240.0)
                .min_size(260.0, 180.0),
            );
        }
    };

    let open_global = {
        let global_windows = global_windows.clone();
        move || {
            let mut st = global_windows.borrow_mut();
            let id = st.alloc_id();
            st.open(
                FloatingWindow::new(
                    id,
                    format!("Global {}", id),
                    Rc::new(move || {
                        Column(Modifier::new().fill_max_size().gap(sp::SM)).child((
                            Text("Global window").size(14.0).color(theme().on_surface),
                            Caption("Persists across navigation"),
                        ))
                    }),
                )
                .position(220.0, 140.0)
                .size(320.0, 200.0)
                .min_size(240.0, 160.0),
            );
        }
    };

    let window_count = windows.borrow().windows.len();

    let content = Section(
        "Multi-Window / Popout Panels",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            Hint("Floating windows are hosted in-app. Drag, resize, and focus them."),
            Row(Modifier::new().align_items(AlignItems::Center).gap(10.0)).child(vec![
                Button(Modifier::new(), open_note, ButtonConfig::default(), || {
                    Text("New Note")
                }),
                Button(Modifier::new(), open_log, ButtonConfig::default(), || {
                    Text("New Log")
                }),
                Button(Modifier::new(), open_tools, ButtonConfig::default(), || {
                    Text("Tools")
                }),
                Button(
                    Modifier::new(),
                    open_palette,
                    ButtonConfig::default(),
                    || Text("Palette"),
                ),
                Button(
                    Modifier::new(),
                    open_global,
                    ButtonConfig::default(),
                    || Text("Global Window"),
                ),
                Spacer(),
                Caption(format!("{} windows", window_count)),
            ]),
            Stack(
                Modifier::new()
                    .height(240.0)
                    .fill_max_width()
                    .background(theme().surface_variant)
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
            )
            .child((
                Column(Modifier::new().fill_max_size()).child((
                    Caption("Stage").modifier(Modifier::new().padding(10.0)),
                    Caption("Drop windows here; the host surface stays interactive.")
                        .modifier(Modifier::new().padding(10.0)),
                )),
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(16.0), Some(120.0), None, None)
                    .size(120.0, 68.0)
                    .background(theme().primary.with_alpha(40))
                    .border(1.0, theme().primary, 10.0)
                    .clip_rounded(10.0))
                .child(
                    Text("Canvas")
                        .size(12.0)
                        .color(theme().primary)
                        .modifier(Modifier::new().padding(10.0)),
                ),
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(160.0), Some(80.0), None, None)
                    .size(160.0, 90.0)
                    .background(theme().surface)
                    .border(1.0, theme().outline, 10.0)
                    .clip_rounded(10.0))
                .child(Column(Modifier::new().padding(10.0).gap(6.0)).child((
                    Caption("Pinned"),
                    Text("Navigator").size(12.0).color(theme().on_surface),
                ))),
            )),
            ScrollArea(
                Modifier::new()
                    .height(180.0)
                    .fill_max_width()
                    .border(1.0, theme().outline, 12.0)
                    .clip_rounded(12.0),
                list_state,
                Column(Modifier::new().fill_max_width()).child((
                    Caption("Spawned windows are listed here for debugging.")
                        .modifier(Modifier::new().padding(sp::SM)),
                    Column(Modifier::new().fill_max_width()).child(
                        windows
                            .borrow()
                            .windows
                            .iter()
                            .enumerate()
                            .map(|(i, w)| {
                                Row(Modifier::new()
                                    .fill_max_width()
                                    .padding(sp::SM)
                                    .background(theme().surface)
                                    .border(1.0, theme().outline, 10.0)
                                    .clip_rounded(10.0))
                                .child((
                                    Text(format!("{}  {}", i + 1, w.title))
                                        .size(13.0)
                                        .color(theme().on_surface),
                                    Spacer(),
                                    Caption(format!(
                                        "{} x {}",
                                        w.size.width as i32, w.size.height as i32
                                    )),
                                ))
                            })
                            .collect::<Vec<_>>(),
                    ),
                )),
            ),
        )),
    );

    WindowHost(
        "showcase_windows",
        Modifier::new().fill_max_size(),
        windows,
        content,
    )
}
