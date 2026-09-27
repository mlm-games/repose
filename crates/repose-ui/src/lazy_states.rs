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
    let revision = callback.borrow().as_ref().map(|revision| revision(key));
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

fn cache_revision_for_uniform(
    callback: &RefCell<Option<CacheRevision>>,
    key: u64,
    value_identity: usize,
    height: f32,
    variation: u64,
) -> u64 {
    let identity = if callback.borrow().is_some() {
        0
    } else {
        value_identity
    };
    cache_revision_for(callback, key, identity, height, variation)
}

pub(crate) struct LazyColumnGeometry {
    keys: Vec<u64>,
    heights: Vec<f32>,
    fenwick: Vec<f32>,
    total: f32,
}

impl LazyColumnGeometry {
    fn new(keys: Vec<u64>, heights: Vec<f32>) -> Self {
        let mut geometry = Self {
            keys,
            heights,
            fenwick: Vec::new(),
            total: 0.0,
        };
        geometry.rebuild();
        geometry
    }

    fn rebuild(&mut self) {
        self.fenwick.clear();
        self.fenwick.resize(self.heights.len() + 1, 0.0);
        for (index, height) in self.heights.iter().copied().enumerate() {
            self.fenwick[index + 1] = height;
        }
        for index in 1..self.fenwick.len() {
            let parent = index + (index & index.wrapping_neg());
            if parent < self.fenwick.len() {
                self.fenwick[parent] += self.fenwick[index];
            }
        }
        self.total = self.prefix_sum(self.heights.len());
    }

    fn prefix_sum(&self, count: usize) -> f32 {
        let mut index = count.min(self.heights.len());
        let mut sum = 0.0;
        while index > 0 {
            sum += self.fenwick[index];
            index &= index - 1;
        }
        sum
    }

    pub(crate) fn total(&self) -> f32 {
        self.total
    }

    pub(crate) fn top(&self, index: usize) -> f32 {
        self.prefix_sum(index)
    }

    pub(crate) fn height(&self, index: usize) -> f32 {
        self.heights.get(index).copied().unwrap_or(0.0)
    }

    /// Fenwick binary-lifting: largest item count whose cumulative height
    /// still fits within `offset`.
    fn prefix_within(&self, offset: f32) -> usize {
        let mut index = 0;
        let mut bit = 1usize;
        while bit <= self.heights.len() {
            bit <<= 1;
        }
        let mut sum = 0.0;
        while bit != 0 {
            let next = index + bit;
            if next <= self.heights.len() && sum + self.fenwick[next] <= offset {
                index = next;
                sum += self.fenwick[next];
            }
            bit >>= 1;
        }
        index
    }

    pub(crate) fn first_visible(&self, offset: f32) -> usize {
        if offset <= 0.0 || self.heights.is_empty() {
            return 0;
        }
        self.prefix_within(offset)
    }

    pub(crate) fn end_visible(&self, offset: f32) -> usize {
        if offset <= 0.0 {
            return 0;
        }
        let index = self.prefix_within(offset);
        if index < self.heights.len() {
            index + 1
        } else {
            self.heights.len()
        }
    }
}

struct LazyColumnGeometryCache {
    data: Option<Rc<LazyColumnGeometry>>,
}

impl LazyColumnGeometryCache {
    fn new() -> Self {
        Self { data: None }
    }

    fn update(&mut self, keys: &[u64], heights: &[f32]) -> Rc<LazyColumnGeometry> {
        if let Some(data) = &self.data
            && data.keys.len() == keys.len()
            && data
                .keys
                .iter()
                .zip(keys)
                .all(|(left, right)| left == right)
            && data.heights.len() == heights.len()
            && data
                .heights
                .iter()
                .zip(heights)
                .all(|(left, right)| left.to_bits() == right.to_bits())
        {
            return data.clone();
        }
        let data = Rc::new(LazyColumnGeometry::new(keys.to_vec(), heights.to_vec()));
        self.data = Some(data.clone());
        data
    }
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

    fn uniform_height(&self) -> Option<f32> {
        None
    }
}

impl<T> ItemHeight<T> for f32 {
    fn get(&self, _item: &T) -> f32 {
        *self
    }

    fn uniform_height(&self) -> Option<f32> {
        Some(*self)
    }
}

impl<T, F: Fn(&T) -> f32> ItemHeight<T> for F {
    fn get(&self, item: &T) -> f32 {
        (self)(item)
    }
}

/// Scroll state for a single axis: current offset, viewport extent, content
/// extent. Every lazy container scrolls on one or two of these.
pub(crate) struct LazyAxis {
    pub(crate) offset: Signal<f32>,
    pub(crate) viewport: Cell<f32>,
    pub(crate) content: Signal<f32>,
}

impl LazyAxis {
    fn new() -> Self {
        Self {
            offset: signal(0.0),
            viewport: Cell::new(0.0),
            content: signal(0.0),
        }
    }

    /// Drops sub-pixel writes so layout jitter below half a pixel does not
    /// invalidate dependents.
    pub(crate) fn set_viewport(&self, px: f32) {
        let px = px.max(0.0);
        if (self.viewport.get() - px).abs() > 0.5 {
            self.viewport.set(px);
            request_frame();
        }
    }

