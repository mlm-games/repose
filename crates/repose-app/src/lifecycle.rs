use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppLifecycle {
    Foreground,
    Background,
}

type LifecycleCallback = Box<dyn Fn(AppLifecycle) + Send>;
type DeeplinkCallback = Box<dyn Fn(Vec<u8>) + Send>;

struct LifecycleSlot {
    callback: Mutex<Option<LifecycleCallback>>,
}

struct DeeplinkSlot {
    callback: Mutex<Option<DeeplinkCallback>>,
}

#[derive(Clone)]
struct LifecycleEntry {
    id: u64,
    slot: Arc<LifecycleSlot>,
}

#[derive(Clone)]
struct DeeplinkEntry {
    id: u64,
    slot: Arc<DeeplinkSlot>,
}

pub struct LifecycleDispatcher {
    current: AtomicU8,
    primary: Mutex<Option<Arc<LifecycleSlot>>>,
    pending: Mutex<Vec<AppLifecycle>>,
    listeners: Mutex<Vec<LifecycleEntry>>,
    next_id: AtomicU64,
    processing: Mutex<()>,
}

impl LifecycleDispatcher {
    const fn new() -> Self {
        Self {
            current: AtomicU8::new(0),
            primary: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            listeners: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            processing: Mutex::new(()),
        }
    }

    pub fn set_callback(&self, callback: Box<dyn Fn(AppLifecycle) + Send>) {
        *lock(&self.primary) = Some(Arc::new(LifecycleSlot {
            callback: Mutex::new(Some(callback)),
        }));
    }

    pub fn add_listener(&self, callback: Box<dyn Fn(AppLifecycle) + Send>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.listeners).push(LifecycleEntry {
            id,
            slot: Arc::new(LifecycleSlot {
                callback: Mutex::new(Some(callback)),
            }),
        });
        id
    }

    pub fn remove_listener(&self, id: u64) -> bool {
        let mut listeners = lock(&self.listeners);
        let before = listeners.len();
        listeners.retain(|entry| entry.id != id);
        listeners.len() != before
    }

    pub fn current(&self) -> Option<AppLifecycle> {
        lifecycle_from_code(self.current.load(Ordering::Relaxed))
    }

    pub fn push(&self, state: AppLifecycle) {
        self.current.store(lifecycle_code(state), Ordering::Relaxed);
        lock(&self.pending).push(state);
    }

    pub fn process(&self) {
        let _processing = match self.processing.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let batch = std::mem::take(&mut *lock(&self.pending));
        if batch.is_empty() {
            return;
        }
        let mut retained = Vec::new();
        for state in batch {
            let primary = lock(&self.primary).clone();
            let mut handled = primary.as_ref().is_some_and(|slot| {
                invoke_lifecycle(slot, state, || {
                    lock(&self.primary)
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, slot))
                })
            });
            let listeners = lock(&self.listeners).clone();
            for entry in &listeners {
                if lifecycle_listener_is_current(&self.listeners, entry.id, &entry.slot)
                    && invoke_lifecycle(&entry.slot, state, || {
                        lifecycle_listener_is_current(&self.listeners, entry.id, &entry.slot)
                    })
                {
                    handled = true;
                }
            }
            if !handled {
                retained.push(state);
            }
        }
        if !retained.is_empty() {
            lock(&self.pending).splice(0..0, retained);
        }
    }
}

impl Default for LifecycleDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct RuntimeDispatchers {
    pub lifecycle: Arc<LifecycleDispatcher>,
    pub deeplink: Arc<DeeplinkDispatcher>,
}

pub struct ActiveDispatcherScope {
    previous: Option<RuntimeDispatchers>,
}

static EXTERNAL_LIFECYCLE: Mutex<Vec<AppLifecycle>> = Mutex::new(Vec::new());
static EXTERNAL_DEEPLINKS: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

thread_local! {
    static ACTIVE_DISPATCHERS: RefCell<Option<RuntimeDispatchers>> = const { RefCell::new(None) };
}

pub fn enter_dispatchers(dispatchers: RuntimeDispatchers) -> ActiveDispatcherScope {
    let lifecycle = std::mem::take(&mut *lock(&EXTERNAL_LIFECYCLE));
    for state in lifecycle {
        dispatchers.lifecycle.push(state);
    }
    let deeplinks = std::mem::take(&mut *lock(&EXTERNAL_DEEPLINKS));
    for data in deeplinks {
        dispatchers.deeplink.push(data);
    }
    let previous = ACTIVE_DISPATCHERS.with(|slot| slot.borrow_mut().replace(dispatchers));
    ActiveDispatcherScope { previous }
}

impl Drop for ActiveDispatcherScope {
    fn drop(&mut self) {
        ACTIVE_DISPATCHERS.with(|slot| {
            *slot.borrow_mut() = self.previous.take();
        });
    }
}

