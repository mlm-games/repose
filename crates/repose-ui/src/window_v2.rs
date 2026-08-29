#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use repose_core::{Rect, Size, Vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScreenInsets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub id: String,
    pub bounds: Rect,
    pub insets: ScreenInsets,
}

impl Screen {
    pub fn new(id: impl Into<String>, bounds: Rect, insets: ScreenInsets) -> Self {
        Self {
            id: id.into(),
            bounds,
            insets,
        }
    }
    pub fn available_bounds(&self) -> Rect {
        Rect {
            x: self.bounds.x + self.insets.left,
            y: self.bounds.y + self.insets.top,
            w: (self.bounds.w - self.insets.left - self.insets.right).max(0.0),
            h: (self.bounds.h - self.insets.top - self.insets.bottom).max(0.0),
        }
    }
    pub fn primary(host_bounds: Rect) -> Self {
        Self {
            id: "primary".into(),
            bounds: host_bounds,
            insets: ScreenInsets::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WindowPlacement {
    #[default]
    Floating,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Debug)]
pub struct WindowMetrics {
    pub screen: Screen,
    pub bounds: Rect,
    pub insets: ScreenInsets,
}
impl WindowMetrics {
    pub fn new(screen: Screen, bounds: Rect, insets: ScreenInsets) -> Self {
        Self {
            screen,
            bounds,
            insets,
        }
    }
}

pub struct WindowScreenProviderScope {
    pub screens: Vec<Screen>,
    pub default_screen: Screen,
}
impl WindowScreenProviderScope {
    pub fn new(screens: Vec<Screen>, default_screen: Screen) -> Self {
        Self {
            screens,
            default_screen,
        }
    }
    pub fn eval(&self, p: &WindowScreenProvider) -> Screen {
        p.get_screen(self)
    }
}

#[derive(Clone)]
pub struct WindowScreenProvider {
    inner: Rc<dyn Fn(&WindowScreenProviderScope) -> Screen>,
}
impl WindowScreenProvider {
    pub fn new<F: Fn(&WindowScreenProviderScope) -> Screen + 'static>(f: F) -> Self {
        Self { inner: Rc::new(f) }
    }
    pub fn get_screen(&self, scope: &WindowScreenProviderScope) -> Screen {
        (self.inner)(scope)
    }
    pub fn default_screen() -> Self {
        Self::new(|s| s.default_screen.clone())
    }
    pub fn with_id(id: impl Into<String>) -> Self {
        let wanted = id.into();
        Self::new(move |s| {
            s.screens
                .iter()
                .find(|x| x.id == wanted)
                .cloned()
                .unwrap_or_else(|| s.default_screen.clone())
        })
    }
}
impl Default for WindowScreenProvider {
    fn default() -> Self {
        Self::default_screen()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WindowConstraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
}
impl WindowConstraints {
    pub const INFINITY: f32 = f32::INFINITY;
}

pub struct WindowGeometryProviderScope<'a> {
    pub parent_metrics: Option<WindowMetrics>,
    pub window_metrics: WindowMetrics,
    pub measure_content: Rc<dyn Fn(WindowConstraints) -> Size + 'a>,
}
impl<'a> WindowGeometryProviderScope<'a> {
    pub fn new(
        parent_metrics: Option<WindowMetrics>,
        window_metrics: WindowMetrics,
        measure_content: impl Fn(WindowConstraints) -> Size + 'a,
    ) -> Self {
        Self {
            parent_metrics,
            window_metrics,
            measure_content: Rc::new(measure_content),
        }
    }
    pub fn content_to_window_size(&self, c: Size) -> Size {
        let ins = self.window_metrics.insets;
        let raw = Size {
            width: c.width + ins.left + ins.right,
            height: c.height + ins.top + ins.bottom,
        };
        let avail = self.window_metrics.screen.available_bounds();
        Size {
            width: raw.width.min(avail.w),
            height: raw.height.min(avail.h),
        }
    }
    pub fn measure_window_content(&self, min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> Size {
        (self.measure_content)(WindowConstraints {
            min_width: min_w.max(0.0),
            max_width: max_w,
            min_height: min_h.max(0.0),
            max_height: max_h,
        })
    }
    pub(crate) fn preferred_width_for_height(&self, h: f32) -> f32 {
        self.measure_window_content(0.0, WindowConstraints::INFINITY, h, h)
            .width
    }
    pub(crate) fn preferred_height_for_width(&self, w: f32) -> f32 {
        self.measure_window_content(w, w, 0.0, WindowConstraints::INFINITY)
            .height
    }
    pub fn eval_size(&self, p: &WindowSizeProvider) -> Size {
        p.get_size(self)
    }
    pub fn eval_position(&self, p: &WindowPositionProvider, sz: Size) -> Vec2 {
        p.get_position(self, sz)
    }
    pub fn eval_bounds(&self, p: &WindowBoundsProvider) -> Rect {
        p.get_bounds(self)
    }
}

#[derive(Clone)]
pub struct WindowBoundsProvider {
    inner: Rc<dyn Fn(&WindowGeometryProviderScope) -> Rect>,
}
impl WindowBoundsProvider {
    pub fn new<F: Fn(&WindowGeometryProviderScope) -> Rect + 'static>(f: F) -> Self {
        Self { inner: Rc::new(f) }
    }
    pub fn get_bounds(&self, s: &WindowGeometryProviderScope) -> Rect {
        let r = (self.inner)(s);
        debug_assert!(r.w.is_finite() && r.h.is_finite() && r.x.is_finite() && r.y.is_finite());
        r
    }
    pub fn default() -> Self {
        Self::new_provider(
            WindowSizeProvider::default(),
            WindowPositionProvider::default(),
        )
    }
    pub fn absolute(rect: Rect) -> Self {
        Self::new(move |_| rect)
    }
    pub fn new_provider(
        size_provider: WindowSizeProvider,
        position_provider: WindowPositionProvider,
    ) -> Self {
        Self::new(move |scope| {
            let sz = size_provider.get_size(scope);
            let pos = position_provider.get_position(scope, sz);
            Rect {
                x: pos.x,
                y: pos.y,
                w: sz.width,
                h: sz.height,
            }
        })
    }
}
impl Default for WindowBoundsProvider {
    fn default() -> Self {
        Self::default()
    }
}

static CASCADE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct WindowPositionProvider {
    inner: Rc<dyn Fn(&WindowGeometryProviderScope, Size) -> Vec2>,
}
impl WindowPositionProvider {
    pub fn new<F: Fn(&WindowGeometryProviderScope, Size) -> Vec2 + 'static>(f: F) -> Self {
        Self { inner: Rc::new(f) }
    }
    pub fn get_position(&self, s: &WindowGeometryProviderScope, sz: Size) -> Vec2 {
        let v = (self.inner)(s, sz);
        debug_assert!(v.x.is_finite() && v.y.is_finite());
        v
    }
    pub fn default() -> Self {
        Self::new(|_, _| {
            let n = CASCADE_COUNTER.fetch_add(1, Ordering::Relaxed) as f32;
            Vec2 {
                x: 40.0 + (n * 24.0) % 200.0,
                y: 40.0 + (n * 24.0) % 200.0,
            }
        })
    }
    pub fn current() -> Self {
        Self::new(|s, _| Vec2 {
            x: s.window_metrics.bounds.x,
            y: s.window_metrics.bounds.y,
        })
    }
    pub fn centered_on_screen() -> Self {
        Self::centered_in_screen_bounds(Vec2::ZERO)
    }
    pub fn centered_in_screen_bounds(offset: Vec2) -> Self {
        Self::new(move |s, sz| {
            let avail = s.window_metrics.screen.available_bounds();
            Vec2 {
                x: avail.x + (avail.w - sz.width) / 2.0 + offset.x,
                y: avail.y + (avail.h - sz.height) / 2.0 + offset.y,
            }
        })
    }
    pub fn centered_in_screen() -> Self {
        Self::new(|s, sz| {
            let b = s.window_metrics.screen.bounds;
            Vec2 {
                x: b.x + (b.w - sz.width) / 2.0,
                y: b.y + (b.h - sz.height) / 2.0,
            }
        })
    }
    pub fn aligned_to_screen_available(ax: f32, ay: f32, offset: Vec2) -> Self {
        Self::new(move |s, sz| {
            let avail = s.window_metrics.screen.available_bounds();
            Vec2 {
                x: avail.x + (avail.w - sz.width) * ax.clamp(0.0, 1.0) + offset.x,
                y: avail.y + (avail.h - sz.height) * ay.clamp(0.0, 1.0) + offset.y,
            }
        })
    }
    pub fn absolute(pos: Vec2) -> Self {
        Self::new(move |_, _| pos)
    }
    pub fn absolute_xy(x: f32, y: f32) -> Self {
        Self::absolute(Vec2 { x, y })
    }
    pub fn aligned_to_parent(
        anchor_x: f32,
        anchor_y: f32,
        align_x: f32,
        align_y: f32,
        offset: Vec2,
        exclude_parent_insets: bool,
    ) -> Self {
        Self::new(move |s, sz| {
            let pm = s
                .parent_metrics
                .as_ref()
                .expect("AlignedToParentWindow requires parent_metrics");
            let parent_bounds = if exclude_parent_insets {
                let ins = pm.insets;
                Rect {
                    x: pm.bounds.x + ins.left,
                    y: pm.bounds.y + ins.top,
                    w: (pm.bounds.w - ins.left - ins.right).max(0.0),
                    h: (pm.bounds.h - ins.top - ins.bottom).max(0.0),
                }
            } else {
                pm.bounds
            };
            let anchor = Vec2 {
                x: parent_bounds.x + parent_bounds.w * anchor_x.clamp(0.0, 1.0),
                y: parent_bounds.y + parent_bounds.h * anchor_y.clamp(0.0, 1.0),
            };
            let target = Rect {
                x: anchor.x - sz.width,
                y: anchor.y - sz.height,
                w: sz.width * 2.0,
                h: sz.height * 2.0,
            };
            Vec2 {
                x: target.x + (target.w - sz.width) * align_x.clamp(0.0, 1.0) + offset.x,
                y: target.y + (target.h - sz.height) * align_y.clamp(0.0, 1.0) + offset.y,
            }
        })
    }
    pub fn centered_in_parent(offset: Vec2) -> Self {
        Self::aligned_to_parent(0.5, 0.5, 0.5, 0.5, offset, false)
    }
}
impl Default for WindowPositionProvider {
    fn default() -> Self {
        Self::default()
    }
}

