use std::sync::{
    Mutex,
    atomic::{AtomicU8, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLifecycle {
    Foreground,
    Background,
}

static CURRENT_LIFECYCLE: AtomicU8 = AtomicU8::new(0);
static LIFECYCLE_CB: Mutex<Option<Box<dyn Fn(AppLifecycle) + Send>>> = Mutex::new(None);
static PENDING_LIFECYCLE: Mutex<Vec<AppLifecycle>> = Mutex::new(Vec::new());

static DEEPLINK_CB: Mutex<Option<Box<dyn Fn(Vec<u8>) + Send>>> = Mutex::new(None);
static PENDING_DEEPLINKS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

thread_local! {
    static PRE_REDRAW: std::cell::RefCell<Option<Box<dyn FnMut(&repose_core::RenderContext)>>> =
        const { std::cell::RefCell::new(None) };
}

pub fn set_pre_redraw(cb: Option<Box<dyn FnMut(&repose_core::RenderContext)>>) {
    PRE_REDRAW.with(|c| *c.borrow_mut() = cb);
}

pub fn run_pre_redraw(ctx: &repose_core::RenderContext) {
    PRE_REDRAW.with(|c| {
        if let Some(cb) = c.borrow_mut().as_mut() {
            cb(ctx);
        }
    });
}

pub fn set_on_lifecycle(callback: Box<dyn Fn(AppLifecycle) + Send>) {
    *LIFECYCLE_CB.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
}

pub fn current_lifecycle() -> Option<AppLifecycle> {
    match CURRENT_LIFECYCLE.load(Ordering::Relaxed) {
        1 => Some(AppLifecycle::Foreground),
        2 => Some(AppLifecycle::Background),
        _ => None,
    }
}

pub fn push_lifecycle(state: AppLifecycle) {
    let code = match state {
        AppLifecycle::Foreground => 1,
        AppLifecycle::Background => 2,
    };
    CURRENT_LIFECYCLE.store(code, Ordering::Relaxed);
    PENDING_LIFECYCLE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(state);
}

pub fn process_lifecycle() {
    let batch = std::mem::take(&mut *PENDING_LIFECYCLE.lock().unwrap_or_else(|e| e.into_inner()));
    if batch.is_empty() {
        return;
    }
    if let Some(cb) = LIFECYCLE_CB
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        for state in batch {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(state)));
            if let Err(e) = res {
                log::error!(
                    "lifecycle callback panicked: {}",
                    e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown")
                );
            }
        }
    }
}

pub fn set_on_deeplink(callback: Box<dyn Fn(Vec<u8>) + Send>) {
    *DEEPLINK_CB.lock().unwrap_or_else(|e| e.into_inner()) = Some(callback);
}

pub fn push_deeplink(data: Vec<u8>) {
    PENDING_DEEPLINKS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(data);
}

pub fn process_deeplinks() {
    let mut queue = PENDING_DEEPLINKS.lock().unwrap_or_else(|e| e.into_inner());
    if queue.is_empty() {
        return;
    }
    let batch = std::mem::take(&mut *queue);
    drop(queue);
    if let Some(cb) = DEEPLINK_CB
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
    {
        for data in batch {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(data.clone())));
            if let Err(e) = res {
                log::error!(
                    "deeplink callback panicked: {}",
                    e.downcast_ref::<String>()
                        .map(|s| s.as_str())
                        .or_else(|| e.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown")
                );
            }
        }
    }
}
