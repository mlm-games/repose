use std::cell::{Cell, RefCell};
use std::rc::Rc;

use web_time::Instant;

use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource};
use crate::{Signal, Vec2, request_frame, signal};

/// Axis (or axes) a scroll modifier operates on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxis {
    Vertical,
    Horizontal,
    Both,
}

// Holds all callbacks that the layout engine needs from a scroll state.
// Stored in `Modifier.scroll`.

/// Callbacks for a single-axis (vertical or horizontal) scroll container.
#[derive(Clone, Default)]
pub struct ScrollAxisBinding {
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    /// Set viewport size along the scroll axis (height for V, width for H).
    pub set_viewport_main: Option<Rc<dyn Fn(f32)>>,
    /// Set content size along the scroll axis.
    pub set_content_main: Option<Rc<dyn Fn(f32)>>,
    pub get_offset_main: Option<Rc<dyn Fn() -> f32>>,
    pub set_offset_main: Option<Rc<dyn Fn(f32)>>,
    pub show_scrollbar: bool,
    pub tick: Option<Rc<dyn Fn()>>,
    pub set_nested_scroll_parent: Option<Rc<dyn Fn(NestedScrollConnection)>>,
}

impl std::fmt::Debug for ScrollAxisBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollAxisBinding")
            .field("has_on_scroll", &self.on_scroll.is_some())
            .field("set_viewport_main", &self.set_viewport_main.is_some())
            .field("set_content_main", &self.set_content_main.is_some())
            .field("get_offset_main", &self.get_offset_main.is_some())
            .field("set_offset_main", &self.set_offset_main.is_some())
            .field("show_scrollbar", &self.show_scrollbar)
            .field("tick", &self.tick.is_some())
            .field(
                "set_nested_scroll_parent",
                &self.set_nested_scroll_parent.is_some(),
            )
            .finish()
    }
}

/// Callbacks for a 2D (both-axis) scroll container.
#[derive(Clone, Default)]
pub struct ScrollBothBinding {
    pub on_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    pub set_viewport_width: Option<Rc<dyn Fn(f32)>>,
    pub set_viewport_height: Option<Rc<dyn Fn(f32)>>,
    pub set_content_width: Option<Rc<dyn Fn(f32)>>,
    pub set_content_height: Option<Rc<dyn Fn(f32)>>,
    pub get_offset_xy: Option<Rc<dyn Fn() -> (f32, f32)>>,
    pub set_offset_xy: Option<Rc<dyn Fn(f32, f32)>>,
    pub show_scrollbar: bool,
    pub tick: Option<Rc<dyn Fn()>>,
    pub set_nested_scroll_parent: Option<Rc<dyn Fn(NestedScrollConnection)>>,
}

impl std::fmt::Debug for ScrollBothBinding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScrollBothBinding")
            .field("has_on_scroll", &self.on_scroll.is_some())
            .field("set_viewport_width", &self.set_viewport_width.is_some())
            .field("set_viewport_height", &self.set_viewport_height.is_some())
            .field("set_content_width", &self.set_content_width.is_some())
            .field("set_content_height", &self.set_content_height.is_some())
            .field("get_offset_xy", &self.get_offset_xy.is_some())
            .field("set_offset_xy", &self.set_offset_xy.is_some())
            .field("show_scrollbar", &self.show_scrollbar)
            .field("tick", &self.tick.is_some())
            .field(
                "set_nested_scroll_parent",
                &self.set_nested_scroll_parent.is_some(),
            )
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum ScrollBinding {
    Vertical(ScrollAxisBinding),
    Horizontal(ScrollAxisBinding),
    Both(ScrollBothBinding),
}

impl ScrollBinding {
    pub fn axis(&self) -> ScrollAxis {
        match self {
            ScrollBinding::Vertical(_) => ScrollAxis::Vertical,
            ScrollBinding::Horizontal(_) => ScrollAxis::Horizontal,
            ScrollBinding::Both(_) => ScrollAxis::Both,
        }
    }
}

const OVERSHOOT_DECAY_PER_60HZ: f32 = 0.78;

