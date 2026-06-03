use repose_core::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::anim_ext::{AnimatedContent, EnterTransition, ExitTransition};

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
    modifier: Modifier,
    page_builder: impl Fn(usize) -> View + 'static,
) -> View {
    let key = key.into();
    let page = state.current_page.get();

    // Drag-to-swipe gesture handling
    let drag_start_x = Rc::new(remember_with_key(format!("pager_drag:{key}"), || {
        RefCell::new(None::<f32>)
    }));

    let on_down = {
        let d = drag_start_x.clone();
        move |e: PointerEvent| {
            *d.borrow_mut() = Some(e.position.x);
        }
    };

    let st = state.clone();
    let on_up = {
        let d = drag_start_x.clone();
        move |e: PointerEvent| {
            if let Some(start_x) = *d.borrow() {
                let delta = e.position.x - start_x;
                let threshold = 50.0;
                if delta.abs() > threshold {
                    if delta < 0.0 {
                        let next =
                            (st.current_page.get() + 1).min(st.page_count.get().saturating_sub(1));
                        st.current_page.set(next);
                    } else {
                        let prev = st.current_page.get().saturating_sub(1);
                        st.current_page.set(prev);
                    }
                }
            }
            *d.borrow_mut() = None;
        }
    };

    let content = AnimatedContent(
        format!("page_content:{key}"),
        page,
        AnimationSpec::spring_gentle(),
        EnterTransition::Composite(vec![
            EnterTransition::FadeIn,
            EnterTransition::SlideIn {
                offset_x: 800.0,
                offset_y: 0.0,
            },
        ]),
        ExitTransition::Composite(vec![
            ExitTransition::FadeOut,
            ExitTransition::SlideOut {
                offset_x: -800.0,
                offset_y: 0.0,
            },
        ]),
        move |p| page_builder(p),
    );

    crate::ZStack(Modifier::new().fill_max_size().then(modifier)).with_children(vec![
        crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        crate::Box(
            Modifier::new()
                .fill_max_size()
                .hit_passthrough()
                .on_pointer_down(on_down)
                .on_pointer_up(on_up),
        ),
    ])
}

/// A vertically swipable pager with animated page transitions.
///
/// Mirror of `HorizontalPager` - drag up/down to flip pages.
#[allow(non_snake_case)]
pub fn VerticalPager(
    key: impl Into<String>,
    state: Rc<PagerState>,
    modifier: Modifier,
    page_builder: impl Fn(usize) -> View + 'static,
) -> View {
    let key = key.into();
    let page = state.current_page.get();

    let drag_start_y = Rc::new(remember_with_key(format!("vpager_drag:{key}"), || {
        RefCell::new(None::<f32>)
    }));

    let on_down = {
        let d = drag_start_y.clone();
        move |e: PointerEvent| {
            *d.borrow_mut() = Some(e.position.y);
        }
    };

    let st = state.clone();
    let on_up = {
        let d = drag_start_y.clone();
        move |e: PointerEvent| {
            if let Some(start_y) = *d.borrow() {
                let delta = e.position.y - start_y;
                let threshold = 50.0;
                if delta.abs() > threshold {
                    if delta < 0.0 {
                        let next =
                            (st.current_page.get() + 1).min(st.page_count.get().saturating_sub(1));
                        st.current_page.set(next);
                    } else {
                        let prev = st.current_page.get().saturating_sub(1);
                        st.current_page.set(prev);
                    }
                }
            }
            *d.borrow_mut() = None;
        }
    };

    let content = AnimatedContent(
        format!("vpage_content:{key}"),
        page,
        AnimationSpec::spring_gentle(),
        EnterTransition::Composite(vec![
            EnterTransition::FadeIn,
            EnterTransition::SlideIn {
                offset_x: 0.0,
                offset_y: 600.0,
            },
        ]),
        ExitTransition::Composite(vec![
            ExitTransition::FadeOut,
            ExitTransition::SlideOut {
                offset_x: 0.0,
                offset_y: -600.0,
            },
        ]),
        move |p| page_builder(p),
    );

    crate::ZStack(Modifier::new().fill_max_size().then(modifier)).with_children(vec![
        crate::Box(Modifier::new().fill_max_size()).with_children(vec![content]),
        crate::Box(
            Modifier::new()
                .fill_max_size()
                .hit_passthrough()
                .on_pointer_down(on_down)
                .on_pointer_up(on_up),
        ),
    ])
}
