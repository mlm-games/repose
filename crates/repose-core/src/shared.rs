use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::animation::{AnimatedValue, AnimationSpec};
use crate::animation_driver;
use crate::geometry::Rect;
use crate::request_frame;
use crate::runtime::{ComposerSlots, replace_composer_slots};
use crate::scope::Scope;
use crate::scope_cache::with_scope_key;
use crate::view::View;

static NEXT_MOVABLE_ID: AtomicU64 = AtomicU64::new(0);

/// State-preserving content that can move between composition locations.
///
/// Create one [`MovableContent::new`] and keep the handle alive (in
/// `remember`, a `remember_mutable`, or a field), then call
/// [`invoke`](Self::invoke) at the location where it should compose.
/// Remembered state inside the content follows the handle instead of the call
/// site, so moving the invocation preserves it.
pub struct MovableContent<T: 'static> {
    inner: Rc<MovableInner<T>>,
}

struct MovableInner<T: 'static> {
    content: Box<dyn Fn(T) -> View>,
    scope: Scope,
    key: String,
    slots: RefCell<ComposerSlots>,
}

impl<T: 'static> MovableContent<T> {
    pub fn new(content: impl Fn(T) -> View + 'static) -> Self {
        let id = NEXT_MOVABLE_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Rc::new(MovableInner {
                content: Box::new(content),
                scope: Scope::new(),
                key: format!("movable:{id}"),
                slots: RefCell::new(ComposerSlots::default()),
            }),
        }
    }

    pub fn invoke(&self, value: T) -> View {
        let inner = &*self.inner;
        let content = &inner.content;
        let key = inner.key.as_str();
        let slots = &mut *inner.slots.borrow_mut();
        with_own_slots(slots, || {
            inner
                .scope
                .run(|| with_scope_key(key, || content(value)))
        })
    }
}

impl<T: 'static> Clone for MovableContent<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T: 'static> Drop for MovableInner<T> {
    fn drop(&mut self) {
        self.scope.clone().dispose();
    }
}

fn with_own_slots<R>(target: &mut ComposerSlots, f: impl FnOnce() -> R) -> R {
    target.cursor = 0;
    target.auto_counts.clear();
    let outer = replace_composer_slots(std::mem::take(target));
    struct Restore<'a> {
        target: &'a mut ComposerSlots,
        outer: Option<ComposerSlots>,
    }
    impl Drop for Restore<'_> {
        fn drop(&mut self) {
            let outer = self.outer.take().expect("movable slots restored twice");
            let mut inner = replace_composer_slots(outer);
            inner.slots.truncate(inner.cursor);
            inner.slot_callers.truncate(inner.cursor);
            *self.target = inner;
        }
    }
    let _restore = Restore {
        target,
        outer: Some(outer),
    };
    f()
}

static NEXT_SHARED_ID: AtomicU64 = AtomicU64::new(0);

/// Identity and bounds animation shared by one or more elements.
///
/// Attach it with [`Modifier::shared_element`](crate::Modifier::shared_element).
/// When the node that owns the state changes location, or a different node
/// claims the same state, the painted element eases from the last bounds to the
/// new layout bounds. When several nodes claim one state in a frame, the last
/// painted one drives the transition and the others are not painted.
#[derive(Clone)]
pub struct SharedElementState {
    inner: Rc<SharedElementInner>,
}

struct SharedElementInner {
    id: u64,
    spec: AnimationSpec,
    animation: RefCell<AnimatedValue<Rect>>,
    target: Cell<Rect>,
    known: Cell<bool>,
    tick_key: String,
}

impl SharedElementState {
    pub fn new() -> Self {
        Self::with_spec(AnimationSpec::spring_gentle())
    }

    pub fn with_spec(spec: AnimationSpec) -> Self {
        let id = NEXT_SHARED_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Rc::new(SharedElementInner {
                id,
                spec,
                animation: RefCell::new(AnimatedValue::new(Rect::default(), spec)),
                target: Cell::new(Rect::default()),
                known: Cell::new(false),
                tick_key: format!("shared_element:{id}"),
            }),
        }
    }

    pub fn id(&self) -> u64 {
        self.inner.id
    }

    #[doc(hidden)]
    pub fn prepare(&self, bounds: Rect) -> Rect {
        let inner = &*self.inner;
        if !inner.known.replace(true) {
            let mut animation = inner.animation.borrow_mut();
            *animation = AnimatedValue::new(bounds, inner.spec);
            inner.target.set(bounds);
            return bounds;
        }

        if !rect_close(inner.target.get(), bounds) {
            inner.target.set(bounds);
            let mut animation = inner.animation.borrow_mut();
            animation.set_target(bounds);
            drop(animation);
            self.register_tick();
        }

        let animation = inner.animation.borrow();
        let current = *animation.get();
        let animating = animation.is_animating();
        drop(animation);
        if animating {
            request_frame();
        }
        current
    }

    #[doc(hidden)]
    pub fn is_animating(&self) -> bool {
        self.inner.animation.borrow().is_animating()
    }

    fn register_tick(&self) {
        let inner = self.inner.clone();
        let key = self.inner.tick_key.clone();
        animation_driver::register(
            key,
            Rc::new(RefCell::new(move || {
                let still = inner.animation.borrow_mut().update();
                if still {
                    request_frame();
                }
                still
            })),
        );
    }
}

impl Default for SharedElementState {
    fn default() -> Self {
        Self::new()
    }
}

fn rect_close(a: Rect, b: Rect) -> bool {
    (a.x - b.x).abs() <= 0.01
        && (a.y - b.y).abs() <= 0.01
        && (a.w - b.w).abs() <= 0.01
        && (a.h - b.h).abs() <= 0.01
}

