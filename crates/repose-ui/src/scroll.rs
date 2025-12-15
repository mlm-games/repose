//! # Scroll model
//!
//! Repose separates *visual scroll containers* from *scroll state* that can
//! include inertia and programmatic control.
//!
//! There are three scrolling primitives:
//!
//! - `ScrollState` — vertical (Y) inertia.
//! - `HorizontalScrollState` — horizontal (X) inertia.
//! - `ScrollStateXY` — 2D scroll with independent X/Y offsets.
//!
//! Each state stores viewport size, content size, offset, and velocity. The
//! `scroll_immediate` methods consume a requested delta and return leftover
//! motion that parent scroll views can use for nested scrolling.
//!
//! Example: vertical `ScrollArea`
//!
//! ```rust
//! use repose_core::*;
//! use repose_ui::*;
//!
//! fn LongList() -> View {
//!     let state = scroll::remember_scroll_state("list");
//!
//!     let content = Column(Modifier::new()).child(
//!         (0..100).map(|i| Text(format!("Row {i}"))).collect::<Vec<_>>()
//!     );
//!
//!     scroll::ScrollArea(Modifier::new().fill_max_size(), state, content)
//! }
//! ```
//!
//! Internally, `ScrollArea` builds a `ViewKind::ScrollV` node with:
//!
//! - `on_scroll: Rc<dyn Fn(Vec2) -> Vec2>` that calls `state.scroll_immediate`.
//! - `set_viewport_height` / `set_content_height` callbacks that keep the
//!   scroll state clamped when sizes change.
//! - `get_scroll_offset` / `set_scroll_offset` used by the layout pass and
//!   scrollbars.
//!
//! `layout_and_paint`:
//!
//! - Uses the inner content rect (after padding) as the *viewport*.
//! - Clips children into that rect.
//! - Applies the current scroll offsets as a translation.
//! - Clamps child `HitRegion`s into the viewport.
//! - Draws vertical/horizontal scrollbars that can be dragged by pointer.

use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Inertial scroll state (single axis Y for now).
pub struct ScrollState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    content_height: Signal<f32>,

    // physics
    vel: RefCell<f32>,
    last_t: RefCell<Instant>,
    animating: RefCell<bool>,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: signal(0.0),
            content_height: signal(0.0),
            vel: RefCell::new(0.0),
            last_t: RefCell::new(Instant::now()),
            animating: RefCell::new(false),
        }
    }

    pub fn set_viewport_height(&self, h: f32) {
        self.viewport_height.set(h.max(0.0));
        self.clamp_offset();
    }
    pub fn set_content_height(&self, h: f32) {
        self.content_height.set(h.max(0.0));
        self.clamp_offset();
    }
    pub fn set_offset(&self, off: f32) {
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);
        self.scroll_offset.set(off.clamp(0.0, max_off));
    }

    fn clamp_offset(&self) {
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);
        self.scroll_offset.update(|o| {
            if *o > max_off {
                *o = max_off;
            }
            if *o < 0.0 {
                *o = 0.0;
            }
        });
    }

    pub fn get(&self) -> f32 {
        self.scroll_offset.get()
    }

    /// Consume dy (pixels), clamp to bounds, return leftover.
    pub fn scroll_immediate(&self, dy: f32) -> f32 {
        let before = self.scroll_offset.get();
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);

        let new_off = (before + dy).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);

        // Update velocity for fling
        let consumed = new_off - before;
        *self.vel.borrow_mut() = consumed; // px/frame baseline
        *self.animating.borrow_mut() = consumed.abs() > 0.25;

        dy - (new_off - before)
    }

    /// Advance physics one tick; returns true if animating.
    pub fn tick(&self) -> bool {
        if !*self.animating.borrow() {
            return false;
        }

        let now = Instant::now();
        let dt = (now - *self.last_t.borrow()).as_secs_f32().min(0.1);
        *self.last_t.borrow_mut() = now;
        if dt <= 0.0 {
            return false;
        }

        let mut vel = *self.vel.borrow();
        if vel.abs() < 0.05 {
            *self.vel.borrow_mut() = 0.0;
            *self.animating.borrow_mut() = false;
            return false;
        }

        let before = self.scroll_offset.get();
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);

        // Integrate and clamp
        let new_off = (before + vel).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);

        // Friction
        vel *= 0.9;
        *self.vel.borrow_mut() = vel;

        true
    }
}

/// X-only state
pub struct HorizontalScrollState {
    scroll_offset: Signal<f32>,
    viewport_width: Signal<f32>,
    content_width: Signal<f32>,
    vel: RefCell<f32>,
    last_t: RefCell<Instant>,
    animating: RefCell<bool>,
}
impl Default for HorizontalScrollState {
    fn default() -> Self {
        Self::new()
    }
}

