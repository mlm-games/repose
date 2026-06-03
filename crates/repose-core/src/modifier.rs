use std::rc::Rc;

use taffy::{AlignContent, AlignItems, AlignSelf, FlexDirection, FlexWrap, JustifyContent};

use crate::animation::AnimationSpec;
use crate::{Brush, Color, PointerEvent, Size, Transform, Vec2};

/// State-driven colors for interactive components.
/// The layout engine selects the appropriate color based on hover/press/disabled state
/// and animates transitions between them.
#[derive(Clone, Copy, Debug)]
pub struct StateColors {
    pub default: Color,
    pub hovered: Color,
    pub pressed: Color,
    pub disabled: Color,
}

/// State-driven elevation for interactive components.
#[derive(Clone, Copy, Debug)]
pub struct StateElevation {
    pub default: f32,
    pub hovered: f32,
    pub pressed: f32,
    pub disabled: f32,
}

macro_rules! merge_opts {
    ($dst:ident, $src:ident; $($f:ident),+ $(,)?) => {
        $( $dst.$f = $src.$f.or($dst.$f); )+
    };
}
macro_rules! merge_flags {
    ($dst:ident, $src:ident; $($f:ident),+ $(,)?) => {
        $( $dst.$f |= $src.$f; )+
    };
}

macro_rules! impl_option_fields {
    ($ty:ty, $fn:ident) => {
        impl $ty {
            $fn!(replace);
        }
    };
    ($ty:ident) => {
        impl $ty {
            /// Chain another modifier's settings onto this one.
            /// Useful for creating reusable modifier templates.
            pub fn then(mut self, other: Self) -> Self {
                merge_opts!(self, other;
                    key, size, width, height,
                    padding, padding_values,
                    min_width, min_height, max_width, max_height,
                    background, state_colors, state_elevation, border,
                    flex_grow, flex_shrink, flex_basis, flex_wrap, flex_dir,
                    gap, row_gap, column_gap,
                    align_self, justify_content, align_items_container, align_content,
                    clip_rounded, render_z_index,
                    on_scroll,
                    on_pointer_down, on_pointer_move, on_pointer_up,
                    on_pointer_enter, on_pointer_leave,
                    semantics, alpha, transform,
                    grid, grid_col_span, grid_row_span,
                    position_type,
                    offset_left, offset_right, offset_top, offset_bottom,
                    margin_left, margin_right, margin_top, margin_bottom,
                    aspect_ratio, painter,
                    on_drag_start, on_drag_end, on_drag_enter, on_drag_over, on_drag_leave, on_drop,
                    on_action, cursor, animate_content_size, focus_requester, on_focus_changed,
                );
                merge_flags!(self, other;
                    fill_max, fill_max_w, fill_max_h,
                    hit_passthrough, input_blocker, repaint_boundary, click, disabled,
                );
                if other.z_index != 0.0 {
                    self.z_index = other.z_index;
                }
                self
            }
        }
    };
}

