#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use repose_core::*;
use repose_ui::TextStyle;
use repose_ui::*;

pub type PanelId = u64;

#[derive(Clone)]
pub struct DockPanel {
    pub id: PanelId,
    pub title: String,
    pub content: Rc<dyn Fn() -> View>,
}

#[derive(Clone, Default)]
pub struct DockCallbacks {
    /// Optional popout handler. If provided, the docking system will call it
    /// when a panel is dropped on the "float" target or when user taps popout.
    pub on_popout: Option<Rc<dyn Fn(PanelId)>>,

    /// Optional close handler (tab close button).
    pub on_close: Option<Rc<dyn Fn(PanelId)>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal, // left/right
    Vertical,   // top/bottom
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropZone {
    Center,
    Left,
    Right,
    Top,
    Bottom,
    Float,
}

/// Persistent docking state.
/// Store this in `remember_state_with_key(...)` or `SavedState` etc.
#[derive(Clone)]
pub struct DockState {
    pub root: DockNode,
    next_id: u64,
}

#[derive(Clone)]
pub struct DockNode {
    pub id: u64,
    pub kind: DockKind,
}

#[derive(Clone)]
pub enum DockKind {
    Empty,
    Tabs {
        tabs: Vec<PanelId>,
        active: Option<PanelId>,
    },
    Split {
        dir: SplitDir,
        ratio: f32, // 0..1
        a: Box<DockNode>,
        b: Box<DockNode>,
    },
}

impl DockState {
    pub fn new_with_tabs(tabs: Vec<PanelId>) -> Self {
        let mut st = Self {
            root: DockNode {
                id: 1,
                kind: DockKind::Empty,
            },
            next_id: 2,
        };
        st.root.kind = DockKind::Tabs { tabs, active: None };
        st.normalize();
        st
    }

