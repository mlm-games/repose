use repose_core::*;
use std::cell::{Cell, RefCell};

use crate::scroll::{NestedScrollConnection, ScrollPhysics};

pub trait ItemHeight<T> {
    fn get(&self, item: &T) -> f32;
}

impl<T> ItemHeight<T> for f32 {
    fn get(&self, _item: &T) -> f32 {
        *self
    }
}

impl<T, F: Fn(&T) -> f32> ItemHeight<T> for F {
    fn get(&self, item: &T) -> f32 {
        (self)(item)
    }
}

pub struct LazyColumnState {
    pub(crate) scroll_offset: Signal<f32>,
    pub(crate) viewport_height: Cell<f32>,
    pub(crate) content_height: Signal<f32>,
    pub(crate) physics: RefCell<ScrollPhysics>,
    pub(crate) parent_connection: RefCell<Option<NestedScrollConnection>>,
}

impl Default for LazyColumnState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: Cell::new(0.0),
            content_height: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
            parent_connection: RefCell::new(None),
        }
    }

    pub fn set_vp_height(&self, h_px: f32) {
        self.viewport_height.set(h_px.max(0.0));
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);

        let consumed = new_offset - before;

        self.physics.borrow_mut().record_input(consumed);

        delta_px - consumed
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}

pub struct LazyGridState {
    pub(crate) scroll_offset: Signal<f32>,
    pub(crate) viewport_height: Cell<f32>,
    pub(crate) content_height: Signal<f32>,
    pub(crate) viewport_width: Cell<f32>,
    pub(crate) content_width: Signal<f32>,
    pub(crate) physics: RefCell<ScrollPhysics>,
    pub(crate) parent_connection: RefCell<Option<NestedScrollConnection>>,
}

impl Default for LazyGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyGridState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: Cell::new(0.0),
            content_height: signal(0.0),
            viewport_width: Cell::new(0.0),
            content_width: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
            parent_connection: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }

    pub fn set_offset_x(&self, off: f32, content_width: f32) {
        let vw = self.viewport_width.get();
        let max_off = (content_width - vw).max(0.0);
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate_x(&self, delta_px: f32, content_width_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick_x(&self, content_width_px: f32) -> bool {
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}

pub struct LazyRowState {
    pub(crate) scroll_offset: Signal<f32>,
    pub(crate) viewport_width: Cell<f32>,
    pub(crate) content_width: Signal<f32>,
    pub(crate) physics: RefCell<ScrollPhysics>,
    pub(crate) parent_connection: RefCell<Option<NestedScrollConnection>>,
}

impl Default for LazyRowState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyRowState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_width: Cell::new(0.0),
            content_width: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
            parent_connection: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_offset(&self, off: f32, content_width: f32) {
        let vw = self.viewport_width.get();
        let max_off = (content_width - vw).max(0.0);
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_width_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick(&self, content_width_px: f32) -> bool {
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}

pub struct LazyVerticalStaggeredGridState {
    pub(crate) scroll_offset: Signal<f32>,
    pub(crate) viewport_height: Cell<f32>,
    pub(crate) content_height: Signal<f32>,
    pub(crate) physics: RefCell<ScrollPhysics>,
    pub(crate) parent_connection: RefCell<Option<NestedScrollConnection>>,
}

impl Default for LazyVerticalStaggeredGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyVerticalStaggeredGridState {
    pub fn new() -> Self {
        Self {
            scroll_offset: signal(0.0),
            viewport_height: Cell::new(0.0),
            content_height: signal(0.0),
            physics: RefCell::new(ScrollPhysics::new(0.90, 5.0, 10.0)),
            parent_connection: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        self.scroll_offset.set(new_offset);
        let consumed = new_offset - before;
        self.physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let mut p = self.physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.scroll_offset.get(), 0.0, max_offset) {
            drop(p);
            self.scroll_offset.set(new_off);
            true
        } else {
            false
        }
    }
}