#[derive(Clone)]
pub struct WindowSizeProvider {
    inner: Rc<dyn Fn(&WindowGeometryProviderScope) -> Size>,
}
impl WindowSizeProvider {
    pub fn new<F: Fn(&WindowGeometryProviderScope) -> Size + 'static>(f: F) -> Self {
        Self { inner: Rc::new(f) }
    }
    pub fn get_size(&self, s: &WindowGeometryProviderScope) -> Size {
        let sz = (self.inner)(s);
        debug_assert!(
            sz.width.is_finite() && sz.height.is_finite() && sz.width >= 0.0 && sz.height >= 0.0
        );
        sz
    }
    pub fn default() -> Self {
        Self::fixed(Size {
            width: 800.0,
            height: 600.0,
        })
    }
    pub fn current() -> Self {
        Self::new(|s| Size {
            width: s.window_metrics.bounds.w,
            height: s.window_metrics.bounds.h,
        })
    }
    pub fn fixed(sz: Size) -> Self {
        Self::new(move |_| sz)
    }
    pub fn fixed_xy(w: f32, h: f32) -> Self {
        Self::fixed(Size {
            width: w,
            height: h,
        })
    }
    pub fn unconstrained() -> Self {
        Self::new(|s| {
            let avail = s.window_metrics.screen.available_bounds();
            let unconstrained = s.content_to_window_size(s.measure_window_content(
                0.0,
                WindowConstraints::INFINITY,
                0.0,
                WindowConstraints::INFINITY,
            ));
            let w_fits = unconstrained.width <= avail.w;
            let h_fits = unconstrained.height <= avail.h;
            if w_fits && h_fits {
                unconstrained
            } else if !w_fits && !h_fits {
                Size {
                    width: avail.w,
                    height: avail.h,
                }
            } else if !w_fits {
                let h = s.preferred_height_for_width(avail.w);
                s.content_to_window_size(Size {
                    width: avail.w,
                    height: h,
                })
            } else {
                let w = s.preferred_width_for_height(avail.h);
                s.content_to_window_size(Size {
                    width: w,
                    height: avail.h,
                })
            }
        })
    }
    pub fn preferred_width(h: f32) -> Self {
        Self::new(move |s| {
            let w = s.preferred_width_for_height(h);
            s.content_to_window_size(Size {
                width: w,
                height: h,
            })
        })
    }
    pub fn preferred_height(w: f32) -> Self {
        Self::new(move |s| {
            let h = s.preferred_height_for_width(w);
            s.content_to_window_size(Size {
                width: w,
                height: h,
            })
        })
    }
}
impl Default for WindowSizeProvider {
    fn default() -> Self {
        Self::default()
    }
}