    /// Create a DockState from a pre-built root node.
    /// The `max_node_id` should be higher than any node ID used in the tree.
    pub fn from_root(root: DockNode, max_node_id: u64) -> Self {
        let mut st = Self {
            root,
            next_id: max_node_id + 1,
        };
        st.normalize();
        st
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn normalize(&mut self) {
        normalize_node(&mut self.root);
    }

    /// Remove panel without normalizing - for use in compound operations
    pub fn remove_panel_no_normalize(&mut self, pid: PanelId) -> bool {
        remove_panel_in_node(&mut self.root, pid)
    }

    pub fn remove_panel(&mut self, pid: PanelId) -> bool {
        let removed = remove_panel_in_node(&mut self.root, pid);
        if removed {
            normalize_node(&mut self.root);
        }
        removed
    }

    pub fn set_active(&mut self, tabs_node_id: u64, pid: PanelId) {
        if let Some(n) = find_node_mut(&mut self.root, tabs_node_id)
            && let DockKind::Tabs { tabs, active } = &mut n.kind
            && tabs.contains(&pid)
        {
            *active = Some(pid);
        }
    }

    pub fn set_split_ratio(&mut self, split_node_id: u64, ratio: f32) {
        let ratio = ratio.clamp(0.05, 0.95);
        if let Some(n) = find_node_mut(&mut self.root, split_node_id)
            && let DockKind::Split { ratio: r, .. } = &mut n.kind
        {
            *r = ratio;
        }
    }

    pub fn dock_panel(&mut self, target_node_id: u64, zone: DropZone, pid: PanelId) -> bool {
        self.remove_panel_no_normalize(pid);

        let result = match zone {
            DropZone::Center => self.insert_as_tab(target_node_id, pid),
            DropZone::Left | DropZone::Right | DropZone::Top | DropZone::Bottom => {
                self.insert_as_split(target_node_id, zone, pid)
            }
            DropZone::Float => false,
        };

        self.normalize();
        result
    }

    fn insert_as_tab(&mut self, target_node_id: u64, pid: PanelId) -> bool {
        let Some(n) = find_node_mut(&mut self.root, target_node_id) else {
            return false;
        };

        match &mut n.kind {
            DockKind::Tabs { tabs, active } => {
                if !tabs.contains(&pid) {
                    tabs.push(pid);
                }
                *active = Some(pid);
                self.normalize();
                true
            }
            DockKind::Empty => {
                n.kind = DockKind::Tabs {
                    tabs: vec![pid],
                    active: Some(pid),
                };
                self.normalize();
                true
            }
            DockKind::Split { .. } => false,
        }
    }

    fn insert_as_split(&mut self, target_node_id: u64, zone: DropZone, pid: PanelId) -> bool {
        // Allocate all IDs upfront before borrowing
        let new_tabs_id = self.alloc_id();
        let new_split_id = self.alloc_id();

        let Some(n) = find_node_mut(&mut self.root, target_node_id) else {
            return false;
        };

        let old_kind = std::mem::replace(&mut n.kind, DockKind::Empty);

        let dir = match zone {
            DropZone::Left | DropZone::Right => SplitDir::Horizontal,
            DropZone::Top | DropZone::Bottom => SplitDir::Vertical,
            _ => SplitDir::Horizontal,
        };

        let new_tabs = DockNode {
            id: new_tabs_id,
            kind: DockKind::Tabs {
                tabs: vec![pid],
                active: Some(pid),
            },
        };

        // Old content KEEPS the original target_node_id
        let old_node = DockNode {
            id: target_node_id,
            kind: old_kind,
        };

        let (a, b) = match zone {
            DropZone::Left | DropZone::Top => (Box::new(new_tabs), Box::new(old_node)),
            DropZone::Right | DropZone::Bottom => (Box::new(old_node), Box::new(new_tabs)),
            _ => (Box::new(old_node), Box::new(new_tabs)),
        };

        // The node at this position becomes a split with a NEW ID
        n.id = new_split_id;
        n.kind = DockKind::Split {
            dir,
            ratio: 0.5,
            a,
            b,
        };

        self.normalize();
        true
    }
}

#[derive(Clone, Debug)]
pub struct DockTabPayload {
    pub panel_id: PanelId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HoverHint {
    node_id: u64,
    zone: DropZone,
}

#[derive(Clone)]
struct SplitDrag {
    node_id: u64,
    dir: SplitDir,
}

/// Ephemeral, reusable dock behavior handle. Created by [`remember_dock_handle`]
/// and passed to the [`DockModifierExt`] helpers so custom chrome can reuse the
/// exact same docking behavior.
#[derive(Clone)]
pub struct DockHandle {
    pub(crate) key: String,
    pub(crate) state: Rc<RefCell<DockState>>,
    pub(crate) callbacks: DockCallbacks,
    pub(crate) hover_sig: Signal<Option<HoverHint>>,
    pub(crate) tab_hover: Signal<Option<PanelId>>,
    pub(crate) drag_active: Signal<bool>,
}

/// Create a [`DockHandle`] bound to a dock state and callbacks. Remember this in
/// your widget so it survives recompositions.
pub fn remember_dock_handle(
    key: impl Into<String>,
    state: Rc<RefCell<DockState>>,
    callbacks: DockCallbacks,
) -> DockHandle {
    let key = key.into();

    let hover_sig = remember_with_key(format!("dock:hover:{key}"), || signal(None::<HoverHint>));
    let tab_hover = remember_with_key(format!("dock:tab_hover:{key}"), || signal(None::<PanelId>));
    let drag_active = remember_with_key(format!("dock:drag_active:{key}"), || signal(false));

    DockHandle {
        key,
        state,
        callbacks,
        hover_sig: (*hover_sig).clone(),
        tab_hover: (*tab_hover).clone(),
        drag_active: (*drag_active).clone(),
    }
}

/// Modular dock behavior modifiers. Lets custom chrome reuse docking behavior:
///
/// ```ignore
/// use repose_docking::{DockModifierExt, DockHandle};
/// Row(Modifier::new().dock_tab_source(&dock, panel_id)).child(...)
/// ```
pub trait DockModifierExt: Sized {
    /// Make this node a draggable dock-tab source.
    fn dock_tab_source(self, dock: &DockHandle, panel_id: PanelId) -> Modifier;

    /// Make a tab strip accept dropped tabs for reorder/insert.
    fn dock_tab_strip_drop_target(
        self,
        dock: &DockHandle,
        node_id: u64,
        tabbar_rect: Rc<RefCell<Rect>>,
    ) -> Modifier;

    /// Make this node one specific dock drop zone.
    fn dock_drop_zone(self, dock: &DockHandle, node_id: u64, zone: DropZone) -> Modifier;

    /// Make this node the outer "float/popout" target.
    fn dock_float_target(self, dock: &DockHandle) -> Modifier;
}

impl DockModifierExt for Modifier {
    fn dock_tab_source(self, dock: &DockHandle, panel_id: PanelId) -> Modifier {
        let drag_active_start = dock.drag_active.clone();

        let hover_end = dock.hover_sig.clone();
        let drag_active_end = dock.drag_active.clone();

        self.cursor(CursorIcon::Grab)
            .drag_source::<DockTabPayload>(move |_start| {
                drag_active_start.set(true);
                Some(DockTabPayload { panel_id })
            })
            .on_drag_end(move |_end| {
                drag_active_end.set(false);
                hover_end.set(None);
            })
    }

