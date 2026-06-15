//! # Scroll model
//!
//! Repose separates visual scroll containers from scroll state.
//!
//! This file implements inertial scroll states.
//!
//! Velocities are expressed in px/sec and integrated with dt,
//! so behavior is frame-rate independent.

use repose_core::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use web_time::Instant;

/// A connection between a nested scrollable and its nearest ancestor scrollable.
/// Allows coordinated scrolling: the parent can pre-consume deltas before the
/// child processes them, and post-consume leftovers after the child is done.
///
/// This enables patterns like collapsing toolbars (parent pre-consumes upward
/// scroll) and pull-to-refresh (parent post-consumes overscroll from child).
pub struct NestedScrollConnection {
    /// Called with the original delta before the child processes it.
    /// Return the delta remaining for the child after parent pre-consumption.
    pub on_pre_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
    /// Called with the leftover delta after the child has processed it.
    /// Return the final leftover (after parent post-consumption).
    pub on_post_scroll: Option<Rc<dyn Fn(Vec2) -> Vec2>>,
}

impl Default for NestedScrollConnection {
    fn default() -> Self {
        Self::new()
    }
}

impl NestedScrollConnection {
    pub fn new() -> Self {
        Self {
            on_pre_scroll: None,
            on_post_scroll: None,
        }
    }
    /// Pre-consume scroll before the child processes it.
    /// `f` receives the original delta, returns what remains for the child.
    pub fn pre_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_pre_scroll = Some(Rc::new(f));
        self
    }
    /// Post-consume leftover scroll after the child has processed it.
    /// `f` receives the leftover from the child, returns the final leftover.
    pub fn post_scroll(mut self, f: impl Fn(Vec2) -> Vec2 + 'static) -> Self {
        self.on_post_scroll = Some(Rc::new(f));
        self
    }
}

fn run_pre_scroll(conn: &RefCell<Option<NestedScrollConnection>>, d: Vec2) -> Vec2 {
    if let Some(ref parent) = *conn.borrow()
        && let Some(ref pre) = parent.on_pre_scroll {
            return pre(d);
        }
    d
}

fn run_post_scroll(conn: &RefCell<Option<NestedScrollConnection>>, leftover: Vec2) -> Vec2 {
    if let Some(ref parent) = *conn.borrow()
        && let Some(ref post) = parent.on_post_scroll {
            return post(leftover);
        }
    leftover
}

/// Handles velocity estimation from input deltas, frame-rate-independent
/// exponential decay, edge snapping, and animation state tracking.
pub struct ScrollPhysics {
    vel: f32,
    last_t: Instant,
    last_input_t: Instant,
    animating: bool,
    stop_velocity: f32,
    input_activate_velocity: f32,
}

impl ScrollPhysics {
    pub(crate) fn new(
        _decay_per_60hz: f32,
        stop_velocity: f32,
        input_activate_velocity: f32,
    ) -> Self {
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

    /// Record a scroll input of `consumed` px. Estimates velocity using
    /// EWMA smoothing over recent frames to avoid spikes from frame jitter.
    /// Capped to `max_velocity` px/s.
    pub(crate) fn record_input(&mut self, consumed: f32) {
        let now = Instant::now();
        let raw_dt = (now - self.last_input_t).as_secs_f32();
        let dt = raw_dt.clamp(1.0 / 240.0, 1.0 / 15.0);
        self.last_input_t = now;

        // Reset velocity if a significant gap occurred (finger was lifted/paused)
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

    /// Return frame dt, capped to avoid physics explosion on lag spikes.
    pub(crate) fn dt(&mut self) -> f32 {
        let now = Instant::now();
        let dt = (now - self.last_t).as_secs_f32().min(0.1);
        self.last_t = now;
        dt
    }

    /// Tick physics: integrate velocity over dt, apply decay, detect edges.
    /// Returns `Some(new_offset)` if still animating or `None` if stopped.
    pub(crate) fn tick_integrate(&mut self, current: f32, min: f32, max: f32) -> Option<f32> {
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

        // Hit an edge: stop immediately.
        if (new - current).abs() < 0.01 && (current <= min || current >= max) {
            self.vel = 0.0;
            self.animating = false;
            return None;
        }

        // Velocity-dependent decay: more friction at low speeds, less at high.
        // This matches Android's spline-based fling where effective friction
        // decreases as velocity increases (ln(0.78)/ln(0.9) spline scaling).
        let speed = self.vel.abs();
        let t = (speed / 4000.0).min(1.0);
        let effective_decay = 0.85 + t * 0.10; // ranges from 0.85 (slow) to 0.95 (fast)
        let decay = effective_decay.powf(dt * 60.0);
        self.vel = vel0 * decay;

        // Re-check stop threshold after decay (avoids sub-threshold oscillation).
        if self.vel.abs() < self.stop_velocity {
            self.vel = 0.0;
            self.animating = false;
            return None;
        }

        Some(new)
    }

    pub(crate) fn is_animating(&self) -> bool {
        self.animating
    }
}

/// Inertial scroll state (single axis Y).
pub struct ScrollState {
    scroll_offset: Signal<f32>,
    viewport_height: Signal<f32>,
    content_height: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
    overscroll: Signal<f32>,
    overscroll_enabled: bool,
    parent_connection: RefCell<Option<NestedScrollConnection>>,
    show_scrollbar: Cell<bool>,
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
            overscroll_enabled: true,
            parent_connection: RefCell::new(None),
            show_scrollbar: Cell::new(true),
        }
    }

