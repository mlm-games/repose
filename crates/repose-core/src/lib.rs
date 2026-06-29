//! # State, Signals, and Effects
//!
//! Repose uses a small reactive core instead of an explicit widget tree with
//! mutable fields. There are three main pieces:
//!
//! - `Signal<T>` - observable, reactive value.
//! - `remember*` - lifecycle‑aware storage bound to composition.
//! - `effect` / `scoped_effect` - side‑effects with cleanup.
//!
//! ## Signals
//!
//! `Signal<T>` is a cloneable handle to a piece of state:
//!
//! ```rust
//! use repose_core::*;
//!
//! let count = signal(0);
//! count.set(1);
//! count.update(|v| *v += 1);
//! assert_eq!(count.get(), 2);
//! ```
//!
//! Reads participate in a dependency graph: when you call `get()` inside an
//! observer or `produce_state`, future writes will automatically recompute that
//! observer.
//!
//! ## Remembered state
//!
//! UI state is typically held in `remember_*` slots rather than globals:
//!
//! ```ignore
//! use repose_core::*;
//!
//! fn CounterView() -> View {
//!     let count = remember_state(|| 0); // Rc<RefCell<i32>>
//!
//!     let on_click = {
//!         let count = count.clone();
//!         move || *count.borrow_mut() += 1
//!     };
//!
//!     repose_ui::Button(
//!         format!("Count = {}", *count.borrow()),
//!         on_click,
//!     )
//! }
//! ```
//!
//! - `remember` and `remember_state` are order‑based: the Nth call in a
//!   composition slot always refers to the Nth stored value.
//! - `remember_with_key` and `remember_state_with_key` are key‑based and more
//!   stable across conditional branches.
//!
//! ## Derived state
//!
//! `produce_state` computes a `Signal<T>` from other signals and recomputes it
//! automatically when dependencies change:
//!
//! ```rust
//! use repose_core::*;
//!
//! let first = signal("Jane".to_string());
//! let last  = signal("Doe".to_string());
//!
//! let full = produce_state("full_name", {
//!     let first = first.clone();
//!     let last  = last.clone();
//!     move || format!("{} {}", first.get(), last.get())
//! });
//!
//! assert_eq!(full.get(), "Jane Doe");
//! ```
//!
//! ## Effects and cleanup
//!
//! Use `effect` / `scoped_effect` for one‑off side‑effects with cleanups:
//!
//! ```ignore
//! use repose_core::*;
//!
//! fn Example() -> View {
//!     scoped_effect(|| {
//!         log::info!("Mounted Example");
//!         on_unmount(|| log::info!("Unmounted Example"))
//!     });
//!
//!     // ...
//!     repose_ui::Box(Modifier::new())
//! }
//! ```
//!
//! - `effect` runs once when the view is composed and returns a `Dispose`
//!   guard that will be run when the scope is torn down.
//! - `scoped_effect` is wired to the current `Scope` and is cleaned up on
//!   scope disposal (e.g. when a navigation entry is popped).
//!
//! For long‑running tasks (network, timers), prefer building small helpers on
//! top of `scoped_effect` so everything cleans up correctly when the UI that
//! owns it disappears.

pub mod animation;
pub mod animation_driver;
pub mod clipboard;
pub mod color;
pub mod cursor;
pub mod dnd;
pub mod effects;
pub mod effects_ext;
pub mod error;
pub mod focus;
pub mod frame_clock;
pub mod geometry;
pub mod input;
pub mod locals;
pub mod modifier;
pub mod prelude;
pub mod reactive;
pub mod render_api;
pub mod runtime;
pub mod scope;
pub mod scope_cache;
pub mod semantics;
pub mod shortcuts;
pub mod signal;
pub mod state;
pub mod tests;
pub mod text;
pub mod view;

pub use color::*;
pub use cursor::*;
pub use dnd::*;
pub use effects::*;
pub use effects_ext::*;
pub use focus::*;
pub use frame_clock::{
    peek_frame_request, request_frame, signal_fired, take_frame_request, take_signal_fired,
};
pub use geometry::*;
pub use locals::*;
pub use modifier::*;
pub use prelude::*;
pub use reactive::*;
pub use render_api::*;
pub use runtime::*;
pub use runtime::{FocusDirection, FocusManager, FocusRequester, take_focus_request};
pub use semantics::*;
pub use signal::*;
pub use state::*;
pub use text::*;
pub use view::*;