#[derive(Clone, Debug)]
pub struct Border {
    pub width: f32,
    pub color: Color,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PaddingValues {
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
pub struct GridConfig {
    pub columns: usize,
    pub row_gap: f32,
    pub column_gap: f32,
}

/// Drop-shadow parameters applied to a graphics layer.
///
/// `blur_radius` is the Gaussian blur radius in dp (1.0 = subtle, 8.0 = soft,
/// 16.0 = very diffuse). `offset_y` is the vertical offset of the shadow in dp
/// (positive = below the layer). `color` is the shadow color (premultiplied
/// alpha controls shadow darkness).
#[derive(Clone, Copy, Debug)]
pub struct ShadowSpec {
    pub blur_radius: f32,
    pub offset_y: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
pub enum PositionType {
    Relative,
    Absolute,
}

#[derive(Clone, Default)]
pub struct Modifier {
    /// Optional stable identity key for this view node.
    ///
    /// If set, `layout_and_paint` will prefer this over child index when assigning stable ViewIds.
    /// This is the “escape hatch” for dynamic lists / conditional UI where index-based identity
    /// would otherwise shift.
    pub key: Option<u64>,

    pub size: Option<Size>,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub fill_max: bool,
    pub fill_max_w: bool,
    pub fill_max_h: bool,
    pub padding: Option<f32>,
    pub padding_values: Option<PaddingValues>,
    pub min_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_width: Option<f32>,
    pub max_height: Option<f32>,
    pub background: Option<Brush>,
    pub state_colors: Option<StateColors>,
    pub state_elevation: Option<StateElevation>,

    pub border: Option<Border>,
    pub flex_grow: Option<f32>,
    pub flex_shrink: Option<f32>,
    pub flex_basis: Option<f32>,
    pub flex_wrap: Option<FlexWrap>,
    pub flex_dir: Option<FlexDirection>,
    pub gap: Option<f32>,
    pub row_gap: Option<f32>,
    pub column_gap: Option<f32>,
    pub align_self: Option<AlignSelf>,
    pub justify_content: Option<JustifyContent>,
    pub align_items_container: Option<AlignItems>,
    pub align_content: Option<AlignContent>,
    pub clip_rounded: Option<f32>,
    /// Z-index for hit-testing order (higher = receives events first).
    pub z_index: f32,
    /// Z-index for render order (higher = painted on top). If None, uses tree order.
    pub render_z_index: Option<f32>,
    /// If true, this view does not create hit regions.
    pub hit_passthrough: bool,
    /// If true, this view blocks pointer/touch input for hits below it.
    pub input_blocker: bool,
    pub repaint_boundary: bool,
    pub click: bool,
    /// When true, the component ignores pointer events and appears disabled.
    pub disabled: bool,
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    pub on_pointer_down: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_move: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_up: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_enter: Option<Rc<dyn Fn(PointerEvent)>>,
    pub on_pointer_leave: Option<Rc<dyn Fn(PointerEvent)>>,
    pub semantics: Option<crate::Semantics>,
    pub alpha: Option<f32>,
    pub graphics_layer: Option<f32>,
    pub shadow: Option<ShadowSpec>,
    pub transform: Option<Transform>,
    pub grid: Option<GridConfig>,
    pub grid_col_span: Option<u16>,
    pub grid_row_span: Option<u16>,
    pub position_type: Option<PositionType>,
    pub offset_left: Option<f32>,
    pub offset_right: Option<f32>,
    pub offset_top: Option<f32>,
    pub offset_bottom: Option<f32>,
    pub margin_left: Option<f32>,
    pub margin_right: Option<f32>,
    pub margin_top: Option<f32>,
    pub margin_bottom: Option<f32>,
    pub aspect_ratio: Option<f32>,
    pub painter: Option<Rc<dyn Fn(&mut crate::Scene, crate::Rect)>>,

    // Drag-drop (internal)
    pub on_drag_start: Option<Rc<dyn Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload>>>,
    pub on_drag_end: Option<Rc<dyn Fn(crate::dnd::DragEnd)>>,
    pub on_drag_enter: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_over: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drag_leave: Option<Rc<dyn Fn(crate::dnd::DragOver)>>,
    pub on_drop: Option<Rc<dyn Fn(crate::dnd::DropEvent) -> bool>>,

    pub on_action: Option<Rc<dyn Fn(crate::shortcuts::Action) -> bool>>,

    /// Cursor icon hint for desktop/web runners.
    pub cursor: Option<crate::CursorIcon>,

    /// If set, the size of this node will smoothly animate to its target size
    /// whenever content size changes. Uses the provided animation spec.
    pub animate_content_size: Option<AnimationSpec>,

    /// A `FocusRequester` handle that will be associated with this view.
    /// When the requester's `request_focus()` is called, keyboard focus will
    /// move to this view.
    pub focus_requester: Option<crate::runtime::FocusRequester>,

    /// Called when this view gains or loses focus. The boolean parameter is
    /// `true` when focused, `false` when unfocused.
    pub on_focus_changed: Option<Rc<dyn Fn(bool)>>,
}

impl std::fmt::Debug for Modifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("Modifier");

        macro_rules! opt_val {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name.is_some() { s.field(stringify!($name), &self.$name); } )+
            };
        }
        opt_val!(
            key,
            size,
            width,
            height,
            padding,
            padding_values,
            min_width,
            min_height,
            max_width,
            max_height,
            background,
            state_colors,
            state_elevation,
            border,
            flex_grow,
            flex_shrink,
            flex_basis,
            flex_wrap,
            flex_dir,
            gap,
            row_gap,
            column_gap,
            align_self,
            justify_content,
            align_items_container,
            align_content,
            clip_rounded,
            render_z_index,
            semantics,
            alpha,
            transform,
            grid,
            grid_col_span,
            grid_row_span,
            position_type,
            offset_left,
            offset_right,
            offset_top,
            offset_bottom,
            margin_left,
            margin_right,
            margin_top,
            margin_bottom,
            aspect_ratio,
            cursor,
            animate_content_size,
        );