pub struct WindowState {
    pub is_initialized: bool,
    screen_id: Option<String>,
    placement: Option<WindowPlacement>,
    is_minimized: Option<bool>,
    bounds: Option<Rect>,
    pending_screen: Option<WindowScreenProvider>,
    pending_placement: Option<WindowPlacement>,
    pending_minimized: Option<bool>,
    pending_bounds: VecDeque<WindowBoundsProvider>,
}
impl WindowState {
    pub fn create_uninitialized() -> Self {
        Self {
            is_initialized: false,
            screen_id: None,
            placement: None,
            is_minimized: None,
            bounds: None,
            pending_screen: None,
            pending_placement: None,
            pending_minimized: None,
            pending_bounds: VecDeque::new(),
        }
    }
    pub fn new(
        initial_screen_provider: WindowScreenProvider,
        initial_placement: WindowPlacement,
        initial_bounds_provider: WindowBoundsProvider,
        initially_minimized: bool,
    ) -> Self {
        let mut s = Self::create_uninitialized();
        s.request_screen(initial_screen_provider);
        s.request_placement(initial_placement);
        s.request_bounds_provider(initial_bounds_provider);
        s.request_minimized(initially_minimized);
        s
    }
    pub fn with_bounds(
        initial_position: Option<Vec2>,
        initial_size: Option<Size>,
        initially_minimized: bool,
    ) -> Self {
        let sp = initial_size
            .map(WindowSizeProvider::fixed)
            .unwrap_or_else(WindowSizeProvider::default);
        let pp = initial_position
            .map(WindowPositionProvider::absolute)
            .unwrap_or_else(WindowPositionProvider::default);
        Self::new(
            WindowScreenProvider::default(),
            WindowPlacement::Floating,
            WindowBoundsProvider::new_provider(sp, pp),
            initially_minimized,
        )
    }
    pub fn initialize(
        &mut self,
        screen_id: String,
        placement: WindowPlacement,
        is_minimized: bool,
        bounds: Rect,
    ) {
        self.is_initialized = true;
        self.screen_id = Some(screen_id);
        self.placement = Some(placement);
        self.is_minimized = Some(is_minimized);
        self.bounds = Some(bounds);
    }
    pub fn screen_id(&self) -> &str {
        self.screen_id
            .as_deref()
            .expect("window not initialized: screenId")
    }
    pub fn placement_value(&self) -> WindowPlacement {
        self.placement.expect("window not initialized: placement")
    }
    pub fn is_minimized_value(&self) -> bool {
        self.is_minimized
            .expect("window not initialized: isMinimized")
    }
    pub fn bounds_value(&self) -> Rect {
        self.bounds.expect("window not initialized: bounds")
    }
    pub fn position(&self) -> Vec2 {
        let b = self.bounds_value();
        Vec2 { x: b.x, y: b.y }
    }
    pub fn size(&self) -> Size {
        let b = self.bounds_value();
        Size {
            width: b.w,
            height: b.h,
        }
    }
    pub fn try_screen_id(&self) -> Option<&str> {
        self.screen_id.as_deref()
    }
    pub fn try_placement(&self) -> Option<WindowPlacement> {
        self.placement
    }
    pub fn try_is_minimized(&self) -> Option<bool> {
        self.is_minimized
    }
    pub fn try_bounds(&self) -> Option<Rect> {
        self.bounds
    }
    pub fn request_screen(&mut self, p: WindowScreenProvider) {
        self.pending_screen = Some(p);
    }
    pub fn request_placement(&mut self, p: WindowPlacement) {
        self.pending_placement = Some(p);
    }
    pub fn request_minimized(&mut self, v: bool) {
        self.pending_minimized = Some(v);
    }
    pub fn request_bounds_provider(&mut self, p: WindowBoundsProvider) {
        self.pending_bounds.push_back(p);
    }
    pub fn request_bounds_fn<F: Fn(&WindowGeometryProviderScope) -> Rect + 'static>(
        &mut self,
        f: F,
    ) {
        self.request_bounds_provider(WindowBoundsProvider::new(f));
    }
    pub fn request_bounds(&mut self, r: Rect) {
        self.request_bounds_provider(WindowBoundsProvider::absolute(r));
    }
    pub fn request_position_provider(&mut self, p: WindowPositionProvider) {
        self.request_bounds_provider(WindowBoundsProvider::new_provider(
            WindowSizeProvider::current(),
            p,
        ));
    }
    pub fn request_position(&mut self, pos: Vec2) {
        self.request_position_provider(WindowPositionProvider::absolute(pos));
    }
    pub fn request_position_xy(&mut self, x: f32, y: f32) {
        self.request_position(Vec2 { x, y });
    }
    pub fn request_size_provider(&mut self, p: WindowSizeProvider) {
        self.request_bounds_provider(WindowBoundsProvider::new_provider(
            p,
            WindowPositionProvider::current(),
        ));
    }
    pub fn request_size(&mut self, sz: Size) {
        self.request_size_provider(WindowSizeProvider::fixed(sz));
    }
    pub fn request_size_xy(&mut self, w: f32, h: f32) {
        self.request_size(Size {
            width: w,
            height: h,
        });
    }
    pub fn take_pending_screen(&mut self) -> Option<WindowScreenProvider> {
        self.pending_screen.take()
    }
    pub fn take_pending_placement(&mut self) -> Option<WindowPlacement> {
        self.pending_placement.take()
    }
    pub fn take_pending_minimized(&mut self) -> Option<bool> {
        self.pending_minimized.take()
    }
    pub fn drain_pending_bounds(&mut self) -> Vec<WindowBoundsProvider> {
        self.pending_bounds.drain(..).collect()
    }
    pub fn has_pending(&self) -> bool {
        self.pending_screen.is_some()
            || self.pending_placement.is_some()
            || self.pending_minimized.is_some()
            || !self.pending_bounds.is_empty()
    }
    pub fn apply_pending(
        &mut self,
        screen_scope: &WindowScreenProviderScope,
        geometry_scope: &WindowGeometryProviderScope,
    ) -> Option<Rect> {
        if let Some(p) = self.take_pending_screen() {
            self.screen_id = Some(p.get_screen(screen_scope).id);
        }
        if let Some(p) = self.take_pending_placement() {
            self.placement = Some(p);
        }
        if let Some(m) = self.take_pending_minimized() {
            self.is_minimized = Some(m);
        }
        let pending = self.drain_pending_bounds();
        if pending.is_empty() {
            return None;
        }
        let mut last = None;
        for p in pending {
            let r = p.get_bounds(geometry_scope);
            self.bounds = Some(r);
            last = Some(r);
            if self.placement != Some(WindowPlacement::Floating) {
                self.placement = Some(WindowPlacement::Floating);
            }
        }
        last
    }
    pub fn on_host_bounds_changed(&mut self, bounds: Rect, screen_id: String) {
        self.bounds = Some(bounds);
        self.screen_id = Some(screen_id);
        if !self.is_initialized {
            self.is_initialized = true;
            if self.placement.is_none() {
                self.placement = Some(WindowPlacement::Floating);
            }
            if self.is_minimized.is_none() {
                self.is_minimized = Some(false);
            }
        }
    }
    pub fn on_host_placement_changed(&mut self, p: WindowPlacement) {
        self.placement = Some(p);
    }
    pub fn on_host_minimized_changed(&mut self, v: bool) {
        self.is_minimized = Some(v);
    }
}
impl Default for WindowState {
    fn default() -> Self {
        Self::new(
            WindowScreenProvider::default(),
            WindowPlacement::Floating,
            WindowBoundsProvider::default(),
            false,
        )
    }
}