impl HorizontalScrollState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_width: signal(0.0),
            content_width: signal(0.0),
            vel: RefCell::new(0.0),
            last_t: RefCell::new(Instant::now()),
            animating: RefCell::new(false),
        }
    }
    pub fn set_viewport_width(&self, w: f32) {
        self.viewport_width.set(w.max(0.0));
        self.clamp();
    }
    pub fn set_content_width(&self, w: f32) {
        self.content_width.set(w.max(0.0));
        self.clamp();
    }
    pub fn set_offset(&self, off: f32) {
        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        self.scroll_offset.set(off.clamp(0.0, max_off));
    }
    fn clamp(&self) {
        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        self.scroll_offset.update(|o| {
            *o = o.clamp(0.0, max_off);
        });
    }
    pub fn get(&self) -> f32 {
        self.scroll_offset.get()
    }
    pub fn scroll_immediate(&self, dx: f32) -> f32 {
        let before = self.scroll_offset.get();
        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        let new_off = (before + dx).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);
        let consumed = new_off - before;
        *self.vel.borrow_mut() = consumed; // px/frame baseline
        *self.animating.borrow_mut() = consumed.abs() > 0.25;
        dx - (new_off - before)
    }
    pub fn tick(&self) -> bool {
        if !*self.animating.borrow() {
            return false;
        }
        let now = Instant::now();
        let dt = (now - *self.last_t.borrow()).as_secs_f32().min(0.1);
        *self.last_t.borrow_mut() = now;
        if dt <= 0.0 {
            return false;
        }
        let vel = *self.vel.borrow();
        if vel.abs() < 0.05 {
            *self.animating.borrow_mut() = false;
            *self.vel.borrow_mut() = 0.0;
            return false;
        }
        let before = self.scroll_offset.get();
        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        let new_off = (before + vel).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);
        *self.vel.borrow_mut() = vel * 0.9;
        true
    }
}

