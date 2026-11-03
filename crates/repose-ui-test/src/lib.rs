use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use repose_app::{ReposeRuntime, compose_frame_inner_with_ancestors};
use repose_core::animation::{ManualClock, SystemClock, set_clock};
use repose_core::animation_driver;
use repose_core::input::PointerButton;
use repose_core::runtime::{Frame, Scheduler, SemNode};
use repose_core::semantics::Role;
use repose_core::{Rect, Vec2, View};
use repose_ui::{hit_region_transformed_origin, hit_test_frame_path};

const FRAME_INTERVAL: Duration = Duration::from_millis(16);
const EVENT_INTERVAL: Duration = Duration::from_millis(16);
const MAX_IDLE_FRAMES: usize = 600;

struct UiTestInner {
    runtime: ReposeRuntime,
    size: (u32, u32),
    pressed: HashSet<u64>,
    frame: Option<Rc<Frame>>,
}

/// Deterministic, headless driver for a Repose view tree.
///
/// Time only moves when asked: [`pump`](Self::pump),
/// [`advance_time_by`](Self::advance_time_by) and
/// [`advance_until_idle`](Self::advance_until_idle) are the frame clock.
/// Animations read a [`ManualClock`], so springs and transitions behave the
/// same on every run instead of depending on wall-clock speed.
pub struct UiTest {
    clock: ManualClock,
    content: RefCell<Box<dyn FnMut() -> View>>,
    inner: RefCell<UiTestInner>,
    auto_advance: Cell<bool>,
    next_pointer: Cell<u64>,
}

impl UiTest {
    /// Compose `content` once at `size_px` with a fresh layout engine.
    pub fn new(size_px: (u32, u32), content: impl FnMut() -> View + 'static) -> Self {
        repose_core::runtime::shutdown_composition();
        repose_ui::reset_layout_engine();

        let clock = ManualClock::new(web_time::Instant::now());
        set_clock(Box::new(clock.clone()));

        let mut runtime = ReposeRuntime::new();
        runtime.set_viewport_and_scale(size_px.0, size_px.1, 1.0);
        runtime.sched.size = size_px;

        let test = Self {
            clock,
            content: RefCell::new(Box::new(content)),
            inner: RefCell::new(UiTestInner {
                runtime,
                size: size_px,
                pressed: HashSet::new(),
                frame: None,
            }),
            auto_advance: Cell::new(false),
            next_pointer: Cell::new(0),
        };
        test.pump_now();
        test
    }

    /// Viewport size in physical px (density scale is `1.0`).
    pub fn size(&self) -> (u32, u32) {
        self.inner.borrow().size
    }

    /// Resize the viewport and compose immediately.
    pub fn set_viewport(&self, width_px: u32, height_px: u32) {
        let mut inner = self.inner.borrow_mut();
        inner.size = (width_px, height_px);
        inner
            .runtime
            .set_viewport_and_scale(width_px, height_px, 1.0);
        drop(inner);
        self.pump_now();
    }

    /// Whether a plain [`pump`](Self::pump) advances the clock by one frame.
    /// Defaults to `false`; deterministic tests advance time explicitly.
    pub fn set_auto_advance(&self, enabled: bool) {
        self.auto_advance.set(enabled);
    }

    pub fn auto_advance(&self) -> bool {
        self.auto_advance.get()
    }

    /// Compose one frame at the current clock value.
    pub fn pump(&self) {
        if self.auto_advance.get() {
            self.clock.advance(FRAME_INTERVAL);
        }
        self.pump_now();
    }

    /// Advance the animation clock by `duration`, composing on every frame step.
    pub fn advance_time_by(&self, duration: Duration) {
        if duration.is_zero() {
            self.pump_now();
            return;
        }
        let mut remaining = duration;
        while !remaining.is_zero() {
            let step = remaining.min(FRAME_INTERVAL);
            self.clock.advance(step);
            self.pump_now();
            remaining = remaining.saturating_sub(step);
        }
    }