fn active_dispatchers() -> Option<RuntimeDispatchers> {
    ACTIVE_DISPATCHERS.with(|slot| slot.borrow().clone())
}

pub struct DeeplinkDispatcher {
    primary: Mutex<Option<Arc<DeeplinkSlot>>>,
    pending: Mutex<Vec<Vec<u8>>>,
    listeners: Mutex<Vec<DeeplinkEntry>>,
    next_id: AtomicU64,
    processing: Mutex<()>,
}

impl DeeplinkDispatcher {
    const fn new() -> Self {
        Self {
            primary: Mutex::new(None),
            pending: Mutex::new(Vec::new()),
            listeners: Mutex::new(Vec::new()),
            next_id: AtomicU64::new(1),
            processing: Mutex::new(()),
        }
    }

    pub fn set_callback(&self, callback: Box<dyn Fn(Vec<u8>) + Send>) {
        *lock(&self.primary) = Some(Arc::new(DeeplinkSlot {
            callback: Mutex::new(Some(callback)),
        }));
    }

    pub fn add_listener(&self, callback: Box<dyn Fn(Vec<u8>) + Send>) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        lock(&self.listeners).push(DeeplinkEntry {
            id,
            slot: Arc::new(DeeplinkSlot {
                callback: Mutex::new(Some(callback)),
            }),
        });
        id
    }

    pub fn remove_listener(&self, id: u64) -> bool {
        let mut listeners = lock(&self.listeners);
        let before = listeners.len();
        listeners.retain(|entry| entry.id != id);
        listeners.len() != before
    }

    pub fn push(&self, data: Vec<u8>) {
        lock(&self.pending).push(data);
    }

    pub fn process(&self) {
        let _processing = match self.processing.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let batch = std::mem::take(&mut *lock(&self.pending));
        if batch.is_empty() {
            return;
        }
        let mut retained = Vec::new();
        for data in batch {
            let primary = lock(&self.primary).clone();
            let mut handled = primary.as_ref().is_some_and(|slot| {
                invoke_deeplink(slot, &data, || {
                    lock(&self.primary)
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, slot))
                })
            });
            let listeners = lock(&self.listeners).clone();
            for entry in &listeners {
                if deeplink_listener_is_current(&self.listeners, entry.id, &entry.slot)
                    && invoke_deeplink(&entry.slot, &data, || {
                        deeplink_listener_is_current(&self.listeners, entry.id, &entry.slot)
                    })
                {
                    handled = true;
                }
            }
            if !handled {
                retained.push(data);
            }
        }
        if !retained.is_empty() {
            lock(&self.pending).splice(0..0, retained);
        }
    }
}

impl Default for DeeplinkDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

static LIFECYCLE_DISPATCHER: LifecycleDispatcher = LifecycleDispatcher::new();
static DEEPLINK_DISPATCHER: DeeplinkDispatcher = DeeplinkDispatcher::new();

thread_local! {
    static PRE_REDRAW: RefCell<Option<Rc<RefCell<Option<Box<dyn FnMut(&repose_core::RenderContext)>>>>>> =
        const { RefCell::new(None) };
}

pub fn set_pre_redraw(cb: Option<Box<dyn FnMut(&repose_core::RenderContext)>>) {
    PRE_REDRAW.with(|current| {
        *current.borrow_mut() = cb.map(|callback| Rc::new(RefCell::new(Some(callback))));
    });
}

pub fn run_pre_redraw(ctx: &repose_core::RenderContext) {
    PRE_REDRAW.with(|current| {
        let Some(slot) = current.borrow().clone() else {
            return;
        };
        let callback = slot.borrow_mut().take();
        let Some(mut callback) = callback else {
            return;
        };
        callback(ctx);
        let still_current = current
            .borrow()
            .as_ref()
            .is_some_and(|active| Rc::ptr_eq(active, &slot));
        if still_current && slot.borrow().is_none() {
            *slot.borrow_mut() = Some(callback);
        }
    });
}

pub fn set_on_lifecycle(callback: Box<dyn Fn(AppLifecycle) + Send>) {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.lifecycle.set_callback(callback);
    } else {
        LIFECYCLE_DISPATCHER.set_callback(callback);
    }
}

pub fn add_lifecycle_listener(callback: Box<dyn Fn(AppLifecycle) + Send>) -> u64 {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.lifecycle.add_listener(callback)
    } else {
        LIFECYCLE_DISPATCHER.add_listener(callback)
    }
}

pub fn remove_lifecycle_listener(id: u64) -> bool {
    active_dispatchers()
        .map(|dispatchers| dispatchers.lifecycle.remove_listener(id))
        .unwrap_or_else(|| LIFECYCLE_DISPATCHER.remove_listener(id))
}

