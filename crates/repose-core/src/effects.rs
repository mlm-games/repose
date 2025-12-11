pub struct Dispose(Box<dyn FnOnce()>);

impl Dispose {
    pub fn new(f: impl FnOnce() + 'static) -> Self {
        Dispose(Box::new(f))
    }
    pub fn run(self) {
        (self.0)()
    }
}

/// Runs `f()` immediately and returns its `Dispose`.
/// NOTE: This does *not* currently tie the cleanup into `Scope`.
/// If you want cleanup on unmount, use `scoped_effect` or `disposable_effect`.
pub fn effect<F>(f: F) -> Dispose
where
    F: FnOnce() -> Dispose + 'static,
{
    f()
}

/// Helper to register cleanup inside effect.
pub fn on_unmount(f: impl FnOnce() + 'static) -> Dispose {
    Dispose::new(f)
}
