use std::cell::RefCell;
use std::rc::Rc;

use repose_core::{
    AlignItems, Color, CursorIcon, JustifyContent, Modifier, PaddingValues, PointerButton,
    PointerEvent, PointerEventKind, Rect, Size, StateColors, Vec2, View, request_frame,
};

use crate::{Box, Column, Row, Spacer, Text, TextStyle, ViewExt, ZStack};

const TITLE_BAR_HEIGHT_DP: f32 = 32.0;
const WINDOW_PADDING_DP: f32 = 8.0;
const RESIZE_HANDLE_DP: f32 = 10.0;
const WINDOW_Z_BASE: f32 = 10_000.0;
const WINDOW_Z_STEP: f32 = 10.0;
const KEEP_VISIBLE_DP: f32 = 24.0;

#[derive(Clone)]
pub struct WindowAction {
    pub label: String,
    pub on_click: Rc<dyn Fn()>,
}

#[derive(Clone)]
pub struct FloatingWindow {
    pub id: u64,
    pub title: String,
    pub content: Rc<dyn Fn() -> View>,
    pub on_close: Option<Rc<dyn Fn()>>,
    /// Position in dp from the host's top-left corner.
    pub position: Vec2,
    /// Size in dp.
    pub size: Size,
    /// Minimum size in dp.
    pub min_size: Size,
    /// Maximum size in dp (optional).
    pub max_size: Option<Size>,
    pub resizable: bool,
    pub closable: bool,
    pub draggable: bool,
    pub actions: Vec<WindowAction>,
}

impl FloatingWindow {
    pub fn new(id: u64, title: impl Into<String>, content: Rc<dyn Fn() -> View>) -> Self {
        Self {
            id,
            title: title.into(),
            content,
            on_close: None,
            position: Vec2 { x: 40.0, y: 40.0 },
            size: Size {
                width: 420.0,
                height: 300.0,
            },
            min_size: Size {
                width: 220.0,
                height: 160.0,
            },
            max_size: None,
            resizable: true,
            closable: true,
            draggable: true,
            actions: Vec::new(),
        }
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = Vec2 { x, y };
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = Size { width, height };
        self
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = Size { width, height };
        self
    }

    pub fn max_size(mut self, width: f32, height: f32) -> Self {
        self.max_size = Some(Size { width, height });
        self
    }

    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    pub fn draggable(mut self, draggable: bool) -> Self {
        self.draggable = draggable;
        self
    }

    pub fn actions(mut self, actions: Vec<WindowAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn on_close(mut self, on_close: Rc<dyn Fn()>) -> Self {
        self.on_close = Some(on_close);
        self
    }
}

#[derive(Clone, Default)]
pub struct WindowManagerState {
    pub windows: Vec<FloatingWindow>,
    next_id: u64,
    pub active: Option<u64>,
}

impl WindowManagerState {
    pub fn new() -> Self {
        Self {
            windows: Vec::new(),
            next_id: 1,
            active: None,
        }
    }

    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn open(&mut self, window: FloatingWindow) {
        let window_id = window.id;
        if let Some(pos) = self.windows.iter().position(|w| w.id == window_id) {
            self.windows[pos] = window;
        } else {
            self.windows.push(window);
        }
        self.bring_to_front(window_id);
    }

