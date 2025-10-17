#![allow(non_snake_case)]
use crate::View;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

pub struct ErrorInfo {
    pub message: String,
    pub component: String,
}

pub fn ErrorBoundary(
    fallback: impl Fn(ErrorInfo) -> View + 'static,
    content: impl Fn() -> View + 'static,
) -> View {
    let fallback = Rc::new(fallback);
    let content = Rc::new(content);

    match catch_unwind(AssertUnwindSafe(|| content())) {
        Ok(view) => view,
        Err(err) => {
            let message = if let Some(s) = err.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = err.downcast_ref::<&str>() {
                s.to_string()
            } else {
                "Unknown panic".to_string()
            };

            fallback(ErrorInfo {
                message,
                component: "Unknown".to_string(),
            })
        }
    }
}