    /// Compose frames until no animation driver tick is registered.
    ///
    /// Panics after roughly ten simulated seconds so an endless animation
    /// fails the test instead of hanging it.
    pub fn advance_until_idle(&self) {
        self.pump_now();
        let mut frames = 0;
        while animation_driver::is_active() {
            self.clock.advance(FRAME_INTERVAL);
            self.pump_now();
            frames += 1;
            assert!(
                frames < MAX_IDLE_FRAMES,
                "advance_until_idle did not settle after {MAX_IDLE_FRAMES} frames"
            );
        }
    }

    pub fn is_idle(&self) -> bool {
        !animation_driver::is_active()
    }

    /// The most recently composed frame.
    pub fn frame(&self) -> Rc<Frame> {
        self.inner
            .borrow()
            .frame
            .clone()
            .expect("UiTest composes a frame on creation")
    }

    pub fn on_node_with_tag(&self, tag: impl Into<String>) -> UiNode<'_> {
        UiNode {
            test: self,
            matcher: NodeMatcher::Tag(tag.into()),
        }
    }

    pub fn on_node_with_text(&self, text: impl Into<String>) -> UiNode<'_> {
        UiNode {
            test: self,
            matcher: NodeMatcher::Text(text.into()),
        }
    }

    pub fn on_node_with_role(&self, role: Role) -> UiNode<'_> {
        UiNode {
            test: self,
            matcher: NodeMatcher::Role(role),
        }
    }

    fn pump_now(&self) {
        let mut content = self.content.borrow_mut();
        let mut inner = self.inner.borrow_mut();
        let inner = &mut *inner;

        animation_driver::tick();

        let scale = inner.runtime.scale;
        let size = inner.size;
        let no_hover = HashSet::new();
        let mut root = |_: &mut Scheduler| (content)();
        let frame = Rc::new(compose_frame_inner_with_ancestors(
            &mut inner.runtime.sched,
            &mut root,
            scale,
            size,
            None,
            &no_hover,
            &inner.pressed,
            &inner.runtime.textfield_states,
        ));

        inner.runtime.cache_frame(frame.clone());
        inner.runtime.ensure_focused_textfield_state();
        inner.frame = Some(frame);
    }

    fn next_pointer(&self) -> u64 {
        let id = self.next_pointer.get();
        self.next_pointer.set(id + 1);
        id
    }

    fn touch_press(&self, pointer: u64, position: Vec2) {
        let mut inner = self.inner.borrow_mut();
        let result =
            inner
                .runtime
                .handle_touch_press(Some(pointer), position, PointerButton::Primary);
        if let Some(capture) = result.capture_id {
            inner.pressed.insert(capture);
        }
    }

    fn touch_move(&self, pointer: u64, position: Vec2) {
        self.inner
            .borrow_mut()
            .runtime
            .handle_touch_move(Some(pointer), position);
    }

    fn touch_release(&self, pointer: u64, position: Vec2) {
        let mut inner = self.inner.borrow_mut();
        inner
            .runtime
            .handle_touch_release(Some(pointer), position, PointerButton::Primary);
        inner.pressed.clear();
    }

    fn touch_cancel(&self, pointer: u64) {
        let mut inner = self.inner.borrow_mut();
        inner.runtime.handle_touch_cancel(Some(pointer));
        inner.pressed.clear();
    }
}

impl Drop for UiTest {
    fn drop(&mut self) {
        animation_driver::shutdown();
        set_clock(Box::new(SystemClock));
        repose_core::back::set_gesture(false, 0.0);
        let _ = repose_core::back::take_committed();
    }
}

enum NodeMatcher {
    Tag(String),
    Text(String),
    Role(Role),
}

