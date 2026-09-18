#![allow(non_snake_case)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use repose_core::*;
use repose_ui::{
    Box, Column, Text, TextStyle, ViewExt, ZStack, anim::animate_f32, overlay::OverlayGuard,
    overlay::ambient_overlay,
};
use web_time::Duration;

use super::*;

/// Where the tooltip sits relative to its anchor.
///
/// Mirrors Compose `TooltipAnchorPosition`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipAnchorPosition {
    #[default]
    Above,
    Below,
    Left,
    Right,
    Start,
    End,
}

impl TooltipAnchorPosition {
    fn resolve(self) -> Self {
        match self {
            Self::Start if text_direction() == TextDirection::Ltr => Self::Left,
            Self::Start => Self::Right,
            Self::End if text_direction() == TextDirection::Ltr => Self::Right,
            Self::End => Self::Left,
            other => other,
        }
    }
}

/// Which M3 color/typography/shape tokens the tooltip uses.
///
/// `Plain` matches `PlainTooltipTokens` (`InverseSurface` /
/// `InverseOnSurface`); `Rich` matches `RichTooltipTokens`
/// (`SurfaceContainer` / `OnSurfaceVariant`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TooltipKind {
    #[default]
    Plain,
    Rich,
}

/// An explicitly set `Color`, or "follow the current theme".
///
/// `TooltipConfig::default()` leaves colors unset so tooltips track theme
/// switches; pass `ThemedColor::Set(..)` to pin a color.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum ThemedColor {
    #[default]
    Theme,
    Set(Color),
}

impl ThemedColor {
    fn resolve(self, themed: Color) -> Color {
        match self {
            Self::Theme => themed,
            Self::Set(c) => c,
        }
    }
}

/// Configuration for tooltip.
#[derive(Clone, Debug)]
pub struct TooltipConfig {
    pub kind: TooltipKind,
    pub position: TooltipAnchorPosition,
    pub container_color: ThemedColor,
    pub content_color: ThemedColor,
    pub title_content_color: ThemedColor,
    pub action_content_color: ThemedColor,
    pub caret: bool,
    pub caret_size: DpSize,
    pub auto_dismiss_timeout: Duration,
    pub spacing: Dp,
    pub max_width: Dp,
    pub tonal_elevation: Dp,
    pub shadow_elevation: Dp,
    pub enable_user_input: bool,
    pub focusable: bool,
}

impl Default for TooltipConfig {
    fn default() -> Self {
        Self {
            kind: TooltipKind::Plain,
            position: TooltipAnchorPosition::Above,
            container_color: ThemedColor::Theme,
            content_color: ThemedColor::Theme,
            title_content_color: ThemedColor::Theme,
            action_content_color: ThemedColor::Theme,
            caret: false,
            caret_size: TooltipDefaults::CARET_SIZE,
            auto_dismiss_timeout: Duration::from_millis(TooltipDefaults::AUTO_DISMISS_TIMEOUT_MS),
            spacing: TooltipDefaults::SPACING,
            max_width: TooltipDefaults::PLAIN_MAX_WIDTH,
            tonal_elevation: Dp::ZERO,
            shadow_elevation: Dp::ZERO,
            enable_user_input: true,
            focusable: false,
        }
    }
}

impl TooltipConfig {
    pub fn rich() -> Self {
        Self {
            kind: TooltipKind::Rich,
            max_width: TooltipDefaults::RICH_MAX_WIDTH,
            shadow_elevation: TooltipDefaults::RICH_SHADOW_ELEVATION,
            ..Self::default()
        }
    }
}

/// State controlling tooltip visibility.
///
/// Non-persistent tooltips auto-dismiss after the config's
/// `auto_dismiss_timeout` (default 1500ms, mirroring Compose
/// `BasicTooltipDefaults.TooltipDuration`); persistent ones stay until
/// dismissed (for actionable content). Mirrors Compose
/// `TooltipState.isPersistent`.
pub struct TooltipState {
    visible: Signal<bool>,
    persistent: bool,
    timer: RefCell<Option<timer::TimerHandle>>,
    on_dismiss: RefCell<Option<Rc<dyn Fn()>>>,
}

