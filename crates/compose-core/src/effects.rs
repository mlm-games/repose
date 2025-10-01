pub struct Dispose(Box<dyn FnOnce()>);

impl Dispose {
    pub fn new(f: impl FnOnce() + 'static) -> Self { Dispose(Box::new(f)) }
    pub fn run(self) { (self.0)() }
}

/// Mimic Compose side-effect. You call effect(|| { ...; on_unmount(...) })
pub fn effect<F>(f: F) -> Dispose
where
    F: FnOnce() -> Dispose + 'static
{
    f()
}

/// Helper to register cleanup inside effect.
pub fn on_unmount(f: impl FnOnce() + 'static) -> Dispose {
    Dispose::new(f)
}