impl NodeMatcher {
    fn matches(&self, node: &SemNode) -> bool {
        match self {
            NodeMatcher::Tag(tag) => node.test_tag.as_deref() == Some(tag.as_str()),
            NodeMatcher::Text(text) => {
                node.label.as_deref() == Some(text.as_str())
                    || node.value.as_deref() == Some(text.as_str())
            }
            NodeMatcher::Role(role) => node.role == *role,
        }
    }

    fn describe(&self) -> String {
        match self {
            NodeMatcher::Tag(tag) => format!("testTag={tag:?}"),
            NodeMatcher::Text(text) => format!("text={text:?}"),
            NodeMatcher::Role(role) => format!("role={role:?}"),
        }
    }
}

/// A lazy handle to one semantics node in the latest frame.
///
/// Every method re-resolves the matcher against the current frame, so the
/// handle stays valid across recompositions.
pub struct UiNode<'a> {
    test: &'a UiTest,
    matcher: NodeMatcher,
}

impl UiNode<'_> {
    fn all(&self) -> Vec<SemNode> {
        self.test
            .frame()
            .semantics_nodes
            .iter()
            .filter(|node| self.matcher.matches(node))
            .cloned()
            .collect()
    }

    fn one(&self) -> SemNode {
        let mut nodes = self.all();
        match nodes.len() {
            1 => nodes.remove(0),
            0 => panic!("no node found for {}", self.matcher.describe()),
            count => panic!(
                "expected one node for {} but found {count}",
                self.matcher.describe()
            ),
        }
    }

    /// Window-space rect. Uses the hit region when the tagged node creates
    /// one, so transforms and clipping are included.
    pub fn bounds_in_window(&self) -> Rect {
        let node = self.one();
        if let NodeMatcher::Tag(tag) = &self.matcher
            && let Some(hit) = self
                .test
                .frame()
                .hit_regions
                .iter()
                .find(|hit| hit.test_tag.as_deref() == Some(tag.as_str()))
        {
            return hit.rect;
        }
        node.rect
    }

    /// Window-space origin, before clipping. Falls back to the semantics rect.
    pub fn origin_in_window(&self) -> Vec2 {
        let node = self.one();
        if let NodeMatcher::Tag(tag) = &self.matcher
            && let Some(hit) = self
                .test
                .frame()
                .hit_regions
                .iter()
                .find(|hit| hit.test_tag.as_deref() == Some(tag.as_str()))
        {
            return hit_region_transformed_origin(hit);
        }
        Vec2 {
            x: node.rect.x,
            y: node.rect.y,
        }
    }

    pub fn text(&self) -> Option<String> {
        let node = self.one();
        node.label.or(node.value)
    }

    pub fn assert_exists(&self) -> &Self {
        self.one();
        self
    }

    pub fn assert_does_not_exist(&self) -> &Self {
        let nodes = self.all();
        assert!(
            nodes.is_empty(),
            "expected no node for {} but found {}",
            self.matcher.describe(),
            nodes.len()
        );
        self
    }

    pub fn assert_is_displayed(&self) -> &Self {
        let node = self.one();
        let (width, height) = self.test.size();
        let viewport = Rect {
            x: 0.0,
            y: 0.0,
            w: width as f32,
            h: height as f32,
        };
        let rect = self.bounds_in_window();
        assert!(
            !node.hidden && rect.w > 0.0 && rect.h > 0.0 && intersects(rect, viewport),
            "node {} is not displayed: hidden={} rect={rect:?}",
            self.matcher.describe(),
            node.hidden
        );
        self
    }

    pub fn assert_text_equals(&self, expected: &str) -> &Self {
        let text = self.text();
        assert_eq!(
            text.as_deref(),
            Some(expected),
            "text mismatch for {}",
            self.matcher.describe()
        );
        self
    }

    pub fn assert_is_enabled(&self) -> &Self {
        let node = self.one();
        assert!(
            node.enabled,
            "node {} is disabled",
            self.matcher.describe()
        );
        self
    }

    /// Dispatch a tap at the node's visible centre and compose the result.
    pub fn perform_click(&self) -> &Self {
        let rect = self.bounds_in_window();
        assert!(
            rect.w > 0.0 && rect.h > 0.0,
            "node {} has no clickable area",
            self.matcher.describe()
        );
        let position = Vec2 {
            x: rect.x + rect.w / 2.0,
            y: rect.y + rect.h / 2.0,
        };
        let path = hit_test_frame_path(&self.test.frame(), position);
        assert!(
            !path.is_empty(),
            "node {} is not hit-testable at {position:?}",
            self.matcher.describe()
        );

        let pointer = self.test.next_pointer();
        self.test.touch_press(pointer, position);
        self.test.touch_release(pointer, position);
        self.test.pump();
        self
    }

    /// Inject touch events in node-local coordinates. Event time advances by
    /// one frame per move, so drag velocity is deterministic.
    pub fn perform_touch_input(&self, block: impl FnOnce(&mut TouchInput<'_>)) -> &Self {
        let _node = self.one();
        let rect = self.bounds_in_window();
        let pointer = self.test.next_pointer();
        let mut input = TouchInput {
            test: self.test,
            origin: Vec2 { x: rect.x, y: rect.y },
            size: Vec2 { x: rect.w, y: rect.h },
            position: Vec2::default(),
            pointer,
            down: false,
        };
        block(&mut input);
        self.test.pump();
        self
    }
}