impl Default for TooltipState {
    fn default() -> Self {
        Self::new()
    }
}

impl TooltipState {
    pub fn new() -> Self {
        Self::persistent(false)
    }

    pub fn persistent(persistent: bool) -> Self {
        Self {
            visible: signal(false),
            persistent,
            timer: RefCell::new(None),
            on_dismiss: RefCell::new(None),
        }
    }

    pub fn is_persistent(&self) -> bool {
        self.persistent
    }

    pub fn is_visible(&self) -> bool {
        self.visible.get()
    }

    pub fn on_dismiss_request(&self, cb: impl Fn() + 'static) {
        *self.on_dismiss.borrow_mut() = Some(Rc::new(cb));
    }

    /// Show with `timeout` as the auto-dismiss delay for non-persistent
    /// tooltips. `TooltipBox` passes `config.auto_dismiss_timeout`; call
    /// [`show`](Self::show) directly for the default.
    pub fn show_with_timeout(&self, timeout: Duration) {
        *self.timer.borrow_mut() = None;
        self.visible.set(true);
        if !self.persistent {
            self.timer.borrow_mut().replace(timer::delay(timeout, {
                let visible = self.visible.clone();
                let on_dismiss = self.on_dismiss.borrow().clone();
                move || {
                    visible.set(false);
                    if let Some(cb) = on_dismiss {
                        cb();
                    }
                }
            }));
        }
    }

    pub fn show(&self) {
        self.show_with_timeout(Duration::from_millis(
            TooltipDefaults::AUTO_DISMISS_TIMEOUT_MS,
        ));
    }

    pub fn dismiss(&self) {
        *self.timer.borrow_mut() = None;
        self.visible.set(false);
        if let Some(cb) = self.on_dismiss.borrow().clone() {
            cb();
        }
    }
}

impl Drop for TooltipState {
    fn drop(&mut self) {
        self.timer.borrow_mut().take();
    }
}

/// Horizontal clamp + above/below flip, mirroring Compose
/// `TooltipPositionProviderImpl.abovePositioning`/`belowPositioning`.
///
/// `window_w` is the window container width in Dp.
pub fn tooltip_position_above(
    anchor: Rect,
    popup_w: f32,
    popup_h: f32,
    spacing: f32,
    window_w: f32,
) -> (f32, f32) {
    let mut x = anchor.x + (anchor.w - popup_w) / 2.0;
    if x < 0.0 {
        let correction = (anchor.x + popup_w - window_w).max(0.0);
        x = anchor.x - correction;
    } else if x + popup_w > window_w {
        x = (anchor.x + anchor.w - popup_w).max(0.0);
    }
    let mut y = anchor.y - popup_h - spacing;
    if y < 0.0 {
        y = anchor.y + anchor.h + spacing;
    }
    (x, y)
}

/// Mirror of `tooltip_position_above` preferring below, mirroring Compose
/// `belowPositioning` (flips above only when overflowing the window bottom).
pub fn tooltip_position_below(
    anchor: Rect,
    popup_w: f32,
    popup_h: f32,
    spacing: f32,
    window_w: f32,
    window_h: f32,
) -> (f32, f32) {
    let (x, _) = tooltip_position_above(anchor, popup_w, popup_h, spacing, window_w);
    let mut y = anchor.y + anchor.h + spacing;
    if y + popup_h > window_h {
        y = anchor.y - popup_h - spacing;
    }
    (x, y)
}

/// Mirror of the horizontal helpers, mirroring Compose `leftPositioning`:
/// prefer left of the anchor, flip to the right on left-edge collision.
pub fn tooltip_position_left(
    anchor: Rect,
    popup_w: f32,
    popup_h: f32,
    spacing: f32,
    window_w: f32,
) -> (f32, f32) {
    let mut x = anchor.x - (popup_w + spacing);
    if x < 0.0 {
        let correction = (anchor.x + anchor.w + spacing + popup_w - window_w).max(0.0);
        x = anchor.x + anchor.w + spacing - correction;
    }
    let y = (anchor.y + anchor.y + anchor.h - popup_h) / 2.0;
    (x, y)
}

