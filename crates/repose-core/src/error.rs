//! Stateful error boundary.
//!
//! [`ErrorBoundary`] catches panics thrown from its content closure (reliable on
//! native) and also supports a WASM-safe failure path via [`throw_boundary`]:
//! panics abort on `wasm32-unknown-unknown` builds compiled with
//! `panic = "abort"`, so `catch_unwind` is best-effort there.
//!
//! The boundary is *sticky*: once a fallback is shown it stays until the `reset`
//! callback fires. This prevents a panicking leaf from re-panicking every frame
//! and means the Reset button lives in the fallback, not the failing content.

#![allow(non_snake_case)]

use crate::{View, remember_state, request_frame};
use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

/// Details about a boundary trip, passed to the fallback.
#[derive(Clone, Debug)]
pub struct ErrorInfo {
    pub message: String,
    pub component: String,
}

thread_local! {
    /// Pending explicit throw (WASM-safe path). Cleared before each content run.
    static BOUNDARY_THROW: RefCell<Option<ErrorInfo>> = const { RefCell::new(None) };
}

/// Call from inside a boundary's content to trip the fallback without `panic!`.
/// This is the WASM-safe path.
pub fn throw_boundary(message: impl Into<String>) {
    BOUNDARY_THROW.with(|t| {
        *t.borrow_mut() = Some(ErrorInfo {
            message: message.into(),
            component: "Unknown".into(),
        });
    });
}

fn take_throw() -> Option<ErrorInfo> {
    BOUNDARY_THROW.with(|t| t.borrow_mut().take())
}

/// Render `fallback` when `content` panics (native) or calls [`throw_boundary`],
/// and stay in fallback until `reset()` runs. `fallback(info, reset)` receives
/// the error and a closure that clears the error and re-enters `content`.
///
/// Prefer Result-style / [`throw_boundary`] over `panic!` on WASM targets.
pub fn ErrorBoundary(
    fallback: impl Fn(ErrorInfo, Rc<dyn Fn()>) -> View + 'static,
    content: impl Fn() -> View + 'static,
) -> View {
    // Sticky error + generation so Reset can force content re-entry.
    let error: Rc<RefCell<Option<ErrorInfo>>> = remember_state(|| None);
    let generation = remember_state(|| 0u64);

    let reset = {
        let error = error.clone();
        let generation = generation.clone();
        Rc::new(move || {
            *error.borrow_mut() = None;
            *generation.borrow_mut() += 1;
            request_frame();
        }) as Rc<dyn Fn()>
    };

    if let Some(info) = error.borrow().clone() {
        return fallback(info, reset);
    }

    // Read generation so recomposition after reset is forced even if other inputs match.
    let _g = *generation.borrow();

    BOUNDARY_THROW.with(|t| *t.borrow_mut() = None);

    let result = catch_unwind(AssertUnwindSafe(content));

    // Prefer explicit throw_boundary over panic.
    if let Some(info) = take_throw() {
        *error.borrow_mut() = Some(info.clone());
        request_frame();
        return fallback(info, reset);
    }

    match result {
        Ok(view) => view,
        Err(err) => {
            let message = if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = err.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "Unknown panic".to_string()
            };
            let info = ErrorInfo {
                message,
                component: "Unknown".into(),
            };
            *error.borrow_mut() = Some(info.clone());
            request_frame();
            fallback(info, reset)
        }
    }
}
