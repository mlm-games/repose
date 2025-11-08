use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}
impl Default for TextDirection {
    fn default() -> Self {
        TextDirection::Ltr
    }
}

thread_local! {
    static LOCALS_STACK: RefCell<Vec<HashMap<TypeId, Box<dyn Any>>>> = RefCell::new(Vec::new());
}

/// density‑independent pixels (dp)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dp(pub f32);

impl Dp {
    /// Converts this dp value into physical pixels using the current Density.
    pub fn to_px(self) -> f32 {
        self.0 * density().scale
    }
}

/// Convenience: convert a raw dp scalar into px using current Density.
pub fn dp_to_px(dp: f32) -> f32 {
    Dp(dp).to_px()
}

pub fn with_text_direction<R>(dir: TextDirection, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(std::any::TypeId::of::<TextDirection>(), Box::new(dir));
        f()
    })
}

pub fn text_direction() -> TextDirection {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&std::any::TypeId::of::<TextDirection>()) {
                if let Some(d) = v.downcast_ref::<TextDirection>() {
                    return *d;
                }
            }
        }
        TextDirection::default()
    })
}

fn with_locals_frame<R>(f: impl FnOnce() -> R) -> R {
    // Non-panicking frame guard (ensures pop on unwind)
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            LOCALS_STACK.with(|st| {
                st.borrow_mut().pop();
            });
        }
    }
    LOCALS_STACK.with(|st| st.borrow_mut().push(HashMap::new()));
    let _guard = Guard;
    f()
}

fn set_local_boxed(t: TypeId, v: Box<dyn Any>) {
    LOCALS_STACK.with(|st| {
        if let Some(top) = st.borrow_mut().last_mut() {
            top.insert(t, v);
        } else {
            // no frame: create a temporary one
            let mut m = HashMap::new();
            m.insert(t, v);
            st.borrow_mut().push(m);
        }
    });
}

// Typed API

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: crate::Color,
    pub surface: crate::Color,
    pub on_surface: crate::Color,
    pub primary: crate::Color,
    pub on_primary: crate::Color,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            background: crate::Color::from_hex("#121212"),
            surface: crate::Color::from_hex("#1E1E1E"),
            on_surface: crate::Color::from_hex("#DDDDDD"),
            primary: crate::Color::from_hex("#34AF82"),
            on_primary: crate::Color::WHITE,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Density {
    pub scale: f32, // dp→px multiplier
}
impl Default for Density {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextScale(pub f32);
impl Default for TextScale {
    fn default() -> Self {
        Self(1.0)
    }
}

// Provide helpers (push a new frame, set the local, run closure, pop frame)

pub fn with_theme<R>(theme: Theme, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<Theme>(), Box::new(theme));
        f()
    })
}

pub fn with_density<R>(density: Density, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<Density>(), Box::new(density));
        f()
    })
}

pub fn with_text_scale<R>(ts: TextScale, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<TextScale>(), Box::new(ts));
        f()
    })
}

// Getters with defaults if not set

pub fn theme() -> Theme {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<Theme>()) {
                if let Some(t) = v.downcast_ref::<Theme>() {
                    return *t;
                }
            }
        }
        Theme::default()
    })
}

pub fn density() -> Density {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<Density>()) {
                if let Some(d) = v.downcast_ref::<Density>() {
                    return *d;
                }
            }
        }
        Density::default()
    })
}

pub fn text_scale() -> TextScale {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<TextScale>()) {
                if let Some(ts) = v.downcast_ref::<TextScale>() {
                    return *ts;
                }
            }
        }
        TextScale::default()
    })
}