/// Mirror of [`tooltip_position_left`], mirroring Compose `rightPositioning`.
pub fn tooltip_position_right(
    anchor: Rect,
    popup_w: f32,
    popup_h: f32,
    spacing: f32,
    window_w: f32,
) -> (f32, f32) {
    let mut x = anchor.x + anchor.w + spacing;
    if x + popup_w > window_w {
        x = (anchor.x - (popup_w + spacing)).max(0.0);
    }
    let y = (anchor.y + anchor.y + anchor.h - popup_h) / 2.0;
    (x, y)
}

/// Position the popup for `position` relative to `anchor` (Dp magnitudes),
/// flipping/clamping into the `window_w` x `window_h` container like Compose
/// `TooltipPositionProviderImpl`.
pub fn tooltip_position(
    position: TooltipAnchorPosition,
    anchor: Rect,
    popup_w: f32,
    popup_h: f32,
    spacing: f32,
    window_w: f32,
    window_h: f32,
) -> (f32, f32) {
    match position.resolve() {
        TooltipAnchorPosition::Above => {
            tooltip_position_above(anchor, popup_w, popup_h, spacing, window_w)
        }
        TooltipAnchorPosition::Below => {
            tooltip_position_below(anchor, popup_w, popup_h, spacing, window_w, window_h)
        }
        TooltipAnchorPosition::Left => {
            tooltip_position_left(anchor, popup_w, popup_h, spacing, window_w)
        }
        TooltipAnchorPosition::Right => {
            tooltip_position_right(anchor, popup_w, popup_h, spacing, window_w)
        }
        // `resolve()` maps Start/End to Left/Right above.
        TooltipAnchorPosition::Start | TooltipAnchorPosition::End => unreachable!(),
    }
}

/// Which edge of the popup the caret sits on, derived from actual geometry
/// like Compose `layoutCaret` (`isBelow`/`isToTheRight`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CaretSide {
    Bottom,
    Top,
    Right,
    Left,
}

fn caret_side(position: TooltipAnchorPosition, popup_y: f32, popup_x: f32, anchor: Rect) -> CaretSide {
    match position.resolve() {
        TooltipAnchorPosition::Left => {
            if popup_x > anchor.x {
                CaretSide::Left
            } else {
                CaretSide::Right
            }
        }
        TooltipAnchorPosition::Right => {
            if popup_x > anchor.x {
                CaretSide::Left
            } else {
                CaretSide::Right
            }
        }
        _ => {
            if popup_y > anchor.y {
                CaretSide::Top
            } else {
                CaretSide::Bottom
            }
        }
    }
}

/// Tooltip-local x of the caret origin for above/below placements, mirroring
/// Compose `caretX`: anchor midpoint, start/end-aligned on screen collision.
fn caret_x(popup_w: f32, window_w: f32, anchor: Rect) -> f32 {
    let anchor_mid = anchor.x + anchor.w / 2.0;
    if popup_w >= window_w {
        anchor_mid
    } else if anchor_mid - popup_w / 2.0 < 0.0 {
        let correction = (popup_w - window_w).max(-anchor.x);
        anchor_mid + correction
    } else if anchor_mid + popup_w / 2.0 > window_w {
        let correction = (popup_w - (anchor.x + anchor.w)).min(0.0);
        anchor_mid + correction
    } else {
        popup_w / 2.0
    }
}

/// Caret triangle mesh in canvas-local px, pointing toward the anchor.
/// Mirrors `DefaultTooltipCaretShape` (16x8, apex past the base edge).
fn caret_mesh(side: CaretSide, w_px: f32, h_px: f32, color: Color) -> Arc<VectorMeshData> {
    let linear = color.to_linear();
    let v = |x: f32, y: f32| VectorVertex {
        pos: [x, y],
        color: linear,
        uv: [0.0; 2],
    };
    let (a, b, c) = match side {
        CaretSide::Bottom => (
            v(-w_px / 2.0, 0.0),
            v(w_px / 2.0, 0.0),
            v(0.0, h_px),
        ),
        CaretSide::Top => (
            v(-w_px / 2.0, h_px),
            v(w_px / 2.0, h_px),
            v(0.0, 0.0),
        ),
        CaretSide::Right => (
            v(0.0, -h_px / 2.0),
            v(0.0, h_px / 2.0),
            v(w_px, 0.0),
        ),
        CaretSide::Left => (
            v(w_px, -h_px / 2.0),
            v(w_px, h_px / 2.0),
            v(0.0, 0.0),
        ),
    };
    Arc::new(VectorMeshData {
        vertices: [a, b, c].into(),
        indices: [0, 1, 2].into(),
    })
}