pub struct DialogState {
    pub is_initialized: bool,
    screen_id: Option<String>,
    bounds: Option<Rect>,
    pending_screen: Option<WindowScreenProvider>,
    pending_bounds: VecDeque<WindowBoundsProvider>,
}
impl DialogState {
    pub fn create_uninitialized() -> Self {
        Self {
            is_initialized: false,
            screen_id: None,
            bounds: None,
            pending_screen: None,
            pending_bounds: VecDeque::new(),
        }
    }
    pub fn new(
        initial_screen_provider: WindowScreenProvider,
        initial_bounds_provider: WindowBoundsProvider,
    ) -> Self {
        let mut s = Self::create_uninitialized();
        s.request_screen(initial_screen_provider);
        s.request_bounds_provider(initial_bounds_provider);
        s
    }
    pub fn with_bounds(initial_position: Option<Vec2>, initial_size: Option<Size>) -> Self {
        let sp = initial_size
            .map(WindowSizeProvider::fixed)
            .unwrap_or_else(WindowSizeProvider::default);
        let pp = initial_position
            .map(WindowPositionProvider::absolute)
            .unwrap_or_else(WindowPositionProvider::default);
        Self::new(
            WindowScreenProvider::default(),
            WindowBoundsProvider::new_provider(sp, pp),
        )
    }
    pub fn screen_id(&self) -> &str {
        self.screen_id
            .as_deref()
            .expect("dialog not initialized: screenId")
    }
    pub fn bounds_value(&self) -> Rect {
        self.bounds.expect("dialog not initialized: bounds")
    }
    pub fn position(&self) -> Vec2 {
        let b = self.bounds_value();
        Vec2 { x: b.x, y: b.y }
    }
    pub fn size(&self) -> Size {
        let b = self.bounds_value();
        Size {
            width: b.w,
            height: b.h,
        }
    }
    pub fn try_screen_id(&self) -> Option<&str> {
        self.screen_id.as_deref()
    }
    pub fn try_bounds(&self) -> Option<Rect> {
        self.bounds
    }
    pub fn request_screen(&mut self, p: WindowScreenProvider) {
        self.pending_screen = Some(p);
    }
    pub fn request_bounds_provider(&mut self, p: WindowBoundsProvider) {
        self.pending_bounds.push_back(p);
    }
    pub fn request_bounds_fn<F: Fn(&WindowGeometryProviderScope) -> Rect + 'static>(
        &mut self,
        f: F,
    ) {
        self.request_bounds_provider(WindowBoundsProvider::new(f));
    }
    pub fn request_bounds(&mut self, r: Rect) {
        self.request_bounds_provider(WindowBoundsProvider::absolute(r));
    }
    pub fn request_position_provider(&mut self, p: WindowPositionProvider) {
        self.request_bounds_provider(WindowBoundsProvider::new_provider(
            WindowSizeProvider::current(),
            p,
        ));
    }
    pub fn request_position(&mut self, pos: Vec2) {
        self.request_position_provider(WindowPositionProvider::absolute(pos));
    }
    pub fn request_position_xy(&mut self, x: f32, y: f32) {
        self.request_position(Vec2 { x, y });
    }
    pub fn request_size_provider(&mut self, p: WindowSizeProvider) {
        self.request_bounds_provider(WindowBoundsProvider::new_provider(
            p,
            WindowPositionProvider::current(),
        ));
    }
    pub fn request_size(&mut self, sz: Size) {
        self.request_size_provider(WindowSizeProvider::fixed(sz));
    }
    pub fn request_size_xy(&mut self, w: f32, h: f32) {
        self.request_size(Size {
            width: w,
            height: h,
        });
    }
    pub fn take_pending_screen(&mut self) -> Option<WindowScreenProvider> {
        self.pending_screen.take()
    }
    pub fn drain_pending_bounds(&mut self) -> Vec<WindowBoundsProvider> {
        self.pending_bounds.drain(..).collect()
    }
    pub fn apply_pending(
        &mut self,
        screen_scope: &WindowScreenProviderScope,
        geometry_scope: &WindowGeometryProviderScope,
    ) -> Option<Rect> {
        if let Some(p) = self.take_pending_screen() {
            self.screen_id = Some(p.get_screen(screen_scope).id);
        }
        let pending = self.drain_pending_bounds();
        if pending.is_empty() {
            return None;
        }
        let mut last = None;
        for p in pending {
            let r = p.get_bounds(geometry_scope);
            self.bounds = Some(r);
            last = Some(r);
        }
        last
    }
    pub fn on_host_bounds_changed(&mut self, bounds: Rect, screen_id: String) {
        self.bounds = Some(bounds);
        self.screen_id = Some(screen_id);
        if !self.is_initialized {
            self.is_initialized = true;
        }
    }
    pub fn initialize(&mut self, screen_id: String, bounds: Rect) {
        self.is_initialized = true;
        self.screen_id = Some(screen_id);
        self.bounds = Some(bounds);
    }
}
impl Default for DialogState {
    fn default() -> Self {
        Self::new(
            WindowScreenProvider::default(),
            WindowBoundsProvider::default(),
        )
    }
}

