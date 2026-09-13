use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::anim_ext::{AnimatedContent, AnimatedContentConfig, EnterTransition, ExitTransition};

/// Configuration for [`HorizontalPager`] and [`VerticalPager`].
#[derive(Clone)]
pub struct PagerConfig {
    pub modifier: Modifier,
    pub page_spacing: Dp,
    pub user_scroll_enabled: bool,
    pub content_padding: PaddingValues,
}

impl Default for PagerConfig {
    fn default() -> Self {
        Self {
            modifier: Modifier::new(),
            page_spacing: Dp::ZERO,
            user_scroll_enabled: true,
            content_padding: PaddingValues::default(),
        }
    }
}

/// Shared swipe-gesture state for pagers: start point + per-gesture handling
/// with axis lock, density-scaled threshold, and cancel safety.
struct PagerDrag {
    start: Option<(f32, f32)>,
}

fn pager_threshold_px() -> f32 {
    Dp(24.0).to_px().0.max(8.0)
}

/// Build down/up/cancel handlers for a horizontal pager.
fn horizontal_handlers(
    key: &str,
    state: &Rc<PagerState>,
) -> (
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
) {
    let drag = Rc::new(remember_with_key(format!("pager_drag:{key}"), || {
        RefCell::new(PagerDrag { start: None })
    }));
    let st = state.clone();
    let on_down = {
        let drag = drag.clone();
        move |e: PointerEvent| {
            drag.borrow_mut().start = Some((e.position.x, e.position.y));
        }
    };
    let on_up = {
        let drag = drag.clone();
        move |e: PointerEvent| {
            if let Some((sx, sy)) = drag.borrow().start {
                let dx = e.position.x - sx;
                let dy = e.position.y - sy;
                let threshold = pager_threshold_px();
                if dx.abs() > threshold && dx.abs() > dy.abs() * 1.5 {
                    if dx < 0.0 {
                        let next =
                            (st.current_page.get() + 1).min(st.page_count.get().saturating_sub(1));
                        st.current_page.set(next);
                    } else {
                        let prev = st.current_page.get().saturating_sub(1);
                        st.current_page.set(prev);
                    }
                }
            }
            drag.borrow_mut().start = None;
        }
    };
    let on_cancel = {
        let drag = drag.clone();
        move |_: PointerEvent| {
            drag.borrow_mut().start = None;
        }
    };
    (on_down, on_up, on_cancel)
}

fn vertical_handlers(
    key: &str,
    state: &Rc<PagerState>,
) -> (
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
    impl Fn(PointerEvent) + Clone + 'static,
) {
    let drag = Rc::new(remember_with_key(format!("vpager_drag:{key}"), || {
        RefCell::new(PagerDrag { start: None })
    }));
    let st = state.clone();
    let on_down = {
        let drag = drag.clone();
        move |e: PointerEvent| {
            drag.borrow_mut().start = Some((e.position.x, e.position.y));
        }
    };
    let on_up = {
        let drag = drag.clone();
        move |e: PointerEvent| {
            if let Some((sx, sy)) = drag.borrow().start {
                let dx = e.position.x - sx;
                let dy = e.position.y - sy;
                let threshold = pager_threshold_px();
                if dy.abs() > threshold && dy.abs() > dx.abs() * 1.5 {
                    if dy < 0.0 {
                        let next =
                            (st.current_page.get() + 1).min(st.page_count.get().saturating_sub(1));
                        st.current_page.set(next);
                    } else {
                        let prev = st.current_page.get().saturating_sub(1);
                        st.current_page.set(prev);
                    }
                }
            }
            drag.borrow_mut().start = None;
        }
    };
    let on_cancel = {
        let drag = drag.clone();
        move |_: PointerEvent| {
            drag.borrow_mut().start = None;
        }
    };
    (on_down, on_up, on_cancel)
}

/// State for a horizontal pager with page snapping.
pub struct PagerState {
    current_page: Signal<usize>,
    page_count: Signal<usize>,
}

impl PagerState {
    pub fn new(page_count: usize) -> Self {
        Self {
            current_page: signal(0),
            page_count: signal(page_count.max(1)),
        }
    }