/// Caret triangle view (`TooltipDefaults::CARET_SIZE`), or `None` when
/// `config.caret` is off. `popup` is the measured body size in Dp.
fn tooltip_caret(
    config: &TooltipConfig,
    container: Color,
    side: CaretSide,
    popup: Vec2,
    anchor: Rect,
    window_w: f32,
) -> Option<View> {
    if !config.caret {
        return None;
    }
    let (w_dp, h_dp) = match side {
        CaretSide::Bottom | CaretSide::Top => (
            config.caret_size.width,
            config.caret_size.height,
        ),
        CaretSide::Right | CaretSide::Left => (
            config.caret_size.height,
            config.caret_size.width,
        ),
    };
    let w_px = dp_to_px(w_dp).0;
    let h_px = dp_to_px(h_dp).0;
    if !(w_px > 0.0 && h_px > 0.0) {
        return None;
    }
    let (left, top) = match side {
        CaretSide::Bottom => (
            caret_x(popup.x, window_w, anchor) - w_dp.0 / 2.0,
            popup.y,
        ),
        CaretSide::Top => (
            caret_x(popup.x, window_w, anchor) - w_dp.0 / 2.0,
            -h_dp.0,
        ),
        CaretSide::Right => (popup.x, popup.y / 2.0 - h_dp.0 / 2.0),
        CaretSide::Left => (-w_dp.0, popup.y / 2.0 - h_dp.0 / 2.0),
    };
    let mesh = caret_mesh(side, w_px, h_px, container);
    let caret = repose_canvas::Canvas(Modifier::new().size(w_dp, h_dp), move |s| {
        s.draw_vector_mesh(mesh.clone(), [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], PaintDesc::Solid);
    });
    Some(Box(Modifier::new()
        .absolute()
        .offset(Some(Dp(left)), Some(Dp(top)), None, None)
        .hit_passthrough())
    .child(caret))
}

/// Plain tooltip body: inverse-surface container, `body_small` text.
pub fn PlainTooltip(text: impl Into<String>, config: &TooltipConfig) -> View {
    let th = theme();
    let container = config
        .container_color
        .resolve(TooltipDefaults::container_color());
    let content = config
        .content_color
        .resolve(TooltipDefaults::content_color());
    tooltip_surface(
        config,
        container,
        th.shapes.extra_small,
        Box(Modifier::new()
            .min_width(TooltipDefaults::MIN_WIDTH)
            .min_height(TooltipDefaults::MIN_HEIGHT)
            .padding_values(PaddingValues {
                left: TooltipDefaults::PLAIN_HORIZONTAL_PADDING,
                right: TooltipDefaults::PLAIN_HORIZONTAL_PADDING,
                top: TooltipDefaults::PLAIN_VERTICAL_PADDING,
                bottom: TooltipDefaults::PLAIN_VERTICAL_PADDING,
            }))
        .child(
            Text(text.into())
                .color(content)
                .size(th.typography.body_small),
        ),
    )
}