    pub(crate) fn set_offset(&self, off: f32, content: f32) {
        let max_off = (content - self.viewport.get()).max(0.0);
        let off = if off.is_finite() { off } else { 0.0 };
        let clamped = off.clamp(0.0, max_off);
        if (self.offset.get() - clamped).abs() > 0.5 {
            self.offset.set(clamped);
        }
    }

    /// Applies `delta_px` from real user input, pre-empting any in-flight
    /// fling. Returns the part the axis could not consume.
    pub(crate) fn scroll_immediate(
        &self,
        delta_px: f32,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
    ) -> f32 {
        physics.borrow_mut().cancel_fling();
        self.apply_delta(delta_px, content_px, physics)
    }

    /// Applies leftover from a nested child without cancelling this axis's own
    /// fling. See [`repose_core::scroll::ScrollState::scroll_nested`].
    pub(crate) fn scroll_nested(
        &self,
        delta_px: f32,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
    ) -> f32 {
        self.apply_delta(delta_px, content_px, physics)
    }

    fn apply_delta(&self, delta_px: f32, content_px: f32, physics: &RefCell<ScrollPhysics>) -> f32 {
        let before = self.offset.get();
        let max_offset = (content_px - self.viewport.get()).max(0.0);
        let delta_px = if delta_px.is_finite() { delta_px } else { 0.0 };
        let new_offset = (before + delta_px).clamp(0.0, max_offset);
        let new_offset = if new_offset.is_finite() {
            new_offset
        } else {
            before
        };
        self.offset.set(new_offset);
        let consumed = new_offset - before;
        physics.borrow_mut().record_input(consumed);
        delta_px - consumed
    }

    pub(crate) fn tick(&self, content_px: f32, physics: &RefCell<ScrollPhysics>) -> bool {
        let max_offset = (content_px - self.viewport.get()).max(0.0);
        let mut p = physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(self.offset.get(), 0.0, max_offset) {
            drop(p);
            self.offset.set(new_off);
            true
        } else {
            false
        }
    }
}

/// Axis-agnostic scroll plumbing shared by every lazy container state:
/// the scrolling axis, its fling physics, nested-scroll wiring, and the
/// cache-revision hook.
pub(crate) struct LazyScrollCore {
    pub(crate) axis: Rc<LazyAxis>,
    pub(crate) physics: Rc<RefCell<ScrollPhysics>>,
    pub(crate) parent_connection: Rc<RefCell<Option<NestedScrollConnection>>>,
    cache_revision: RefCell<Option<CacheRevision>>,
}

impl LazyScrollCore {
    fn new() -> Self {
        Self {
            axis: Rc::new(LazyAxis::new()),
            physics: Rc::new(RefCell::new(ScrollPhysics::new())),
            parent_connection: Rc::new(RefCell::new(None)),
            cache_revision: RefCell::new(None),
        }
    }

    pub(crate) fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        *self.parent_connection.borrow_mut() = Some(conn);
    }

    /// A `NestedScrollConnection` that consumes leftover scroll by scrolling
    /// THIS container, then chains anything still unabsorbed to this
    /// container's own parent. This is what lets a lazy container act as a
    /// nested-scroll parent, matching Compose where `LazyListState` is itself a
    /// `ScrollableState`.
    pub fn connection(&self) -> NestedScrollConnection {
        let axis = Rc::clone(&self.axis);
        let physics = Rc::clone(&self.physics);
        let parent = Rc::clone(&self.parent_connection);
        NestedScrollConnection::new().on_post_scroll(
            move |_consumed: Vec2, available: Vec2, _source: NestedScrollSource| -> Vec2 {
                if available.y.abs() < 0.001 {
                    return Vec2::ZERO;
                }
                let leftover = axis.scroll_nested(available.y, axis.content.get(), &physics);
                let after = repose_core::scroll::run_post_scroll(
                    &parent,
                    Vec2 {
                        x: available.x,
                        y: leftover,
                    },
                );
                Vec2 {
                    x: available.x - after.x,
                    y: available.y - after.y,
                }
            },
        )
    }

    pub(crate) fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        *self.cache_revision.borrow_mut() = Some(Rc::new(revision));
    }

    pub(crate) fn clear_cache_revision(&self) {
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

    pub(crate) fn cache_revision_for_uniform(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        cache_revision_for_uniform(&self.cache_revision, key, value_identity, height, variation)
    }

    pub(crate) fn set_offset(&self, off: f32, content: f32) {
        self.axis.set_offset(off, content);
    }

    pub(crate) fn scroll_immediate(&self, delta_px: f32, content_px: f32) -> f32 {
        self.axis
            .scroll_immediate(delta_px, content_px, &self.physics)
    }

    pub(crate) fn tick(&self, content_px: f32) -> bool {
        self.axis.tick(content_px, &self.physics)
    }
}

pub struct LazyColumnState {
    pub(crate) core: LazyScrollCore,
    geometry: RefCell<LazyColumnGeometryCache>,
}