    pub fn current_page(&self) -> usize {
        self.current_page.get()
    }

    /// Programmatically set the current page (with animation).
    pub fn set_page(&self, page: usize) {
        let max_page = self.page_count.get().saturating_sub(1);
        self.current_page.set(page.min(max_page));
    }

    pub fn page_count(&self) -> usize {
        self.page_count.get()
    }
}

/// A horizontally swipable pager with animated page transitions.
///
/// Supports both programmatic page changes via `state.set_page()` and
/// drag/swipe gestures for page flipping.
///
/// Renders only the current page (previous page fades/slides out,
/// new page fades/slides in).
///
/// # Example
/// ```ignore
/// let state = Rc::new(PagerState::new(5));
/// HorizontalPager(
///     "demo",
///     state.clone(),
///     Modifier::new().fill_max_width().height(300.0),
///     |page| Text(format!("Page {}", page + 1)).size(48.0),
/// )
/// ```
#[allow(non_snake_case)]
pub fn HorizontalPager(
    key: impl Into<String>,
    state: Rc<PagerState>,
    page_builder: impl Fn(usize) -> View + 'static,
    config: PagerConfig,
) -> View {
    let key = key.into();
    let page = state.current_page.get();
    let page_spacing = config.page_spacing;
    let slide_offset = Dp(800.0) + page_spacing;

    let (on_down, on_up, on_cancel) = horizontal_handlers(&key, &state);

    let content = AnimatedContent(
        page,
        page_builder,
        AnimatedContentConfig {
            key: format!("page_content:{key}"),
            spec: AnimationSpec::spring_gentle(),
            enter: EnterTransition::Composite(vec![
                EnterTransition::FadeIn,
                EnterTransition::SlideIn {
                    offset_x: slide_offset,
                    offset_y: Dp::ZERO,
                },
            ]),
            exit: ExitTransition::Composite(vec![
                ExitTransition::FadeOut,
                ExitTransition::SlideOut {
                    offset_x: -(slide_offset),
                    offset_y: Dp::ZERO,
                },
            ]),
        },
    );

    let pager_mod = Modifier::new().fill_max_size().then(config.modifier);
    if config.user_scroll_enabled {
        crate::Box(
            pager_mod
                .on_pointer_down(on_down)
                .on_pointer_up(on_up)
                .on_pointer_cancel(on_cancel),
        )
        .with_children(vec![
            crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        ])
    } else {
        crate::Box(pager_mod).with_children(vec![
            crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        ])
    }
}

/// A vertically swipable pager with animated page transitions.
///
/// Mirror of `HorizontalPager` - drag up/down to flip pages.
#[allow(non_snake_case)]
pub fn VerticalPager(
    key: impl Into<String>,
    state: Rc<PagerState>,
    page_builder: impl Fn(usize) -> View + 'static,
    config: PagerConfig,
) -> View {
    let key = key.into();
    let page = state.current_page.get();
    let page_spacing = config.page_spacing;
    let slide_offset = Dp(600.0) + page_spacing;

    let (on_down, on_up, on_cancel) = vertical_handlers(&key, &state);

    let content = AnimatedContent(
        page,
        page_builder,
        AnimatedContentConfig {
            key: format!("vpage_content:{key}"),
            spec: AnimationSpec::spring_gentle(),
            enter: EnterTransition::Composite(vec![
                EnterTransition::FadeIn,
                EnterTransition::SlideIn {
                    offset_x: Dp::ZERO,
                    offset_y: slide_offset,
                },
            ]),
            exit: ExitTransition::Composite(vec![
                ExitTransition::FadeOut,
                ExitTransition::SlideOut {
                    offset_x: Dp::ZERO,
                    offset_y: -(slide_offset),
                },
            ]),
        },
    );

    let pager_mod = Modifier::new().fill_max_size().then(config.modifier);
    if config.user_scroll_enabled {
        crate::Box(
            pager_mod
                .on_pointer_down(on_down)
                .on_pointer_up(on_up)
                .on_pointer_cancel(on_cancel),
        )
        .with_children(vec![
            crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        ])
    } else {
        crate::Box(pager_mod).with_children(vec![
            crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        ])
    }
}