/// Rich tooltip body: `surface_container` container with title, text and an
/// optional action row. Mirrors Compose `RichTooltip` tokens.
pub fn RichTooltip(
    text: impl Into<String>,
    title: Option<View>,
    action: Option<(String, Rc<dyn Fn()>)>,
    config: &TooltipConfig,
) -> View {
    let th = theme();
    let container = config
        .container_color
        .resolve(TooltipDefaults::rich_container_color());
    let content = config
        .content_color
        .resolve(TooltipDefaults::rich_content_color());
    let title_color = config
        .title_content_color
        .resolve(TooltipDefaults::rich_title_color());
    let action_color = config
        .action_content_color
        .resolve(TooltipDefaults::rich_action_color());

    let mut rows: Vec<View> = Vec::new();
    if let Some(title) = title {
        rows.push(Box(Modifier::new()
            .fill_max_width()
            .padding_values(PaddingValues {
                top: TooltipDefaults::RICH_TITLE_TOP,
                ..PaddingValues::default()
            }))
        .child(title.color(title_color).size(th.typography.title_small)));
    }
    rows.push(Box(Modifier::new()
        .fill_max_width()
        .padding_values(PaddingValues {
            top: TooltipDefaults::RICH_TEXT_TOP,
            bottom: TooltipDefaults::RICH_TEXT_BOTTOM,
            ..PaddingValues::default()
        }))
    .child(
        Text(text.into())
            .color(content)
            .size(th.typography.body_medium),
    ));
    if let Some((label, on_click)) = action {
        let label_view = Box(Modifier::new()
            .min_height(TooltipDefaults::RICH_ACTION_MIN_HEIGHT)
            .padding_values(PaddingValues {
                bottom: TooltipDefaults::RICH_ACTION_BOTTOM,
                ..PaddingValues::default()
            })
            .clickable()
            .on_click(move || {
                on_click();
            }))
        .child(
            Text(label)
                .color(action_color)
                .size(th.typography.label_large),
        );
        rows.push(label_view);
    }

    tooltip_surface(
        config,
        container,
        th.shapes.medium,
        Box(Modifier::new()
            .min_width(TooltipDefaults::MIN_WIDTH)
            .min_height(TooltipDefaults::MIN_HEIGHT)
            .padding_values(PaddingValues {
                left: TooltipDefaults::RICH_HORIZONTAL_PADDING,
                right: TooltipDefaults::RICH_HORIZONTAL_PADDING,
                ..PaddingValues::default()
            }))
        .child(Column(Modifier::new().fill_max_width()).with_children(rows)),
    )
}

fn tooltip_surface(
    config: &TooltipConfig,
    container: Color,
    shape: Dp,
    content: View,
) -> View {
    let th = theme();
    let shadow = if config.shadow_elevation.0 > 0.0 {
        config.shadow_elevation
    } else if config.kind == TooltipKind::Rich {
        th.elevation.level2
    } else {
        Dp::ZERO
    };
    let mut m = Modifier::new()
        .background(container)
        .clip_rounded(shape)
        .max_width(config.max_width)
        .flex_shrink(0.0)
        .hit_passthrough();
    if shadow.0 > 0.0 {
        m = m.shadow(shadow, Dp::ZERO);
    }
    if config.tonal_elevation.0 > 0.0 {
        m = m.state_elevation(StateElevation {
            default: config.tonal_elevation,
            hovered: config.tonal_elevation,
            focused: config.tonal_elevation,
            pressed: config.tonal_elevation,
            dragged: config.tonal_elevation,
            disabled: Dp::ZERO,
        });
    }
    Box(m).child(content)
}