    fn dock_tab_strip_drop_target(
        self,
        dock: &DockHandle,
        node_id: u64,
        tabbar_rect: Rc<RefCell<Rect>>,
    ) -> Modifier {
        let state = dock.state.clone();
        let hover_sig = dock.hover_sig.clone();
        let drag_active = dock.drag_active.clone();

        self.on_drop_typed::<DockTabPayload>(move |ev, p| {
            let mut st = state.borrow_mut();

            // Preserve current logic: remove without normalizing so node_id remains valid.
            st.remove_panel_no_normalize(p.panel_id);

            let r = *tabbar_rect.borrow();
            let t = if r.w > 1.0 {
                ((ev.position.x - r.x) / r.w).clamp(0.0, 1.0)
            } else {
                1.0
            };

            if let Some(n) = find_node_mut(&mut st.root, node_id) {
                if matches!(n.kind, DockKind::Empty) {
                    n.kind = DockKind::Tabs {
                        tabs: Vec::new(),
                        active: None,
                    };
                }

                if let DockKind::Tabs { tabs, active } = &mut n.kind {
                    tabs.retain(|&x| x != p.panel_id);
                    let idx = ((t * (tabs.len() as f32 + 1.0)).floor() as usize).min(tabs.len());
                    tabs.insert(idx, p.panel_id);
                    *active = Some(p.panel_id);
                }
            }

            st.normalize();
            hover_sig.set(None);
            drag_active.set(false);
            request_frame();
            true
        })
    }

    fn dock_drop_zone(self, dock: &DockHandle, node_id: u64, zone: DropZone) -> Modifier {
        let hover_enter = dock.hover_sig.clone();
        let hover_over = dock.hover_sig.clone();
        let hover_leave = dock.hover_sig.clone();
        let hover_drop = dock.hover_sig.clone();
        let state = dock.state.clone();

        self.z_index(3000.0)
            .render_z_index(3000.0)
            .key(hash_zone_key(node_id, zone))
            .on_drag_enter_typed::<DockTabPayload>(move |_ev, _p| {
                hover_enter.set(Some(HoverHint { node_id, zone }));
            })
            .on_drag_over_typed::<DockTabPayload>(move |_ev, _p| {
                hover_over.set(Some(HoverHint { node_id, zone }));
            })
            .on_drag_leave_typed::<DockTabPayload>(move |_ev, _p| {
                if hover_leave.get().as_ref() == Some(&HoverHint { node_id, zone }) {
                    hover_leave.set(None);
                }
            })
            .on_drop_typed::<DockTabPayload>(move |_ev, p| {
                let ok = state.borrow_mut().dock_panel(node_id, zone, p.panel_id);
                hover_drop.set(None);
                request_frame();
                ok
            })
    }