pub fn remember_window_state(
    key: impl Into<String>,
    initial_screen_provider: WindowScreenProvider,
    initial_placement: WindowPlacement,
    initial_bounds_provider: WindowBoundsProvider,
    initially_minimized: bool,
) -> Rc<RefCell<WindowState>> {
    let key = key.into();
    repose_core::remember_with_key(key, move || {
        RefCell::new(WindowState::new(
            initial_screen_provider.clone(),
            initial_placement,
            initial_bounds_provider.clone(),
            initially_minimized,
        ))
    })
}
pub fn remember_window_state_with_bounds(
    key: impl Into<String>,
    initial_position: Option<Vec2>,
    initial_size: Option<Size>,
    initially_minimized: bool,
) -> Rc<RefCell<WindowState>> {
    let key = key.into();
    repose_core::remember_with_key(key, move || {
        RefCell::new(WindowState::with_bounds(
            initial_position,
            initial_size,
            initially_minimized,
        ))
    })
}
pub fn remember_dialog_state(
    key: impl Into<String>,
    initial_screen_provider: WindowScreenProvider,
    initial_bounds_provider: WindowBoundsProvider,
) -> Rc<RefCell<DialogState>> {
    let key = key.into();
    repose_core::remember_with_key(key, move || {
        RefCell::new(DialogState::new(
            initial_screen_provider.clone(),
            initial_bounds_provider.clone(),
        ))
    })
}
pub fn remember_dialog_state_with_bounds(
    key: impl Into<String>,
    initial_position: Option<Vec2>,
    initial_size: Option<Size>,
) -> Rc<RefCell<DialogState>> {
    let key = key.into();
    repose_core::remember_with_key(key, move || {
        RefCell::new(DialogState::with_bounds(initial_position, initial_size))
    })
}

