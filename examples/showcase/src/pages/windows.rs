use std::cell::RefCell;
use std::rc::{Rc, Weak};

use repose_core::prelude::*;
use repose_material::material3::{
    Button, ButtonConfig, OutlinedTextField, OutlinedTextFieldConfig, TextButton,
};
use repose_ui::scroll::{ScrollArea, remember_scroll_state};
use repose_ui::windowing::{FloatingWindow, WindowAction, WindowHost, WindowManagerState};
use repose_ui::*;

use crate::ui::{Caption, Hint, Section, sp};

fn host_window_size(
    compact: bool,
    width: f32,
    height: f32,
    min_width: f32,
    min_height: f32,
) -> DpSize {
    if compact {
        let max_width = 280.0;
        let max_height = 160.0;
        DpSize::new(
            Dp(width.min(max_width).max(min_width.min(max_width))),
            Dp(height.min(max_height).max(min_height.min(max_height))),
        )
    } else {
        DpSize::new(Dp(width), Dp(height))
    }
}

fn host_window_position(compact: bool, x: f32, y: f32, slot: usize) -> DpOffset {
    if compact {
        DpOffset::new(Dp(8.0), Dp(8.0 + slot as f32 * 160.0))
    } else {
        DpOffset::new(Dp(x), Dp(y))
    }
}

fn fit_window(
    window: FloatingWindow,
    compact: bool,
    x: f32,
    y: f32,
    slot: usize,
    width: f32,
    height: f32,
    min_width: f32,
    min_height: f32,
) -> FloatingWindow {
    let position = host_window_position(compact, x, y, slot);
    let size = host_window_size(compact, width, height, min_width, min_height);
    let minimum = host_window_size(compact, min_width, min_height, min_width, min_height);
    window
        .position(position.x, position.y)
        .size(size.width, size.height)
        .min_size(minimum.width, minimum.height)
}

