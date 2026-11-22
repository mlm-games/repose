use repose_core::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use web_time::Instant;

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
            let parent = index + index.isolate_lowest_one();
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

    pub(crate) fn len(&self) -> usize {
        self.heights.len()
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
    pub(crate) fn current(&self) -> Option<Rc<LazyColumnGeometry>> {
        self.data.clone()
    }
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
    /// Animate an item's position when its placement changes (insert, remove
    /// or reorder) instead of snapping to the new slot. Equivalent to Compose's
    /// `LazyItemScope.animateItem()`.
    pub animate_item: Option<AnimationSpec>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
    pub spacing: Dp,
    pub sticky_keys: HashSet<u64>,
    pub content_type: Option<Rc<dyn Fn(usize) -> u64>>,
}

impl Default for LazyColumnConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyColumnState::new()),
            animate_spec: None,
            animate_item: None,
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
            spacing: Dp::ZERO,
            sticky_keys: HashSet::new(),
            content_type: None,
        }
    }
}

/// Configuration for [`LazyRow`].
#[derive(Clone)]
pub struct LazyRowConfig {
    pub modifier: Modifier,
    pub state: Rc<LazyRowState>,
    /// Animate an item's position when its placement changes (insert, remove
    /// or reorder) instead of snapping to the new slot.
    pub animate_item: Option<AnimationSpec>,
    pub content_padding: PaddingValues,
    pub reverse_layout: bool,
    pub user_scroll_enabled: bool,
    pub spacing: Dp,
    pub sticky_keys: HashSet<u64>,
    pub content_type: Option<Rc<dyn Fn(usize) -> u64>>,
}

impl Default for LazyRowConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            state: Rc::new(LazyRowState::new()),
            animate_item: None,
            content_padding: PaddingValues::default(),
            reverse_layout: false,
            user_scroll_enabled: true,
            spacing: Dp::ZERO,
            sticky_keys: HashSet::new(),
            content_type: None,
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
    /// Rubber-band stretch while the axis is pinned past an edge.
    pub(crate) overscroll: Signal<f32>,
    overscroll_enabled: Cell<bool>,
    /// Velocity of the last integrating fling step (px/s).
    last_axis_velocity: Cell<f32>,
}

impl LazyAxis {
    fn new() -> Self {
        Self {
            offset: signal(0.0),
            viewport: Cell::new(0.0),
            content: signal(0.0),
            overscroll: signal(0.0),
            overscroll_enabled: Cell::new(true),
            last_axis_velocity: Cell::new(0.0),
        }
    }

    pub(crate) fn set_overscroll_enabled(&self, enabled: bool) {
        self.overscroll_enabled.set(enabled);
    }

    pub(crate) fn overscroll_offset(&self) -> f32 {
        self.overscroll.get()
    }

    /// What a painter should translate by: scroll position plus rubber band.
    pub(crate) fn paint_offset(&self) -> f32 {
        self.offset.get() + self.overscroll.get()
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
        self.apply_delta(delta_px, content_px, physics, true)
    }

    /// Applies leftover from a nested child without cancelling this axis's own
    /// fling. See [`repose_core::scroll::ScrollState::scroll_nested`].
    pub(crate) fn scroll_nested(
        &self,
        delta_px: f32,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
    ) -> f32 {
        self.apply_delta(delta_px, content_px, physics, false)
    }

    fn apply_delta(
        &self,
        delta_px: f32,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
        user: bool,
    ) -> f32 {
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
        if user {
            physics.borrow_mut().record_user_input(consumed);
        } else {
            physics.borrow_mut().record_nested_input(consumed);
        }
        delta_px - consumed
    }

    /// Fold leftover into the rubber band. Runs last so nested parents get
    /// first dibs on the delta.
    pub(crate) fn apply_overscroll(
        &self,
        leftover: f32,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
    ) -> f32 {
        let max_offset = (content_px - self.viewport.get()).max(0.0);
        repose_core::scroll::os_scroll_axis(
            &self.offset,
            &self.overscroll,
            self.overscroll_enabled.get(),
            max_offset,
            self.offset.get(),
            leftover,
            physics,
        )
    }