/// Handles velocity estimation from input deltas, frame-rate-independent
/// exponential decay, edge snapping, and animation state tracking.
#[derive(Clone)]
pub struct ScrollPhysics {
    pub(crate) vel: f32,
    last_t: Instant,
    last_input_t: Instant,
    pub(crate) animating: bool,
    stop_velocity: f32,
    input_activate_velocity: f32,
}

impl ScrollPhysics {
    pub fn new(_decay_per_60hz: f32, stop_velocity: f32, input_activate_velocity: f32) -> Self {
        let now = Instant::now();
        Self {
            vel: 0.0,
            last_t: now,
            last_input_t: now,
            animating: false,
            stop_velocity,
            input_activate_velocity,
        }
    }

    pub fn record_input(&mut self, consumed: f32) {
        let now = Instant::now();
        let raw_dt = (now - self.last_input_t).as_secs_f32();
        let dt = raw_dt.clamp(1.0 / 240.0, 1.0 / 15.0);
        self.last_input_t = now;
        if raw_dt > 0.1 {
            self.vel = 0.0;
        }
        let instant_vel = consumed / dt;
        const SMOOTHING: f32 = 0.35;
        self.vel = self.vel * (1.0 - SMOOTHING) + instant_vel * SMOOTHING;
        const MAX_VEL: f32 = 8000.0;
        self.vel = self.vel.clamp(-MAX_VEL, MAX_VEL);
        self.animating = self.vel.abs() > self.input_activate_velocity;
    }

    fn dt(&mut self) -> f32 {
        let now = Instant::now();
        let dt = (now - self.last_t).as_secs_f32().min(0.1);
        self.last_t = now;
        dt
    }

    pub fn tick_integrate(&mut self, current: f32, min: f32, max: f32) -> Option<f32> {
        if !self.animating {
            return None;
        }
        let dt = self.dt();
        if dt <= 0.0 {
            return None;
        }
        let vel0 = self.vel;
        if vel0.abs() < self.stop_velocity {
            self.vel = 0.0;
            self.animating = false;
            return None;
        }
        let new = (current + vel0 * dt).clamp(min, max);
        if (new - current).abs() < 0.01 && (current <= min || current >= max) {
            self.vel = 0.0;
            self.animating = false;
            return None;
        }
        let speed = self.vel.abs();
        let t = (speed / 4000.0).min(1.0);
        let effective_decay = 0.85 + t * 0.10;
        let decay = effective_decay.powf(dt * 60.0);
        self.vel = vel0 * decay;
        if self.vel.abs() < self.stop_velocity {
            self.vel = 0.0;
            self.animating = false;
            return None;
        }
        Some(new)
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }
}