    /// Enable or disable the overscroll rubber-band effect (enabled by default).
    pub fn set_overscroll_enabled(&mut self, enabled: bool) {
        self.overscroll_enabled = enabled;
    }

    /// Returns the current overscroll offset (negative = pulled down past top,
    /// positive = pulled up past bottom).
    pub fn overscroll_offset(&self) -> f32 {
        self.overscroll.get()
    }

    /// Set a parent nested scroll connection for coordinated scrolling.
    /// This enables parent-child scroll coordination (e.g., collapsing toolbars,
    /// pull-to-refresh). Call this before the scroll area is first composed.
    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// Enable or disable the scrollbar (enabled by default).
    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    pub fn set_viewport_height(&self, h: f32) {
        self.viewport_height.set(h.max(0.0));
        self.clamp_offset();
    }
    pub fn set_content_height(&self, h: f32) {
        self.content_height.set(h.max(0.0));
        self.clamp_offset();
    }
    /// Set the overscroll value directly. Used by pull-to-refresh to
    /// reset overscroll when the refresh completes.
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

    /// Consume dy (pixels), clamp to bounds, return leftover.
    /// When overscroll is enabled and the scroll boundary is reached,
    /// applies rubber-band resistance instead of returning leftover.
    pub fn scroll_immediate(&self, dy: f32) -> f32 {
        let before = self.scroll_offset.get();
        let vh = self.viewport_height.get();
        let ch = self.content_height.get();
        let max_off = (ch - vh).max(0.0);

        // Handle existing overscroll: if scrolling toward reducing it, ease off first
        let os = self.overscroll.get();
        if self.overscroll_enabled && os.abs() > 0.5 {
            // os.signum() * dy < 0  → scrolling toward reducing overscroll
            if os.signum() * dy < 0.0 {
                // Reduce overscroll, then process remainder as normal scroll
                let reduction = dy.abs().min(os.abs());
                self.overscroll.set(os - os.signum() * reduction);
                let remainder = dy - dy.signum() * reduction;
                if remainder.abs() > 0.5 {
                    let new_off = (before + remainder).clamp(0.0, max_off);
                    self.scroll_offset.set(new_off);
                    let consumed = new_off - before;
                    let leftover = remainder - consumed;
                    self.physics.borrow_mut().record_input(consumed);
                    return leftover;
                }
                return 0.0;
            } else {
                // Scrolling further into overscroll - apply rubber-band
                let total = os + dy;
                let bandied = Self::rubber_band(total, 150.0);
                self.overscroll.set(bandied);
                self.physics.borrow_mut().record_input(dy);
                return 0.0;
            }
        }

        let can_overscroll = self.overscroll_enabled && max_off > 5.0;
        let new_off = (before + dy).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);

        let consumed = new_off - before;
        let leftover = dy - consumed;

        // If at boundary with leftover, apply rubber-band overscroll
        if can_overscroll
            && leftover.abs() > 0.5
            && ((before <= 0.0 && dy < 0.0) || (before >= max_off && dy > 0.0))
        {
            let bandied = Self::rubber_band(leftover, 150.0);
            self.overscroll.set(os + bandied);
            self.physics.borrow_mut().record_input(consumed);
            return 0.0;
        }

        self.physics.borrow_mut().record_input(consumed);

