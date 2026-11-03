//! Drag-following pagers with snapping.
//!
//! [`HorizontalPager`] and [`VerticalPager`] lay out the pages around the
//! current continuous position, so a drag moves the pages under the finger and
//! a release snaps to the nearest page (or one page further when the release
//! velocity is high). [`PagerState`] exposes that position so indicators can
//! follow the drag.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use repose_core::*;
use web_time::Instant;

use crate::subcompose::box_with_constraints_with_key;
use crate::ViewExt;

/// Configuration for [`HorizontalPager`] and [`VerticalPager`].
#[derive(Clone)]
pub struct PagerConfig {
    pub modifier: Modifier,
    pub page_spacing: Dp,
    pub user_scroll_enabled: bool,
    pub content_padding: PaddingValues,
    /// How many pages to lay out on each side of the settled page.
    pub beyond_viewport_page_count: usize,
    /// Release speed in px/s above which a drag moves a whole page even when
    /// the page has not travelled past its halfway point.
    pub fling_velocity_threshold: f32,
    /// Spec used when a release snaps to a page.
    pub snap_spec: AnimationSpec,
}

impl Default for PagerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            page_spacing: Dp::ZERO,
            user_scroll_enabled: true,
            content_padding: PaddingValues::default(),
            beyond_viewport_page_count: 1,
            fling_velocity_threshold: 400.0,
            snap_spec: AnimationSpec::spring_gentle(),
        }
    }
}

struct PagerInner {
    position: Signal<f32>,
    page_count: Signal<usize>,
    animation: Option<AnimatedValue<f32>>,
    animation_target: f32,
}

struct DragState {
    active: bool,
    /// `Some(true)` when the gesture is moving along the pager axis,
    /// `Some(false)` when it was claimed by the cross axis.
    along_axis: Option<bool>,
    start_primary: f32,
    start_cross: f32,
    start_position: f32,
    last_primary: f32,
    last_time: Option<Instant>,
    velocity: f32,
}

impl DragState {
    fn new() -> Self {
        Self {
            active: false,
            along_axis: None,
            start_primary: 0.0,
            start_cross: 0.0,
            start_position: 0.0,
            last_primary: 0.0,
            last_time: None,
            velocity: 0.0,
        }
    }
}

static NEXT_PAGER_ID: AtomicU64 = AtomicU64::new(0);

/// Continuous position and snap animation of a pager.
///
/// `position` is measured in pages: `2.0` means page index 2 is settled at the
/// viewport origin, `2.5` means the pager has been dragged half a page
/// backwards. [`current_page`](Self::current_page) is the nearest page.
pub struct PagerState {
    inner: Rc<RefCell<PagerInner>>,
    drag: Rc<RefCell<DragState>>,
    /// Page extent along the scroll axis, in physical px, published by the
    /// pager each time it measures itself.
    page_extent: Rc<Cell<f32>>,
    tick_key: String,
}