pub fn screen(global_windows: Rc<RefCell<WindowManagerState>>) -> View {
    let windows = remember_with_key("windows:state", || RefCell::new(WindowManagerState::new()));
    let initialized = remember_with_key("windows:initialized", || signal(false));
    let list_state = remember_scroll_state("windows:list");
    let compact = !window_size_class().is_expanded_width();

    let note_text = remember(|| signal(String::new()));
    let log_lines = remember(|| signal(vec!["System ready".to_string()]));

    // Note editor body: identical for every note window (shared signal).
    let note_body: Rc<dyn Fn() -> View> = {
        let note_text = note_text.clone();
        Rc::new(move || {
            let tf_state =
                remember_with_key("note_body_tf_state", || RefCell::new(TextFieldState::new()));
            if tf_state.borrow().text != note_text.get() {
                tf_state.borrow_mut().text = note_text.get();
            }
            BasicTextField(
                tf_state.clone(),
                Modifier::new().fill_max_size(),
                "Write a detached note",
                TextFieldConfig {
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
                                    .size(Sp(12.0))
                                    .color(theme().on_surface)
                                    .modifier(Modifier::new().padding(Dp(6.0)))
                            })
                            .collect::<Vec<_>>(),
                    ),
                )
            })
        }
    };

    if !initialized.get() {
        let mut st = windows.borrow_mut();
        if st.windows.is_empty() {
            let note_slot = st.windows.len();
            let note_id = st.alloc_id();
            st.open(fit_window(
                FloatingWindow::new(note_id, "Notes", note_body.clone()),
                compact,
                80.0,
                80.0,
                note_slot,
                360.0,
                220.0,
                260.0,
                160.0,
            ));

            let log_slot = st.windows.len();
            let log_id = st.alloc_id();
            st.open(
                fit_window(
                    FloatingWindow::new(log_id, "Activity", log_body(log_id)),
                    compact,
                    480.0,
                    120.0,
                    log_slot,
                    340.0,
                    240.0,
                    240.0,
                    160.0,
                )
                .actions(vec![WindowAction {
                    label: "Add".to_string(),
                    on_click: {
                        let log_lines = log_lines.clone();
                        Rc::new(move || {
                            let stamp = web_time::SystemTime::now()
                                .duration_since(web_time::UNIX_EPOCH)
                                .map(|value| value.as_secs())
                                .unwrap_or_default();
                            log_lines.update(|lines| {
                                lines.push(format!("Log entry at {stamp}"));
                                if lines.len() > 200 {
                                    lines.remove(0);
                                }
                            });
                        })
                    },
                }]),
            );

            let inspector_slot = st.windows.len();
            let inspector_id = st.alloc_id();
            st.open(
                fit_window(
                    FloatingWindow::new(
                        inspector_id,
                        "Inspector",
                        Rc::new(|| {
                            Column(Modifier::new().fill_max_size().gap(sp::SM)).child(vec![
                                Hint("Selection"),
                                Text("No selection")
                                    .size(Sp(15.0))
                                    .color(theme().on_surface),
                                Hint("Transform"),
                                Caption("Position: 0, 0"),
                                Caption("Rotation: 0 deg"),
                                Caption("Scale: 1.0"),
                            ])
                        }),
                    ),
                    compact,
                    200.0,
                    380.0,
                    inspector_slot,
                    300.0,
                    220.0,
                    220.0,
                    160.0,
                )
                .resizable(false),
            );
        }
        initialized.set(true);
    }

    let open_note = {
        let windows = windows.clone();
        let note_body = note_body.clone();
        move || {
            let mut st = windows.borrow_mut();
            let slot = st.windows.len();
            let id = st.alloc_id();
            st.open(fit_window(
                FloatingWindow::new(id, format!("Note {}", id), note_body.clone()),
                compact,
                140.0,
                140.0,
                slot,
                320.0,
                200.0,
                240.0,
                160.0,
            ));
        }
    };

    let open_log = {
        let windows = windows.clone();
        let log_body = log_body.clone();
        move || {
            let mut st = windows.borrow_mut();
            let slot = st.windows.len();
            let id = st.alloc_id();
            st.open(fit_window(
                FloatingWindow::new(id, format!("Log {}", id), log_body(id)),
                compact,
                520.0,
                160.0,
                slot,
                320.0,
                220.0,
                240.0,
                160.0,
            ));
        }
    };

    let focus_note: Rc<dyn Fn()> = Rc::new({
        let windows: Weak<RefCell<WindowManagerState>> = Rc::downgrade(&windows);
        move || {
            let Some(windows) = windows.upgrade() else {
                return;
            };
            let mut state = windows.borrow_mut();
            let note_id = state
                .windows
                .iter()
                .find(|window| window.title == "Notes")
                .map(|window| window.id);
            if let Some(note_id) = note_id {
                state.bring_to_front(note_id);
                request_frame();
            }
        }
    });
    let spawn_task: Rc<dyn Fn()> = Rc::new({
        let log_lines = log_lines.clone();
        move || {
            let stamp = web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|value| value.as_secs())
                .unwrap_or_default();
            log_lines.update(|lines| {
                lines.push(format!("Task spawned at {stamp}"));
                if lines.len() > 200 {
                    lines.remove(0);
                }
            });
        }
    });
    let clear_logs: Rc<dyn Fn()> = Rc::new({
        let log_lines = log_lines.clone();
        move || log_lines.set(vec!["System ready".to_string()])
    });

    let open_tools = {
        let windows = windows.clone();
        let focus_note = focus_note.clone();
        let spawn_task = spawn_task.clone();
        let clear_logs = clear_logs.clone();
        move || {
            let mut st = windows.borrow_mut();
            let slot = st.windows.len();
            let id = st.alloc_id();
            st.open(
                fit_window(
                    FloatingWindow::new(
                        id,
                        "Tools",
                        Rc::new({
                            let focus_note = focus_note.clone();
                            let spawn_task = spawn_task.clone();
                            let clear_logs = clear_logs.clone();
                            move || {
                                let focus_action = {
                                    let callback = focus_note.clone();
                                    move || callback()
                                };
                                let spawn_action = {
                                    let callback = spawn_task.clone();
                                    move || callback()
                                };
                                let clear_action = {
                                    let callback = clear_logs.clone();
                                    move || callback()
                                };
                                Column(Modifier::new().fill_max_size().gap(sp::SM)).child((
                                    Hint("Window Actions"),
                                    TextButton(
                                        Modifier::new().fill_max_width(),
                                        focus_action,
                                        ButtonConfig::default(),
                                        || Text("Focus Note"),
                                    ),
                                    TextButton(
                                        Modifier::new().fill_max_width(),
                                        spawn_action,
                                        ButtonConfig::default(),
                                        || Text("Spawn Task"),
                                    ),
                                    TextButton(
                                        Modifier::new().fill_max_width(),
                                        clear_action,
                                        ButtonConfig::default(),
                                        || Text("Clear Logs"),
                                    ),
                                ))
                            }
                        }),
                    ),
                    compact,
                    260.0,
                    120.0,
                    slot,
                    260.0,
                    200.0,
                    220.0,
                    160.0,
                )
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
            let slot = st.windows.len();
            let id = st.alloc_id();
            st.open(fit_window(
                FloatingWindow::new(
                    id,
                    "Palette",
                    Rc::new(move || {
                        Column(Modifier::new().fill_max_size().gap(Dp(6.0))).child((
                            Hint("Command Palette"),
                            OutlinedTextField(
                                Modifier::new().fill_max_width(),
                                palette_text.get(),
                                {
                                    let t = palette_text.clone();
                                    move |v| t.set(v)
                                },
                                OutlinedTextFieldConfig {
                                    placeholder: Some("Type a command or search".into()),
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
                                        .padding(Dp(6.0))
                                        .background(theme().surface_variant)
                                        .clip_rounded(Dp(6.0))
                                        .key(i as u64))
                                    .child(Text(*label).size(Sp(12.0)).color(theme().on_surface))
                                })
                                .collect::<Vec<_>>(),
                            ),
                        ))
                    }),
                ),
                compact,
                360.0,
                220.0,
                slot,
                360.0,
                240.0,
                260.0,
                180.0,
            ));
        }
    };

    let open_global = {
        let global_windows = global_windows.clone();
        move || {
            let mut st = global_windows.borrow_mut();
            let slot = st.windows.len();
            let id = st.alloc_id();
            st.open(fit_window(
                FloatingWindow::new(
                    id,
                    format!("Global {}", id),
                    Rc::new(move || {
                        Column(Modifier::new().fill_max_size().gap(sp::SM)).child((
                            Text("Global window")
                                .size(Sp(14.0))
                                .color(theme().on_surface),
                            Caption("Persists across navigation"),
                        ))
                    }),
                ),
                compact,
                220.0,
                140.0,
                slot,
                320.0,
                200.0,
                240.0,
                160.0,
            ));
        }
    };

    let window_count = windows.borrow().windows.len();
    let button_modifier = if compact {
        Modifier::new().fill_max_width()
    } else {
        Modifier::new()
    };
    let control_views = vec![
        Button(
            button_modifier.clone(),
            open_note,
            ButtonConfig::default(),
            || Text("New Note"),
        ),
        Button(
            button_modifier.clone(),
            open_log,
            ButtonConfig::default(),
            || Text("New Log"),
        ),
        Button(
            button_modifier.clone(),
            open_tools,
            ButtonConfig::default(),
            || Text("Tools"),
        ),
        Button(
            button_modifier.clone(),
            open_palette,
            ButtonConfig::default(),
            || Text("Palette"),
        ),
        Button(
            button_modifier.clone(),
            open_global,
            ButtonConfig::default(),
            || Text("Global Window"),
        ),
    ];
    let controls = if compact {
        Column(Modifier::new().fill_max_width().gap(sp::SM)).with_children(control_views)
    } else {
        FlowRow(
            Modifier::new().fill_max_width().gap(Dp(10.0)),
            FlowRowConfig::default(),
        )
        .with_children(control_views)
    };

    let content = Section(
        "Multi-Window / Popout Panels",
        Column(Modifier::new().padding(sp::MD).gap(sp::MD)).child((
            Hint("Floating windows are hosted in-app. Drag, resize, and focus them."),
            controls,
            Caption(format!("{} windows", window_count)),
            Column(
                Modifier::new()
                    .height(Dp(240.0))
                    .fill_max_width()
                    .background(theme().surface_variant)
                    .border(Dp(1.0), theme().outline, Dp(12.0))
                    .clip_rounded(Dp(12.0)),
            )
            .child((
                Column(Modifier::new().fill_max_size()).child((
                    Caption("Stage").modifier(Modifier::new().padding(Dp(10.0))),
                    Caption("Drop windows here; the host surface stays interactive.")
                        .modifier(Modifier::new().padding(Dp(10.0))),
                )),
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(Dp(16.0)), Some(Dp(120.0)), None, None)
                    .size(Dp(120.0), Dp(68.0))
                    .background(theme().primary.with_alpha(40))
                    .border(Dp(1.0), theme().primary, Dp(10.0))
                    .clip_rounded(Dp(10.0)))
                .child(
                    Text("Canvas")
                        .size(Sp(12.0))
                        .color(theme().primary)
                        .modifier(Modifier::new().padding(Dp(10.0))),
                ),
                Box(Modifier::new()
                    .absolute()
                    .offset(Some(Dp(160.0)), Some(Dp(80.0)), None, None)
                    .size(Dp(160.0), Dp(90.0))
                    .background(theme().surface)
                    .border(Dp(1.0), theme().outline, Dp(10.0))
                    .clip_rounded(Dp(10.0)))
                .child(
                    Column(Modifier::new().padding(Dp(10.0)).gap(Dp(6.0))).child((
                        Caption("Pinned"),
                        Text("Navigator").size(Sp(12.0)).color(theme().on_surface),
                    )),
                ),
            )),
            ScrollArea(
                Modifier::new()
                    .height(Dp(180.0))
                    .fill_max_width()
                    .border(Dp(1.0), theme().outline, Dp(12.0))
                    .clip_rounded(Dp(12.0)),
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
                                let title = Text(format!("{}  {}", i + 1, w.title))
                                    .size(Sp(13.0))
                                    .color(theme().on_surface);
                                let dimensions = Caption(format!(
                                    "{} x {}",
                                    w.size.width.0 as i32, w.size.height.0 as i32
                                ));
                                let modifier = Modifier::new()
                                    .fill_max_width()
                                    .padding(sp::SM)
                                    .background(theme().surface)
                                    .border(Dp(1.0), theme().outline, Dp(10.0))
                                    .clip_rounded(Dp(10.0));
                                if compact {
                                    Column(modifier).child((title, dimensions))
                                } else {
                                    Row(modifier).child((title, Spacer(), dimensions))
                                }
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