use crate::windowing::FloatingWindow;

pub fn apply_window_state_to_floating(
    state: &mut WindowState,
    window: &mut FloatingWindow,
    host_bounds: Rect,
    measure_content: impl Fn(WindowConstraints) -> Size + 'static,
) {
    let screen = Screen::primary(host_bounds);
    let screen_scope = WindowScreenProviderScope::new(vec![screen.clone()], screen.clone());
    let window_metrics = WindowMetrics::new(
        screen.clone(),
        Rect {
            x: window.position.x,
            y: window.position.y,
            w: window.size.width,
            h: window.size.height,
        },
        ScreenInsets::default(),
    );
    let geometry_scope = WindowGeometryProviderScope::new(None, window_metrics, measure_content);
    if !state.is_initialized {
        let pending = state.drain_pending_bounds();
        let rect = if pending.is_empty() {
            WindowBoundsProvider::default().get_bounds(&geometry_scope)
        } else {
            let mut last = None;
            for p in pending {
                last = Some(p.get_bounds(&geometry_scope));
            }
            last.unwrap()
        };
        let screen_id = screen_scope
            .eval(
                &state
                    .take_pending_screen()
                    .unwrap_or_else(WindowScreenProvider::default),
            )
            .id;
        let placement = state
            .take_pending_placement()
            .unwrap_or(WindowPlacement::Floating);
        let minimized = state.take_pending_minimized().unwrap_or(false);
        state.initialize(screen_id, placement, minimized, rect);
    } else {
        state.apply_pending(&screen_scope, &geometry_scope);
    }
    if let Some(bounds) = state.try_bounds() {
        let mut sz = Size {
            width: bounds.w,
            height: bounds.h,
        };
        sz.width = sz.width.clamp(
            window.min_size.width,
            window.max_size.map(|s| s.width).unwrap_or(f32::INFINITY),
        );
        sz.height = sz.height.clamp(
            window.min_size.height,
            window.max_size.map(|s| s.height).unwrap_or(f32::INFINITY),
        );
        sz.width = sz.width.min(host_bounds.w.max(sz.width));
        sz.height = sz.height.min(host_bounds.h.max(sz.height));
        let mut pos = Vec2 {
            x: bounds.x,
            y: bounds.y,
        };
        if host_bounds.w > 1.0 && host_bounds.h > 1.0 {
            let keep = 24.0;
            let min_x = host_bounds.x - sz.width + keep;
            let max_x = host_bounds.x + host_bounds.w - keep;
            let min_y = host_bounds.y - sz.height + keep;
            let max_y = host_bounds.y + host_bounds.h - keep;
            pos.x = pos.x.clamp(min_x, max_x);
            pos.y = pos.y.clamp(min_y, max_y);
        }
        window.position = pos;
        window.size = sz;
    }
    if let Some(p) = state.try_placement() {
        match p {
            WindowPlacement::Maximized | WindowPlacement::Fullscreen => {
                window.position = Vec2 {
                    x: host_bounds.x,
                    y: host_bounds.y,
                };
                window.size = Size {
                    width: host_bounds.w,
                    height: host_bounds.h,
                };
            }
            WindowPlacement::Floating => {}
        }
    }
}

