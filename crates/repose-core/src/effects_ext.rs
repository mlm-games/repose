/// runs once when key changes
pub fn launched_effect<K: PartialEq + Clone + 'static>(key: K, effect: impl FnOnce() + 'static) {
    let last_key = remember_with_key(format!("launched_{:?}", std::ptr::addr_of!(&key)), || {
        RefCell::new(None::<K>)
    });

    let mut last = last_key.borrow_mut();
    if last.as_ref() != Some(&key) {
        *last = Some(key);
        effect();
    }
}

/// cleanup on key change or unmount
pub fn disposable_effect<K: PartialEq + Clone + 'static>(
    key: K,
    effect: impl FnOnce() -> Box<dyn FnOnce()> + 'static,
) {
    scoped_effect(|| {
        let cleanup = effect();
        cleanup
    });
}

/// runs on every recomposition
pub fn side_effect(effect: impl Fn()) {
    effect();
}