impl PagerState {
    pub fn new(page_count: usize) -> Self {
        Self {
            inner: Rc::new(RefCell::new(PagerInner {
                position: signal(0.0),
                page_count: signal(page_count.max(1)),
                animation: None,
                animation_target: 0.0,
            })),
            drag: Rc::new(RefCell::new(DragState::new())),
            page_extent: Rc::new(Cell::new(0.0)),
            tick_key: format!(
                "pager_tick:{}",
                NEXT_PAGER_ID.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    /// The continuous position in pages.
    pub fn position(&self) -> f32 {
        self.inner.borrow().position.get()
    }

    /// The page closest to the current position.
    pub fn current_page(&self) -> usize {
        let max = self.page_count().saturating_sub(1) as f32;
        self.position().round().clamp(0.0, max) as usize
    }

    /// How far the pager sits from [`current_page`](Self::current_page), in
    /// `-0.5..=0.5` pages. Negative means the next page is being revealed.
    pub fn current_page_offset_fraction(&self) -> f32 {
        let position = self.position();
        position - position.round()
    }

    pub fn page_count(&self) -> usize {
        self.inner.borrow().page_count.get()
    }

    /// Animate to `page`, clamped to the page range.
    pub fn set_page(&self, page: usize) {
        self.animate_scroll_to_page(page, AnimationSpec::spring_gentle());
    }

    /// Animate to `page` with an explicit spec.
    pub fn animate_scroll_to_page(&self, page: usize, spec: AnimationSpec) {
        let max = self.page_count().saturating_sub(1);
        self.animate_to_position(page.min(max) as f32, spec);
    }

    /// Jump to `page` without animating.
    pub fn scroll_to_page(&self, page: usize) {
        let max = self.page_count().saturating_sub(1);
        let target = page.min(max) as f32;
        let mut inner = self.inner.borrow_mut();
        inner.animation = None;
        inner.animation_target = target;
        inner.position.set_neq(target);
    }

    /// Update the page count, clamping the position into range.
    pub fn set_page_count(&self, count: usize) {
        let count = count.max(1);
        let max = (count - 1) as f32;
        let mut inner = self.inner.borrow_mut();
        inner.page_count.set_neq(count);
        let position = inner.position.get();
        if position > max {
            inner.animation = None;
            inner.animation_target = max;
            inner.position.set_neq(max);
        }
    }

    /// Whether a snap animation is currently running.
    pub fn is_animating(&self) -> bool {
        self.inner.borrow().animation.is_some()
    }

    /// Move the pager by `delta_pages` pages without animating. Positive
    /// values reveal later pages, negative earlier ones.
    pub fn scroll_by_pages(&self, delta_pages: f32) {
        if !delta_pages.is_finite() {
            return;
        }
        let max = (self.page_count().saturating_sub(1)) as f32;
        let mut inner = self.inner.borrow_mut();
        inner.animation = None;
        let position = (inner.position.get() + delta_pages).clamp(0.0, max);
        inner.animation_target = position;
        inner.position.set_neq(position);
    }

    fn begin_drag(&self, primary: f32, cross: f32) {
        let mut inner = self.inner.borrow_mut();
        inner.animation = None;
        let position = inner.position.get();
        drop(inner);

        let mut drag = self.drag.borrow_mut();
        drag.active = true;
        drag.along_axis = None;
        drag.start_primary = primary;
        drag.start_cross = cross;
        drag.start_position = position;
        drag.last_primary = primary;
        drag.last_time = Some(Instant::now());
        drag.velocity = 0.0;
    }

    fn drag(&self, primary: f32, cross: f32) {
        let extent = self.page_extent.get();
        if extent <= 0.0 {
            return;
        }
        let mut drag = self.drag.borrow_mut();
        if !drag.active {
            return;
        }
        if drag.along_axis.is_none() {
            let along = (primary - drag.start_primary).abs();
            let across = (cross - drag.start_cross).abs();
            let slop = Dp(8.0).to_px().0.max(4.0);
            if along.max(across) < slop {
                return;
            }
            let claimed = along >= across;
            drag.along_axis = Some(claimed);
            if !claimed {
                drag.active = false;
                return;
            }
        }

        let now = Instant::now();
        if let Some(last) = drag.last_time {
            let dt = now.duration_since(last).as_secs_f32();
            if dt > 1e-4 {
                let instant = (primary - drag.last_primary) / dt;
                drag.velocity = drag.velocity * 0.6 + instant * 0.4;
            }
        }
        drag.last_primary = primary;
        drag.last_time = Some(now);

        let max = (self.page_count().saturating_sub(1)) as f32;
        let travelled = (primary - drag.start_primary) / extent;
        let position = (drag.start_position - travelled).clamp(0.0, max);
        drop(drag);

        self.inner.borrow_mut().position.set_neq(position);
    }

    fn end_drag(&self, fling_threshold: f32, spec: AnimationSpec) {
        let mut drag = self.drag.borrow_mut();
        let was_along_axis = drag.along_axis == Some(true);
        let velocity = drag.velocity;
        drag.active = false;
        drag.along_axis = None;
        drop(drag);

        if !was_along_axis {
            self.snap_to_nearest(spec);
            return;
        }

        let position = self.position();
        let max = (self.page_count().saturating_sub(1)) as f32;
        // Position grows when the finger moves toward the start edge, so the
        // release velocity points the opposite way from the page it reveals.
        let fling = if velocity.abs() > fling_threshold {
            -velocity.signum()
        } else {
            0.0
        };
        let target = if fling == 0.0 {
            position.round()
        } else {
            (position + fling).round()
        };
        // A single gesture moves at most one page, so a fast flick out of a
        // half-dragged page cannot skip over the neighbour.
        let anchor = position.round();
        let target = target.clamp(anchor - 1.0, anchor + 1.0).clamp(0.0, max);
        self.animate_to_position(target, spec);
    }

    fn cancel_drag(&self, spec: AnimationSpec) {
        let mut drag = self.drag.borrow_mut();
        drag.active = false;
        drag.along_axis = None;
        drop(drag);
        self.snap_to_nearest(spec);
    }

    /// Settle on the closest page when a touch stopped a snap animation
    /// without becoming a drag.
    fn snap_to_nearest(&self, spec: AnimationSpec) {
        let position = self.position();
        if (position - position.round()).abs() < 1e-4 {
            return;
        }
        let max = (self.page_count().saturating_sub(1)) as f32;
        let target = position.round().clamp(0.0, max);
        self.animate_to_position(target, spec);
    }

    fn animate_to_position(&self, target: f32, spec: AnimationSpec) {
        let mut inner = self.inner.borrow_mut();
        let current = inner.position.get();
        if (current - target).abs() < 1e-4 {
            inner.animation = None;
            inner.animation_target = target;
            inner.position.set_neq(target);
            return;
        }
        let mut animation = AnimatedValue::new(current, spec);
        animation.set_target(target);
        inner.animation = Some(animation);
        inner.animation_target = target;
        drop(inner);
        self.register_tick();
        request_frame();
    }

    fn register_tick(&self) {
        let inner = self.inner.clone();
        let key = self.tick_key.clone();
        animation_driver::register(
            key,
            Rc::new(RefCell::new(move || {
                let mut inner = inner.borrow_mut();
                let (still, value) = {
                    let Some(animation) = inner.animation.as_mut() else {
                        return false;
                    };
                    let still = animation.update();
                    (still, *animation.get())
                };
                inner.position.set_neq(value);
                if !still {
                    inner.position.set_neq(inner.animation_target);
                    inner.animation = None;
                }
                still
            })),
        );
    }
}

impl PagerState {
    fn set_page_extent(&self, px: f32) {
        self.page_extent.set(px);
    }
}

fn pager_pages(
    state: &PagerState,
    page_builder: &Rc<dyn Fn(usize) -> View>,
    config: &PagerConfig,
    scope: BoxWithConstraintsScope,
    horizontal: bool,
) -> View {
    let padding = config.content_padding;
    let (start_padding, end_padding, cross_padding) = if horizontal {
        (padding.left, padding.right, (padding.top, padding.bottom))
    } else {
        (padding.top, padding.bottom, (padding.left, padding.right))
    };
    let (extent, cross) = if horizontal {
        (scope.max_width, scope.max_height)
    } else {
        (scope.max_height, scope.max_width)
    };
    let extent = if extent.0.is_finite() { extent } else { Dp::ZERO };
    let cross = if cross.0.is_finite() { cross } else { Dp::ZERO };

    let page_extent = Dp((extent.0 - start_padding.0 - end_padding.0).max(0.0));
    state.set_page_extent(page_extent.to_px().0);
    let cross_extent = Dp((cross.0 - cross_padding.0.0 - cross_padding.1.0).max(0.0));
    let stride = page_extent.0 + config.page_spacing.0;

    let position = state.position();
    let count = state.page_count();
    let center = position.round() as isize;
    let beyond = config.beyond_viewport_page_count as isize;

    let mut pages = Vec::new();
    for index in (center - beyond)..=(center + beyond) {
        if index < 0 || index as usize >= count {
            continue;
        }
        let offset = Dp(start_padding.0 + (index as f32 - position) * stride);
        let placement = if horizontal {
            Modifier::new()
                .offset_left(offset)
                .offset_top(cross_padding.0)
                .width(page_extent)
                .height(cross_extent)
        } else {
            Modifier::new()
                .offset_left(cross_padding.0)
                .offset_top(offset)
                .width(cross_extent)
                .height(page_extent)
        };
        pages.push(
            crate::Box(placement.key(index as u64 + 1).absolute())
                .child(page_builder(index as usize)),
        );
    }

    crate::Box(Modifier::new().fill_max_size()).with_children(pages)
}

fn pager_handlers(
    state: &Rc<PagerState>,
    config: &PagerConfig,
    horizontal: bool,
) -> (
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
) {
    let threshold = config.fling_velocity_threshold;
    let spec = config.snap_spec.clone();
    let primary = move |event: &PointerEvent| {
        let position = event.position_in_window();
        if horizontal {
            (position.x, position.y)
        } else {
            (position.y, position.x)
        }
    };

    let down_state = state.clone();
    let down_primary = primary;
    let on_down = move |event: PointerEvent| {
        let (primary, cross) = down_primary(&event);
        down_state.begin_drag(primary, cross);
    };

    let move_state = state.clone();
    let move_primary = primary;
    let on_move = move |event: PointerEvent| {
        let (primary, cross) = move_primary(&event);
        move_state.drag(primary, cross);
    };

    let up_state = state.clone();
    let up_primary = primary;
    let up_spec = spec.clone();
    let on_up = move |event: PointerEvent| {
        let (primary, cross) = up_primary(&event);
        up_state.drag(primary, cross);
        up_state.end_drag(threshold, up_spec.clone());
    };

    let cancel_state = state.clone();
    let cancel_spec = spec;
    let on_cancel = move |_event: PointerEvent| {
        cancel_state.cancel_drag(cancel_spec.clone());
    };

    (on_down, on_move, on_up, on_cancel)
}

fn pager_view(
    state: Rc<PagerState>,
    page_builder: impl Fn(usize) -> View + 'static,
    config: PagerConfig,
    horizontal: bool,
) -> View {
    let position = state.position();
    let count = state.page_count();
    let builder: Rc<dyn Fn(usize) -> View> = Rc::new(page_builder);

    let mut modifier = Modifier::new().fill_max_size().then(config.modifier.clone());
    if config.user_scroll_enabled {
        let (on_down, on_move, on_up, on_cancel) = pager_handlers(&state, &config, horizontal);
        modifier = modifier
            .on_pointer_down(on_down)
            .on_pointer_move(on_move)
            .on_pointer_up(on_up)
            .on_pointer_cancel(on_cancel);
    }

    let content_state = state.clone();
    let content_config = config.clone();
    box_with_constraints_with_key(
        (position.to_bits(), count),
        modifier,
        move |scope| {
            pager_pages(
                &content_state,
                &builder,
                &content_config,
                scope,
                horizontal,
            )
        },
    )
}

/// A horizontally swipable pager with drag-following pages and snapping.
///
/// The pages around the settled page are laid out side by side and offset by
/// the continuous [`PagerState::position`], so a drag moves them under the
/// finger. Releasing snaps to the nearest page, or to the next one when the
/// release velocity exceeds
/// [`PagerConfig::fling_velocity_threshold`].
///
/// # Example
/// ```ignore
/// let state = Rc::new(PagerState::new(5));
/// HorizontalPager(
///     "demo",
///     state.clone(),
///     |page| Text(format!("Page {}", page + 1)).size(48.0),
///     PagerConfig {
///         modifier: Modifier::new().fill_max_width().height(300.0),
///         ..Default::default()
///     },
/// )
/// ```
#[allow(non_snake_case)]
pub fn HorizontalPager(
    _key: impl Into<String>,
    state: Rc<PagerState>,
    page_builder: impl Fn(usize) -> View + 'static,
    config: PagerConfig,
) -> View {
    pager_view(state, page_builder, config, true)
}

/// A vertically swipable pager with drag-following pages and snapping.
#[allow(non_snake_case)]
pub fn VerticalPager(
    _key: impl Into<String>,
    state: Rc<PagerState>,
    page_builder: impl Fn(usize) -> View + 'static,
    config: PagerConfig,
) -> View {
    pager_view(state, page_builder, config, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::LayoutEngine;
    use crate::Interactions;
    use std::collections::HashMap;

    fn page_rects(state: &Rc<PagerState>, width: u32) -> Vec<(Color, Rect)> {
        let state = state.clone();
        let root = HorizontalPager(
            "test",
            state,
            |page| {
                crate::Box(
                    Modifier::new()
                        .fill_max_size()
                        .background(Color::from_rgb(page as u8 * 10 + 1, 0, 0)),
                )
            },
            PagerConfig {
                modifier: Modifier::new().size(Dp(width as f32), Dp(200.0)),
                ..Default::default()
            },
        );
        let mut engine = LayoutEngine::new();
        let (scene, _, _) = engine.layout_frame(
            &root,
            (width, 200),
            &HashMap::new(),
            &Interactions::default(),
            None,
        );
        scene
            .nodes
            .iter()
            .filter_map(|node| match node {
                SceneNode::Rect { rect, brush, .. } => match brush {
                    Brush::Solid(color) if color.1 == 0 && color.2 == 0 => Some((*color, *rect)),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn pages_are_laid_out_around_the_position() {
        let state = Rc::new(PagerState::new(3));
        let settled = page_rects(&state, 300);
        let first = settled
            .iter()
            .find(|(color, _)| color.0 == 1)
            .map(|(_, rect)| *rect)
            .expect("page 0 painted");
        assert!(first.x.abs() < 0.5, "page 0 at {}", first.x);

        state.scroll_to_page(1);
        let scrolled = page_rects(&state, 300);
        let second = scrolled
            .iter()
            .find(|(color, _)| color.0 == 11)
            .map(|(_, rect)| *rect)
            .expect("page 1 painted");
        assert!(second.x.abs() < 0.5, "page 1 at {}", second.x);
    }

    #[test]
    fn drag_moves_pages_with_the_finger() {
        let state = Rc::new(PagerState::new(3));
        page_rects(&state, 300);

        state.begin_drag(0.0, 0.0);
        state.drag(-150.0, 0.0);
        assert!((state.position() - 0.5).abs() < 1e-4, "{}", state.position());

        let dragged = page_rects(&state, 300);
        let first = dragged
            .iter()
            .find(|(color, _)| color.0 == 1)
            .map(|(_, rect)| *rect)
            .expect("page 0 painted");
        let second = dragged
            .iter()
            .find(|(color, _)| color.0 == 11)
            .map(|(_, rect)| *rect)
            .expect("page 1 painted");
        assert!((first.x + 150.0).abs() < 0.5, "page 0 at {}", first.x);
        assert!((second.x - 150.0).abs() < 0.5, "page 1 at {}", second.x);

        state.end_drag(400.0, AnimationSpec::spring_gentle());
        assert_eq!(state.current_page(), 1);
    }

    #[test]
    fn a_fast_flick_advances_even_below_halfway() {
        let state = Rc::new(PagerState::new(3));
        page_rects(&state, 300);
        state.begin_drag(0.0, 0.0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        state.drag(-30.0, 0.0);
        assert!(state.position() < 0.2, "{}", state.position());
        state.end_drag(400.0, AnimationSpec::spring_gentle());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.is_animating() && std::time::Instant::now() < deadline {
            animation_driver::tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(state.current_page(), 1);
    }

    #[test]
    fn a_fast_flick_back_returns_to_the_previous_page() {
        let state = Rc::new(PagerState::new(3));
        state.scroll_to_page(2);
        page_rects(&state, 300);
        state.begin_drag(0.0, 0.0);
        std::thread::sleep(std::time::Duration::from_millis(10));
        state.drag(30.0, 0.0);
        assert!(state.position() > 1.8, "{}", state.position());
        state.end_drag(400.0, AnimationSpec::spring_gentle());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.is_animating() && std::time::Instant::now() < deadline {
            animation_driver::tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(state.current_page(), 1);
    }

    #[test]
    fn a_tap_during_a_snap_settles_on_a_page() {
        let state = Rc::new(PagerState::new(3));
        page_rects(&state, 300);
        state.animate_scroll_to_page(2, AnimationSpec::spring_gentle());
        for _ in 0..4 {
            animation_driver::tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(state.position() > 0.0 && state.position() < 2.0);

        state.begin_drag(0.0, 0.0);
        state.end_drag(400.0, AnimationSpec::spring_gentle());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.is_animating() && std::time::Instant::now() < deadline {
            animation_driver::tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let position = state.position();
        assert!(
            (position - position.round()).abs() < 1e-3,
            "left stranded at {position}"
        );
    }

    #[test]
    fn page_state_survives_a_drag() {
        let state = Rc::new(PagerState::new(3));
        let last_build: Rc<std::cell::Cell<usize>> = Rc::new(std::cell::Cell::new(0));
        let build = |state: Rc<PagerState>, last_build: Rc<std::cell::Cell<usize>>| {
            HorizontalPager(
                "stateful",
                state,
                move |page| {
                    let count =
                        remember_state_with_key(format!("pager_page_builds:{page}"), || 0usize);
                    *count.borrow_mut() += 1;
                    if page == 0 {
                        last_build.set(*count.borrow());
                    }
                    crate::Box(
                        Modifier::new()
                            .fill_max_size()
                            .background(Color::from_rgb(page as u8, 0, 0)),
                    )
                },
                PagerConfig {
                    modifier: Modifier::new().size(Dp(300.0), Dp(200.0)),
                    ..Default::default()
                },
            )
        };

        let mut engine = LayoutEngine::new();
        let mut render = |view: &View| {
            engine.layout_frame(
                view,
                (300, 200),
                &HashMap::new(),
                &Interactions::default(),
                None,
            );
        };
        render(&build(state.clone(), last_build.clone()));
        assert_eq!(last_build.get(), 1);
        state.scroll_by_pages(0.5);
        render(&build(state.clone(), last_build.clone()));
        assert_eq!(last_build.get(), 2, "page state reset during drag");
        state.scroll_by_pages(0.5);
        render(&build(state.clone(), last_build.clone()));
        assert_eq!(last_build.get(), 3, "page state reset after drag");
    }

    #[test]
    fn release_below_halfway_snaps_back() {
        let state = Rc::new(PagerState::new(3));
        page_rects(&state, 300);
        state.begin_drag(0.0, 0.0);
        state.drag(-60.0, 0.0);
        state.end_drag(400.0, AnimationSpec::spring_gentle());
        assert_eq!(state.current_page(), 0);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.is_animating() && std::time::Instant::now() < deadline {
            animation_driver::tick();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!state.is_animating(), "snap did not settle");
        assert!((state.position()).abs() < 1e-3, "{}", state.position());
        let settled = page_rects(&state, 300);
        let first = settled
            .iter()
            .find(|(color, _)| color.0 == 1)
            .map(|(_, rect)| *rect)
            .expect("page 0 painted");
        assert!(first.x.abs() < 0.5, "page 0 at {}", first.x);
    }
}
