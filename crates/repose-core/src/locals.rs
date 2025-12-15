//! # Theming and locals
//!
//! Repose uses thread‑local “composition locals” for global UI parameters:
//!
//! - `Theme` — colors for surfaces, text, controls, etc.
//! - `Density` — dp→px scale factor.
//! - `TextScale` — user text scaling.
//! - `TextDirection` — LTR or RTL.
//!
//! You can override these for a subtree using `with_theme`, `with_density`,
//! `with_text_scale`, and `with_text_direction`:
//!
//! ```rust
//! use repose_core::*;
//!
//! let light = Theme {
//!     background: Color::WHITE,
//!     surface: Color::from_hex("#F5F5F5"),
//!     on_surface: Color::from_hex("#222222"),
//!     primary: Color::from_hex("#0061A4"),
//!     on_primary: Color::WHITE,
//!     ..Theme::default()
//! };
//!
//! with_theme(light, || {
//!     // all views composed here will see the light theme
//! });
//! ```
//!
//! Widgets in `repose-ui` and `repose-material` read from `theme()` and
//! should avoid hard‑coding colors where possible.

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
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
            if let Some(v) = frame.get(&std::any::TypeId::of::<TextDirection>())
                && let Some(d) = v.downcast_ref::<TextDirection>() {
                    return *d;
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

/// High‑level color theme used by widgets and layout.
///
/// This is intentionally small and semantic rather than a full Material 3
/// spec. Higher‑level libraries (e.g. `repose-material`) can build richer
/// schemes on top.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Window background / app root.
    pub background: Color,
    /// Default container surface (cards, sheets, panels).
    pub surface: Color,
    /// Primary foreground color on top of `surface`/`background`.
    pub on_surface: Color,

    /// Primary accent color for buttons, sliders, progress, etc.
    pub primary: Color,
    /// Foreground color used on top of `primary`.
    pub on_primary: Color,

    /// Low‑emphasis outline/border color.
    pub outline: Color,
    /// Color for focus rings and accessibility highlights.
    pub focus: Color,

    /// Default button background.
    pub button_bg: Color,
    /// Button background on hover.
    pub button_bg_hover: Color,
    /// Button background on pressed.
    pub button_bg_pressed: Color,

    /// Scrollbar track background (low emphasis).
    pub scrollbar_track: Color,
    /// Scrollbar thumb (higher emphasis).
    pub scrollbar_thumb: Color,

    ///Error
    pub error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#121212"),
            surface: Color::from_hex("#1E1E1E"),
            on_surface: Color::from_hex("#DDDDDD"),
            primary: Color::from_hex("#34AF82"),
            on_primary: Color::WHITE,
            outline: Color::from_hex("#555555"),
            focus: Color::from_hex("#88CCFF"),
            button_bg: Color::from_hex("#34AF82"),
            button_bg_hover: Color::from_hex("#2A8F6A"),
            button_bg_pressed: Color::from_hex("#1F7556"),
            scrollbar_track: Color(0xDD, 0xDD, 0xDD, 32),
            scrollbar_thumb: Color(0xDD, 0xDD, 0xDD, 140),

            error: Color::from_hex("#ae3636"),
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
            if let Some(v) = frame.get(&TypeId::of::<Theme>())
                && let Some(t) = v.downcast_ref::<Theme>() {
                    return *t;
                }
        }
        Theme::default()
    })
}

pub fn density() -> Density {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<Density>())
                && let Some(d) = v.downcast_ref::<Density>() {
                    return *d;
                }
        }
        Density::default()
    })
}

pub fn text_scale() -> TextScale {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<TextScale>())
                && let Some(ts) = v.downcast_ref::<TextScale>() {
                    return *ts;
                }
        }
        TextScale::default()
    })
}
