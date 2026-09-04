use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub trait UnresolvedListener: Send + Sync + 'static {
    fn on_unresolved_codepoints(&self, _codepoints: &HashSet<u32>) {}
    fn on_new_font_installed(&self) {}
}

pub struct UnresolvedSymbolsRegistry {
    inner: Mutex<Inner>,
}

struct Inner {
    unresolved: HashSet<u32>,
    listeners: Vec<Weak<dyn UnresolvedListener>>,
}

impl UnresolvedSymbolsRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                unresolved: HashSet::new(),
                listeners: Vec::new(),
            }),
        }
    }

    pub fn add_listener(&self, listener: Arc<dyn UnresolvedListener>) {
        let mut inner = self.inner.lock().unwrap();
        inner.listeners.retain(|w| w.upgrade().is_some());
        inner.listeners.push(Arc::downgrade(&listener));
    }

    pub fn remove_listener(&self, listener: &Arc<dyn UnresolvedListener>) {
        let mut inner = self.inner.lock().unwrap();
        inner.listeners.retain(|w| {
            if let Some(strong) = w.upgrade() {
                !Arc::ptr_eq(&strong, listener)
            } else {
                false
            }
        });
    }

    pub fn add_unresolved_codepoints(&self, codepoints: &[u32]) {
        let mut to_add = Vec::new();
        {
            let inner = self.inner.lock().unwrap();
            for &cp in codepoints {
                if !inner.unresolved.contains(&cp) {
                    to_add.push(cp);
                }
            }
            if to_add.is_empty() {
                return;
            }
        }
        let (snapshot, listeners): (HashSet<u32>, Vec<Arc<dyn UnresolvedListener>>) = {
            let mut inner = self.inner.lock().unwrap();
            let mut actually_new = Vec::new();
            for cp in to_add {
                if inner.unresolved.insert(cp) {
                    actually_new.push(cp);
                }
            }
            if actually_new.is_empty() {
                return;
            }
            inner.listeners.retain(|w| w.upgrade().is_some());
            let live: Vec<_> = inner.listeners.iter().filter_map(|w| w.upgrade()).collect();
            (inner.unresolved.clone(), live)
        };
        for l in listeners {
            l.on_unresolved_codepoints(&snapshot);
        }
    }

    pub fn add_unresolved_vec(&self, codepoints: Vec<u32>) {
        self.add_unresolved_codepoints(&codepoints);
    }

    pub fn on_new_font_installed(&self) {
        let listeners: Vec<Arc<dyn UnresolvedListener>> = {
            let mut inner = self.inner.lock().unwrap();
            inner.unresolved.clear();
            inner.listeners.retain(|w| w.upgrade().is_some());
            inner.listeners.iter().filter_map(|w| w.upgrade()).collect()
        };
        for l in listeners {
            l.on_new_font_installed();
        }
    }

    pub fn unresolved_len(&self) -> usize {
        self.inner.lock().unwrap().unresolved.len()
    }
}

static WEB_REGISTRY: OnceLock<UnresolvedSymbolsRegistry> = OnceLock::new();

pub fn web_unresolved_registry() -> &'static UnresolvedSymbolsRegistry {
    WEB_REGISTRY.get_or_init(|| UnresolvedSymbolsRegistry::new())
}

static GLOBAL_REGISTRY: OnceLock<UnresolvedSymbolsRegistry> = OnceLock::new();

pub fn global_registry() -> &'static UnresolvedSymbolsRegistry {
    GLOBAL_REGISTRY.get_or_init(|| UnresolvedSymbolsRegistry::new())
}

pub fn get_unresolved_registry() -> Option<&'static UnresolvedSymbolsRegistry> {
    #[cfg(target_arch = "wasm32")]
    {
        Some(web_unresolved_registry())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestListener {
        unresolved_calls: AtomicUsize,
        new_font_calls: AtomicUsize,
        last_len: AtomicUsize,
    }
    impl UnresolvedListener for TestListener {
        fn on_unresolved_codepoints(&self, cps: &HashSet<u32>) {
            self.unresolved_calls.fetch_add(1, Ordering::SeqCst);
            self.last_len.store(cps.len(), Ordering::SeqCst);
        }
        fn on_new_font_installed(&self) {
            self.new_font_calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn dedupes_and_notifies() {
        let reg = UnresolvedSymbolsRegistry::new();
        let listener: Arc<dyn UnresolvedListener> = Arc::new(TestListener {
            unresolved_calls: AtomicUsize::new(0),
            new_font_calls: AtomicUsize::new(0),
            last_len: AtomicUsize::new(0),
        });
        reg.add_listener(listener.clone());
        reg.add_unresolved_codepoints(&[0x1F600, 0x1F600]);
        assert_eq!(reg.unresolved_len(), 1);
        reg.add_unresolved_codepoints(&[0x1F600]);
        reg.add_unresolved_codepoints(&[0x1F389]);
        assert_eq!(reg.unresolved_len(), 2);
    }

    #[test]
    fn on_new_font_clears() {
        let reg = UnresolvedSymbolsRegistry::new();
        reg.add_unresolved_codepoints(&[1, 2, 3]);
        assert_eq!(reg.unresolved_len(), 3);
        reg.on_new_font_installed();
        assert_eq!(reg.unresolved_len(), 0);
    }
}