/// Inertial scroll state (single axis Y).
#[derive(Clone)]
pub struct ScrollState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    content_height: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
    overscroll: Signal<f32>,
    overscroll_enabled: Cell<bool>,
    pub(crate) parent_connection: Rc<RefCell<Option<NestedScrollConnection>>>,
    show_scrollbar: Cell<bool>,
    prev_tick: Cell<Instant>,
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
            physics: RefCell::new(ScrollPhysics::new(0.90, 15.0, 50.0)),
            overscroll: signal(0.0),
            overscroll_enabled: Cell::new(true),
            parent_connection: Rc::new(RefCell::new(None)),
            show_scrollbar: Cell::new(true),
            prev_tick: Cell::new(Instant::now()),
        }
    }

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.overscroll_enabled.set(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.overscroll.get()
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// Use it to wire a child scrollable to a parent scroll container
    /// explicitly: `child_state.set_nested_scroll_parent(parent_state.connection())`.
    pub fn connection(&self) -> NestedScrollConnection {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        NestedScrollConnection::new().on_post_scroll(
            move |_consumed: Vec2, available: Vec2, _source: NestedScrollSource| -> Vec2 {
                if available.x.abs() < 0.001 && available.y.abs() < 0.001 {
                    return Vec2::ZERO;
                }
                let leftover_y = this.scroll_immediate(available.y);
                let after = run_post_scroll(
                    &pc,
                    Vec2 {
                        x: available.x,
                        y: leftover_y,
                    },
                );
                Vec2 {
                    x: available.x - after.x,
                    y: available.y - after.y,
                }
            },
        )
    }

    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    pub fn set_viewport_height(&self, h: f32) {
        let h = h.max(0.0);
        if (self.viewport_height.get() - h).abs() > 0.5 {
            self.viewport_height.set(h);
            self.clamp_offset();
        }
    }

    pub fn set_content_height(&self, h: f32) {
        let h = h.max(0.0);
        if (self.content_height.get() - h).abs() > 0.5 {
            self.content_height.set(h);
            self.clamp_offset();
        }
    }

    pub fn set_overscroll(&self, val: f32) {
        self.overscroll.set(val);
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

    pub fn scroll_immediate(&self, dy: f32) -> f32 {
        let before = self.scroll_offset.get();
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);

        let new_off = (before + dy).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);
        let consumed = new_off - before;
        self.physics.borrow_mut().record_input(consumed);
        dy - consumed
    }

    /// Feed leftover (after the nested parent chain has had its chance via
    /// post-scroll) into the rubber-band overscroll. Returns the amount still
    /// unconsumed. Overscroll deliberately runs LAST so parents get first dibs.
    pub fn apply_overscroll(&self, leftover: f32) -> f32 {
        if !self.overscroll_enabled.get() || leftover.abs() < 0.001 {
            return leftover;
        }
        let os = self.overscroll.get();
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);
        let before = self.scroll_offset.get();

        // Recovering from an active rubber band: absorb the reverse delta first.
        if os.abs() > 0.5 && os.signum() * leftover < 0.0 {
            let reduction = leftover.abs().min(os.abs());
            self.overscroll.set(os - os.signum() * reduction);
            let remainder = leftover - leftover.signum() * reduction;
            if remainder.abs() > 0.5 {
                let new_off = (before + remainder).clamp(0.0, max_off);
                self.scroll_offset.set(new_off);
                let consumed = new_off - before;
                self.physics.borrow_mut().record_input(consumed);
                return remainder - consumed;
            }
            return 0.0;
        }

        // At the edge, pushing further: stretch the rubber band.
        let at_edge = (before <= 0.0 && leftover < 0.0) || (before >= max_off && leftover > 0.0);
        if max_off > 5.0 && at_edge && leftover.abs() > 0.5 {
            let bandied = Self::rubber_band(leftover, 150.0);
            self.overscroll.set(os + bandied);
            self.physics.borrow_mut().record_input(0.0);
            return 0.0;
        }
        leftover
    }

    fn rubber_band(amount: f32, max: f32) -> f32 {
        let sign = amount.signum();
        let abs_val = amount.abs();
        if abs_val <= 0.0 {
            return 0.0;
        }
        (1.0 - 1.0 / (1.0 + abs_val / max)) * max * sign
    }

    pub fn tick(&self) -> bool {
        let now = Instant::now();
        let dt = (now - self.prev_tick.get()).as_secs_f32().min(0.1);
        self.prev_tick.set(now);

        if self.overscroll_enabled.get() {
            let os = self.overscroll.get();
            if os.abs() > 0.5 {
                let decayed = os * OVERSHOOT_DECAY_PER_60HZ.powf(dt * 60.0);
                if decayed.abs() < 0.5 {
                    self.overscroll.set(0.0);
                } else {
                    self.overscroll.set(decayed);
                }
                request_frame();
                return true;
            }
        }

        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);

        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_off) {
            drop(p);
            self.scroll_offset.set(new_off);
            request_frame();
            true
        } else {
            false
        }
    }

    pub fn show_scrollbar(&self) -> bool {
        self.show_scrollbar.get()
    }

    /// Build a `ScrollBinding::Vertical` from this state for use with `Modifier::vertical_scroll`.
    pub fn to_binding(&self) -> ScrollBinding {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        let c_on_scroll = {
            let state = Rc::clone(&pc);
            let this = Rc::clone(&this);
            Rc::new(move |d: Vec2| -> Vec2 {
                let d = run_pre_scroll(&*state, d);
                // Clamp this scroller first; leftover goes to the nested parent
                // chain (post-scroll) BEFORE rubber-band overscroll.
                let leftover_y = this.scroll_immediate(d.y);
                let result = Vec2 {
                    x: d.x,
                    y: leftover_y,
                };
                let after_parent = run_post_scroll(&*state, result);
                let final_y = this.apply_overscroll(after_parent.y);
                Vec2 {
                    x: after_parent.x,
                    y: final_y,
                }
            })
        } as Rc<dyn Fn(Vec2) -> Vec2>;
        let c_set_viewport = {
            let this = Rc::clone(&this);
            Rc::new(move |h: f32| this.set_viewport_height(h))
        };
        let c_set_content = {
            let this = Rc::clone(&this);
            Rc::new(move |h: f32| this.set_content_height(h))
        };
        let c_get = {
            let this = Rc::clone(&this);
            Rc::new(move || this.get() + this.overscroll_offset())
        };
        let c_set = {
            let this = Rc::clone(&this);
            Rc::new(move |off: f32| this.set_offset(off))
        };
        let c_tick = {
            let this = Rc::clone(&this);
            Rc::new(move || {
                this.tick();
            })
        };
        let c_set_nested = {
            let state = Rc::clone(&pc);
            Rc::new(move |conn| {
                *state.borrow_mut() = Some(conn);
            })
        };
        ScrollBinding::Vertical(ScrollAxisBinding {
            on_scroll: Some(c_on_scroll),
            set_viewport_main: Some(c_set_viewport),
            set_content_main: Some(c_set_content),
            get_offset_main: Some(c_get),
            set_offset_main: Some(c_set),
            show_scrollbar: self.show_scrollbar(),
            tick: Some(c_tick),
            set_nested_scroll_parent: Some(c_set_nested),
        })
    }
}