/// Wraps `content` with a tooltip shown when `state` is visible.
///
/// The popup renders in the ambient overlay layer (never clipped by parents
/// or scroll containers) and is flipped/clamped into the window container
/// like Compose `TooltipBox` + `TooltipPositionProviderImpl`.
///
/// When [`TooltipConfig::enable_user_input`] is true (default), the tooltip is
/// shown on pointer hover and dismissed on leave.
pub fn TooltipBox(
    text: impl Into<String>,
    state: Rc<TooltipState>,
    modifier: Modifier,
    content: View,
    config: TooltipConfig,
) -> View {
    let overlay = ambient_overlay();
    let text: Rc<str> = Rc::from(text.into());
    let id = remember(unique_component_id);
    let th = theme();
    let spec = th.motion.overlay;

    let tooltip_body = match config.kind {
        TooltipKind::Plain => PlainTooltip((*text).to_string(), &config),
        TooltipKind::Rich => RichTooltip((*text).to_string(), None, None, &config),
    };
    let current_body = remember_state_with_key(format!("tt_body_{id}"), || tooltip_body.clone());
    *current_body.borrow_mut() = tooltip_body;
    let current_config =
        remember_state_with_key(format!("tt_cfg_{id}"), || config.clone());
    *current_config.borrow_mut() = config.clone();

    let anchor_rect = remember_state_with_key(format!("tt_anchor_{id}"), Rect::default);
    let popup_size =
        remember_state_with_key(format!("tt_popup_{id}"), || Vec2 { x: 0.0, y: 0.0 });
    let trigger = Box(Modifier::new().on_globally_positioned({
        let anchor_rect = anchor_rect.clone();
        move |rect| {
            *anchor_rect.borrow_mut() = rect;
        }
    }))
    .child(content);

    let mut host = modifier.flex_shrink(0.0);
    if host.align_self.is_none() {
        host = host.align_self(AlignSelf::FLEX_START);
    }
    if config.enable_user_input {
        let timeout = config.auto_dismiss_timeout;
        let enter = state.clone();
        let leave = state.clone();
        host = host.hoverable(
            move || enter.show_with_timeout(timeout),
            move || leave.dismiss(),
        );
        // Touch/stylus: show on long-press, dismiss on release, mirroring
        // Compose `BasicTooltipBox.handleGestures` (mouse uses hover above;
        // `PointerKind` distinguishes them here). A quick tap never reaches
        // the long-press timeout, so the tooltip only appears on hold.
        let long = state.clone();
        let up = state.clone();
        host = host
            .on_long_click(move || long.show_with_timeout(timeout))
            .on_pointer_up(move |e| {
                if e.kind != PointerKind::Mouse {
                    up.dismiss();
                }
            });
    }
    let host_view = Box(host).child(trigger);

    let anim_key = format!("tooltip_alpha_{id}");
    let alpha = animate_f32(
        anim_key.clone(),
        if state.is_visible() { 1.0 } else { 0.0 },
        spec,
    );
    let tooltip_visible = state.is_visible() || alpha > 0.01;
    let overlay_guard = remember_with_key(format!("tt_oguard_{id}"), || {
        RefCell::new(None::<OverlayGuard>)
    });

    if tooltip_visible {
        if overlay_guard.borrow().is_none()
            && let Some(overlay) = overlay.clone()
        {
            let current_body = current_body.clone();
            let current_config = current_config.clone();
            let anchor_rect = anchor_rect.clone();
            let popup_size = popup_size.clone();
            let state_for_scrim = state.clone();
            // The overlay entry rebuilds every frame, so re-read the animation
            // target-tracked value here instead of capturing the frame's copy.
            let anim_key = anim_key.clone();
            let spec = spec;
            *overlay_guard.borrow_mut() = Some(overlay.show_guard(
                Rc::new(move || {
                    let frame_alpha = animate_f32(
                        anim_key.clone(),
                        if state_for_scrim.is_visible() {
                            1.0
                        } else {
                            0.0
                        },
                        spec,
                    );
                    let body = current_body.borrow().clone();
                    let config = current_config.borrow().clone();
                    let anchor = *anchor_rect.borrow();
                    let win_w = get_window_container_width();
                    let win_h = get_window_container_height();
                    let popup_w = config.max_width.0.min(win_w).max(1.0);
                    let popup_h = TooltipDefaults::MIN_HEIGHT.0;
                    let (x, y) = tooltip_position(
                        config.position,
                        anchor,
                        popup_w,
                        popup_h,
                        config.spacing.0,
                        win_w,
                        win_h,
                    );
                    let scale = 0.8 + 0.2 * frame_alpha.min(1.0);
                    let measured = *popup_size.borrow();
                    let real = Vec2 {
                        x: if measured.x > 0.0 { measured.x } else { popup_w },
                        y: if measured.y > 0.0 { measured.y } else { popup_h },
                    };
                    let side = caret_side(config.position, y, x, anchor);
                    let container = match config.kind {
                        TooltipKind::Plain => config
                            .container_color
                            .resolve(TooltipDefaults::container_color()),
                        TooltipKind::Rich => config
                            .container_color
                            .resolve(TooltipDefaults::rich_container_color()),
                    };
                    let caret = tooltip_caret(&config, container, side, real, anchor, win_w);
                    let popup_size = popup_size.clone();
                    let body = Box(Modifier::new().on_size_changed(move |s| {
                        let mut slot = popup_size.borrow_mut();
                        if *slot != s {
                            *slot = s;
                            request_frame();
                        }
                    }))
                    .child(body);
                    let mut popup_children = vec![body];
                    if let Some(caret) = caret {
                        popup_children.push(caret);
                    }
                    let popup = Box(Modifier::new()
                        .absolute()
                        .offset(Some(Dp(x.max(0.0))), Some(Dp(y)), None, None)
                        .hit_passthrough()
                        .alpha(frame_alpha)
                        .scale(scale))
                    .child(ZStack(Modifier::new()).with_children(popup_children));
                    let scrim = if config.focusable {
                        let s = state_for_scrim.clone();
                        Box(Modifier::new().fill_max_size().on_pointer_down(move |_| {
                            s.dismiss();
                        }))
                    } else {
                        Box(Modifier::new())
                    };
                    ZStack(Modifier::new().fill_max_size().absolute())
                        .child((scrim, popup))
                }),
                10_000.0,
                true,
            ));
        }
    } else {
        *overlay_guard.borrow_mut() = None;
    }

    host_view
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn above_prefers_centered() {
        let (x, y) =
            tooltip_position_above(anchor(100.0, 200.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!((x, y), (75.0, 156.0));
    }

    #[test]
    fn above_flips_below_when_no_room() {
        let (x, y) =
            tooltip_position_above(anchor(100.0, 10.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!((x, y), (75.0, 34.0));
    }

    #[test]
    fn above_start_aligns_on_left_collision() {
        let (x, _) =
            tooltip_position_above(anchor(10.0, 200.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!(x, 10.0);
    }

    #[test]
    fn above_end_aligns_on_right_collision() {
        let (x, _) =
            tooltip_position_above(anchor(350.0, 200.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!(x, 300.0);
    }

    #[test]
    fn below_flips_above_when_overflowing_bottom() {
        let (_, y) = tooltip_position_below(
            anchor(100.0, 750.0, 50.0, 20.0),
            100.0,
            40.0,
            4.0,
            400.0,
            800.0,
        );
        assert_eq!(y, 750.0 - 40.0 - 4.0);
    }

    #[test]
    fn left_flips_right_on_left_collision() {
        let (x, y) = tooltip_position_left(anchor(10.0, 200.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!((x, y), (64.0, 190.0));
    }

    #[test]
    fn right_clamps_on_right_collision() {
        let (x, _) =
            tooltip_position_right(anchor(350.0, 200.0, 50.0, 20.0), 100.0, 40.0, 4.0, 400.0);
        assert_eq!(x, 246.0);
    }

    #[test]
    fn caret_x_centers_on_anchor() {
        assert_eq!(caret_x(100.0, 400.0, anchor(100.0, 200.0, 50.0, 20.0)), 50.0);
    }

    #[test]
    fn caret_x_pins_to_anchor_mid_when_wider_than_window() {
        assert_eq!(caret_x(500.0, 400.0, anchor(100.0, 200.0, 50.0, 20.0)), 125.0);
    }

    #[test]
    fn caret_side_follows_actual_geometry() {
        let anchor = anchor(100.0, 200.0, 50.0, 20.0);
        assert_eq!(
            caret_side(TooltipAnchorPosition::Above, 150.0, 75.0, anchor),
            CaretSide::Bottom
        );
        assert_eq!(
            caret_side(TooltipAnchorPosition::Above, 224.0, 75.0, anchor),
            CaretSide::Top
        );
        assert_eq!(
            caret_side(TooltipAnchorPosition::Left, 150.0, 10.0, anchor),
            CaretSide::Right
        );
        assert_eq!(
            caret_side(TooltipAnchorPosition::Right, 150.0, 154.0, anchor),
            CaretSide::Left
        );
    }

    #[test]
    fn caret_mesh_is_one_triangle() {
        let mesh = caret_mesh(CaretSide::Bottom, 16.0, 8.0, Color::WHITE);
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(&*mesh.indices, &[0, 1, 2]);
        assert_eq!(mesh.vertices[2].pos, [0.0, 8.0]);
    }
}