    fn dock_float_target(self, dock: &DockHandle) -> Modifier {
        let state = dock.state.clone();
        let hover_sig = dock.hover_sig.clone();
        let cb_pop = dock.callbacks.on_popout.clone();

        self.on_drop_typed::<DockTabPayload>(move |_ev, p| {
            let Some(pop) = cb_pop.as_ref() else {
                return false;
            };

            state.borrow_mut().remove_panel(p.panel_id);
            pop(p.panel_id);
            hover_sig.set(None);
            request_frame();
            true
        })
    }
}

pub fn DockArea(
    key: impl Into<String>,
    modifier: Modifier,
    state: Rc<RefCell<DockState>>,
    panels: Vec<DockPanel>,
    callbacks: DockCallbacks,
) -> View {
    let key = key.into();
    let registry = Rc::new(build_registry(panels));

    let dock = remember_dock_handle(key.clone(), state, callbacks);

    let split_hover = remember_with_key(format!("dock:split_hover:{key}"), || signal(None::<u64>));
    let split_drag = remember_with_key(format!("dock:split_drag:{key}"), || {
        RefCell::new(None::<SplitDrag>)
    });

    // Outer "float" drop target: if you drop a tab anywhere not handled by inner targets.
    // We set z-index low so inner targets win.
    let float_target = Box(
        Modifier::new()
            .fill_max_size()
            .z_index(-1000.0)
            .dock_float_target(&dock),
    );

    // Actual docking UI
    let root_view = {
        let st = dock.state.borrow().clone();
        render_node(
            &st.root,
            &registry,
            &dock,
            &split_hover,
            &split_drag,
            key.as_str(),
        )
    };

    ZStack(modifier.fill_max_size()).child((
        Box(Modifier::new()
            .absolute()
            .offset(Some(0.0), Some(0.0), Some(0.0), Some(0.0)))
        .child(float_target),
        Box(Modifier::new()
            .absolute()
            .offset(Some(0.0), Some(0.0), Some(0.0), Some(0.0)))
        .child(root_view),
    ))
}

fn build_registry(panels: Vec<DockPanel>) -> HashMap<PanelId, DockPanel> {
    let mut m = HashMap::new();
    for p in panels {
        m.insert(p.id, p);
    }
    m
}

fn render_node(
    node: &DockNode,
    registry: &Rc<HashMap<PanelId, DockPanel>>,
    dock: &DockHandle,
    split_hover: &Signal<Option<u64>>,
    split_drag: &Rc<RefCell<Option<SplitDrag>>>,
    key_prefix: &str,
) -> View {
    match &node.kind {
        DockKind::Empty => Box(
            Modifier::new()
                .fill_max_size()
                .padding(6.0)
                .background(theme().surface_container_lowest)
                .clip_rounded(theme().shapes.medium)
                .border(
                    1.0,
                    theme().outline_variant.with_alpha(80),
                    theme().shapes.medium,
                )
                .key(node.id),
        )
        .child(
            Box(Modifier::new().fill_max_size().padding(16.0)).child(
                Text("Drop panel here")
                    .size(theme().typography.label_medium)
                    .color(theme().on_surface_variant),
            ),
        ),

        DockKind::Tabs { tabs, active } => render_tabs(
            node.id,
            tabs,
            *active,
            registry,
            dock,
            split_hover,
            key_prefix,
        ),

        DockKind::Split { dir, ratio, a, b } => render_split(
            node.id,
            *dir,
            *ratio,
            a,
            b,
            registry,
            dock,
            split_hover,
            split_drag,
            key_prefix,
        ),
    }
}

fn render_tabs(
    node_id: u64,
    tabs: &Vec<PanelId>,
    active: Option<PanelId>,
    registry: &Rc<HashMap<PanelId, DockPanel>>,
    dock: &DockHandle,
    _split_hover: &Signal<Option<u64>>,
    key_prefix: &str,
) -> View {
    let th = theme();

    const PANEL_PAD: f32 = 5.0;
    const TAB_BAR_H: f32 = 44.0;
    const TAB_H: f32 = 32.0;
    const TAB_RADIUS: f32 = 16.0;

    // Ensure active is valid
    let active_pid = active.or_else(|| tabs.first().copied());

    let tabbar_rect = remember_with_key(format!("dock:tabbar_rect:{key_prefix}:{node_id}"), || {
        RefCell::new(Rect::default())
    });

    let strip_bg = th.surface_container_low;
    let active_bg = th.secondary_container;
    let active_fg = th.on_secondary_container;
    let inactive_fg = th.on_surface_variant;
    let hover_bg = th.surface_container_high;

    let mut bar_mod = Modifier::new()
        .fill_max_width()
        .height(TAB_BAR_H)
        .background(strip_bg)
        .padding_values(PaddingValues {
            left: 8.0,
            right: 8.0,
            top: 6.0,
            bottom: 6.0,
        })
        .gap(6.0)
        .painter({
            let tabbar_rect = tabbar_rect.clone();
            move |_scene, r, _alpha| *tabbar_rect.borrow_mut() = r
        });

    if dock.drag_active.get() {
        bar_mod = bar_mod.dock_tab_strip_drop_target(dock, node_id, tabbar_rect.clone());
    }

    let tab_bar = Row(bar_mod).with_children(
        tabs.iter()
            .copied()
            .filter_map(|pid| {
                let panel = registry.get(&pid)?;
                let is_active = Some(pid) == active_pid;
                let is_hovered = dock.tab_hover.get() == Some(pid);

                let state_set = dock.state.clone();
                let title = panel.title.clone();
                let drag_pid = pid;

                let cb_close = dock.callbacks.on_close.clone();
                let cb_pop = dock.callbacks.on_popout.clone();

                let tab_bg = if is_active {
                    active_bg
                } else if is_hovered {
                    hover_bg
                } else {
                    Color::TRANSPARENT
                };

                let tab_fg = if is_active { active_fg } else { inactive_fg };

                let hover_in = {
                    let tab_hover = dock.tab_hover.clone();
                    move |_| tab_hover.set(Some(pid))
                };

                let hover_out = {
                    let tab_hover = dock.tab_hover.clone();
                    move |_| {
                        if tab_hover.get() == Some(pid) {
                            tab_hover.set(None);
                        }
                    }
                };

                let pop_view = if let Some(pop) = cb_pop {
                    let state_for_pop = dock.state.clone();
                    dock_tab_icon_button("↗", tab_fg, move |_| {
                        state_for_pop.borrow_mut().remove_panel(pid);
                        pop(pid);
                        request_frame();
                    })
                } else {
                    Box(Modifier::new())
                };

                let close_view = if let Some(close) = cb_close {
                    dock_tab_icon_button("×", tab_fg, move |_| {
                        close(pid);
                        request_frame();
                    })
                } else {
                    Box(Modifier::new())
                };

                Some(
                    Row(
                        Modifier::new()
                            .key(pid)
                            .height(TAB_H)
                            .min_width(108.0)
                            .max_width(240.0)
                            .clip_rounded(TAB_RADIUS)
                            .background(tab_bg)
                            .padding_values(PaddingValues {
                                left: 12.0,
                                right: 4.0,
                                top: 0.0,
                                bottom: 0.0,
                            })
                            .gap(4.0)
                            .clickable()
                            .on_pointer_enter(hover_in)
                            .on_pointer_leave(hover_out)
                            .on_pointer_down({
                                let state_set = state_set.clone();
                                move |_| {
                                    state_set.borrow_mut().set_active(node_id, pid);
                                    request_frame();
                                }
                            })
                            .dock_tab_source(dock, drag_pid),
                    )
                    .child((
                        Box(
                            Modifier::new()
                                .height(TAB_H)
                                .weight(1.0)
                                .padding_values(PaddingValues {
                                    left: 0.0,
                                    right: 4.0,
                                    top: 0.0,
                                    bottom: 0.0,
                                })
                                .content_alignment(Alignment::Center),
                        )
                        .child(
                            Text(title)
                                .size(th.typography.label_large)
                                .single_line()
                                .overflow_ellipsize()
                                .color(tab_fg),
                        ),
                        pop_view,
                        close_view,
                    )),
                )
            })
            .collect::<Vec<_>>(),
    );

    // Content
    let content = if let Some(pid) = active_pid {
        if let Some(panel) = registry.get(&pid) {
            (panel.content)()
        } else {
            Text("Missing panel").color(th.error)
        }
    } else {
        Text("No tabs").color(th.on_surface_variant)
    };

    // Drop zones overlay (present only while dragging)
    let overlay = dock_drop_overlay(node_id, dock, key_prefix);

    ZStack(Modifier::new().fill_max_size().key(node_id)).child((
        Column(
            Modifier::new()
                .fill_max_size()
                .padding(PANEL_PAD)
                .clip_rounded(th.shapes.medium)
                .background(th.surface_container_lowest)
                .border(1.0, th.outline_variant.with_alpha(70), th.shapes.medium),
        )
        .child((
            tab_bar,
            Box(
                Modifier::new()
                    .fill_max_size()
                    .background(th.surface_container_lowest),
            )
            .child(Box(Modifier::new().fill_max_size().padding(8.0)).child(content)),
        )),
        Box(Modifier::new()
            .absolute()
            .offset(
                Some(PANEL_PAD),
                Some(PANEL_PAD + TAB_BAR_H),
                Some(PANEL_PAD),
                Some(PANEL_PAD),
            )
            .render_z_index(2000.0))
        .child(overlay),
    ))
}

fn dock_tab_icon_button(
    label: &'static str,
    fg: Color,
    on_click: impl Fn(PointerEvent) + 'static,
) -> View {
    Box(
        Modifier::new()
            .size(26.0, 26.0)
            .padding(2.0)
            .clip_rounded(13.0)
            .background(fg.with_alpha(18))
            .clickable()
            .cursor(CursorIcon::Pointer)
            .on_pointer_down(on_click),
    )
    .child(
        Box(
            Modifier::new()
                .fill_max_size()
                .content_alignment(Alignment::Center),
        )
        .child(Text(label).size(14.0).color(fg)),
    )
}

fn dock_drop_overlay(node_id: u64, dock: &DockHandle, key_prefix: &str) -> View {
    let th = theme();

    if !dock.drag_active.get() {
        return Box(Modifier::new().hit_passthrough());
    }

    let zone_dp = 72.0;
    let hover = dock.hover_sig.get();

    let preview = if let Some(h) = hover.as_ref() {
        if h.node_id == node_id {
            dock_drop_preview(h.zone)
        } else {
            Box(Modifier::new())
        }
    } else {
        Box(Modifier::new())
    };

    let mk_zone = |zone: DropZone, m: Modifier| -> View { Box(m.dock_drop_zone(dock, node_id, zone)) };

    // Layout zones using absolute rects (no need for measured size):
    // left/right/top/bottom thickness = zone_dp; center = remainder.
    let left = mk_zone(
        DropZone::Left,
        Modifier::new()
            .absolute()
            .offset(Some(0.0), Some(0.0), None, Some(0.0))
            .width(zone_dp),
    );

    let right = mk_zone(
        DropZone::Right,
        Modifier::new()
            .absolute()
            .offset(None, Some(0.0), Some(0.0), Some(0.0))
            .width(zone_dp),
    );

    let top = mk_zone(
        DropZone::Top,
        Modifier::new()
            .absolute()
            .offset(Some(zone_dp), Some(0.0), Some(zone_dp), None)
            .height(zone_dp),
    );

    let bottom = mk_zone(
        DropZone::Bottom,
        Modifier::new()
            .absolute()
            .offset(Some(zone_dp), None, Some(zone_dp), Some(0.0))
            .height(zone_dp),
    );

    let center = mk_zone(
        DropZone::Center,
        Modifier::new().absolute().offset(
            Some(zone_dp),
            Some(zone_dp),
            Some(zone_dp),
            Some(zone_dp),
        ),
    );

    ZStack(
        Modifier::new()
            .fill_max_size()
            .key(hash_str_key(key_prefix, node_id)),
    )
    .child((
        // Subtle drag-mode scrim.
        Box(
            Modifier::new()
                .fill_max_size()
                .background(th.scrim.with_alpha(18))
                .hit_passthrough()
                .render_z_index(1000.0),
        ),
        Box(Modifier::new()
            .fill_max_size()
            .hit_passthrough()
            .render_z_index(2000.0))
        .child(preview),
        left,
        right,
        top,
        bottom,
        center,
    ))
}

fn dock_drop_preview(zone: DropZone) -> View {
    let th = theme();

    let fill = th
        .primary
        .with_alpha(38)
        .composite_over(th.surface_container_lowest);
    let border = th.primary.with_alpha(210);
    let radius = th.shapes.large;

    let card = |label: &'static str, modifier: Modifier| -> View {
        Box(
            modifier
                .clip_rounded(radius)
                .background(fill)
                .border(2.0, border, radius),
        )
        .child(
            Box(Modifier::new().padding(12.0)).child(
                Text(label)
                    .size(th.typography.label_medium)
                    .single_line()
                    .color(th.primary),
            ),
        )
    };