/// Touch driver handed to [`UiNode::perform_touch_input`].
///
/// Coordinates are local to the node; `(0, 0)` is its top-left and
/// [`center`](Self::center) is its centre. A move advances the clock by one
/// frame before dispatch.
pub struct TouchInput<'a> {
    test: &'a UiTest,
    origin: Vec2,
    size: Vec2,
    position: Vec2,
    pointer: u64,
    down: bool,
}

impl TouchInput<'_> {
    pub fn center(&self) -> Vec2 {
        Vec2 {
            x: self.size.x / 2.0,
            y: self.size.y / 2.0,
        }
    }

    pub fn down_center(&mut self) {
        let center = self.center();
        self.down_at(center);
    }

    pub fn down_at(&mut self, position: Vec2) {
        assert!(!self.down, "touch is already down");
        self.down = true;
        self.position = position;
        self.test.touch_press(self.pointer, self.world(position));
    }

    pub fn move_to(&mut self, position: Vec2) {
        assert!(self.down, "move_to called while touch is up");
        self.test.clock.advance(EVENT_INTERVAL);
        self.position = position;
        self.test.touch_move(self.pointer, self.world(position));
    }

    pub fn move_by(&mut self, delta: Vec2) {
        let next = self.position + delta;
        self.move_to(next);
    }

    pub fn move_by_steps(&mut self, delta: Vec2, steps: u32) {
        assert!(steps > 0, "move_by_steps needs at least one step");
        let step = Vec2 {
            x: delta.x / steps as f32,
            y: delta.y / steps as f32,
        };
        for _ in 0..steps {
            self.move_by(step);
        }
    }

    pub fn up(&mut self) {
        assert!(self.down, "up called while touch is up");
        self.down = false;
        self.test.touch_release(self.pointer, self.world(self.position));
    }

    pub fn cancel(&mut self) {
        assert!(self.down, "cancel called while touch is up");
        self.down = false;
        self.test.touch_cancel(self.pointer);
    }

    /// Compose the current UI mid-gesture.
    pub fn pump(&self) {
        self.test.pump();
    }

    pub fn advance(&self, duration: Duration) {
        self.test.clock.advance(duration);
    }

    pub fn test(&self) -> &UiTest {
        self.test
    }

    fn world(&self, position: Vec2) -> Vec2 {
        self.origin + position
    }
}

fn intersects(a: Rect, b: Rect) -> bool {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = (a.x + a.w).min(b.x + b.w);
    let bottom = (a.y + a.h).min(b.y + b.h);
    right > left && bottom > top
}
