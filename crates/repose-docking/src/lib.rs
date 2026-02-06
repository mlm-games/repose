#![allow(non_snake_case)]

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use repose_core::*;
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
        if let Some(n) = find_node_mut(&mut self.root, tabs_node_id) {
            if let DockKind::Tabs { tabs, active } = &mut n.kind {
                if tabs.contains(&pid) {
                    *active = Some(pid);
                }
            }
        }
    }

    pub fn set_split_ratio(&mut self, split_node_id: u64, ratio: f32) {
        let ratio = ratio.clamp(0.05, 0.95);
        if let Some(n) = find_node_mut(&mut self.root, split_node_id) {
            if let DockKind::Split { ratio: r, .. } = &mut n.kind {
                *r = ratio;
            }
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

pub fn DockArea(
    key: impl Into<String>,
    modifier: Modifier,
    state: Rc<RefCell<DockState>>,
    panels: Vec<DockPanel>,
    callbacks: DockCallbacks,
) -> View {
    let key = key.into();
    let registry = Rc::new(build_registry(panels));

    // Ephemeral UI state (per DockArea)
    let hover_sig = remember_with_key(format!("dock:hover:{key}"), || signal(None::<HoverHint>));
    let drag_active = remember_with_key(format!("dock:drag_active:{key}"), || signal(false));
    let split_hover = remember_with_key(format!("dock:split_hover:{key}"), || signal(None::<u64>));
    let split_drag = remember_with_key(format!("dock:split_drag:{key}"), || {
        RefCell::new(None::<SplitDrag>)
    });

    // Outer "float" drop target: if you drop a tab anywhere not handled by inner targets.
    // We set z-index low so inner targets win.
    let float_target = {
        let state = state.clone();
        let hover_sig = hover_sig.clone();
        let cb_pop = callbacks.on_popout.clone();

        Box(Modifier::new()
            .fill_max_size()
            .z_index(-1000.0)
            .on_drop(move |ev| {
                // Only accept docking payloads
                let Some(p) = ev.payload.as_ref().downcast_ref::<DockTabPayload>() else {
                    return false;
                };
                let Some(pop) = cb_pop.as_ref() else {
                    return false;
                };

                // Remove from dock tree, then pop out
                state.borrow_mut().remove_panel(p.panel_id);
                pop(p.panel_id);

                hover_sig.set(None);
                true
            }))
    };

    // Actual docking UI
    let root_view = {
        let st = state.borrow().clone();
        render_node(
            &st.root,
            &registry,
            &state,
            &callbacks,
            &hover_sig,
            &drag_active,
            &split_hover,
            &split_drag,
            key.as_str(),
        )
    };

    Stack(modifier.fill_max_size()).child((
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
    state: &Rc<RefCell<DockState>>,
    callbacks: &DockCallbacks,
    hover_sig: &Signal<Option<HoverHint>>,
    drag_active: &Signal<bool>,
    split_hover: &Signal<Option<u64>>,
    split_drag: &Rc<RefCell<Option<SplitDrag>>>,
    key_prefix: &str,
) -> View {
    match &node.kind {
        DockKind::Empty => Surface(
            Modifier::new()
                .fill_max_size()
                .background(theme().surface)
                .key(node.id),
            Box(Modifier::new().fill_max_size()).child(Text("Empty").color(theme().on_surface)),
        ),

        DockKind::Tabs { tabs, active } => render_tabs(
            node.id,
            tabs,
            *active,
            registry,
            state,
            callbacks,
            hover_sig,
            drag_active,
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
            state,
            callbacks,
            hover_sig,
            drag_active,
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
    state: &Rc<RefCell<DockState>>,
    callbacks: &DockCallbacks,
    hover_sig: &Signal<Option<HoverHint>>,
    drag_active: &Signal<bool>,
    _split_hover: &Signal<Option<u64>>,
    key_prefix: &str,
) -> View {
    let th = theme();

    // Ensure active is valid
    let active_pid = active.or_else(|| tabs.first().copied());

    let tabbar_rect = remember_with_key(format!("dock:tabbar_rect:{key_prefix}:{node_id}"), || {
        RefCell::new(Rect::default())
    });

    let mut bar_mod = Modifier::new()
        .fill_max_width()
        .height(40.0)
        .background(th.surface)
        .border(1.0, th.outline, 0.0)
        .padding(6.0)
        .painter({
            let tabbar_rect = tabbar_rect.clone();
            move |_scene, r| *tabbar_rect.borrow_mut() = r
        });

    if drag_active.get() {
        bar_mod = bar_mod.on_drop({
            let state = state.clone();
            let tabbar_rect = tabbar_rect.clone();
            let hover_sig = hover_sig.clone();
            let drag_active = drag_active.clone();

            move |ev| {
                let Some(p) = ev.payload.as_ref().downcast_ref::<DockTabPayload>() else {
                    return false;
                };

                let mut st = state.borrow_mut();

                // Always rm WITHOUT normalizing to preserve node_id validity
                st.remove_panel_no_normalize(p.panel_id);

                let r = *tabbar_rect.borrow();
                let t = if r.w > 1.0 {
                    ((ev.position.x - r.x) / r.w).clamp(0.0, 1.0)
                } else {
                    1.0
                };

                if let Some(n) = find_node_mut(&mut st.root, node_id) {
                    if let DockKind::Tabs { tabs, active } = &mut n.kind {
                        tabs.retain(|&x| x != p.panel_id);
                        let idx =
                            ((t * (tabs.len() as f32 + 1.0)).floor() as usize).min(tabs.len());
                        tabs.insert(idx, p.panel_id);
                        *active = Some(p.panel_id);
                    }
                }

                st.normalize();

                hover_sig.set(None);
                drag_active.set(false);
                request_frame();
                true
            }
        });
    }

    let tab_bar = Row(bar_mod).with_children(
        tabs.iter()
            .copied()
            .filter_map(|pid| {
                let panel = registry.get(&pid)?;
                let is_active = Some(pid) == active_pid;

                let state_set = state.clone();
                let title = panel.title.clone();

                let drag_pid = pid;

                let cb_close = callbacks.on_close.clone();
                let cb_pop = callbacks.on_popout.clone();

                Some(
                    Stack(
                        Modifier::new()
                            .key(pid)
                            .height(32.0)
                            .padding(4.0)
                            .clip_rounded(8.0)
                            .background(if is_active {
                                th.primary.with_alpha(80)
                            } else {
                                th.surface
                            })
                            .border(1.0, th.outline, 8.0),
                    )
                    .child((
                        Box(Modifier::new()
                            .height(24.0)
                            .offset(None, Some(4.0), None, None)
                            .clickable()
                            .cursor(CursorIcon::Grab)
                            .on_pointer_down({
                                let state_set = state_set.clone();
                                move |_| {
                                    state_set.borrow_mut().set_active(node_id, pid);
                                    request_frame();
                                }
                            })
                            .on_drag_start({
                                let drag_active = drag_active.clone();
                                move |_start| {
                                    drag_active.set(true);
                                    Some(Rc::new(DockTabPayload { panel_id: drag_pid })
                                        as Rc<dyn Any>)
                                }
                            })
                            .on_drag_end({
                                let hover_sig = hover_sig.clone();
                                let drag_active = drag_active.clone();
                                move |_end| {
                                    drag_active.set(false);
                                    hover_sig.set(None);
                                }
                            }))
                        .child(
                            Row(Modifier::new().height(24.0))
                                .child((Text(title).color(th.on_surface),)),
                        ),
                        Row(Modifier::new().absolute().height(24.0).offset(
                            None,
                            Some(4.0),
                            Some(2.0),
                            None,
                        ))
                        .child((
                            if let Some(pop) = cb_pop {
                                Button(Text("↗").size(12.0), move || pop(pid))
                                    .modifier(Modifier::new().padding(2.0))
                            } else {
                                Box(Modifier::new())
                            },
                            if let Some(close) = cb_close {
                                Button(Text("×").size(12.0), move || close(pid))
                                    .modifier(Modifier::new().padding(2.0))
                            } else {
                                Box(Modifier::new())
                            },
                        )),
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
        Text("No tabs").color(th.on_surface)
    };

    // Drop zones overlay (always present; highlight only when hovered)
    let overlay = dock_drop_overlay(node_id, state, hover_sig, drag_active, key_prefix);

    let tab_h = 40.0;

    Stack(Modifier::new().fill_max_size().key(node_id)).child((
        Column(Modifier::new().fill_max_size()).child((
            tab_bar,
            Surface(
                Modifier::new().fill_max_size().background(th.background),
                Box(Modifier::new().fill_max_size().padding(8.0)).child(content),
            ),
        )),
        Box(Modifier::new()
            .absolute()
            .offset(Some(0.0), Some(tab_h), Some(0.0), Some(0.0)))
        .child(overlay),
    ))
}

fn dock_drop_overlay(
    node_id: u64,
    state: &Rc<RefCell<DockState>>,
    hover_sig: &Signal<Option<HoverHint>>,
    drag_active: &Signal<bool>,
    key_prefix: &str,
) -> View {
    let th = theme();
    if !drag_active.get() {
        return Box(Modifier::new());
    }

    let zone_dp = 48.0;

    let hover = hover_sig.get();

    let mk_zone = |zone: DropZone, m: Modifier| -> View {
        let state2 = state.clone();
        let hover2 = hover_sig.clone();

        let label = match zone {
            DropZone::Center => "TAB",
            DropZone::Left => "LEFT",
            DropZone::Right => "RIGHT",
            DropZone::Top => "TOP",
            DropZone::Bottom => "BOTTOM",
            DropZone::Float => "FLOAT",
        };

        let highlight = if hover.as_ref() == Some(&HoverHint { node_id, zone }) {
            Stack(
                Modifier::new()
                    .fill_max_size()
                    .background(th.primary.with_alpha(51))
                    .border(2.0, th.primary, 0.0),
            )
            .child(Text(label).size(12.0).color(th.on_primary))
        } else {
            Stack(
                Modifier::new()
                    .fill_max_size()
                    .border(1.0, th.outline_variant, 0.0),
            )
            .child(Text(label).size(12.0).color(th.on_surface_variant))
        };

        Stack(
            m.z_index(2000.0)
                .key(hash_zone_key(node_id, zone))
                .on_drag_enter({
                    let hover2 = hover2.clone();
                    move |_ev| {
                        hover2.set(Some(HoverHint { node_id, zone }));
                    }
                })
                .on_drag_over({
                    let hover2 = hover2.clone();
                    move |_ev| {
                        hover2.set(Some(HoverHint { node_id, zone }));
                    }
                })
                .on_drag_leave({
                    let hover2 = hover2.clone();
                    move |_ev| {
                        // Only clear if we were hovering this
                        if hover2.get().as_ref() == Some(&HoverHint { node_id, zone }) {
                            hover2.set(None);
                        }
                    }
                })
                .on_drop(move |ev| {
                    let Some(p) = ev.payload.as_ref().downcast_ref::<DockTabPayload>() else {
                        return false;
                    };

                    let ok = state2.borrow_mut().dock_panel(node_id, zone, p.panel_id);
                    hover2.set(None);
                    request_frame();
                    ok
                }),
        )
        .child(highlight)
    };

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

    Stack(
        Modifier::new()
            .fill_max_size()
            .key(hash_str_key(key_prefix, node_id)),
    )
    .child((left, right, top, bottom, center))
}

fn render_split(
    node_id: u64,
    dir: SplitDir,
    ratio: f32,
    a: &DockNode,
    b: &DockNode,
    registry: &Rc<HashMap<PanelId, DockPanel>>,
    state: &Rc<RefCell<DockState>>,
    callbacks: &DockCallbacks,
    hover_sig: &Signal<Option<HoverHint>>,
    drag_active: &Signal<bool>,
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
        Modifier::new().painter(move |_scene, r| {
            *rect_rc.borrow_mut() = r;
        })
    };

    let divider_thick = 8.0;

    let start_drag = {
        let split_drag = split_drag.clone();
        move |_pe: PointerEvent| {
            *split_drag.borrow_mut() = Some(SplitDrag { node_id, dir });
        }
    };

    let move_drag = {
        let split_drag = split_drag.clone();
        let rect_rc = rect_rc.clone();
        let state = state.clone();
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
            let t = match dir {
                SplitDir::Horizontal => (pe.position.x - r.x) / r.w,
                SplitDir::Vertical => (pe.position.y - r.y) / r.h,
            };
            state.borrow_mut().set_split_ratio(node_id, t);
            request_frame();
        }
    };

    let end_drag = {
        let split_drag = split_drag.clone();
        move |_pe: PointerEvent| {
            // end any split drag
            *split_drag.borrow_mut() = None;
        }
    };

    // Visible splitter: thin line + thicker hit target (egui-ish)
    let hovered = split_hover.get() == Some(node_id);
    let line_color = if hovered { th.focus } else { th.outline };
    let hit_color = if hovered {
        line_color.with_alpha(40)
    } else {
        line_color.with_alpha(20)
    };

    let splitter_mod = match dir {
        SplitDir::Horizontal => Modifier::new().width(divider_thick).fill_max_height(),
        SplitDir::Vertical => Modifier::new().height(divider_thick).fill_max_width(),
    };

    let divider = Stack(
        splitter_mod
            .background(hit_color)
            .on_pointer_enter({
                let split_hover = split_hover.clone();
                move |_| split_hover.set(Some(node_id))
            })
            .on_pointer_leave({
                let split_hover = split_hover.clone();
                move |_| {
                    if split_hover.get() == Some(node_id) {
                        split_hover.set(None);
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
            .z_index(1500.0),
    )
    .child((
        // Center line
        match dir {
            SplitDir::Horizontal => Box(Modifier::new()
                .absolute()
                .offset(
                    Some((divider_thick - 1.0) * 0.5),
                    Some(0.0),
                    None,
                    Some(0.0),
                )
                .width(1.0)
                .fill_max_height()
                .background(line_color)),
            SplitDir::Vertical => Box(Modifier::new()
                .absolute()
                .offset(
                    Some(0.0),
                    Some((divider_thick - 1.0) * 0.5),
                    Some(0.0),
                    None,
                )
                .height(1.0)
                .fill_max_width()
                .background(line_color)),
        },
    ));

    let a_view = render_node(
        a,
        registry,
        state,
        callbacks,
        hover_sig,
        drag_active,
        split_hover,
        split_drag,
        key_prefix,
    );
    let b_view = render_node(
        b,
        registry,
        state,
        callbacks,
        hover_sig,
        drag_active,
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

fn find_node_mut<'a>(node: &'a mut DockNode, id: u64) -> Option<&'a mut DockNode> {
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
            } else {
                if active.is_none() || !tabs.contains(&active.unwrap()) {
                    *active = tabs.first().copied();
                }
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
        // Now root is split; find some tabs node to dock into
        // We dock panel 2 into root center should fail because root isn't tabs
        assert!(!st.dock_panel(1, DropZone::Center, 2));
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