pub fn current_lifecycle() -> Option<AppLifecycle> {
    active_dispatchers()
        .map(|dispatchers| dispatchers.lifecycle.current())
        .unwrap_or_else(|| LIFECYCLE_DISPATCHER.current())
}

pub fn push_lifecycle(state: AppLifecycle) {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.lifecycle.push(state);
    } else {
        LIFECYCLE_DISPATCHER.push(state);
        lock(&EXTERNAL_LIFECYCLE).push(state);
    }
}

pub fn process_lifecycle() {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.lifecycle.process();
    } else {
        LIFECYCLE_DISPATCHER.process();
    }
}

pub fn set_on_deeplink(callback: Box<dyn Fn(Vec<u8>) + Send>) {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.deeplink.set_callback(callback);
    } else {
        DEEPLINK_DISPATCHER.set_callback(callback);
    }
}

pub fn add_deeplink_listener(callback: Box<dyn Fn(Vec<u8>) + Send>) -> u64 {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.deeplink.add_listener(callback)
    } else {
        DEEPLINK_DISPATCHER.add_listener(callback)
    }
}

pub fn remove_deeplink_listener(id: u64) -> bool {
    active_dispatchers()
        .map(|dispatchers| dispatchers.deeplink.remove_listener(id))
        .unwrap_or_else(|| DEEPLINK_DISPATCHER.remove_listener(id))
}

pub fn push_deeplink(data: Vec<u8>) {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.deeplink.push(data);
    } else {
        DEEPLINK_DISPATCHER.push(data.clone());
        lock(&EXTERNAL_DEEPLINKS).push(data);
    }
}

pub fn process_deeplinks() {
    if let Some(dispatchers) = active_dispatchers() {
        dispatchers.deeplink.process();
    } else {
        DEEPLINK_DISPATCHER.process();
    }
}

fn lifecycle_code(state: AppLifecycle) -> u8 {
    match state {
        AppLifecycle::Foreground => 1,
        AppLifecycle::Background => 2,
    }
}

fn lifecycle_from_code(code: u8) -> Option<AppLifecycle> {
    match code {
        1 => Some(AppLifecycle::Foreground),
        2 => Some(AppLifecycle::Background),
        _ => None,
    }
}

fn lifecycle_listener_is_current(
    listeners: &Mutex<Vec<LifecycleEntry>>,
    id: u64,
    slot: &Arc<LifecycleSlot>,
) -> bool {
    lock(listeners)
        .iter()
        .any(|entry| entry.id == id && Arc::ptr_eq(&entry.slot, slot))
}

fn deeplink_listener_is_current(
    listeners: &Mutex<Vec<DeeplinkEntry>>,
    id: u64,
    slot: &Arc<DeeplinkSlot>,
) -> bool {
    lock(listeners)
        .iter()
        .any(|entry| entry.id == id && Arc::ptr_eq(&entry.slot, slot))
}

fn invoke_lifecycle(slot: &LifecycleSlot, state: AppLifecycle, restore: impl Fn() -> bool) -> bool {
    let callback = slot
        .callback
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    let Some(callback) = callback else {
        return false;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(state)));
    if let Err(error) = result {
        log::error!("lifecycle callback panicked: {}", panic_message(&error));
    }
    if restore()
        && slot
            .callback
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_none()
    {
        *slot
            .callback
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(callback);
    }
    true
}

fn invoke_deeplink(slot: &DeeplinkSlot, data: &[u8], restore: impl Fn() -> bool) -> bool {
    let callback = slot
        .callback
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .take();
    let Some(callback) = callback else {
        return false;
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback(data.to_vec())));
    if let Err(error) = result {
        log::error!("deeplink callback panicked: {}", panic_message(&error));
    }
    if restore()
        && slot
            .callback
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .is_none()
    {
        *slot
            .callback
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(callback);
    }
    true
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn panic_message(error: &Box<dyn std::any::Any + Send>) -> &str {
    error
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| error.downcast_ref::<&str>().copied())
        .unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn active_dispatchers_receive_legacy_global_calls() {
        let lifecycle = Arc::new(LifecycleDispatcher::default());
        let deeplink = Arc::new(DeeplinkDispatcher::default());
        let seen = Arc::new(AtomicUsize::new(0));
        let callback_seen = seen.clone();
        {
            let _scope = enter_dispatchers(RuntimeDispatchers {
                lifecycle: lifecycle.clone(),
                deeplink: deeplink.clone(),
            });
            set_on_lifecycle(Box::new(move |_state| {
                callback_seen.fetch_add(1, Ordering::Relaxed);
            }));
            push_lifecycle(AppLifecycle::Foreground);
            process_lifecycle();
        }
        assert_eq!(seen.load(Ordering::Relaxed), 1);
    }
}
