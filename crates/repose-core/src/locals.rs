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
#[must_use]
pub struct ColorScheme {
    pub background: Color,
    pub surface: Color,
    pub surface_variant: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,

    pub primary: Color,
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,

    pub outline: Color,
    pub outline_variant: Color,
    pub error: Color,
    pub on_error: Color,
    pub focus: Color,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#121212"),
            surface: Color::from_hex("#1E1E1E"),
            surface_variant: Color::from_hex("#2A2A2A"),
            on_surface: Color::from_hex("#DDDDDD"),
            on_surface_variant: Color::from_hex("#B8B8B8"),
            primary: Color::from_hex("#34AF82"),
            on_primary: Color::WHITE,
            secondary: Color::from_hex("#7BB6FF"),
            on_secondary: Color::from_hex("#0E1A2B"),
            tertiary: Color::from_hex("#E7A3FF"),
            on_tertiary: Color::from_hex("#2B0E2B"),
            outline: Color::from_hex("#555555"),
            outline_variant: Color::from_hex("#3A3A3A"),
            error: Color::from_hex("#AE3636"),
            on_error: Color::WHITE,
            focus: Color::from_hex("#88CCFF"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Typography {
    pub display_large: f32,
    pub display_medium: f32,
    pub display_small: f32,
    pub headline_large: f32,
    pub headline_medium: f32,
    pub headline_small: f32,
    pub title_large: f32,
    pub title_medium: f32,
    pub title_small: f32,
    pub body_large: f32,
    pub body_medium: f32,
    pub body_small: f32,
    pub label_large: f32,
    pub label_medium: f32,
    pub label_small: f32,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            display_large: 57.0,
            display_medium: 45.0,
            display_small: 36.0,
            headline_large: 32.0,
            headline_medium: 28.0,
            headline_small: 24.0,
            title_large: 22.0,
            title_medium: 16.0,
            title_small: 14.0,
            body_large: 16.0,
            body_medium: 14.0,
            body_small: 12.0,
            label_large: 14.0,
            label_medium: 12.0,
            label_small: 11.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Shapes {
    pub extra_small: f32,
    pub small: f32,
    pub medium: f32,
    pub large: f32,
    pub extra_large: f32,
}

impl Default for Shapes {
    fn default() -> Self {
        Self {
            extra_small: 4.0,
            small: 8.0,
            medium: 12.0,
            large: 16.0,
            extra_large: 28.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Spacing {
    pub xs: f32,
    pub sm: f32,
    pub md: f32,
    pub lg: f32,
    pub xl: f32,
    pub xxl: f32,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
            xxl: 32.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Elevation {
    pub level0: f32,
    pub level1: f32,
    pub level2: f32,
    pub level3: f32,
    pub level4: f32,
    pub level5: f32,
}

impl Default for Elevation {
    fn default() -> Self {
        Self {
            level0: 0.0,
            level1: 1.0,
            level2: 3.0,
            level3: 6.0,
            level4: 8.0,
            level5: 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Motion {
    pub fast_ms: u32,
    pub medium_ms: u32,
    pub slow_ms: u32,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            fast_ms: 120,
            medium_ms: 240,
            slow_ms: 360,
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Theme {
    pub colors: ColorScheme,
    pub typography: Typography,
    pub shapes: Shapes,
    pub spacing: Spacing,
    pub elevation: Elevation,
    pub motion: Motion,

    pub focus: Color,
    pub scrollbar_track: Color,
    pub scrollbar_thumb: Color,
    pub button_bg: Color,
    pub button_bg_hover: Color,
    pub button_bg_pressed: Color,

    pub background: Color,
    pub surface: Color,
    pub surface_variant: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub error: Color,
    pub on_error: Color,
}

impl Default for Theme {
    fn default() -> Self {
        let colors = ColorScheme::default();
        Self {
            background: colors.background,
            surface: colors.surface,
            surface_variant: colors.surface_variant,
            on_surface: colors.on_surface,
            on_surface_variant: colors.on_surface_variant,
            primary: colors.primary,
            on_primary: colors.on_primary,
            secondary: colors.secondary,
            on_secondary: colors.on_secondary,
            tertiary: colors.tertiary,
            on_tertiary: colors.on_tertiary,
            outline: colors.outline,
            outline_variant: colors.outline_variant,
            error: colors.error,
            on_error: colors.on_error,
            colors,
            typography: Typography::default(),
            shapes: Shapes::default(),
            spacing: Spacing::default(),
            elevation: Elevation::default(),
            motion: Motion::default(),
            focus: colors.focus,
            scrollbar_track: Color(0xDD, 0xDD, 0xDD, 32),
            scrollbar_thumb: Color(0xDD, 0xDD, 0xDD, 140),
            button_bg: colors.primary,
            button_bg_hover: Color::from_hex("#2A8F6A"),
            button_bg_pressed: Color::from_hex("#1F7556"),
        }
    }
}

impl Theme {
    pub fn apply_colors(&mut self) {
        self.background = self.colors.background;
        self.surface = self.colors.surface;
        self.surface_variant = self.colors.surface_variant;
        self.on_surface = self.colors.on_surface;
        self.on_surface_variant = self.colors.on_surface_variant;
        self.primary = self.colors.primary;
        self.on_primary = self.colors.on_primary;
        self.secondary = self.colors.secondary;
        self.on_secondary = self.colors.on_secondary;
        self.tertiary = self.colors.tertiary;
        self.on_tertiary = self.colors.on_tertiary;
        self.outline = self.colors.outline;
        self.outline_variant = self.colors.outline_variant;
        self.error = self.colors.error;
        self.on_error = self.colors.on_error;
        self.focus = self.colors.focus;
        self.button_bg = self.colors.primary;
    }

    pub fn sync_colors_from_fields(&mut self) {
        self.colors.background = self.background;
        self.colors.surface = self.surface;
        self.colors.surface_variant = self.surface_variant;
        self.colors.on_surface = self.on_surface;
        self.colors.on_surface_variant = self.on_surface_variant;
        self.colors.primary = self.primary;
        self.colors.on_primary = self.on_primary;
        self.colors.secondary = self.secondary;
        self.colors.on_secondary = self.on_secondary;
        self.colors.tertiary = self.tertiary;
        self.colors.on_tertiary = self.on_tertiary;
        self.colors.outline = self.outline;
        self.colors.outline_variant = self.outline_variant;
        self.colors.error = self.error;
        self.colors.on_error = self.on_error;
        self.colors.focus = self.focus;
    }

    pub fn with_colors(mut self, colors: ColorScheme) -> Self {
        self.colors = colors;
        self.apply_colors();
        self
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