pub use repose_macros::View;

/// Memoized composition scope with input + signal tracking.
///
/// Wraps a composable block, caching its output as long as:
/// 1. The explicit inputs are unchanged (by `Hash` comparison).
/// 2. No signal read during body execution has been written since last run.
///
/// When the cache is hit, the body is NOT executed — the previously-composed
/// View is returned instead, with proper ID and composer cursor advancement
/// to keep sibling scopes consistent.
///
/// # Usage
///
/// ```ignore
/// use repose_core::*;
///
/// fn MyView(s: &mut Scheduler, title: &str, count: i32) -> View {
///     scope!("my_view", s, [title, count], {
///         Column(Modifier::new()).child((
///             Text(title),
///             Text(format!("Count: {count}")),
///         ))
///     })
/// }
/// ```
///
/// # Signal auto-tracking
///
/// Any `Signal::get()` call inside the body automatically registers the scope
/// as a dependency. When that signal is written, the scope is marked dirty and
/// recomposed on the next frame. You don't need to put signal values in the
/// input list — the reactive system handles dependencies implicitly.
///
/// ```ignore
/// let size = signal(100.0);
/// scope!("animated", s, [], {
///     let cur = size.get();  // auto-tracked; cache invalidated on write
///     Box(Modifier::new().size(cur, cur))
/// })
/// ```
///
/// # `f32`/`f64` in explicit inputs
///
/// Float types don't implement `Hash`. For float inputs, use `.to_bits()`:
///
/// ```ignore
/// scope!("s", s, [my_float.to_bits()], { ... })
/// ```
///
/// Or — better — read floats from a `Signal<f32>` inside the body (auto-tracked).
///
/// # Compatibility with `remember`
///
/// `remember` slots consumed inside the body are tracked and properly advanced
/// on cache hit, so sibling `remember` calls remain consistent.
#[macro_export]
macro_rules! scope {
    // With explicit inputs
    ($key:expr, $s:expr, [$($input:expr),+ $(,)?], $body:block) => {{
        let _key: &str = $key;

        let _input_hash = {
            use std::hash::{Hash, Hasher};
            let mut _hasher = std::collections::hash_map::DefaultHasher::new();
            $(
                Hash::hash(&$input, &mut _hasher);
            )*
            _hasher.finish()
        };

        if !$crate::scope_cache::should_run(_key, _input_hash) {
            $crate::scope_cache::get_cached(_key, $s)
        } else {
            $crate::scope_cache::clear_scope_deps(_key);

            let _prev_cursor = $crate::runtime::COMPOSER.with(|c| c.borrow().cursor);
            let _prev_id = $s.snapshot_id();

            let _result = $crate::scope_cache::with_scope_key(_key, || $body);

            let _id_count = $s.ids_used_since(_prev_id);
            let _slot_delta = $crate::runtime::COMPOSER.with(|c| c.borrow().cursor) - _prev_cursor;

            $crate::scope_cache::set_cache(_key, _input_hash, _result.clone(), _id_count, _slot_delta);

            _result
        }
    }};

    // Without explicit inputs — skip Hash import
    ($key:expr, $s:expr, [], $body:block) => {{
        let _key: &str = $key;
        let _input_hash: u64 = 0;

        if !$crate::scope_cache::should_run(_key, _input_hash) {
            $crate::scope_cache::get_cached(_key, $s)
        } else {
            $crate::scope_cache::clear_scope_deps(_key);

            let _prev_cursor = $crate::runtime::COMPOSER.with(|c| c.borrow().cursor);
            let _prev_id = $s.snapshot_id();

            let _result = $crate::scope_cache::with_scope_key(_key, || $body);

            let _id_count = $s.ids_used_since(_prev_id);
            let _slot_delta = $crate::runtime::COMPOSER.with(|c| c.borrow().cursor) - _prev_cursor;

            $crate::scope_cache::set_cache(_key, _input_hash, _result.clone(), _id_count, _slot_delta);

            _result
        }
    }};
}