/// X-only state
#[derive(Clone)]
pub struct HorizontalScrollState {
    scroll_offset: Signal<f32>,
    viewport_width: Signal<f32>,
    content_width: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
    overscroll: Signal<f32>,
    overscroll_enabled: Cell<bool>,
    pub(crate) parent_connection: Rc<RefCell<Option<NestedScrollConnection>>>,
    show_scrollbar: Cell<bool>,
    prev_tick: Cell<Instant>,
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
            physics: RefCell::new(ScrollPhysics::new(0.90, 15.0, 50.0)),
            overscroll: signal(0.0),
            overscroll_enabled: Cell::new(true),
            parent_connection: Rc::new(RefCell::new(None)),
            show_scrollbar: Cell::new(true),
            prev_tick: Cell::new(Instant::now()),
        }
    }

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.overscroll_enabled.set(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.overscroll.get()
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// A `NestedScrollConnection` that consumes leftover scroll by scrolling
    /// THIS container, then chains anything still unabsorbed up to this
    /// container's own parent connection.
    pub fn connection(&self) -> NestedScrollConnection {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        NestedScrollConnection::new().on_post_scroll(
            move |_consumed: Vec2, available: Vec2, _source: NestedScrollSource| -> Vec2 {
                if available.x.abs() < 0.001 && available.y.abs() < 0.001 {
                    return Vec2::ZERO;
                }
                let leftover_x = this.scroll_immediate(available.x);
                let after = run_post_scroll(
                    &pc,
                    Vec2 {
                        x: leftover_x,
                        y: available.y,
                    },
                );
                Vec2 {
                    x: available.x - after.x,
                    y: available.y - after.y,
                }
            },
        )
    }

    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    pub fn set_viewport_width(&self, w: f32) {
        let w = w.max(0.0);
        if (self.viewport_width.get() - w).abs() > 0.5 {
            self.viewport_width.set(w);
            self.clamp();
        }
    }

    pub fn set_content_width(&self, w: f32) {
        let w = w.max(0.0);
        if (self.content_width.get() - w).abs() > 0.5 {
            self.content_width.set(w);
            self.clamp();
        }
    }

    pub fn set_overscroll(&self, val: f32) {
        self.overscroll.set(val);
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
        self.physics.borrow_mut().record_input(consumed);
        dx - consumed
    }

    /// Feed leftover (after the nested parent chain) into rubber-band overscroll.
    pub fn apply_overscroll(&self, leftover: f32) -> f32 {
        if !self.overscroll_enabled.get() || leftover.abs() < 0.001 {
            return leftover;
        }
        let os = self.overscroll.get();
        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        let before = self.scroll_offset.get();

        if os.abs() > 0.5 && os.signum() * leftover < 0.0 {
            let reduction = leftover.abs().min(os.abs());
            self.overscroll.set(os - os.signum() * reduction);
            let remainder = leftover - leftover.signum() * reduction;
            if remainder.abs() > 0.5 {
                let new_off = (before + remainder).clamp(0.0, max_off);
                self.scroll_offset.set(new_off);
                let consumed = new_off - before;
                self.physics.borrow_mut().record_input(consumed);
                return remainder - consumed;
            }
            return 0.0;
        }

        let at_edge = (before <= 0.0 && leftover < 0.0) || (before >= max_off && leftover > 0.0);
        if max_off > 5.0 && at_edge && leftover.abs() > 0.5 {
            let bandied = ScrollState::rubber_band(leftover, 150.0);
            self.overscroll.set(os + bandied);
            self.physics.borrow_mut().record_input(0.0);
            return 0.0;
        }
        leftover
    }

    pub fn tick(&self) -> bool {
        let now = Instant::now();
        let dt = (now - self.prev_tick.get()).as_secs_f32().min(0.1);
        self.prev_tick.set(now);

        if self.overscroll_enabled.get() {
            let os = self.overscroll.get();
            if os.abs() > 0.5 {
                let decayed = os * OVERSHOOT_DECAY_PER_60HZ.powf(dt * 60.0);
                if decayed.abs() < 0.5 {
                    self.overscroll.set(0.0);
                } else {
                    self.overscroll.set(decayed);
                }
                request_frame();
                return true;
            }
        }

        let max_off = (self.content_width.get() - self.viewport_width.get()).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_off) {
            drop(p);
            self.scroll_offset.set(new_off);
            request_frame();
            true
        } else {
            false
        }
    }

    pub fn show_scrollbar(&self) -> bool {
        self.show_scrollbar.get()
    }

    pub fn to_binding(&self) -> ScrollBinding {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        let c_on_scroll = {
            let state = Rc::clone(&pc);
            let this = Rc::clone(&this);
            Rc::new(move |d: Vec2| -> Vec2 {
                let d = run_pre_scroll(&*state, d);
                let leftover_x = this.scroll_immediate(d.x);
                let result = Vec2 {
                    x: leftover_x,
                    y: d.y,
                };
                let after_parent = run_post_scroll(&*state, result);
                let final_x = this.apply_overscroll(after_parent.x);
                Vec2 {
                    x: final_x,
                    y: after_parent.y,
                }
            })
        } as Rc<dyn Fn(Vec2) -> Vec2>;
        let c_set_vp = {
            let this = Rc::clone(&this);
            Rc::new(move |w: f32| this.set_viewport_width(w))
        };
        let c_set_ct = {
            let this = Rc::clone(&this);
            Rc::new(move |w: f32| this.set_content_width(w))
        };
        let c_get = {
            let this = Rc::clone(&this);
            Rc::new(move || this.get() + this.overscroll_offset())
        };
        let c_set = {
            let this = Rc::clone(&this);
            Rc::new(move |off: f32| this.set_offset(off))
        };
        let c_tick = {
            let this = Rc::clone(&this);
            Rc::new(move || {
                this.tick();
            })
        };
        let c_set_nested = {
            let state = Rc::clone(&pc);
            Rc::new(move |conn| {
                *state.borrow_mut() = Some(conn);
            })
        };
        ScrollBinding::Horizontal(ScrollAxisBinding {
            on_scroll: Some(c_on_scroll),
            set_viewport_main: Some(c_set_vp),
            set_content_main: Some(c_set_ct),
            get_offset_main: Some(c_get),
            set_offset_main: Some(c_set),
            show_scrollbar: this.show_scrollbar(),
            tick: Some(c_tick),
            set_nested_scroll_parent: Some(c_set_nested),
        })
    }
}