impl Default for LazyColumnState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyColumnState {
    pub fn new() -> Self {
        Self {
            core: LazyScrollCore::new(),
            geometry: RefCell::new(LazyColumnGeometryCache::new()),
        }
    }

    pub fn set_vp_height(&self, h_px: f32) {
        self.core.axis.set_viewport(h_px);
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        self.core.set_nested_scroll_parent(conn);
    }

    /// Wire a child scrollable to this container as its nested-scroll parent.
    pub fn connection(&self) -> NestedScrollConnection {
        self.core.connection()
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        self.core.set_cache_revision(revision);
    }

    pub fn clear_cache_revision(&self) {
        self.core.clear_cache_revision();
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        self.core
            .cache_revision_for_with(key, value_identity, height, variation)
    }

    pub(crate) fn cache_revision_for_uniform(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        self.core
            .cache_revision_for_uniform(key, value_identity, height, variation)
    }

    pub(crate) fn geometry(&self, keys: &[u64], heights: &[f32]) -> Rc<LazyColumnGeometry> {
        self.geometry.borrow_mut().update(keys, heights)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        self.core.set_offset(off, content_height);
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        self.core.scroll_immediate(delta_px, content_height_px)
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        self.core.tick(content_height_px)
    }
}

/// Two-axis state. `core.axis` is the vertical axis used by
/// [`crate::lazy::LazyVerticalGrid`]; `x` is the horizontal axis used by
/// [`crate::lazy::LazyHorizontalGrid`]. Both share one fling physics.
pub struct LazyGridState {
    pub(crate) core: LazyScrollCore,
    pub(crate) x: LazyAxis,
}

impl Default for LazyGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyGridState {
    pub fn new() -> Self {
        Self {
            core: LazyScrollCore::new(),
            x: LazyAxis::new(),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        self.core.set_nested_scroll_parent(conn);
    }

    /// Wire a child scrollable to this container as its nested-scroll parent.
    pub fn connection(&self) -> NestedScrollConnection {
        self.core.connection()
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        self.core.set_cache_revision(revision);
    }

    pub fn clear_cache_revision(&self) {
        self.core.clear_cache_revision();
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        self.core
            .cache_revision_for_with(key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        self.core.set_offset(off, content_height);
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        self.core.scroll_immediate(delta_px, content_height_px)
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        self.core.tick(content_height_px)
    }

    pub fn set_offset_x(&self, off: f32, content_width: f32) {
        self.x.set_offset(off, content_width);
    }

    pub fn scroll_immediate_x(&self, delta_px: f32, content_width_px: f32) -> f32 {
        self.x
            .scroll_immediate(delta_px, content_width_px, &self.core.physics)
    }

    pub fn tick_x(&self, content_width_px: f32) -> bool {
        self.x.tick(content_width_px, &self.core.physics)
    }
}

pub struct LazyRowState {
    pub(crate) core: LazyScrollCore,
}

impl Default for LazyRowState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyRowState {
    pub fn new() -> Self {
        Self {
            core: LazyScrollCore::new(),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        self.core.set_nested_scroll_parent(conn);
    }

    /// Wire a child scrollable to this container as its nested-scroll parent.
    pub fn connection(&self) -> NestedScrollConnection {
        self.core.connection()
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        self.core.set_cache_revision(revision);
    }

    pub fn clear_cache_revision(&self) {
        self.core.clear_cache_revision();
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        self.core
            .cache_revision_for_with(key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_width: f32) {
        self.core.set_offset(off, content_width);
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_width_px: f32) -> f32 {
        self.core.scroll_immediate(delta_px, content_width_px)
    }

    pub fn tick(&self, content_width_px: f32) -> bool {
        self.core.tick(content_width_px)
    }
}

pub struct LazyVerticalStaggeredGridState {
    pub(crate) core: LazyScrollCore,
}

impl Default for LazyVerticalStaggeredGridState {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyVerticalStaggeredGridState {
    pub fn new() -> Self {
        Self {
            core: LazyScrollCore::new(),
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        self.core.set_nested_scroll_parent(conn);
    }

    /// Wire a child scrollable to this container as its nested-scroll parent.
    pub fn connection(&self) -> NestedScrollConnection {
        self.core.connection()
    }

    pub fn set_cache_revision(&self, revision: impl Fn(u64) -> u64 + 'static) {
        self.core.set_cache_revision(revision);
    }

    pub fn clear_cache_revision(&self) {
        self.core.clear_cache_revision();
    }

    pub(crate) fn cache_revision_for_with(
        &self,
        key: u64,
        value_identity: usize,
        height: f32,
        variation: u64,
    ) -> u64 {
        self.core
            .cache_revision_for_with(key, value_identity, height, variation)
    }

    pub fn set_offset(&self, off: f32, content_height: f32) {
        self.core.set_offset(off, content_height);
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_height_px: f32) -> f32 {
        self.core.scroll_immediate(delta_px, content_height_px)
    }

    pub fn tick(&self, content_height_px: f32) -> bool {
        self.core.tick(content_height_px)
    }
}