    /// Returns whether the axis is still active plus the velocity a fling had
    /// left when it clamped at a boundary, for nested-scroll parent chaining.
    pub(crate) fn tick(
        &self,
        content_px: f32,
        physics: &RefCell<ScrollPhysics>,
        dt: f32,
    ) -> (bool, Option<f32>) {
        if repose_core::scroll::tick_os_axis(&self.overscroll, self.overscroll_enabled.get(), dt) {
            return (true, None);
        }
        let max_offset = (content_px - self.viewport.get()).max(0.0);
        let before = self.offset.get();
        let was_animating = physics.borrow().is_animating();
        let mut p = physics.borrow_mut();
        if let Some(new_off) = p.tick_integrate(before, 0.0, max_offset) {
            if dt > 0.0 {
                self.last_axis_velocity.set((new_off - before) / dt);
            }
            drop(p);
            self.offset.set(new_off);
            // Repose only composes on demand, so a live fling has to ask for
            // the next frame itself or it freezes after one integration.
            request_frame();
            (true, None)
        } else {
            let clamped = if was_animating
                && (before <= 0.0 || before >= max_offset)
                && self.last_axis_velocity.get().abs() > 0.5
            {
                Some(self.last_axis_velocity.get())
            } else {
                None
            };
            drop(p);
            (false, clamped)
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
    prev_tick: Cell<Instant>,
    anim: RefCell<Option<repose_core::animation::AnimatedValue<f32>>>,
    pad_main: Cell<f32>,
    spacing_main: Cell<f32>,
    uniform_main: Cell<f32>,
    visible: Signal<(usize, usize)>,
    item_count: Cell<usize>,
    /// The key that was first visible on the last composition, plus the
    /// distance from that item's start to the viewport start. Mirrors Compose's
    /// `LazyListScrollPosition.lastKnownFirstItemKey`.
    anchor: Cell<Option<(u64, f32)>>,
    /// Velocity handed over by a child through post-fling dispatch, integrated
    /// by this container until it stops or clamps at a boundary.
    pending_fling: Rc<Cell<f32>>,
    /// Velocity estimated from the last integrating fling step, used to hand a
    /// clamped fling to the nested-scroll parent.
    last_axis_velocity: Cell<f32>,
    /// Guards against dispatching the same clamped fling more than once.
    fling_dispatched: Rc<Cell<bool>>,
}

impl LazyScrollCore {
    fn new() -> Self {
        Self {
            axis: Rc::new(LazyAxis::new()),
            physics: Rc::new(RefCell::new(ScrollPhysics::new())),
            parent_connection: Rc::new(RefCell::new(None)),
            cache_revision: RefCell::new(None),
            prev_tick: Cell::new(Instant::now()),
            anim: RefCell::new(None),
            pad_main: Cell::new(0.0),
            spacing_main: Cell::new(0.0),
            uniform_main: Cell::new(0.0),
            visible: signal((0, 0)),
            item_count: Cell::new(0),
            anchor: Cell::new(None),
            pending_fling: Rc::new(Cell::new(0.0)),
            last_axis_velocity: Cell::new(0.0),
            fling_dispatched: Rc::new(Cell::new(false)),
        }
    }

    pub(crate) fn pad(&self) -> f32 {
        self.pad_main.get()
    }

    pub(crate) fn item_count(&self) -> usize {
        self.item_count.get()
    }

    pub(crate) fn set_item_count(&self, count: usize) {
        self.item_count.set(count);
    }

    pub(crate) fn set_content_padding(&self, pad_px: f32) {
        self.pad_main.set(pad_px.max(0.0));
    }

    pub(crate) fn set_item_metrics(&self, spacing_px: f32, uniform_px: f32) {
        self.spacing_main.set(spacing_px.max(0.0));
        self.uniform_main.set(uniform_px.max(0.0));
    }

    pub(crate) fn set_visible_range(&self, first: usize, last: usize) {
        self.visible.set_neq((first, last));
    }

    pub fn visible_range(&self) -> (usize, usize) {
        self.visible.get()
    }

    pub fn visible_range_signal(&self) -> Signal<(usize, usize)> {
        self.visible.clone()
    }

    pub(crate) fn anchor(&self) -> Option<(u64, f32)> {
        self.anchor.get()
    }

    pub(crate) fn set_anchor(&self, key: u64, inner_px: f32) {
        self.anchor.set(Some((key, inner_px)));
    }

    pub(crate) fn clear_anchor(&self) {
        self.anchor.set(None);
    }

    /// Move the offset without invalidating the key anchor: this is the
    /// per-composition correction, not an explicit scroll.
    pub(crate) fn set_offset_anchored(&self, off: f32, content: f32) {
        self.axis.set_offset(off, content);
    }

    pub(crate) fn uniform_offset_for_index(&self, index: usize) -> f32 {
        self.pad_main.get() + index as f32 * (self.uniform_main.get() + self.spacing_main.get())
    }

    pub(crate) fn animate_offset_to(&self, target_px: f32, content_px: f32) {
        self.clear_anchor();
        let max = (content_px - self.axis.viewport.get()).max(0.0);
        let target = target_px.clamp(0.0, max);
        let mut anim = repose_core::animation::AnimatedValue::new(
            self.axis.offset.get(),
            repose_core::animation::AnimationSpec::spring_gentle(),
        );
        anim.set_target(target);
        *self.anim.borrow_mut() = Some(anim);
        repose_core::request_frame();
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
        let fling_target = Rc::clone(&self.pending_fling);
        let fling_guard = Rc::clone(&self.fling_dispatched);
        NestedScrollConnection::new()
            .on_post_scroll(
                move |consumed: Vec2, available: Vec2, source: NestedScrollSource| -> Vec2 {
                    if available.y.abs() < 0.001 {
                        return Vec2::ZERO;
                    }
                    let leftover = axis.scroll_nested(available.y, axis.content.get(), &physics);
                    let own_consumed = available.y - leftover;
                    let after = repose_core::scroll::run_post_scroll(
                        &parent,
                        Vec2 {
                            x: consumed.x,
                            y: consumed.y + own_consumed,
                        },
                        Vec2 {
                            x: available.x,
                            y: leftover,
                        },
                        source,
                    );
                    Vec2 {
                        x: available.x - after.x,
                        y: available.y - after.y,
                    }
                },
            )
            .on_post_fling(
                move |_consumed: Vec2, available: Vec2, _source: NestedScrollSource| -> Vec2 {
                    if available.x.abs() < 0.001 && available.y.abs() < 0.001 {
                        return Vec2::ZERO;
                    }
                    let velocity = if available.x.abs() > available.y.abs() {
                        available.x
                    } else {
                        available.y
                    };
                    if velocity.abs() >= 50.0 {
                        fling_target.set(velocity.clamp(-8000.0, 8000.0));
                        fling_guard.set(false);
                        request_frame();
                    }
                    available
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
        self.fling_dispatched.set(false);
        self.axis
            .scroll_immediate(delta_px, content_px, &self.physics)
    }

    pub(crate) fn apply_overscroll(&self, leftover: f32, content_px: f32) -> f32 {
        self.axis
            .apply_overscroll(leftover, content_px, &self.physics)
    }

    pub(crate) fn tick(&self, content_px: f32) -> bool {
        let now = Instant::now();
        let dt = (now - self.prev_tick.get()).as_secs_f32().min(0.1);
        self.prev_tick.set(now);
        let finished = {
            let mut guard = self.anim.borrow_mut();
            match guard.as_mut() {
                None => false,
                Some(anim) => {
                    if anim.update() {
                        let max = (content_px - self.axis.viewport.get()).max(0.0);
                        self.axis.offset.set(anim.get().clamp(0.0, max));
                        repose_core::request_frame();
                        return true;
                    }
                    true
                }
            }
        };
        if finished {
            self.anim.borrow_mut().take();
        }
        let mut pending_active = false;
        let pending = self.pending_fling.get();
        if pending.abs() > 0.5 {
            let max = (content_px - self.axis.viewport.get()).max(0.0);
            let before = self.axis.offset.get();
            let target = (before + pending * dt).clamp(0.0, max);
            let moved = target - before;
            self.axis.offset.set(target);
            request_frame();
            if moved.abs() < 0.01 && (before <= 0.0 || before >= max) {
                self.dispatch_post_fling(pending);
                self.pending_fling.set(0.0);
            } else {
                let speed = pending.abs();
                let t = (speed / 4000.0).min(1.0);
                let decay = (0.85_f32 + t * 0.10).powf(dt * 60.0);
                let next = pending * decay;
                let next = if next.abs() < 15.0 { 0.0 } else { next };
                self.pending_fling.set(next);
                pending_active = next != 0.0;
            }
        }
        let (active, clamped) = self.axis.tick(content_px, &self.physics, dt);
        if let Some(velocity) = clamped {
            self.dispatch_post_fling(velocity);
        }
        active || pending_active
    }

    /// Hand a clamped fling's remaining velocity to this container's nested
    /// parent. The guard keeps a single fling from being forwarded twice.
    fn dispatch_post_fling(&self, velocity: f32) {
        if self.fling_dispatched.get() || velocity.abs() < 0.5 {
            return;
        }
        if let Some(ref parent) = *self.parent_connection.borrow() {
            self.fling_dispatched.set(true);
            parent.dispatch_post_fling(
                Vec2::ZERO,
                Vec2 {
                    x: 0.0,
                    y: velocity,
                },
                NestedScrollSource::SideEffect,
            );
        }
    }
}

pub struct LazyColumnState {
    pub(crate) core: LazyScrollCore,
    geometry: RefCell<LazyColumnGeometryCache>,
    reverse_layout: Cell<bool>,
    padding_bottom_px: Cell<f32>,
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
            reverse_layout: Cell::new(false),
            padding_bottom_px: Cell::new(0.0),
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

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.core.axis.set_overscroll_enabled(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.core.axis.overscroll_offset()
    }

    /// Fold leftover into the rubber band. Runs after the nested parent chain
    /// so parents get first dibs on the delta.
    pub fn apply_overscroll(&self, leftover: f32, content_px: f32) -> f32 {
        self.core.apply_overscroll(leftover, content_px)
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

    pub(crate) fn set_content_padding(&self, pad_px: f32) {
        self.core.set_content_padding(pad_px);
    }

    pub(crate) fn set_item_metrics(&self, spacing_px: f32, uniform_px: f32) {
        self.core.set_item_metrics(spacing_px, uniform_px);
    }

    pub(crate) fn set_item_count(&self, count: usize) {
        self.core.set_item_count(count);
    }

    pub(crate) fn set_visible_range(&self, first: usize, last: usize) {
        self.core.set_visible_range(first, last);
    }

    pub fn visible_range(&self) -> (usize, usize) {
        self.core.visible_range()
    }

    pub fn visible_range_signal(&self) -> Signal<(usize, usize)> {
        self.core.visible_range_signal()
    }

    pub(crate) fn set_reverse_layout(&self, reverse_layout: bool, padding_bottom_px: f32) {
        self.reverse_layout.set(reverse_layout);
        self.padding_bottom_px.set(padding_bottom_px);
    }

    pub fn offset_for_index(&self, index: usize) -> Option<f32> {
        let count = self.core.item_count();
        if index >= count {
            return None;
        }
        if self.reverse_layout.get() {
            let padding = self.padding_bottom_px.get();
            if let Some(geometry) = self.geometry.borrow().current()
                && geometry.len() == count
                && self.core.uniform_main.get() == 0.0
            {
                let after = count - index;
                if after == count {
                    return Some(padding);
                }
                return Some(padding + geometry.total() - geometry.top(after)
                    + self.core.spacing_main.get());
            }
            return Some(padding + self.core.uniform_offset_for_index(index) - self.core.pad());
        }
        if let Some(geo) = self.geometry.borrow().current()
            && index < geo.len()
        {
            return Some(self.core.pad() + geo.top(index));
        }
        Some(self.core.uniform_offset_for_index(index))
    }

    pub fn scroll_to_item(&self, index: usize, extra_offset_px: f32) {
        if let Some(top) = self.offset_for_index(index) {
            self.core.clear_anchor();
            self.core
                .set_offset(top + extra_offset_px, self.core.axis.content.get());
        }
    }

    pub fn animate_scroll_to_item(&self, index: usize, extra_offset_px: f32) {
        if let Some(top) = self.offset_for_index(index) {
            self.core
                .animate_offset_to(top + extra_offset_px, self.core.axis.content.get());
        }
    }
}

/// Two-axis state. `core.axis` is the vertical axis used by
/// [`crate::lazy::LazyVerticalGrid`]; `x` is the horizontal axis used by
/// [`crate::lazy::LazyHorizontalGrid`]. Both share one fling physics.
/// State for both grid orientations. A grid scrolls on exactly one axis, and
/// the component decides how to read it: [`crate::lazy::LazyVerticalGrid`]
/// treats it as vertical, [`crate::lazy::LazyHorizontalGrid`] as horizontal.
/// This mirrors Compose, where `LazyGridState` is a single-axis
/// `ScrollableState` and the orientation is supplied at measure time rather
/// than stored in the state.
pub struct LazyGridState {
    pub(crate) core: LazyScrollCore,
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
        }
    }

    pub fn set_nested_scroll_parent(&self, conn: NestedScrollConnection) {
        self.core.set_nested_scroll_parent(conn);
    }

    /// Wire a child scrollable to this container as its nested-scroll parent.
    pub fn connection(&self) -> NestedScrollConnection {
        self.core.connection()
    }

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.core.axis.set_overscroll_enabled(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.core.axis.overscroll_offset()
    }

    /// Fold leftover into the rubber band. Runs after the nested parent chain
    /// so parents get first dibs on the delta.
    pub fn apply_overscroll(&self, leftover: f32, content_px: f32) -> f32 {
        self.core.apply_overscroll(leftover, content_px)
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

    pub fn set_offset(&self, off: f32, content_main: f32) {
        self.core.set_offset(off, content_main);
    }

    pub fn scroll_immediate(&self, delta_px: f32, content_main_px: f32) -> f32 {
        self.core.scroll_immediate(delta_px, content_main_px)
    }

    pub fn tick(&self, content_main_px: f32) -> bool {
        self.core.tick(content_main_px)
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

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.core.axis.set_overscroll_enabled(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.core.axis.overscroll_offset()
    }

    /// Fold leftover into the rubber band. Runs after the nested parent chain
    /// so parents get first dibs on the delta.
    pub fn apply_overscroll(&self, leftover: f32, content_px: f32) -> f32 {
        self.core.apply_overscroll(leftover, content_px)
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

    pub(crate) fn set_content_padding(&self, pad_px: f32) {
        self.core.set_content_padding(pad_px);
    }

    pub(crate) fn set_item_metrics(&self, spacing_px: f32, uniform_px: f32) {
        self.core.set_item_metrics(spacing_px, uniform_px);
    }

    pub(crate) fn set_item_count(&self, count: usize) {
        self.core.set_item_count(count);
    }

    pub(crate) fn set_visible_range(&self, first: usize, last: usize) {
        self.core.set_visible_range(first, last);
    }

    pub fn visible_range(&self) -> (usize, usize) {
        self.core.visible_range()
    }

    pub fn visible_range_signal(&self) -> Signal<(usize, usize)> {
        self.core.visible_range_signal()
    }

    pub fn offset_for_index(&self, index: usize) -> Option<f32> {
        if index >= self.core.item_count() {
            return None;
        }
        Some(self.core.uniform_offset_for_index(index))
    }

    pub fn scroll_to_item(&self, index: usize, extra_offset_px: f32) {
        if let Some(left) = self.offset_for_index(index) {
            self.core.clear_anchor();
            self.core
                .set_offset(left + extra_offset_px, self.core.axis.content.get());
        }
    }

    pub fn animate_scroll_to_item(&self, index: usize, extra_offset_px: f32) {
        if let Some(left) = self.offset_for_index(index) {
            self.core
                .animate_offset_to(left + extra_offset_px, self.core.axis.content.get());
        }
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

    pub fn set_overscroll_enabled(&self, enabled: bool) {
        self.core.axis.set_overscroll_enabled(enabled);
    }

    pub fn overscroll_offset(&self) -> f32 {
        self.core.axis.overscroll_offset()
    }

    /// Fold leftover into the rubber band. Runs after the nested parent chain
    /// so parents get first dibs on the delta.
    pub fn apply_overscroll(&self, leftover: f32, content_px: f32) -> f32 {
        self.core.apply_overscroll(leftover, content_px)
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

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: web_time::Duration = web_time::Duration::from_millis(16);

    fn frame() {
        std::thread::sleep(FRAME);
    }

    fn seeded() -> LazyColumnState {
        let st = LazyColumnState::new();
        st.set_vp_height(100.0);
        st.core.axis.content.set(4000.0);
        st
    }

    /// Lazy lists share the same integrator, so they inherit the same
    /// double-count regression if the stand-down is missing.
    #[test]
    fn lazy_drag_does_not_double_count() {
        let st = seeded();
        let (frames, step) = (10, 30.0);
        for _ in 0..frames {
            st.scroll_immediate(step, 4000.0);
            frame();
            st.tick(4000.0);
        }
        let expected = step * frames as f32;
        let got = st.core.axis.offset.get();
        assert!(
            (got - expected).abs() < 1.0,
            "drag of {expected} moved {got}"
        );
    }

    /// The rubber band absorbs leftover at the edge instead of the offset
    /// running away, and it decays back to rest.
    #[test]
    fn overscroll_absorbs_at_edge_and_decays() {
        let st = seeded();
        st.set_offset(0.0, 4000.0);
        // Push past the start edge: nothing left to consume.
        let leftover = st.apply_overscroll(-60.0, 4000.0);
        assert!(leftover.abs() < 0.001, "band should absorb, got {leftover}");
        let band = st.overscroll_offset();
        assert!(
            band < 0.0,
            "band should stretch against the overscroll, got {band}"
        );

        for _ in 0..40 {
            frame();
            st.tick(4000.0);
        }
        assert!(
            st.overscroll_offset().abs() < 0.5,
            "band settled to {}",
            st.overscroll_offset()
        );
    }

    /// A live fling must request the next frame. Repose composes only on
    /// demand, so without this the lazy fling integrates once and freezes.
    #[test]
    fn fling_requests_follow_up_frame() {
        let st = seeded();
        st.scroll_immediate(200.0, 4000.0);
        frame();
        st.tick(4000.0);
        while repose_core::frame_clock::take_frame_request() {}
        assert!(
            st.core.physics.borrow().is_animating(),
            "still coasting, so a frame must be pending"
        );
        frame();
        assert!(st.tick(4000.0), "fling integrates");
        assert!(
            repose_core::frame_clock::peek_frame_request(),
            "an integrating fling must request the next frame"
        );
    }

    #[test]
    fn test_offset_for_index_uniform_with_spacing() {
        let state = LazyColumnState::new();
        state.set_content_padding(10.0);
        state.set_item_metrics(4.0, 50.0);
        state.set_item_count(10);
        assert_eq!(state.offset_for_index(0), Some(10.0));
        assert_eq!(state.offset_for_index(2), Some(10.0 + 2.0 * 54.0));
        assert_eq!(state.offset_for_index(10), None);
        state.set_visible_range(2, 5);
        assert_eq!(state.visible_range(), (2, 5));
    }

    #[test]
    fn reversed_column_scroll_offsets_follow_item_order_from_bottom() {
        let state = LazyColumnState::new();
        state.set_reverse_layout(true, 6.0);
        state.set_content_padding(10.0);
        state.set_item_count(3);
        state.set_item_metrics(4.0, 50.0);
        assert_eq!(state.offset_for_index(1), Some(60.0));

        state.set_item_metrics(4.0, 0.0);
        state.geometry(&[3, 2, 1], &[24.0, 34.0, 40.0]);
        assert_eq!(state.offset_for_index(0), Some(6.0));
        assert_eq!(state.offset_for_index(1), Some(50.0));
        assert_eq!(state.offset_for_index(2), Some(84.0));
        assert_eq!(state.offset_for_index(3), None);
    }

    #[test]
    fn test_row_offset_for_index_and_animated_target() {
        let state = LazyRowState::new();
        state.set_content_padding(8.0);
        state.set_item_metrics(6.0, 100.0);
        state.set_item_count(5);
        assert_eq!(state.offset_for_index(3), Some(8.0 + 3.0 * 106.0));
        state.scroll_to_item(1, 0.0);
        state.animate_scroll_to_item(2, 0.0);
        assert!(state.core.anim.borrow().is_some());
    }
}