    pub fn close(&mut self, id: u64) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            self.windows.remove(idx);
            if self.active == Some(id) {
                self.active = self.windows.last().map(|w| w.id);
            }
            true
        } else {
            false
        }
    }

    pub fn bring_to_front(&mut self, id: u64) -> bool {
        if let Some(idx) = self.windows.iter().position(|w| w.id == id) {
            let window = self.windows.remove(idx);
            self.windows.push(window);
            self.active = Some(id);
            true
        } else {
            false
        }
    }

    pub fn set_position(&mut self, id: u64, position: Vec2) -> bool {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.position = position;
            true
        } else {
            false
        }
    }

    pub fn set_size(&mut self, id: u64, size: Size) -> bool {
        if let Some(w) = self.windows.iter_mut().find(|w| w.id == id) {
            w.size = size;
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ResizeHandle {
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    Move,
    Resize(ResizeHandle),
}

#[derive(Clone, Copy, Debug)]
struct DragState {
    window_id: u64,
    kind: DragKind,
    start_pointer: Vec2,
    start_pos: Vec2,
    start_size: Size,
    min_size: Size,
    max_size: Option<Size>,
}

pub fn WindowHost(
    key: impl Into<String>,
    modifier: Modifier,
    state: Rc<RefCell<WindowManagerState>>,
    content: View,
) -> View {
    let key = key.into();
    let bounds = repose_core::remember_with_key(format!("window:bounds:{key}"), || {
        RefCell::new(Rect::default())
    });
    let drag_state = repose_core::remember_with_key(format!("window:drag:{key}"), || {
        RefCell::new(None::<DragState>)
    });

    let bounds_capture = bounds.clone();
    let host_mod = modifier.painter(move |_scene, rect_px, _alpha| {
        let mut bounds_dp = rect_px_to_dp(rect_px);
        bounds_dp.x = 0.0;
        bounds_dp.y = 0.0;
        *bounds_capture.borrow_mut() = bounds_dp;
    });

    let active_id = state.borrow().active;
    let windows = state.borrow().windows.clone();

    let window_views = windows
        .into_iter()
        .enumerate()
        .map(|(idx, window)| {
            let z_base = WINDOW_Z_BASE + (idx as f32 * WINDOW_Z_STEP);
            let chrome_z = 2.0;
            let content_z = 1.0;

            let window_id = window.id;
            let window_actions = window.actions.clone();
            let window_closable = window.closable;
            let window_on_close = window.on_close.clone();
            let window_content = window.content.clone();
            let window_pos = window.position;
            let window_size = window.size;
            let window_title = window.title.clone();
            let window_draggable = window.draggable;
            let window_resizable = window.resizable;

            let is_active = active_id == Some(window_id);
            let th = repose_core::locals::theme();
            let border_color = if is_active {
                th.focus
            } else {
                th.outline_variant
            };
            let title_fg = if is_active {
                th.on_surface
            } else {
                th.on_surface_variant
            };
            let title_bg = if is_active {
                th.surface_variant
            } else {
                th.surface
            };

            let start_drag = {
                let drag_state = drag_state.clone();
                let state = state.clone();
                move |kind: DragKind, pe: PointerEvent| {
                    if !matches!(pe.event, PointerEventKind::Down(PointerButton::Primary)) {
                        return;
                    }

                    let (pos, size, min_size, max_size) = {
                        let st = state.borrow();
                        let Some(w) = st.windows.iter().find(|w| w.id == window_id) else {
                            return;
                        };
                        (w.position, w.size, w.min_size, w.max_size)
                    };

                    let start = DragState {
                        window_id,
                        kind,
                        start_pointer: px_vec_to_dp(pe.position),
                        start_pos: pos,
                        start_size: size,
                        min_size,
                        max_size,
                    };
                    *drag_state.borrow_mut() = Some(start);
                    state.borrow_mut().bring_to_front(window_id);
                    request_frame();
                }
            };

            let bring_to_front = {
                let state = state.clone();
                move || {
                    state.borrow_mut().bring_to_front(window_id);
                    request_frame();
                }
            };

            let move_drag = {
                let drag_state = drag_state.clone();
                let state = state.clone();
                let bounds = bounds.clone();
                move |pe: PointerEvent| {
                    let Some(ds) = *drag_state.borrow() else {
                        return;
                    };
                    if ds.window_id != window_id {
                        return;
                    }

                    let cur = px_vec_to_dp(pe.position);
                    let delta = Vec2 {
                        x: cur.x - ds.start_pointer.x,
                        y: cur.y - ds.start_pointer.y,
                    };
                    let bounds = *bounds.borrow();

                    let (mut pos, mut size) = match ds.kind {
                        DragKind::Move => (
                            Vec2 {
                                x: ds.start_pos.x + delta.x,
                                y: ds.start_pos.y + delta.y,
                            },
                            ds.start_size,
                        ),
                        DragKind::Resize(handle) => resize_from_handle(ds, handle, delta),
                    };

                    let (clamped_pos, clamped_size) =
                        clamp_rect(pos, size, ds.min_size, ds.max_size, bounds);
                    pos = clamped_pos;
                    size = clamped_size;

                    let mut st = state.borrow_mut();
                    st.set_position(window_id, pos);
                    st.set_size(window_id, size);
                    request_frame();
                }
            };

            let end_drag = {
                let drag_state = drag_state.clone();
                move |_pe: PointerEvent| {
                    *drag_state.borrow_mut() = None;
                }
            };

            let is_dragging = drag_state
                .borrow()
                .as_ref()
                .is_some_and(|d| d.window_id == window_id && d.kind == DragKind::Move);
            let title_cursor = if is_dragging {
                CursorIcon::Grabbing
            } else {
                CursorIcon::Grab
            };

            let title_bar = {
                let window_id = window_id;
                let actions = window_actions.clone();
                let close_enabled = window_closable;
                let close_state = state.clone();
                let close_handler = window_on_close.clone();
                let focus_state = state.clone();
                let mut action_views = Vec::new();

                for (idx, action) in actions.into_iter().enumerate() {
                    let label = action.label.clone();
                    let on_click = action.on_click.clone();
                    let focus_state = focus_state.clone();
                    let action_id = window_id;
                    action_views.push(
                        Row(Modifier::new()
                            .padding_values(PaddingValues {
                                left: 6.0,
                                right: 6.0,
                                top: 0.0,
                                bottom: 0.0,
                            })
                            .height(20.0)
                            .clip_rounded(10.0)
                            .justify_content(JustifyContent::CENTER)
                            .align_items(AlignItems::CENTER)
                            .state_colors(StateColors {
                                default: th.surface_variant,
                                hovered: th.on_surface.with_alpha(16),
                                pressed: th.on_surface.with_alpha(24),
                                disabled: Color::TRANSPARENT,
                            })
                            .clickable()
                            .on_pointer_down(move |_| {
                                focus_state.borrow_mut().bring_to_front(action_id);
                                (on_click)();
                                request_frame();
                            })
                            .z_index(1.0)
                            .key(key_for(window_id, 60 + idx as u64)))
                        .child(
                            Text(label)
                                .size(th.typography.label_medium)
                                .color(th.primary)
                                .single_line(),
                        ),
                    );
                }

                if close_enabled {
                    let close_id = window_id;
                    let focus_state = focus_state.clone();
                    action_views.push(
                        Row(Modifier::new()
                            .width(20.0)
                            .height(20.0)
                            .clip_rounded(10.0)
                            .justify_content(JustifyContent::CENTER)
                            .align_items(AlignItems::CENTER)
                            .state_colors(StateColors {
                                default: th.error.with_alpha(20),
                                hovered: th.error.with_alpha(40),
                                pressed: th.error.with_alpha(60),
                                disabled: Color::TRANSPARENT,
                            })
                            .clickable()
                            .on_pointer_down(move |_| {
                                focus_state.borrow_mut().bring_to_front(close_id);
                                if let Some(handler) = close_handler.as_ref() {
                                    (handler)();
                                } else {
                                    close_state.borrow_mut().close(close_id);
                                }
                                request_frame();
                            })
                            .z_index(1.0)
                            .key(key_for(window_id, 90)))
                        .child(
                            Text("\u{E5CD}")
                                .font_family("Material Symbols Outlined")
                                .size(14.0)
                                .color(th.error),
                        ),
                    );
                }

                let mut bar_mod = Modifier::new()
                    .fill_max_width()
                    .height(TITLE_BAR_HEIGHT_DP)
                    .background(title_bg)
                    .padding_values(PaddingValues {
                        left: 10.0,
                        right: 8.0,
                        top: 6.0,
                        bottom: 6.0,
                    })
                    .align_items(AlignItems::CENTER)
                    .key(key_for(window_id, 10));

                if window_draggable {
                    bar_mod = bar_mod
                        .cursor(title_cursor)
                        .on_pointer_down({
                            let start_drag = start_drag.clone();
                            move |pe| start_drag(DragKind::Move, pe)
                        })
                        .on_pointer_move(move_drag.clone())
                        .on_pointer_up(end_drag.clone());
                } else {
                    bar_mod = bar_mod.on_pointer_down(move |_| bring_to_front());
                }

                let bar = Row(bar_mod).child((
                    Text(window_title)
                        .size(th.typography.title_small)
                        .color(title_fg)
                        .single_line()
                        .overflow_ellipsize(),
                    Spacer(),
                    Row(Modifier::new().align_items(AlignItems::CENTER))
                        .with_children(action_views),
                ));

                apply_z_offset(bar, chrome_z)
            };

            let content_view = {
                let content_builder = window_content.clone();
                let focus_cb = {
                    let state = state.clone();
                    let window_id = window_id;
                    Rc::new(move || {
                        state.borrow_mut().bring_to_front(window_id);
                        request_frame();
                    })
                };
                let inner = inject_focus_handlers((content_builder)(), focus_cb);
                apply_z_offset(inner, content_z)
            };

            let content_shell =
                Box(Modifier::new().fill_max_size().padding(WINDOW_PADDING_DP)).child(content_view);

            let resize_handles = if window_resizable {
                let handles = build_resize_handles(
                    window_id,
                    start_drag.clone(),
                    move_drag.clone(),
                    end_drag.clone(),
                );
                apply_z_offset(handles, chrome_z + 1.0)
            } else {
                Box(Modifier::new())
            };

            let column = Column(Modifier::new().fill_max_size()).child((title_bar, content_shell));

            let focus_on_pointer_down = {
                let state = state.clone();
                move |pe: PointerEvent| {
                    if matches!(pe.event, PointerEventKind::Down(PointerButton::Primary)) {
                        state.borrow_mut().bring_to_front(window_id);
                        request_frame();
                    }
                }
            };

            let mut window_view = Box(Modifier::new()
                .key(key_for(window_id, 1))
                .absolute()
                .offset(Some(window_pos.x), Some(window_pos.y), None, None)
                .size(window_size.width, window_size.height)
                .background(th.surface_container_high)
                .border(1.0, border_color, th.shapes.medium)
                .clip_rounded(th.shapes.medium)
                .z_index(-1.0)
                .on_pointer_down(focus_on_pointer_down))
            .child(ZStack(Modifier::new().fill_max_size()).child((column, resize_handles)));
            window_view = apply_z_offset(window_view, z_base);
            window_view
        })
        .collect::<Vec<_>>();

    Column(host_mod).child((
        content,
        Box(Modifier::new()
            .absolute()
            .offset(Some(0.0), Some(0.0), Some(0.0), Some(0.0)))
        .child(Column(Modifier::new().fill_max_size()).with_children(window_views)),
    ))
}

fn build_resize_handles(
    window_id: u64,
    start_drag: impl Fn(DragKind, PointerEvent) + Clone + 'static,
    move_drag: impl Fn(PointerEvent) + Clone + 'static,
    end_drag: impl Fn(PointerEvent) + Clone + 'static,
) -> View {
    let handles = [
        (
            ResizeHandle::Left,
            handle_mod_left(),
            CursorIcon::EwResize,
            20,
        ),
        (
            ResizeHandle::Right,
            handle_mod_right(),
            CursorIcon::EwResize,
            21,
        ),
        (
            ResizeHandle::Top,
            handle_mod_top(),
            CursorIcon::NsResize,
            22,
        ),
        (
            ResizeHandle::Bottom,
            handle_mod_bottom(),
            CursorIcon::NsResize,
            23,
        ),
        (
            ResizeHandle::TopLeft,
            handle_mod_corner(true, true),
            CursorIcon::EwResize,
            24,
        ),
        (
            ResizeHandle::TopRight,
            handle_mod_corner(false, true),
            CursorIcon::EwResize,
            25,
        ),
        (
            ResizeHandle::BottomLeft,
            handle_mod_corner(true, false),
            CursorIcon::EwResize,
            26,
        ),
        (
            ResizeHandle::BottomRight,
            handle_mod_corner(false, false),
            CursorIcon::EwResize,
            27,
        ),
    ];

    Column(Modifier::new().fill_max_size()).with_children(
        handles
            .into_iter()
            .map(|(handle, modifier, cursor, key)| {
                Box(modifier
                    .cursor(cursor)
                    .on_pointer_down({
                        let start_drag = start_drag.clone();
                        move |pe| start_drag(DragKind::Resize(handle), pe)
                    })
                    .on_pointer_move(move_drag.clone())
                    .on_pointer_up(end_drag.clone())
                    .key(key_for(window_id, key)))
            })
            .collect::<Vec<_>>(),
    )
}

fn handle_mod_left() -> Modifier {
    Modifier::new()
        .absolute()
        .offset(Some(0.0), Some(0.0), None, Some(0.0))
        .width(RESIZE_HANDLE_DP)
}

fn handle_mod_right() -> Modifier {
    Modifier::new()
        .absolute()
        .offset(None, Some(0.0), Some(0.0), Some(0.0))
        .width(RESIZE_HANDLE_DP)
}

fn handle_mod_top() -> Modifier {
    Modifier::new()
        .absolute()
        .offset(Some(0.0), Some(0.0), Some(0.0), None)
        .height(RESIZE_HANDLE_DP)
}

fn handle_mod_bottom() -> Modifier {
    Modifier::new()
        .absolute()
        .offset(Some(0.0), None, Some(0.0), Some(0.0))
        .height(RESIZE_HANDLE_DP)
}

fn handle_mod_corner(left: bool, top: bool) -> Modifier {
    Modifier::new()
        .absolute()
        .offset(
            if left { Some(0.0) } else { None },
            if top { Some(0.0) } else { None },
            if left { None } else { Some(0.0) },
            if top { None } else { Some(0.0) },
        )
        .size(RESIZE_HANDLE_DP * 1.4, RESIZE_HANDLE_DP * 1.4)
}

fn resize_from_handle(ds: DragState, handle: ResizeHandle, delta: Vec2) -> (Vec2, Size) {
    let mut pos = ds.start_pos;
    let mut size = ds.start_size;

    match handle {
        ResizeHandle::Left => {
            pos.x += delta.x;
            size.width -= delta.x;
        }
        ResizeHandle::Right => {
            size.width += delta.x;
        }
        ResizeHandle::Top => {
            pos.y += delta.y;
            size.height -= delta.y;
        }
        ResizeHandle::Bottom => {
            size.height += delta.y;
        }
        ResizeHandle::TopLeft => {
            pos.x += delta.x;
            size.width -= delta.x;
            pos.y += delta.y;
            size.height -= delta.y;
        }
        ResizeHandle::TopRight => {
            size.width += delta.x;
            pos.y += delta.y;
            size.height -= delta.y;
        }
        ResizeHandle::BottomLeft => {
            pos.x += delta.x;
            size.width -= delta.x;
            size.height += delta.y;
        }
        ResizeHandle::BottomRight => {
            size.width += delta.x;
            size.height += delta.y;
        }
    }

    (pos, size)
}

fn clamp_rect(
    mut pos: Vec2,
    mut size: Size,
    min_size: Size,
    max_size: Option<Size>,
    bounds: Rect,
) -> (Vec2, Size) {
    let min_w = min_size.width.max(120.0);
    let min_h = min_size.height.max(TITLE_BAR_HEIGHT_DP + 40.0);
    size.width = size.width.max(min_w);
    size.height = size.height.max(min_h);

    if let Some(max) = max_size {
        size.width = size.width.min(max.width.max(min_w));
        size.height = size.height.min(max.height.max(min_h));
    }

    if bounds.w > 1.0 && bounds.h > 1.0 {
        let max_w = bounds.w.max(min_w);
        let max_h = bounds.h.max(min_h);
        size.width = size.width.min(max_w);
        size.height = size.height.min(max_h);

        let min_x = bounds.x - size.width + KEEP_VISIBLE_DP;
        let max_x = bounds.x + bounds.w - KEEP_VISIBLE_DP;
        let min_y = bounds.y - size.height + KEEP_VISIBLE_DP;
        let max_y = bounds.y + bounds.h - KEEP_VISIBLE_DP;

        pos.x = clamp_f32(pos.x, min_x, max_x);
        pos.y = clamp_f32(pos.y, min_y, max_y);
    }

    (pos, size)
}

fn clamp_f32(v: f32, min: f32, max: f32) -> f32 {
    if max < min { min } else { v.clamp(min, max) }
}

fn apply_z_offset(mut view: View, z: f32) -> View {
    view.modifier.z_index += z;
    if let Some(rz) = view.modifier.render_z_index {
        view.modifier.render_z_index = Some(rz + z);
    }
    view.children = view
        .children
        .into_iter()
        .map(|child| apply_z_offset(child, z))
        .collect();
    view
}

fn inject_focus_handlers(mut view: View, focus: Rc<dyn Fn()>) -> View {
    let needs_focus = modifier_handles_hit(&view.modifier)
        || view.modifier.text_input.is_some()
        || modifier_has_hit(&view.modifier);
    if needs_focus {
        let existing = view.modifier.on_pointer_down.clone();
        let focus_cb = focus.clone();
        view.modifier.on_pointer_down = Some(Rc::new(move |pe: PointerEvent| {
            if matches!(pe.event, PointerEventKind::Down(PointerButton::Primary)) {
                focus_cb();
            }
            if let Some(cb) = existing.as_ref() {
                cb(pe);
            }
        }));
    }

    view.children = view
        .children
        .into_iter()
        .map(|child| inject_focus_handlers(child, focus.clone()))
        .collect();
    view
}

fn modifier_handles_hit(modifier: &Modifier) -> bool {
    modifier.scroll.is_some()
}

fn modifier_has_hit(modifier: &Modifier) -> bool {
    modifier.click
        || modifier.on_action.is_some()
        || modifier.on_pointer_down.is_some()
        || modifier.on_pointer_move.is_some()
        || modifier.on_pointer_up.is_some()
        || modifier.on_pointer_enter.is_some()
        || modifier.on_pointer_leave.is_some()
        || modifier.on_drag_start.is_some()
        || modifier.on_drag_end.is_some()
        || modifier.on_drag_enter.is_some()
        || modifier.on_drag_over.is_some()
        || modifier.on_drag_leave.is_some()
        || modifier.on_drop.is_some()
}

fn key_for(window_id: u64, part: u64) -> u64 {
    window_id ^ (part.wrapping_mul(0x9E3779B97F4A7C15))
}

fn px_to_dp(px: f32) -> f32 {
    let scale = repose_core::locals::density().scale * repose_core::locals::ui_scale().0;
    if scale > 0.0001 { px / scale } else { px }
}

fn px_vec_to_dp(v: Vec2) -> Vec2 {
    Vec2 {
        x: px_to_dp(v.x),
        y: px_to_dp(v.y),
    }
}

fn rect_px_to_dp(r: Rect) -> Rect {
    Rect {
        x: px_to_dp(r.x),
        y: px_to_dp(r.y),
        w: px_to_dp(r.w),
        h: px_to_dp(r.h),
    }
}