    match zone {
        DropZone::Center => card(
            "Add as tab",
            Modifier::new()
                .absolute()
                .offset(Some(14.0), Some(14.0), Some(14.0), Some(14.0)),
        ),

        DropZone::Left => Row(Modifier::new().fill_max_size().padding(14.0).gap(10.0)).child((
            card("Split left", Modifier::new().weight(0.44).fill_max_height()),
            Box(Modifier::new().weight(0.56)),
        )),

        DropZone::Right => Row(Modifier::new().fill_max_size().padding(14.0).gap(10.0)).child((
            Box(Modifier::new().weight(0.56)),
            card("Split right", Modifier::new().weight(0.44).fill_max_height()),
        )),

        DropZone::Top => Column(
            Modifier::new()
                .fill_max_size()
                .padding(14.0)
                .gap(10.0),
        )
        .child((
            card("Split top", Modifier::new().weight(0.44).fill_max_width()),
            Box(Modifier::new().weight(0.56)),
        )),

        DropZone::Bottom => Column(
            Modifier::new()
                .fill_max_size()
                .padding(14.0)
                .gap(10.0),
        )
        .child((
            Box(Modifier::new().weight(0.56)),
            card("Split bottom", Modifier::new().weight(0.44).fill_max_width()),
        )),

        DropZone::Float => Box(Modifier::new()),
    }
}

fn render_split(
    node_id: u64,
    dir: SplitDir,
    ratio: f32,
    a: &DockNode,
    b: &DockNode,
    registry: &Rc<HashMap<PanelId, DockPanel>>,
    dock: &DockHandle,
    split_hover: &Signal<Option<u64>>,
    split_drag: &Rc<RefCell<Option<SplitDrag>>>,
    key_prefix: &str,
) -> View {
    let th = theme();
    let ratio = ratio.clamp(0.05, 0.95);

    // Track this split container rect so the divider can compute ratio from pointer position.
    let rect_rc = remember_with_key(format!("dock:split_rect:{}:{node_id}", key_prefix), || {
        RefCell::new(Rect::default())
    });

    // Paint-only hook to store rect
    let track = {
        let rect_rc = rect_rc.clone();
        Modifier::new().painter(move |_scene, r, _alpha| {
            *rect_rc.borrow_mut() = r;
        })
    };

    let divider_thick = 8.0;

    let start_drag = {
        let split_drag = split_drag.clone();
        move |_pe: PointerEvent| {
            *split_drag.borrow_mut() = Some(SplitDrag { node_id, dir });
            request_frame();
        }
    };

    let move_drag = {
        let split_drag = split_drag.clone();
        let rect_rc = rect_rc.clone();
        let state = dock.state.clone();
        move |pe: PointerEvent| {
            let Some(sd) = split_drag.borrow().clone() else {
                return;
            };
            if sd.node_id != node_id {
                return;
            }
            let r = *rect_rc.borrow();
            if r.w <= 1.0 || r.h <= 1.0 {
                return;
            }
            let mut t = match dir {
                SplitDir::Horizontal => (pe.position.x - r.x) / r.w,
                SplitDir::Vertical => (pe.position.y - r.y) / r.h,
            };
            for snap in [0.25_f32, 0.5, 0.75] {
                if (t - snap).abs() < 0.018 {
                    t = snap;
                    break;
                }
            }
            state.borrow_mut().set_split_ratio(node_id, t);
            request_frame();
        }
    };

    let end_drag = {
        let split_drag = split_drag.clone();
        move |_pe: PointerEvent| {
            // end any split drag
            *split_drag.borrow_mut() = None;
            request_frame();
        }
    };

    // M3-ish splitter: big invisible hit target, subtle tonal gutter,
    // rounded grabber only on hover/drag.
    let hovered = split_hover.get() == Some(node_id);
    let dragging = split_drag
        .borrow()
        .as_ref()
        .map(|sd| sd.node_id == node_id)
        .unwrap_or(false);

    let active = hovered || dragging;

    let gutter_color = if active {
        th.primary.with_alpha(24)
    } else {
        Color::TRANSPARENT
    };

    let grabber_color = if active {
        th.primary
    } else {
        th.outline_variant.with_alpha(0)
    };

    let splitter_mod = match dir {
        SplitDir::Horizontal => Modifier::new().width(divider_thick).fill_max_height(),
        SplitDir::Vertical => Modifier::new().height(divider_thick).fill_max_width(),
    };

    let grabber = match dir {
        SplitDir::Horizontal => Box(
            Modifier::new()
                .absolute()
                .offset(Some(2.0), Some(24.0), None, Some(24.0))
                .width(4.0)
                .fill_max_height()
                .clip_rounded(4.0)
                .background(grabber_color),
        ),
        SplitDir::Vertical => Box(
            Modifier::new()
                .absolute()
                .offset(Some(24.0), Some(2.0), Some(24.0), None)
                .height(4.0)
                .fill_max_width()
                .clip_rounded(4.0)
                .background(grabber_color),
        ),
    };

    let divider = Box(
        splitter_mod
            .background(gutter_color)
            .on_pointer_enter({
                let split_hover = split_hover.clone();
                move |_| {
                    split_hover.set(Some(node_id));
                    request_frame();
                }
            })
            .on_pointer_leave({
                let split_hover = split_hover.clone();
                move |_| {
                    if split_hover.get() == Some(node_id) {
                        split_hover.set(None);
                        request_frame();
                    }
                }
            })
            .on_pointer_down(start_drag)
            .on_pointer_move(move_drag)
            .on_pointer_up(end_drag)
            .cursor(match dir {
                SplitDir::Horizontal => CursorIcon::EwResize,
                SplitDir::Vertical => CursorIcon::NsResize,
            })
            .z_index(1500.0)
            .render_z_index(1500.0),
    )
    .child(grabber);

    let a_view = render_node(
        a,
        registry,
        dock,
        split_hover,
        split_drag,
        key_prefix,
    );
    let b_view = render_node(
        b,
        registry,
        dock,
        split_hover,
        split_drag,
        key_prefix,
    );

    match dir {
        SplitDir::Horizontal => Row(track.fill_max_size().key(node_id)).child((
            Box(Modifier::new().weight(ratio)).child(a_view),
            divider,
            Box(Modifier::new().weight(1.0 - ratio)).child(b_view),
        )),
        SplitDir::Vertical => Column(track.fill_max_size().key(node_id)).child((
            Box(Modifier::new().weight(ratio)).child(a_view),
            divider,
            Box(Modifier::new().weight(1.0 - ratio)).child(b_view),
        )),
    }
}

fn find_node_mut(node: &mut DockNode, id: u64) -> Option<&mut DockNode> {
    if node.id == id {
        return Some(node);
    }
    match &mut node.kind {
        DockKind::Split { a, b, .. } => find_node_mut(a, id).or_else(|| find_node_mut(b, id)),
        _ => None,
    }
}

fn remove_panel_in_node(node: &mut DockNode, pid: PanelId) -> bool {
    match &mut node.kind {
        DockKind::Empty => false,

        DockKind::Tabs { tabs, active } => {
            let before = tabs.len();
            tabs.retain(|&x| x != pid);
            if tabs.len() != before {
                if active == &Some(pid) {
                    *active = tabs.first().copied();
                }
                if tabs.is_empty() {
                    node.kind = DockKind::Empty;
                }
                true
            } else {
                false
            }
        }

        DockKind::Split { a, b, .. } => {
            let ra = remove_panel_in_node(a, pid);
            let rb = remove_panel_in_node(b, pid);
            ra || rb
        }
    }
}

fn normalize_node(node: &mut DockNode) {
    match &mut node.kind {
        DockKind::Empty => {}
        DockKind::Tabs { tabs, active } => {
            if tabs.is_empty() {
                node.kind = DockKind::Empty;
            } else if active.is_none() || !tabs.contains(&active.unwrap()) {
                *active = tabs.first().copied();
            }
        }
        DockKind::Split { a, b, ratio, .. } => {
            *ratio = ratio.clamp(0.05, 0.95);
            normalize_node(a);
            normalize_node(b);

            let a_empty = matches!(a.kind, DockKind::Empty);
            let b_empty = matches!(b.kind, DockKind::Empty);

            // Collapse empties
            if a_empty && !b_empty {
                node.kind = std::mem::replace(&mut b.kind, DockKind::Empty);
            } else if b_empty && !a_empty {
                node.kind = std::mem::replace(&mut a.kind, DockKind::Empty);
            } else if a_empty && b_empty {
                node.kind = DockKind::Empty;
            }
        }
    }
}

fn hash_zone_key(node_id: u64, zone: DropZone) -> u64 {
    let z = match zone {
        DropZone::Center => 1u64,
        DropZone::Left => 2,
        DropZone::Right => 3,
        DropZone::Top => 4,
        DropZone::Bottom => 5,
        DropZone::Float => 6,
    };
    node_id ^ (z.wrapping_mul(0x9E3779B97F4A7C15))
}

fn hash_str_key(prefix: &str, node_id: u64) -> u64 {
    let mut h = 1469598103934665603u64;
    for b in prefix.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211u64);
    }
    h ^ node_id.wrapping_mul(0x9E3779B97F4A7C15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_tab_into_center() {
        let mut st = DockState::new_with_tabs(vec![1, 2, 3]);
        // Create a second tabs node by splitting
        assert!(st.dock_panel(1, DropZone::Right, 3));
        // Root is now a Split node; docking center into a Split should fail
        assert!(!st.dock_panel(st.root.id, DropZone::Center, 2));
    }

    #[test]
    fn remove_collapses_empty_split() {
        let mut st = DockState::new_with_tabs(vec![10]);
        assert!(st.dock_panel(1, DropZone::Right, 20)); // split created
        assert!(st.remove_panel(10));
        st.normalize();
        // should still not be empty (20 remains)
        // root may collapse; ensure at least one tab exists somewhere
        fn count_tabs(n: &DockNode) -> usize {
            match &n.kind {
                DockKind::Tabs { tabs, .. } => tabs.len(),
                DockKind::Split { a, b, .. } => count_tabs(a) + count_tabs(b),
                DockKind::Empty => 0,
            }
        }
        assert_eq!(count_tabs(&st.root), 1);
    }
}
