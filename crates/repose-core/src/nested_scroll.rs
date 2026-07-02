use std::rc::Rc;

use crate::Vec2;

/// Possible sources of scroll events in the nested scroll system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NestedScrollSource {
    UserInput,
    SideEffect,
}

pub type PreScrollFn = Rc<dyn Fn(Vec2, NestedScrollSource) -> Vec2>;
pub type PostScrollFn = Rc<dyn Fn(Vec2, Vec2, NestedScrollSource) -> Vec2>;

#[derive(Clone, Default)]
pub struct NestedScrollConnection {
    pub on_pre_scroll: Option<PreScrollFn>,
    pub on_post_scroll: Option<PostScrollFn>,
    pub on_pre_fling: Option<PreScrollFn>,
    pub on_post_fling: Option<PostScrollFn>,
}

impl std::fmt::Debug for NestedScrollConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NestedScrollConnection")
            .field("has_pre_scroll", &self.on_pre_scroll.is_some())
            .field("has_post_scroll", &self.on_post_scroll.is_some())
            .field("has_pre_fling", &self.on_pre_fling.is_some())
            .field("has_post_fling", &self.on_post_fling.is_some())
            .finish()
    }
}

impl NestedScrollConnection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_pre_scroll(mut self, f: impl Fn(Vec2, NestedScrollSource) -> Vec2 + 'static) -> Self {
        self.on_pre_scroll = Some(Rc::new(f));
        self
    }

    pub fn on_post_scroll(
        mut self,
        f: impl Fn(Vec2, Vec2, NestedScrollSource) -> Vec2 + 'static,
    ) -> Self {
        self.on_post_scroll = Some(Rc::new(f));
        self
    }

    pub fn on_pre_fling(mut self, f: impl Fn(Vec2, NestedScrollSource) -> Vec2 + 'static) -> Self {
        self.on_pre_fling = Some(Rc::new(f));
        self
    }

    pub fn on_post_fling(
        mut self,
        f: impl Fn(Vec2, Vec2, NestedScrollSource) -> Vec2 + 'static,
    ) -> Self {
        self.on_post_fling = Some(Rc::new(f));
        self
    }

    pub fn dispatch_pre_scroll(&self, available: Vec2, source: NestedScrollSource) -> Vec2 {
        self.on_pre_scroll
            .as_ref()
            .map(|f| f(available, source))
            .unwrap_or(Vec2::ZERO)
    }

    pub fn dispatch_post_scroll(
        &self,
        consumed: Vec2,
        available: Vec2,
        source: NestedScrollSource,
    ) -> Vec2 {
        self.on_post_scroll
            .as_ref()
            .map(|f| f(consumed, available, source))
            .unwrap_or(Vec2::ZERO)
    }

    pub fn dispatch_pre_fling(&self, available: Vec2, source: NestedScrollSource) -> Vec2 {
        self.on_pre_fling
            .as_ref()
            .map(|f| f(available, source))
            .unwrap_or(Vec2::ZERO)
    }

    pub fn dispatch_post_fling(
        &self,
        consumed: Vec2,
        available: Vec2,
        source: NestedScrollSource,
    ) -> Vec2 {
        self.on_post_fling
            .as_ref()
            .map(|f| f(consumed, available, source))
            .unwrap_or(Vec2::ZERO)
    }

    pub fn then(&self, further: &Self) -> Self {
        let on_pre_scroll: Option<PreScrollFn> = match (&self.on_pre_scroll, &further.on_pre_scroll)
        {
            (Some(close), Some(far)) => {
                let close = close.clone();
                let far = far.clone();
                Some(Rc::new(move |v, s| {
                    let fc = far(v, s);
                    let cc = close(v - fc, s);
                    fc + cc
                }))
            }
            (Some(close), None) => Some(close.clone()),
            (None, Some(far)) => Some(far.clone()),
            (None, None) => None,
        };
        let on_post_scroll: Option<PostScrollFn> =
            match (&self.on_post_scroll, &further.on_post_scroll) {
                (Some(close), Some(far)) => {
                    let close = close.clone();
                    let far = far.clone();
                    Some(Rc::new(move |consumed, available, s| {
                        let cc = close(consumed, available, s);
                        let fc = far(consumed + cc, available - cc, s);
                        cc + fc
                    }))
                }
                (Some(close), None) => Some(close.clone()),
                (None, Some(far)) => Some(far.clone()),
                (None, None) => None,
            };
        let on_pre_fling: Option<PreScrollFn> = match (&self.on_pre_fling, &further.on_pre_fling) {
            (Some(close), Some(far)) => {
                let close = close.clone();
                let far = far.clone();
                Some(Rc::new(move |v, s| {
                    let fc = far(v, s);
                    let cc = close(v - fc, s);
                    fc + cc
                }))
            }
            (Some(close), None) => Some(close.clone()),
            (None, Some(far)) => Some(far.clone()),
            (None, None) => None,
        };
        let on_post_fling: Option<PostScrollFn> = match (&self.on_post_fling, &further.on_post_fling)
        {
            (Some(close), Some(far)) => {
                let close = close.clone();
                let far = far.clone();
                Some(Rc::new(move |consumed, available, s| {
                    let cc = close(consumed, available, s);
                    let fc = far(consumed + cc, available - cc, s);
                    cc + fc
                }))
            }
            (Some(close), None) => Some(close.clone()),
            (None, Some(far)) => Some(far.clone()),
            (None, None) => None,
        };
        Self {
            on_pre_scroll,
            on_post_scroll,
            on_pre_fling,
            on_post_fling,
        }
    }
}