pub fn apply_dialog_state_to_floating(
    state: &mut DialogState,
    dialog_window: &mut FloatingWindow,
    host_bounds: Rect,
    parent_window: Option<&FloatingWindow>,
    measure_content: impl Fn(WindowConstraints) -> Size + 'static,
) {
    let screen = Screen::primary(host_bounds);
    let screen_scope = WindowScreenProviderScope::new(vec![screen.clone()], screen.clone());
    let parent_metrics = parent_window.map(|pw| {
        WindowMetrics::new(
            screen.clone(),
            Rect {
                x: pw.position.x,
                y: pw.position.y,
                w: pw.size.width,
                h: pw.size.height,
            },
            ScreenInsets::default(),
        )
    });
    let window_metrics = WindowMetrics::new(
        screen.clone(),
        Rect {
            x: dialog_window.position.x,
            y: dialog_window.position.y,
            w: dialog_window.size.width,
            h: dialog_window.size.height,
        },
        ScreenInsets::default(),
    );
    let geometry_scope =
        WindowGeometryProviderScope::new(parent_metrics, window_metrics, measure_content);
    if !state.is_initialized {
        let pending = state.drain_pending_bounds();
        let rect = if pending.is_empty() {
            WindowBoundsProvider::default().get_bounds(&geometry_scope)
        } else {
            let mut last = None;
            for p in pending {
                last = Some(p.get_bounds(&geometry_scope));
            }
            last.unwrap()
        };
        let screen_id = screen_scope
            .eval(
                &state
                    .take_pending_screen()
                    .unwrap_or_else(WindowScreenProvider::default),
            )
            .id;
        state.initialize(screen_id, rect);
    } else {
        state.apply_pending(&screen_scope, &geometry_scope);
    }
    if let Some(bounds) = state.try_bounds() {
        let mut sz = Size {
            width: bounds.w,
            height: bounds.h,
        };
        sz.width = sz.width.clamp(
            dialog_window.min_size.width,
            dialog_window
                .max_size
                .map(|s| s.width)
                .unwrap_or(f32::INFINITY),
        );
        sz.height = sz.height.clamp(
            dialog_window.min_size.height,
            dialog_window
                .max_size
                .map(|s| s.height)
                .unwrap_or(f32::INFINITY),
        );
        dialog_window.position = Vec2 {
            x: bounds.x,
            y: bounds.y,
        };
        dialog_window.size = sz;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn host() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            w: 1280.0,
            h: 800.0,
        }
    }
    fn dummy_measure(_c: WindowConstraints) -> Size {
        Size {
            width: 400.0,
            height: 200.0,
        }
    }
    #[test]
    fn centered_fixed() {
        let mut state = WindowState::new(
            WindowScreenProvider::default(),
            WindowPlacement::Floating,
            WindowBoundsProvider::new_provider(
                WindowSizeProvider::fixed(Size {
                    width: 400.0,
                    height: 200.0,
                }),
                WindowPositionProvider::centered_on_screen(),
            ),
            false,
        );
        let mut win = FloatingWindow::new(
            1,
            "test",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        );
        apply_window_state_to_floating(&mut state, &mut win, host(), dummy_measure);
        assert!(state.is_initialized);
        assert!((win.position.x - 440.0).abs() < 1.0);
        assert!((win.position.y - 300.0).abs() < 1.0);
        assert_eq!(win.size.width, 400.0);
    }
    #[test]
    fn unconstrained_sizes_to_content() {
        let mut state = WindowState::new(
            WindowScreenProvider::default(),
            WindowPlacement::Floating,
            WindowBoundsProvider::new_provider(
                WindowSizeProvider::unconstrained(),
                WindowPositionProvider::centered_on_screen(),
            ),
            false,
        );
        let mut win = FloatingWindow::new(
            1,
            "test",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        );
        apply_window_state_to_floating(&mut state, &mut win, host(), dummy_measure);
        assert!((win.size.width - 400.0).abs() < 1.0);
        assert!((win.size.height - 200.0).abs() < 1.0);
    }
    #[test]
    fn async_request_distinction() {
        let mut state = WindowState::default();
        let h = host();
        let mut win = FloatingWindow::new(
            1,
            "test",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        );
        apply_window_state_to_floating(&mut state, &mut win, h, dummy_measure);
        let initial = win.position;
        state.request_position(Vec2 { x: 100.0, y: 100.0 });
        assert_eq!(win.position, initial);
        apply_window_state_to_floating(&mut state, &mut win, h, dummy_measure);
        assert!((win.position.x - 100.0).abs() < 1.0);
    }
    #[test]
    fn request_size_preserves_position() {
        let mut state = WindowState::with_bounds(
            Some(Vec2 { x: 50.0, y: 60.0 }),
            Some(Size {
                width: 300.0,
                height: 200.0,
            }),
            false,
        );
        let h = host();
        let mut win = FloatingWindow::new(
            1,
            "test",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        );
        apply_window_state_to_floating(&mut state, &mut win, h, dummy_measure);
        assert!((win.position.x - 50.0).abs() < 1.5);
        state.request_size(Size {
            width: 500.0,
            height: 400.0,
        });
        apply_window_state_to_floating(&mut state, &mut win, h, dummy_measure);
        assert!((win.position.x - 50.0).abs() < 1.5);
        assert!((win.size.width - 500.0).abs() < 1.0);
    }
    #[test]
    fn dialog_centered_in_parent() {
        let parent = FloatingWindow::new(
            1,
            "parent",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        )
        .position(100.0, 100.0)
        .size(400.0, 300.0);
        let mut dialog_state = DialogState::new(
            WindowScreenProvider::default(),
            WindowBoundsProvider::new_provider(
                WindowSizeProvider::fixed(Size {
                    width: 200.0,
                    height: 100.0,
                }),
                WindowPositionProvider::centered_in_parent(Vec2::ZERO),
            ),
        );
        let mut dialog_win = FloatingWindow::new(
            2,
            "dialog",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        );
        apply_dialog_state_to_floating(
            &mut dialog_state,
            &mut dialog_win,
            host(),
            Some(&parent),
            dummy_measure,
        );
        assert!((dialog_win.position.x - 200.0).abs() < 1.0);
        assert!((dialog_win.position.y - 200.0).abs() < 1.0);
    }
    #[test]
    fn min_max_clamping() {
        let mut state = WindowState::new(
            WindowScreenProvider::default(),
            WindowPlacement::Floating,
            WindowBoundsProvider::new_provider(
                WindowSizeProvider::fixed(Size {
                    width: 100.0,
                    height: 100.0,
                }),
                WindowPositionProvider::absolute(Vec2 { x: 0.0, y: 0.0 }),
            ),
            false,
        );
        let mut win = FloatingWindow::new(
            1,
            "test",
            Rc::new(|| repose_core::View::new(0, repose_core::ViewKind::Box)),
        )
        .min_size(200.0, 200.0)
        .max_size(300.0, 300.0);
        apply_window_state_to_floating(&mut state, &mut win, host(), dummy_measure);
        assert_eq!(win.size.width, 200.0);
        state.request_size(Size {
            width: 500.0,
            height: 500.0,
        });
        apply_window_state_to_floating(&mut state, &mut win, host(), dummy_measure);
        assert_eq!(win.size.width, 300.0);
    }
    #[test]
    fn screen_selection() {
        let s1 = Screen::new(
            "screen1",
            Rect {
                x: 0.0,
                y: 0.0,
                w: 1280.0,
                h: 800.0,
            },
            ScreenInsets::default(),
        );
        let s2 = Screen::new(
            "screen2",
            Rect {
                x: 1280.0,
                y: 0.0,
                w: 1280.0,
                h: 800.0,
            },
            ScreenInsets::default(),
        );
        let scope = WindowScreenProviderScope::new(vec![s1.clone(), s2.clone()], s1.clone());
        let provider = WindowScreenProvider::with_id("screen2");
        assert_eq!(provider.get_screen(&scope).id, "screen2");
    }
}