/// 2D state
#[derive(Clone)]
pub struct ScrollStateXY {
    off_x: Signal<f32>,
    off_y: Signal<f32>,
    vp_w: Signal<f32>,
    vp_h: Signal<f32>,
    c_w: Signal<f32>,
    c_h: Signal<f32>,
    physics_x: RefCell<ScrollPhysics>,
    physics_y: RefCell<ScrollPhysics>,
    os_x: Signal<f32>,
    os_y: Signal<f32>,
    overscroll_enabled: Cell<bool>,
    pub(crate) parent_connection: Rc<RefCell<Option<NestedScrollConnection>>>,
    show_scrollbar: Cell<bool>,
    prev_tick: Cell<Instant>,
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
            physics_x: RefCell::new(ScrollPhysics::new(0.90, 15.0, 50.0)),
            physics_y: RefCell::new(ScrollPhysics::new(0.90, 15.0, 50.0)),
            os_x: signal(0.0),
            os_y: signal(0.0),
            overscroll_enabled: Cell::new(true),
            parent_connection: Rc::new(RefCell::new(None)),
            show_scrollbar: Cell::new(true),
            prev_tick: Cell::new(Instant::now()),
        }
    }

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.overscroll_enabled.set(enabled);
    }

    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    pub fn overscroll_offset(&self) -> (f32, f32) {
        (self.os_x.get(), self.os_y.get())
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// A `NestedScrollConnection` that consumes leftover scroll by scrolling
    /// THIS container, then chains anything still unabsorbed up to this
    /// container's own parent connection.
    pub fn connection(&self) -> NestedScrollConnection {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        NestedScrollConnection::new().on_post_scroll(
            move |_consumed: Vec2, available: Vec2, _source: NestedScrollSource| -> Vec2 {
                if available.x.abs() < 0.001 && available.y.abs() < 0.001 {
                    return Vec2::ZERO;
                }
                let after = this.scroll_immediate(available);
                let after2 = run_post_scroll(&pc, after);
                Vec2 {
                    x: available.x - after2.x,
                    y: available.y - after2.y,
                }
            },
        )
    }

    pub fn set_viewport(&self, w: f32, h: f32) {
        let w = w.max(0.0);
        let h = h.max(0.0);
        let changed = (self.vp_w.get() - w).abs() > 0.5 || (self.vp_h.get() - h).abs() > 0.5;
        if changed {
            self.vp_w.set(w);
            self.vp_h.set(h);
            self.clamp();
        }
    }
    pub fn set_content(&self, w: f32, h: f32) {
        let w = w.max(0.0);
        let h = h.max(0.0);
        let changed = (self.c_w.get() - w).abs() > 0.5 || (self.c_h.get() - h).abs() > 0.5;
        if changed {
            self.c_w.set(w);
            self.c_h.set(h);
            self.clamp();
        }
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
        self.off_x.update(|x| *x = x.clamp(0.0, max_x));
        self.off_y.update(|y| *y = y.clamp(0.0, max_y));
    }
    pub fn get(&self) -> (f32, f32) {
        (self.off_x.get(), self.off_y.get())
    }
    fn rubber_band(amount: f32, max: f32) -> f32 {
        let sign = amount.signum();
        let abs_val = amount.abs();
        let result = if abs_val <= 0.0 {
            0.0
        } else {
            (1.0 - 1.0 / (1.0 + abs_val / max)) * max
        };
        result * sign
    }
    fn os_scroll_axis(
        off: &Signal<f32>,
        os: &Signal<f32>,
        overscroll_enabled: bool,
        max_off: f32,
        before: f32,
        leftover: f32,
        physics: &RefCell<ScrollPhysics>,
    ) -> f32 {
        if !overscroll_enabled || leftover.abs() < 0.001 {
            return leftover;
        }
        let os_val = os.get();
        // Recovering from an active rubber band: absorb the reverse delta first.
        if os_val.abs() > 0.5 && os_val.signum() * leftover < 0.0 {
            let reduction = leftover.abs().min(os_val.abs());
            os.set(os_val - os_val.signum() * reduction);
            let remainder = leftover - leftover.signum() * reduction;
            if remainder.abs() > 0.5 {
                let new_off = (before + remainder).clamp(0.0, max_off);
                off.set(new_off);
                let consumed = new_off - before;
                physics.borrow_mut().record_input(consumed);
                return remainder - consumed;
            }
            return 0.0;
        }

        // At the edge, pushing further: stretch the rubber band.
        let at_edge = (before <= 0.0 && leftover < 0.0) || (before >= max_off && leftover > 0.0);
        if max_off > 5.0 && at_edge && leftover.abs() > 0.5 {
            let bandied = Self::rubber_band(leftover, 150.0);
            os.set(os_val + bandied);
            physics.borrow_mut().record_input(0.0);
            return 0.0;
        }
        leftover
    }
    pub fn scroll_immediate(&self, d: Vec2) -> Vec2 {
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        let (bx, by) = (self.off_x.get(), self.off_y.get());

        let nx = (bx + d.x).clamp(0.0, max_x);
        let ny = (by + d.y).clamp(0.0, max_y);
        self.off_x.set(nx);
        self.off_y.set(ny);
        let (cx, cy) = (nx - bx, ny - by);

        let mut px = self.physics_x.borrow_mut();
        let mut py = self.physics_y.borrow_mut();
        px.record_input(cx);
        py.record_input(cy);
        drop((px, py));

        Vec2 { x: d.x - cx, y: d.y - cy }
    }

    /// Feed leftover (after the nested parent chain) into rubber-band overscroll.
    pub fn apply_overscroll(&self, d: Vec2) -> Vec2 {
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        let (bx, by) = (self.off_x.get(), self.off_y.get());

        let lx = Self::os_scroll_axis(
            &self.off_x,
            &self.os_x,
            self.overscroll_enabled.get(),
            max_x,
            bx,
            d.x,
            &self.physics_x,
        );
        let ly = Self::os_scroll_axis(
            &self.off_y,
            &self.os_y,
            self.overscroll_enabled.get(),
            max_y,
            by,
            d.y,
            &self.physics_y,
        );
        Vec2 { x: lx, y: ly }
    }
    fn tick_os_axis(os: &Signal<f32>, enabled: bool, dt: f32) -> bool {
        if !enabled {
            return false;
        }
        let v = os.get();
        if v.abs() > 0.5 {
            let decayed = v * OVERSHOOT_DECAY_PER_60HZ.powf(dt * 60.0);
            if decayed.abs() < 0.5 {
                os.set(0.0);
            } else {
                os.set(decayed);
            }
            request_frame();
            true
        } else {
            false
        }
    }
    pub fn tick(&self) -> bool {
        let now = Instant::now();
        let dt = (now - self.prev_tick.get()).as_secs_f32().min(0.1);
        self.prev_tick.set(now);

        if self.overscroll_enabled.get() {
            if Self::tick_os_axis(&self.os_x, true, dt) || Self::tick_os_axis(&self.os_y, true, dt)
            {
                return true;
            }
        }

        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);
        let (bx, by) = (self.off_x.get(), self.off_y.get());

        let mut px = self.physics_x.borrow_mut();
        let mut py = self.physics_y.borrow_mut();
        if !px.animating && !py.animating {
            return false;
        }
        if px.animating {
            if px.vel.abs() < px.stop_velocity {
                px.vel = 0.0;
                px.animating = false;
            } else {
                let nx = (bx + px.vel * dt).clamp(0.0, max_x);
                if (nx - bx).abs() < 0.01 && (bx <= 0.0 || bx >= max_x) {
                    px.vel = 0.0;
                    px.animating = false;
                } else {
                    let speed = px.vel.abs();
                    let t = (speed / 4000.0).min(1.0);
                    let effective_decay = 0.85 + t * 0.10;
                    px.vel *= effective_decay.powf(dt * 60.0);
                    if px.vel.abs() < px.stop_velocity {
                        px.vel = 0.0;
                        px.animating = false;
                    } else {
                        self.off_x.set(nx);
                    }
                }
            }
        }
        if py.animating {
            if py.vel.abs() < py.stop_velocity {
                py.vel = 0.0;
                py.animating = false;
            } else {
                let ny = (by + py.vel * dt).clamp(0.0, max_y);
                if (ny - by).abs() < 0.01 && (by <= 0.0 || by >= max_y) {
                    py.vel = 0.0;
                    py.animating = false;
                } else {
                    let speed = py.vel.abs();
                    let t = (speed / 4000.0).min(1.0);
                    let effective_decay = 0.85 + t * 0.10;
                    py.vel *= effective_decay.powf(dt * 60.0);
                    if py.vel.abs() < py.stop_velocity {
                        py.vel = 0.0;
                        py.animating = false;
                    } else {
                        self.off_y.set(ny);
                    }
                }
            }
        }
        let running = px.animating || py.animating;
        if running {
            request_frame();
        }
        running
    }

    pub fn show_scrollbar(&self) -> bool {
        self.show_scrollbar.get()
    }

    pub fn to_binding(&self) -> ScrollBinding {
        let this = Rc::new(self.clone());
        let pc = Rc::clone(&this.parent_connection);
        let c_on_scroll = {
            let state = Rc::clone(&pc);
            let this = Rc::clone(&this);
            Rc::new(move |d: Vec2| -> Vec2 {
                let d = run_pre_scroll(&*state, d);
                let result = this.scroll_immediate(d);
                let after_parent = run_post_scroll(&*state, result);
                this.apply_overscroll(after_parent)
            })
        } as Rc<dyn Fn(Vec2) -> Vec2>;
        let c_set_vw = {
            let this = Rc::clone(&this);
            Rc::new(move |w: f32| this.set_viewport(w, this.vp_h.get()))
        };
        let c_set_vh = {
            let this = Rc::clone(&this);
            Rc::new(move |h: f32| this.set_viewport(this.vp_w.get(), h))
        };
        let c_set_cw = {
            let this = Rc::clone(&this);
            Rc::new(move |w: f32| this.set_content(w, this.c_h.get()))
        };
        let c_set_ch = {
            let this = Rc::clone(&this);
            Rc::new(move |h: f32| this.set_content(this.c_w.get(), h))
        };
        let c_get_xy = {
            let this = Rc::clone(&this);
            Rc::new(move || {
                let (ox, oy) = this.get();
                let (osx, osy) = this.overscroll_offset();
                (ox + osx, oy + osy)
            })
        };
        let c_set_xy = {
            let this = Rc::clone(&this);
            Rc::new(move |x: f32, y: f32| this.set_offset_xy(x, y))
        };
        let c_tick = {
            let this = Rc::clone(&this);
            Rc::new(move || {
                this.tick();
            })
        };
        let c_set_nested = {
            let state = Rc::clone(&pc);
            Rc::new(move |conn| {
                *state.borrow_mut() = Some(conn);
            })
        };
        ScrollBinding::Both(ScrollBothBinding {
            on_scroll: Some(c_on_scroll),
            set_viewport_width: Some(c_set_vw),
            set_viewport_height: Some(c_set_vh),
            set_content_width: Some(c_set_cw),
            set_content_height: Some(c_set_ch),
            get_offset_xy: Some(c_get_xy),
            set_offset_xy: Some(c_set_xy),
            show_scrollbar: this.show_scrollbar(),
            tick: Some(c_tick),
            set_nested_scroll_parent: Some(c_set_nested),
        })
    }
}

pub fn run_pre_scroll(conn: &RefCell<Option<NestedScrollConnection>>, d: Vec2) -> Vec2 {
    if let Some(ref parent) = *conn.borrow() {
        let consumed = parent.dispatch_pre_scroll(d, NestedScrollSource::UserInput);
        d - consumed
    } else {
        d
    }
}

pub fn run_post_scroll(
    conn: &RefCell<Option<NestedScrollConnection>>,
    leftover: Vec2,
) -> Vec2 {
    if let Some(ref parent) = *conn.borrow() {
        let added =
            parent.dispatch_post_scroll(Vec2::ZERO, leftover, NestedScrollSource::UserInput);
        leftover - added
    } else {
        leftover
    }
}