        macro_rules! opt_cb {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name.is_some() { s.field(stringify!($name), &"…"); } )+
            };
        }
        opt_cb!(
            on_scroll,
            on_pointer_down,
            on_pointer_move,
            on_pointer_up,
            on_pointer_enter,
            on_pointer_leave,
            painter,
            on_drag_start,
            on_drag_end,
            on_drag_enter,
            on_drag_over,
            on_drag_leave,
            on_drop,
            on_action,
            on_focus_changed,
        );

        macro_rules! flag {
            ($($name:ident),+ $(,)?) => {
                $( if self.$name { s.field(stringify!($name), &true); } )+
            };
        }
        flag!(
            fill_max,
            fill_max_w,
            fill_max_h,
            hit_passthrough,
            input_blocker,
            repaint_boundary,
            click,
            disabled,
        );

        if self.z_index != 0.0 {
            s.field("z_index", &self.z_index);
        }

        s.finish()
    }
}

impl_option_fields!(Modifier);

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches a stable identity key to this view node.
    /// Use for dynamic lists / conditional UI where index-based identity can shift.
    pub fn key(mut self, key: u64) -> Self {
        self.key = Some(key);
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = Some(Size {
            width: w,
            height: h,
        });
        self
    }
    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
    pub fn fill_max_size(mut self) -> Self {
        self.fill_max = true;
        self
    }
    pub fn fill_max_width(mut self) -> Self {
        self.fill_max_w = true;
        self
    }
    pub fn fill_max_height(mut self) -> Self {
        self.fill_max_h = true;
        self
    }
    pub fn padding(mut self, v: f32) -> Self {
        self.padding = Some(v);
        self
    }
    pub fn padding_values(mut self, padding: PaddingValues) -> Self {
        self.padding_values = Some(padding);
        self
    }
    /// Add padding equal to the current IME (soft keyboard) bottom inset.
    /// Combine with `system_bars_padding()` to handle both system bars and keyboard.
    pub fn ime_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let mut p = self.padding_values.unwrap_or_default();
        p.bottom += insets.ime_bottom;
        self.padding_values = Some(p);
        self
    }
    /// Add padding equal to the current system bar insets (status bar top, nav bar bottom).
    pub fn system_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let mut p = self.padding_values.unwrap_or_default();
        p.top += insets.top;
        p.bottom += insets.bottom;
        self.padding_values = Some(p);
        self
    }
    /// Add status bar inset as top padding.
    pub fn status_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let mut p = self.padding_values.unwrap_or_default();
        p.top += insets.top;
        self.padding_values = Some(p);
        self
    }
    /// Add navigation bar inset as bottom padding.
    pub fn navigation_bars_padding(mut self) -> Self {
        let insets = crate::locals::window_insets();
        let mut p = self.padding_values.unwrap_or_default();
        p.bottom += insets.bottom;
        self.padding_values = Some(p);
        self
    }
    pub fn min_size(mut self, w: f32, h: f32) -> Self {
        self.min_width = Some(w);
        self.min_height = Some(h);
        self
    }
    pub fn max_size(mut self, w: f32, h: f32) -> Self {
        self.max_width = Some(w);
        self.max_height = Some(h);
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w);
        self
    }
    pub fn min_height(mut self, h: f32) -> Self {
        self.min_height = Some(h);
        self
    }
    pub fn max_width(mut self, w: f32) -> Self {
        self.max_width = Some(w);
        self
    }
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }
    /// Set a solid color background.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(Brush::Solid(color));
        self
    }
    /// Set a brush (solid, gradient, etc.) background.
    pub fn background_brush(mut self, brush: Brush) -> Self {
        self.background = Some(brush);
        self
    }
    pub fn border(mut self, width: f32, color: Color, radius: f32) -> Self {
        self.border = Some(Border {
            width,
            color,
            radius,
        });
        self
    }
    pub fn flex_grow(mut self, v: f32) -> Self {
        self.flex_grow = Some(v);
        self
    }
    pub fn flex_shrink(mut self, v: f32) -> Self {
        self.flex_shrink = Some(v);
        self
    }
    pub fn flex_basis(mut self, v: f32) -> Self {
        self.flex_basis = Some(v);
        self
    }
    pub fn flex_wrap(mut self, w: FlexWrap) -> Self {
        self.flex_wrap = Some(w);
        self
    }
    pub fn flex_dir(mut self, d: FlexDirection) -> Self {
        self.flex_dir = Some(d);
        self
    }
    pub fn gap(mut self, v: f32) -> Self {
        let v = v.max(0.0);
        self.gap = Some(v);
        self.row_gap = Some(v);
        self.column_gap = Some(v);
        self
    }
    pub fn row_gap(mut self, v: f32) -> Self {
        self.row_gap = Some(v.max(0.0));
        self
    }
    pub fn column_gap(mut self, v: f32) -> Self {
        self.column_gap = Some(v.max(0.0));
        self
    }
    pub fn align_self(mut self, a: AlignSelf) -> Self {
        self.align_self = Some(a);
        self
    }
    pub fn align_self_center(mut self) -> Self {
        self.align_self = Some(AlignSelf::Center);
        self
    }
    pub fn justify_content(mut self, j: JustifyContent) -> Self {
        self.justify_content = Some(j);
        self
    }
    pub fn align_items(mut self, a: AlignItems) -> Self {
        self.align_items_container = Some(a);
        self
    }
    pub fn align_content(mut self, a: AlignContent) -> Self {
        self.align_content = Some(a);
        self
    }
    pub fn clip_rounded(mut self, radius: f32) -> Self {
        self.clip_rounded = Some(radius);
        self
    }
    pub fn z_index(mut self, z: f32) -> Self {
        self.z_index = z;
        self
    }

    /// Sets the render z-index for this view. Higher values are painted on top.
    /// Unlike `z_index` (which only affects hit-testing), this affects visual layering.
    pub fn render_z_index(mut self, z: f32) -> Self {
        self.render_z_index = Some(z);
        self
    }

    /// Prevent pointer/touch from reaching lower layers.
    pub fn input_blocker(mut self) -> Self {
        self.input_blocker = true;
        self
    }

    pub fn hit_passthrough(mut self) -> Self {
        self.hit_passthrough = true;
        self
    }
    pub fn clickable(mut self) -> Self {
        self.click = true;
        self
    }
    /// Set state-driven background colors for hover, press, disabled states.
    /// The layout engine automatically selects and animates between these based on interaction.
    pub fn state_colors(mut self, colors: StateColors) -> Self {
        self.state_colors = Some(colors);
        self
    }
    /// Set state-driven elevation values for hover, press, disabled states.
    pub fn state_elevation(mut self, elev: StateElevation) -> Self {
        self.state_elevation = Some(elev);
        self
    }
    /// Mark this component as disabled - it won't respond to pointer events.
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    /// Mark this component as enabled or disabled.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.disabled = !enabled;
        self
    }
    pub fn on_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_scroll = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_down(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_down = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_move(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_move = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_up(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_up = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_enter(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_enter = Some(Rc::new(f));
        self
    }
    pub fn on_pointer_leave(mut self, f: impl Fn(PointerEvent) + 'static) -> Self {
        self.on_pointer_leave = Some(Rc::new(f));
        self
    }
    pub fn semantics(mut self, s: crate::Semantics) -> Self {
        self.semantics = Some(s);
        self
    }
    pub fn alpha(mut self, a: f32) -> Self {
        self.alpha = Some(a);
        self
    }
    /// Render this subtree into an offscreen texture, then composite it
    /// back into the parent with the given group `alpha` (0.0..=1.0).
    /// Allows correct blending when children overlap inside the layer, and
    /// sets up the architecture for future layer effects (shadow, blur, clip).
    pub fn graphics_layer(mut self, alpha: f32) -> Self {
        self.graphics_layer = Some(alpha.clamp(0.0, 1.0));
        self
    }
    /// Drop shadow with the given `blur_radius` (dp) and vertical `offset_y` (dp).
    /// The shadow color defaults to black with alpha 64 (~25%). Combines with
    /// [`Modifier::graphics_layer`] to draw a shadow underneath the layer.
    pub fn shadow(mut self, blur_radius: f32, offset_y: f32) -> Self {
        self.shadow = Some(ShadowSpec {
            blur_radius: blur_radius.max(0.0),
            offset_y,
            color: Color(0, 0, 0, 64),
        });
        self
    }
    /// Drop shadow with a custom color. Alpha 0..=255.
    pub fn shadow_with_color(mut self, blur_radius: f32, offset_y: f32, color: Color) -> Self {
        self.shadow = Some(ShadowSpec {
            blur_radius: blur_radius.max(0.0),
            offset_y,
            color,
        });
        self
    }
    /// Material-style elevation. Auto-scales blur and offset by `level` (dp)
    /// and uses a default shadow color. Level 0 = no shadow; 4 = subtle;
    /// 16 = strong. Requires [`Modifier::graphics_layer`] to take effect.
    pub fn elevation(mut self, level: f32) -> Self {
        if level <= 0.0 {
            self.shadow = None;
            return self;
        }
        self.shadow = Some(ShadowSpec {
            blur_radius: level * 2.0,
            offset_y: level * 0.5,
            color: Color(0, 0, 0, (level * 8.0).clamp(8.0, 80.0) as u8),
        });
        self
    }
    pub fn transform(mut self, t: Transform) -> Self {
        self.transform = Some(t);
        self
    }
    pub fn grid(mut self, columns: usize, row_gap: f32, column_gap: f32) -> Self {
        self.grid = Some(GridConfig {
            columns,
            row_gap,
            column_gap,
        });
        self
    }
    pub fn grid_span(mut self, col_span: u16, row_span: u16) -> Self {
        self.grid_col_span = Some(col_span);
        self.grid_row_span = Some(row_span);
        self
    }
    pub fn absolute(mut self) -> Self {
        self.position_type = Some(PositionType::Absolute);
        self
    }
    pub fn offset(
        mut self,
        left: Option<f32>,
        top: Option<f32>,
        right: Option<f32>,
        bottom: Option<f32>,
    ) -> Self {
        self.offset_left = left;
        self.offset_top = top;
        self.offset_right = right;
        self.offset_bottom = bottom;
        self
    }
    pub fn offset_left(mut self, v: f32) -> Self {
        self.offset_left = Some(v);
        self
    }
    pub fn offset_right(mut self, v: f32) -> Self {
        self.offset_right = Some(v);
        self
    }
    pub fn offset_top(mut self, v: f32) -> Self {
        self.offset_top = Some(v);
        self
    }
    pub fn offset_bottom(mut self, v: f32) -> Self {
        self.offset_bottom = Some(v);
        self
    }

    pub fn margin(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }

    pub fn margin_horizontal(mut self, v: f32) -> Self {
        self.margin_left = Some(v);
        self.margin_right = Some(v);
        self
    }

    pub fn margin_vertical(mut self, v: f32) -> Self {
        self.margin_top = Some(v);
        self.margin_bottom = Some(v);
        self
    }
    pub fn aspect_ratio(mut self, ratio: f32) -> Self {
        self.aspect_ratio = Some(ratio);
        self
    }
    pub fn painter(mut self, f: impl Fn(&mut crate::Scene, crate::Rect) + 'static) -> Self {
        self.painter = Some(Rc::new(f));
        self
    }
    pub fn scale(self, s: f32) -> Self {
        self.scale2(s, s)
    }
    pub fn scale2(mut self, sx: f32, sy: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.scale_x *= sx;
        t.scale_y *= sy;
        self.transform = Some(t);
        self
    }
    pub fn translate(mut self, x: f32, y: f32) -> Self {
        let t = self.transform.unwrap_or_else(Transform::identity);
        self.transform = Some(t.combine(&Transform::translate(x, y)));
        self
    }
    pub fn translate_vec2(self, v: Vec2) -> Self {
        self.translate(v.x, v.y)
    }
    pub fn rotate(mut self, radians: f32) -> Self {
        let mut t = self.transform.unwrap_or_else(Transform::identity);
        t.rotate += radians;
        self.transform = Some(t);
        self
    }
    pub fn weight(mut self, w: f32) -> Self {
        let w = w.max(0.0);
        self.flex_grow = Some(w);
        self.flex_shrink = Some(1.0);
        // dp units; 0 is fine.
        self.flex_basis = Some(0.0);
        self
    }
    /// Marks this view as a repaint boundary candidate.
    ///
    /// The engine may cache its painted output.
    pub fn repaint_boundary(mut self) -> Self {
        self.repaint_boundary = true;
        self
    }
    pub fn on_action(mut self, f: impl Fn(crate::shortcuts::Action) -> bool + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Mark this node as a drag source. Return `Some(payload)` to start dragging.
    pub fn on_drag_start(
        mut self,
        f: impl Fn(crate::dnd::DragStart) -> Option<crate::dnd::DragPayload> + 'static,
    ) -> Self {
        self.on_drag_start = Some(Rc::new(f));
        self
    }

    /// Called when a drag ends (drop accepted or canceled/ignored).
    pub fn on_drag_end(mut self, f: impl Fn(crate::dnd::DragEnd) + 'static) -> Self {
        self.on_drag_end = Some(Rc::new(f));
        self
    }

    /// Called when a drag first enters this target.
    pub fn on_drag_enter(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_enter = Some(Rc::new(f));
        self
    }

    /// Called on every pointer move while a drag is over this target.
    pub fn on_drag_over(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_over = Some(Rc::new(f));
        self
    }

    /// Called when a drag leaves this target.
    pub fn on_drag_leave(mut self, f: impl Fn(crate::dnd::DragOver) + 'static) -> Self {
        self.on_drag_leave = Some(Rc::new(f));
        self
    }

    /// Called on pointer release while a drag is over this target.
    /// Return `true` to accept the drop.
    pub fn on_drop(mut self, f: impl Fn(crate::dnd::DropEvent) -> bool + 'static) -> Self {
        self.on_drop = Some(Rc::new(f));
        self
    }

    /// Set the cursor icon hint for desktop/web runners.
    pub fn cursor(mut self, c: crate::CursorIcon) -> Self {
        self.cursor = Some(c);
        self
    }

    /// Animate size changes smoothly when the content's natural size changes.
    /// Uses the provided `AnimationSpec` for the transition.
    /// The content will be clipped to the animated size during transitions.
    pub fn animate_content_size(mut self, spec: AnimationSpec) -> Self {
        self.animate_content_size = Some(spec);
        self
    }

    /// Attach a `FocusRequester` to this view. The requester will be associated
    /// with the view's focusable element, allowing programmatic focus requests.
    pub fn focus_requester(mut self, fr: crate::runtime::FocusRequester) -> Self {
        self.focus_requester = Some(fr);
        self
    }

    /// Register a callback that fires when this view gains or loses keyboard focus.
    /// The argument is `true` when the view receives focus, `false` when it loses it.
    pub fn on_focus_changed(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_focus_changed = Some(Rc::new(f));
        self
    }
}
