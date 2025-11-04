//! Core runtime, view model, signals, composition locals, and animation clock.
//!
//! See repose-ui for widgets/layout and repose-platform for runners.

pub mod animation;
pub mod color;
pub mod effects;
pub mod effects_ext;
pub mod error;
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
pub mod signal;
pub mod state;
pub mod tests;
pub mod view;

pub use color::*;
pub use effects::*;
pub use effects_ext::*;
pub use geometry::*;
pub use locals::*;
pub use modifier::*;
pub use prelude::*;
pub use reactive::*;
pub use render_api::*;
pub use runtime::*;
pub use semantics::*;
pub use signal::*;
pub use state::*;
pub use view::*;

// Ensure a clock is installed even if platform didn't (tests, benches).
#[doc(hidden)]
#[allow(dead_code)]
fn __ensure_clock() {
    animation::ensure_system_clock();
}
