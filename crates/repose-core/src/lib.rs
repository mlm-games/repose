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
pub mod clipboard;
pub mod color;
pub mod cursor;
pub mod dnd;
pub mod effects;
pub mod effects_ext;
pub mod error;
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
pub use frame_clock::{peek_frame_request, request_frame, signal_fired, take_frame_request, take_signal_fired};
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
