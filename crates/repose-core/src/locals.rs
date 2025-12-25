//! # Theming and locals
//!
//! Repose uses thread‑local “composition locals” for global UI parameters:
//!
//! - `Theme` — colors for surfaces, text, controls, etc.
//! - `Density` — dp→px device scale factor (platform sets this).
//! - `UiScale` — app-controlled UI scale multiplier (defaults to 1.0).
//! - `TextScale` — user text scaling (defaults to 1.0).
//! - `TextDirection` — LTR or RTL (defaults to LTR).
//!
//! Locals can be overridden for a subtree with `with_*`. If no local is set,
//! getters fall back to global defaults (which an app can set each frame).

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::Color;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

thread_local! {
    static LOCALS_STACK: RefCell<Vec<HashMap<TypeId, Box<dyn Any>>>> = RefCell::new(Vec::new());
}

#[derive(Clone, Copy, Debug)]
struct Defaults {
    theme: Theme,
    text_direction: TextDirection,
    ui_scale: UiScale,
    text_scale: TextScale,
    density: Density,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            text_direction: TextDirection::default(),
            ui_scale: UiScale::default(),
            text_scale: TextScale::default(),
            density: Density::default(),
        }
    }
}

static DEFAULTS: OnceLock<RwLock<Defaults>> = OnceLock::new();

fn defaults() -> &'static RwLock<Defaults> {
    DEFAULTS.get_or_init(|| RwLock::new(Defaults::default()))
}

/// Set the global default theme used when no local Theme is active.
pub fn set_theme_default(t: Theme) {
    defaults().write().theme = t;
}

/// Set the global default text direction used when no local TextDirection is active.
pub fn set_text_direction_default(d: TextDirection) {
    defaults().write().text_direction = d;
}

/// Set the global default UI scale used when no local UiScale is active.
pub fn set_ui_scale_default(s: UiScale) {
    defaults().write().ui_scale = UiScale(s.0.max(0.0));
}

/// Set the global default text scale used when no local TextScale is active.
pub fn set_text_scale_default(s: TextScale) {
    defaults().write().text_scale = TextScale(s.0.max(0.0));
}

/// Set the global default device density (dp→px) used when no local Density is active.
/// Platform runners should call this whenever the window scale factor changes.
pub fn set_density_default(d: Density) {
    defaults().write().density = Density {
        scale: d.scale.max(0.0),
    };
}

// ---- Units ----

/// density‑independent pixels (dp)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dp(pub f32);

impl Dp {
    /// Converts this dp value into physical pixels using current Density * UiScale.
    pub fn to_px(self) -> f32 {
        self.0 * density().scale * ui_scale().0
    }
}

/// Convenience: convert a raw dp scalar into px using current Density * UiScale.
pub fn dp_to_px(dp: f32) -> f32 {
    Dp(dp).to_px()
}

fn with_locals_frame<R>(f: impl FnOnce() -> R) -> R {
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

fn get_local<T: 'static + Copy>() -> Option<T> {
    LOCALS_STACK.with(|st| {
        for frame in st.borrow().iter().rev() {
            if let Some(v) = frame.get(&TypeId::of::<T>())
                && let Some(t) = v.downcast_ref::<T>()
            {
                return Some(*t);
            }
        }
        None
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub background: Color,
    pub surface: Color,
    pub on_surface: Color,

    pub primary: Color,
    pub on_primary: Color,

    pub outline: Color,
    pub focus: Color,

    pub button_bg: Color,
    pub button_bg_hover: Color,
    pub button_bg_pressed: Color,

    pub scrollbar_track: Color,
    pub scrollbar_thumb: Color,

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

/// Platform/device scale (dp→px multiplier). Platform runner should set this.
#[derive(Clone, Copy, Debug)]
pub struct Density {
    pub scale: f32,
}
impl Default for Density {
    fn default() -> Self {
        Self { scale: 1.0 }
    }
}

/// Additional UI scale multiplier (app-controlled).
#[derive(Clone, Copy, Debug)]
pub struct UiScale(pub f32);
impl Default for UiScale {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TextScale(pub f32);
impl Default for TextScale {
    fn default() -> Self {
        Self(1.0)
    }
}

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

pub fn with_ui_scale<R>(s: UiScale, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<UiScale>(), Box::new(s));
        f()
    })
}

pub fn with_text_scale<R>(ts: TextScale, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<TextScale>(), Box::new(ts));
        f()
    })
}

pub fn with_text_direction<R>(dir: TextDirection, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<TextDirection>(), Box::new(dir));
        f()
    })
}

pub fn theme() -> Theme {
    get_local::<Theme>().unwrap_or_else(|| defaults().read().theme)
}

pub fn density() -> Density {
    get_local::<Density>().unwrap_or_else(|| defaults().read().density)
}

pub fn ui_scale() -> UiScale {
    get_local::<UiScale>().unwrap_or_else(|| defaults().read().ui_scale)
}

pub fn text_scale() -> TextScale {
    get_local::<TextScale>().unwrap_or_else(|| defaults().read().text_scale)
}

pub fn text_direction() -> TextDirection {
    get_local::<TextDirection>().unwrap_or_else(|| defaults().read().text_direction)
}