        leftover
    }

    /// Rubber-band function: applies increasing resistance as the offset grows.
    /// `amount` is the raw delta past the boundary, `max` controls the stiffness.
    /// Lower `max` = stiffer (more resistance). Higher = looser (more travel).
    fn rubber_band(amount: f32, max: f32) -> f32 {
        let sign = amount.signum();
        let abs_val = amount.abs();
        if abs_val <= 0.0 {
            return 0.0;
        }
        (1.0 - 1.0 / (1.0 + abs_val / max)) * max * sign
    }

    /// Advance physics one tick; returns true if animating.
    pub fn tick(&self) -> bool {
        // Handle overscroll decay (frame-rate-independent, ~20% decay per 60Hz frame)
        if self.overscroll_enabled {
            let os = self.overscroll.get();
            if os.abs() > 0.5 {
                let decayed = os * 0.78;
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
}

/// X-only state
pub struct HorizontalScrollState {
    scroll_offset: Signal<f32>,
    viewport_width: Signal<f32>,
    content_width: Signal<f32>,
    physics: RefCell<ScrollPhysics>,
    overscroll: Signal<f32>,
    overscroll_enabled: bool,
    parent_connection: RefCell<Option<NestedScrollConnection>>,
    show_scrollbar: Cell<bool>,
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
            overscroll_enabled: true,
            parent_connection: RefCell::new(None),
            show_scrollbar: Cell::new(true),
        }
    }

    /// Enable or disable the overscroll rubber-band effect (enabled by default).
    pub fn set_overscroll_enabled(&mut self, enabled: bool) {
        self.overscroll_enabled = enabled;
    }

    /// Returns the current overscroll offset.
    pub fn overscroll_offset(&self) -> f32 {
        self.overscroll.get()
    }

    /// Set a parent nested scroll connection for coordinated scrolling.
    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// Enable or disable the scrollbar (enabled by default).
    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    pub fn set_viewport_width(&self, w: f32) {
        self.viewport_width.set(w.max(0.0));
        self.clamp();
    }
    pub fn set_content_width(&self, w: f32) {
        self.content_width.set(w.max(0.0));
        self.clamp();
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

        let os = self.overscroll.get();
        if self.overscroll_enabled && os.abs() > 0.5 {
            if os.signum() * dx < 0.0 {
                let reduction = dx.abs().min(os.abs());
                self.overscroll.set(os - os.signum() * reduction);
                let remainder = dx - dx.signum() * reduction;
                if remainder.abs() > 0.5 {
                    let new_off = (before + remainder).clamp(0.0, max_off);
                    self.scroll_offset.set(new_off);
                    let consumed = new_off - before;
                    let leftover = remainder - consumed;
                    self.physics.borrow_mut().record_input(consumed);
                    return leftover;
                }
                return 0.0;
            } else {
                let total = os + dx;
                let bandied = ScrollState::rubber_band(total, 150.0);
                self.overscroll.set(bandied);
                self.physics.borrow_mut().record_input(dx);
                return 0.0;
            }
        }

        let can_overscroll = self.overscroll_enabled && max_off > 5.0;
        let new_off = (before + dx).clamp(0.0, max_off);
        self.scroll_offset.set(new_off);

        let consumed = new_off - before;
        let leftover = dx - consumed;

        if can_overscroll
            && leftover.abs() > 0.5
            && ((before <= 0.0 && dx < 0.0) || (before >= max_off && dx > 0.0))
        {
            let bandied = ScrollState::rubber_band(leftover, 150.0);
            self.overscroll.set(os + bandied);
            self.physics.borrow_mut().record_input(consumed);
            return 0.0;
        }

        self.physics.borrow_mut().record_input(consumed);
        leftover
    }
    pub fn tick(&self) -> bool {
        if self.overscroll_enabled {
            let os = self.overscroll.get();
            if os.abs() > 0.5 {
                let decayed = os * 0.78;
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
}

/// 2D state
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
    overscroll_enabled: bool,
    parent_connection: RefCell<Option<NestedScrollConnection>>,
    show_scrollbar: Cell<bool>,
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
            overscroll_enabled: true,
            parent_connection: RefCell::new(None),
            show_scrollbar: Cell::new(true),
        }
    }

    /// Enable or disable the overscroll rubber-band effect (enabled by default).
    pub fn set_overscroll_enabled(&mut self, enabled: bool) {
        self.overscroll_enabled = enabled;
    }

    /// Enable or disable the scrollbar (enabled by default).
    pub fn set_show_scrollbar(&self, show: bool) {
        self.show_scrollbar.set(show);
    }

    /// Returns the current overscroll offsets as (x, y).
    pub fn overscroll_offset(&self) -> (f32, f32) {
        (self.os_x.get(), self.os_y.get())
    }

    /// Set a parent nested scroll connection for coordinated scrolling.
    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
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
        os: &Signal<f32>,
        overscroll_enabled: bool,
        before: f32,
        max_off: f32,
        dx: f32,
        physics: &mut ScrollPhysics,
    ) -> f32 {
        let os_val = os.get();
        if overscroll_enabled && os_val.abs() > 0.5 {
            if os_val.signum() * dx < 0.0 {
                let reduction = dx.abs().min(os_val.abs());
                os.set(os_val - os_val.signum() * reduction);
                let remainder = dx - dx.signum() * reduction;
                if remainder.abs() > 0.5 {
                    let new_off = (before + remainder).clamp(0.0, max_off);
                    let consumed = new_off - before;
                    let leftover = remainder - consumed;
                    physics.record_input(consumed);
                    return leftover;
                }
                return 0.0;
            } else {
                let total = os_val + dx;
                let bandied = Self::rubber_band(total, 150.0);
                os.set(bandied);
                physics.record_input(dx);
                return 0.0;
            }
        }

        let can_os = overscroll_enabled && max_off > 5.0;
        let new_off = (before + dx).clamp(0.0, max_off);
        let consumed = new_off - before;
        let leftover = dx - consumed;

        if can_os
            && leftover.abs() > 0.5
            && ((before <= 0.0 && dx < 0.0) || (before >= max_off && dx > 0.0))
        {
            let bandied = Self::rubber_band(leftover, 150.0);
            os.set(os_val + bandied);
            physics.record_input(consumed);
            return 0.0;
        }

        physics.record_input(consumed);
        leftover
    }
    pub fn scroll_immediate(&self, d: Vec2) -> Vec2 {
        let bx = self.off_x.get();
        let by = self.off_y.get();
        let max_x = (self.c_w.get() - self.vp_w.get()).max(0.0);
        let max_y = (self.c_h.get() - self.vp_h.get()).max(0.0);

        let mut px = self.physics_x.borrow_mut();
        let mut py = self.physics_y.borrow_mut();
        let lx = Self::os_scroll_axis(&self.os_x, self.overscroll_enabled, bx, max_x, d.x, &mut px);
        let ly = Self::os_scroll_axis(&self.os_y, self.overscroll_enabled, by, max_y, d.y, &mut py);
        drop((px, py));

        Vec2 { x: lx, y: ly }
    }
    fn tick_os_axis(os: &Signal<f32>, enabled: bool) -> bool {
        if !enabled {
            return false;
        }
        let v = os.get();
        if v.abs() > 0.5 {
            let decayed = v * 0.78;
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
    /// Advance physics for both axes using a shared dt.
    /// Returns true if either axis is still animating.
    pub fn tick(&self) -> bool {
        if self.overscroll_enabled {
            let os_active =
                Self::tick_os_axis(&self.os_x, true) || Self::tick_os_axis(&self.os_y, true);
            if os_active {
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

        let dt = px.dt().max(py.dt());

        // Integrate X
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

        // Integrate Y
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
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            // Pre-scroll: let parent consume first
            let d = run_pre_scroll(&st.parent_connection, d);
            // My scroll
            let leftover_y = st.scroll_immediate(d.y);
            let result = Vec2 {
                x: d.x,
                y: leftover_y,
            };
            // Post-scroll: parent gets the leftover
            run_post_scroll(&st.parent_connection, result)
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
            st.get() + st.overscroll_offset()
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
            show_scrollbar: state.show_scrollbar.get(),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

pub fn HorizontalScrollArea(
    modifier: Modifier,
    state: Rc<HorizontalScrollState>,
    mut content: View,
) -> View {
    // Prevent content from shrinking below its natural width in the Row layout.
    content.modifier = content.modifier.flex_shrink(0.0);
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            // Pre-scroll: let parent consume first
            let d = run_pre_scroll(&st.parent_connection, d);
            let result = if d.x.abs() > 0.001 {
                let leftover_x = st.scroll_immediate(d.x);
                Vec2 {
                    x: leftover_x,
                    y: d.y,
                }
            } else {
                let leftover = st.scroll_immediate(d.y);
                Vec2 {
                    x: 0.0,
                    y: leftover,
                }
            };
            run_post_scroll(&st.parent_connection, result)
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
            (st.get() + st.overscroll_offset(), 0.0)
        })
    };
    let set_xy = {
        let st = state.clone();
        Rc::new(move |x: f32, _y: f32| st.set_offset(x))
    };
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
            show_scrollbar: state.show_scrollbar.get(),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}

pub fn ScrollAreaXY(modifier: Modifier, state: Rc<ScrollStateXY>, content: View) -> View {
    let on_scroll = {
        let st = state.clone();
        Rc::new(move |d: Vec2| -> Vec2 {
            let d = run_pre_scroll(&st.parent_connection, d);
            let result = st.scroll_immediate(d);
            run_post_scroll(&st.parent_connection, result)
        })
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
            let (ox, oy) = st.get();
            let (osx, osy) = st.overscroll_offset();
            (ox + osx, oy + osy)
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
            show_scrollbar: state.show_scrollbar.get(),
        },
    )
    .modifier(modifier)
    .with_children(vec![content])
}
