use repose_core::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

type CacheRevision = Rc<dyn Fn(u64) -> u64>;

fn cache_revision_for(
    callback: &RefCell<Option<CacheRevision>>,
    key: u64,
    value_identity: usize,
    height: f32,
    variation: u64,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let revision = callback
        .borrow()
        .as_ref()
        .cloned()
        .map(|revision| revision(key));
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    height.to_bits().hash(&mut hasher);
    variation.hash(&mut hasher);
    match revision {
        Some(revision) => revision.hash(&mut hasher),
        None => value_identity.hash(&mut hasher),
    }
    hasher.finish()
}

/// Configuration for [`LazyColumn`].
#[derive(Clone)]
pub struct LazyColumnConfig {
    pub modifier: Modifier,
    pub state: Rc<LazyColumnState>,
    pub animate_spec: Option<AnimationSpec>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
}

impl Default for LazyColumnConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyColumnState::new()),
            animate_spec: None,
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
        }
    }
}

/// Configuration for [`LazyRow`].
#[derive(Clone)]
pub struct LazyRowConfig {
    pub modifier: Modifier,
    pub state: Rc<LazyRowState>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
}

impl Default for LazyRowConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyRowState::new()),
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
        }
    }
}

/// Configuration for [`LazyVerticalGrid`] and [`LazyHorizontalGrid`].
#[derive(Clone)]
pub struct LazyGridConfig {
    pub modifier: Modifier,
    pub state: Rc<LazyGridState>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
}

impl Default for LazyGridConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyGridState::new()),
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
        }
    }
}

/// Configuration for [`LazyVerticalStaggeredGrid`].
#[derive(Clone)]
pub struct LazyVerticalStaggeredGridConfig {
    pub modifier: Modifier,
    pub state: Rc<LazyVerticalStaggeredGridState>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
}

impl Default for LazyVerticalStaggeredGridConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyVerticalStaggeredGridState::new()),
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
        }
    }
}

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
    cache_revision: RefCell<Option<CacheRevision>>,
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
            cache_revision: RefCell::new(None),
        }
    }

    pub fn set_vp_height(&self, h_px: f32) {
        let height = h_px.max(0.0);
        if (self.viewport_height.get() - height).abs() > 0.5 {
            self.viewport_height.set(height);
            request_frame();
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        *self.cache_revision.borrow_mut() = Some(Rc::new(revision));
    }

    pub fn clear_cache_revision(&self) {
        *self.cache_revision.borrow_mut() = None;
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        cache_revision_for(&self.cache_revision, key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);

        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
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
    cache_revision: RefCell<Option<CacheRevision>>,
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
            cache_revision: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        *self.cache_revision.borrow_mut() = Some(Rc::new(revision));
    }

    pub fn clear_cache_revision(&self) {
        *self.cache_revision.borrow_mut() = None;
    }

    pub(crate) fn cache_revision_for(&self, key: u64, value_identity: usize, height: f32) -> u64 {
        self.cache_revision_for_with(key, value_identity, height, 0)
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        cache_revision_for(&self.cache_revision, key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
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
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate_x(&self, delta_px: f32, content_width_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
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
    cache_revision: RefCell<Option<CacheRevision>>,
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
            cache_revision: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        *self.cache_revision.borrow_mut() = Some(Rc::new(revision));
    }

    pub fn clear_cache_revision(&self) {
        *self.cache_revision.borrow_mut() = None;
    }

    pub(crate) fn cache_revision_for(&self, key: u64, value_identity: usize, height: f32) -> u64 {
        self.cache_revision_for_with(key, value_identity, height, 0)
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        cache_revision_for(&self.cache_revision, key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_width: f32) {
        let vw = self.viewport_width.get();
        let max_off = (content_width - vw).max(0.0);
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_width_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_width.get();
        let max_offset = (content_width_px - viewport).max(0.0);
        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
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
    cache_revision: RefCell<Option<CacheRevision>>,
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
            cache_revision: RefCell::new(None),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        *self.cache_revision.borrow_mut() = Some(Rc::new(revision));
    }

    pub fn clear_cache_revision(&self) {
        *self.cache_revision.borrow_mut() = None;
    }

    pub(crate) fn cache_revision_for(&self, key: u64, value_identity: usize, height: f32) -> u64 {
        self.cache_revision_for_with(key, value_identity, height, 0)
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        cache_revision_for(&self.cache_revision, key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        let vh = self.viewport_height.get();
        let max_off = (content_height - vh).max(0.0);
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.scroll_offset.get() - clamped).abs() > 0.5 {
            self.scroll_offset.set(clamped);
        }
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        let before = self.scroll_offset.get();
        let viewport = self.viewport_height.get();
        let max_offset = (content_height_px - viewport).max(0.0);
        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
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