/// 2D state
pub struct ScrollStateXY {
    off_x: Signal<f32>,
    off_y: Signal<f32>,
    vp_w: Signal<f32>,
    vp_h: Signal<f32>,
    c_w: Signal<f32>,
    c_h: Signal<f32>,
    vel_x: RefCell<f32>,
    vel_y: RefCell<f32>,
    last_t: RefCell<Instant>,
    animating: RefCell<bool>,
}
impl Default for ScrollStateXY {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrollStateXY {
    pub fn new() -> Self {
        Self {
            off_x: signal(0.0),
            off_y: signal(0.0),
            vp_w: signal(0.0),
            vp_h: signal(0.0),
            c_w: signal(0.0),
            c_h: signal(0.0),
            vel_x: RefCell::new(0.0),
            vel_y: RefCell::new(0.0),
            last_t: RefCell::new(Instant::now()),
            animating: RefCell::new(false),
        }
    }
    pub fn set_viewport(&self, w: f32, h: f32) {
        self.vp_w.set(w.max(0.0));
        self.vp_h.set(h.max(0.0));
        self.clamp();
    }
    pub fn set_content(&self, w: f32, h: f32) {
        self.c_w.set(w.max(0.0));
        self.c_h.set(h.max(0.0));
        self.clamp();
    }
    pub fn set_offset_xy(&self, x: f32, y: f32) {
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        self.off_x.set(x.clamp(0.0, max_x));
        self.off_y.set(y.clamp(0.0, max_y));
    }
    fn clamp(&self) {
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        self.off_x.update(|x| {
            *x = x.clamp(0.0, max_x);
        });
        self.off_y.update(|y| {
            *y = y.clamp(0.0, max_y);
        });
    }
    pub fn get(&self) -> (f32, f32) {
        (self.off_x.get(), self.off_y.get())
    }
    pub fn scroll_immediate(&self, d: Vec2) -> Vec2 {
        let bx = self.off_x.get();
        let by = self.off_y.get();
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        let nx = (bx + d.x).clamp(0.0, max_x);
        let ny = (by + d.y).clamp(0.0, max_y);
        self.off_x.set(nx);
        self.off_y.set(ny);
        *self.vel_x.borrow_mut() = (nx - bx) * 5.0;
        *self.vel_y.borrow_mut() = (ny - by) * 5.0;
        *self.animating.borrow_mut() = true;
        Vec2 {
            x: d.x - (nx - bx),
            y: d.y - (ny - by),
        }
    }
    pub fn tick(&self) -> bool {
        if !*self.animating.borrow() {
            return false;
        }
        let now = Instant::now();
        let dt = (now - *self.last_t.borrow()).as_secs_f32().min(0.1);
        *self.last_t.borrow_mut() = now;
        if dt <= 0.0 {
            return false;
        }
        let vx = *self.vel_x.borrow();
        let vy = *self.vel_y.borrow();
        if vx.abs() < 0.5 && vy.abs() < 0.5 {
            *self.animating.borrow_mut() = false;
            *self.vel_x.borrow_mut() = 0.0;
            *self.vel_y.borrow_mut() = 0.0;
            return false;
        }
        let (bx, by) = (self.off_x.get(), self.off_y.get());
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        let nx = (bx + vx * dt * 60.0).clamp(0.0, max_x);
        let ny = (by + vy * dt * 60.0).clamp(0.0, max_y);
        self.off_x.set(nx);
        self.off_y.set(ny);
        *self.vel_x.borrow_mut() = vx * 0.95;
        *self.vel_y.borrow_mut() = vy * 0.95;
        true
    }
}

/// Remembered ScrollState (requires unique key).
pub fn remember_scroll_state(key: impl Into<String>) -> Rc<ScrollState> {
    repose_core::remember_with_key(key.into(), ScrollState::new)
}

pub fn remember_horizontal_scroll_state(key: impl Into<String>) -> Rc<HorizontalScrollState> {
    repose_core::remember_with_key(key.into(), HorizontalScrollState::new)
}
pub fn remember_scroll_state_xy(key: impl Into<String>) -> Rc<ScrollStateXY> {
    repose_core::remember_with_key(key.into(), ScrollStateXY::new)
}

/// Scroll container with inertia, like verticalScroll.
pub fn ScrollArea(modifier: Modifier, state: Rc<ScrollState>, content: View) -> View {
    let st_clone = state.clone();
    let on_scroll = {
        Rc::new(move |d: Vec2| -> Vec2 {
            Vec2 {
                x: d.x,
                y: st_clone.scroll_immediate(d.y),
            }
        })
    };
    let set_viewport = {
        let st = state.clone();
        Rc::new(move |h: f32| st.set_viewport_height(h))
    };
    let set_content = {
        let st = state.clone();
        Rc::new(move |h: f32| st.set_content_height(h))
    };
    let get_scroll = {
        let st = state.clone();
        Rc::new(move || {
            st.tick();
            st.get()
        })
    };
    let set_scroll = {
        let st = state.clone();
        Rc::new(move |off: f32| st.set_offset(off))
    };
    View::new(
        0,
        ViewKind::ScrollV {
            on_scroll: Some(on_scroll),
            set_viewport_height: Some(set_viewport),
            set_content_height: Some(set_content),
            get_scroll_offset: Some(get_scroll),
            set_scroll_offset: Some(set_scroll),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

pub fn HorizontalScrollArea(
    modifier: Modifier,
    state: Rc<HorizontalScrollState>,
    content: View,
) -> View {
    let st_clone = state.clone();
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            Vec2 {
                x: st_clone.scroll_immediate(d.x),
                y: d.y,
            }
        })
    };
    let set_viewport_w = {
        let st = state.clone();
        Rc::new(move |w: f32| st.set_viewport_width(w))
    };
    let set_content_w = {
        let st = state.clone();
        Rc::new(move |w: f32| st.set_content_width(w))
    };
    let get_scroll_xy = {
        let st = state.clone();
        Rc::new(move || {
            st.tick();
            (st.get(), 0.0)
        })
    };
    let set_xy = {
        let st = state.clone();
        Rc::new(move |x: f32, _y: f32| st.set_offset(x))
    };
    // Use ScrollXY, but only X is active
    View::new(
        0,
        ViewKind::ScrollXY {
            on_scroll: Some(on_scroll),
            set_viewport_width: Some(set_viewport_w),
            set_viewport_height: None,
            set_content_width: Some(set_content_w),
            set_content_height: None,
            get_scroll_offset_xy: Some(get_scroll_xy),
            set_scroll_offset_xy: Some(set_xy),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

pub fn ScrollAreaXY(modifier: Modifier, state: Rc<ScrollStateXY>, content: View) -> View {
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 { st.scroll_immediate(d) })
    };
    let set_vw = {
        let st = state.clone();
        Rc::new(move |w: f32| st.set_viewport(w, st.vp_h.get()))
    };
    let set_vh = {
        let st = state.clone();
        Rc::new(move |h: f32| st.set_viewport(st.vp_w.get(), h))
    };
    let set_cw = {
        let st = state.clone();
        Rc::new(move |w: f32| {
            st.set_content(w, st.c_h.get());
        })
    };
    let set_ch = {
        let st = state.clone();
        Rc::new(move |h: f32| {
            st.set_content(st.c_w.get(), h);
        })
    };
    let get_xy = {
        let st = state.clone();
        Rc::new(move || {
            st.tick();
            st.get()
        })
    };
    let set_xy = {
        let st = state.clone();
        Rc::new(move |x: f32, y: f32| st.set_offset_xy(x, y))
    };

    View::new(
        0,
        ViewKind::ScrollXY {
            on_scroll: Some(on_scroll),
            set_viewport_width: Some(set_vw),
            set_viewport_height: Some(set_vh),
            set_content_width: Some(set_cw),
            set_content_height: Some(set_ch),
            get_scroll_offset_xy: Some(get_xy),
            set_scroll_offset_xy: Some(set_xy),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}
