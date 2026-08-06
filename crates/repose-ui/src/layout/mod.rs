//! Incremental layout + paint engine, split across focused modules.
//!
//! `LayoutEngine` is a persistent view tree + Taffy integration that produces a
//! `Scene` per frame with hit regions and semantics, with incremental layout
//! (scope isolation, dirty sets, repaint-boundary caching, intrinsics).

mod engine;
mod helpers;
mod measure;
mod paint;
mod scope;
mod scrollbars;
mod taffy_sync;
mod types;

#[cfg(test)]
mod tests;

pub use types::LayoutEngine;
pub use types::{IntrinsicSizeMode, LayoutStats};

#[doc(hidden)]
pub(crate) use helpers::mul_alpha_color;

// Make shared internals visible to sibling submodules via `use super::*`.
pub(crate) use helpers::*;
pub(crate) use scrollbars::*;
pub(crate) use types::*;
