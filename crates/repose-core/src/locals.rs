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
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,

    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,

    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,

    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,

    pub background: Color,
    pub on_background: Color,
    pub surface: Color,
    pub on_surface: Color,
    pub surface_variant: Color,
    pub on_surface_variant: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_bright: Color,
    pub surface_dim: Color,
    pub surface_tint: Color,

    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,

    pub outline: Color,
    pub outline_variant: Color,

    pub scrim: Color,
    pub shadow: Color,
    pub focus: Color,
}

impl ColorScheme {
    pub fn dark() -> Self {
        Self {
            primary: Color::from_hex("#D0BCFF"),
            on_primary: Color::from_hex("#381E72"),
            primary_container: Color::from_hex("#4F378B"),
            on_primary_container: Color::from_hex("#EADDFF"),

            secondary: Color::from_hex("#CCC2DC"),
            on_secondary: Color::from_hex("#332D41"),
            secondary_container: Color::from_hex("#4A4458"),
            on_secondary_container: Color::from_hex("#E8DEF8"),

            tertiary: Color::from_hex("#EFB8C8"),
            on_tertiary: Color::from_hex("#492532"),
            tertiary_container: Color::from_hex("#633B48"),
            on_tertiary_container: Color::from_hex("#FFD8E4"),

            error: Color::from_hex("#F2B8B5"),
            on_error: Color::from_hex("#601410"),
            error_container: Color::from_hex("#8C1D18"),
            on_error_container: Color::from_hex("#F9DEDC"),

            background: Color::from_hex("#141218"),
            on_background: Color::from_hex("#E6E0E9"),
            surface: Color::from_hex("#141218"),
            on_surface: Color::from_hex("#E6E0E9"),
            surface_variant: Color::from_hex("#49454F"),
            on_surface_variant: Color::from_hex("#CAC4D0"),
            surface_container_lowest: Color::from_hex("#0F0D13"),
            surface_container_low: Color::from_hex("#1D1B20"),
            surface_container: Color::from_hex("#211F26"),
            surface_container_high: Color::from_hex("#2B2930"),
            surface_container_highest: Color::from_hex("#36343B"),
            surface_bright: Color::from_hex("#3B383E"),
            surface_dim: Color::from_hex("#141218"),
            surface_tint: Color::from_hex("#D0BCFF"),

            inverse_surface: Color::from_hex("#E6E0E9"),
            inverse_on_surface: Color::from_hex("#322F35"),
            inverse_primary: Color::from_hex("#6750A4"),

            outline: Color::from_hex("#938F99"),
            outline_variant: Color::from_hex("#49454F"),

            scrim: Color::from_hex("#000000"),
            shadow: Color::from_hex("#000000"),
            focus: Color::from_hex("#88CCFF"),
        }
    }

    pub fn light() -> Self {
        Self {
            primary: Color::from_hex("#6750A4"),
            on_primary: Color::WHITE,
            primary_container: Color::from_hex("#EADDFF"),
            on_primary_container: Color::from_hex("#21005D"),

            secondary: Color::from_hex("#625B71"),
            on_secondary: Color::WHITE,
            secondary_container: Color::from_hex("#E8DEF8"),
            on_secondary_container: Color::from_hex("#1D192B"),

            tertiary: Color::from_hex("#7D5260"),
            on_tertiary: Color::WHITE,
            tertiary_container: Color::from_hex("#FFD8E4"),
            on_tertiary_container: Color::from_hex("#31111D"),

            error: Color::from_hex("#B3261E"),
            on_error: Color::WHITE,
            error_container: Color::from_hex("#F9DEDC"),
            on_error_container: Color::from_hex("#410E0B"),

            background: Color::from_hex("#FEF7FF"),
            on_background: Color::from_hex("#1D1B20"),
            surface: Color::from_hex("#FEF7FF"),
            on_surface: Color::from_hex("#1D1B20"),
            surface_variant: Color::from_hex("#E7E0EC"),
            on_surface_variant: Color::from_hex("#49454F"),
            surface_container_lowest: Color::WHITE,
            surface_container_low: Color::from_hex("#F7F2FA"),
            surface_container: Color::from_hex("#F3EDF7"),
            surface_container_high: Color::from_hex("#ECE6F0"),
            surface_container_highest: Color::from_hex("#E6E0E9"),
            surface_bright: Color::from_hex("#FEF7FF"),
            surface_dim: Color::from_hex("#DED8E1"),
            surface_tint: Color::from_hex("#6750A4"),

            inverse_surface: Color::from_hex("#322F35"),
            inverse_on_surface: Color::from_hex("#F5EFF7"),
            inverse_primary: Color::from_hex("#D0BCFF"),

            outline: Color::from_hex("#79747E"),
            outline_variant: Color::from_hex("#CAC4D0"),

            scrim: Color::from_hex("#000000"),
            shadow: Color::from_hex("#000000"),
            focus: Color::from_hex("#1D4ED8"),
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::dark()
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
    pub on_background: Color,
    pub surface: Color,
    pub surface_variant: Color,
    pub on_surface: Color,
    pub on_surface_variant: Color,
    pub surface_container_lowest: Color,
    pub surface_container_low: Color,
    pub surface_container: Color,
    pub surface_container_high: Color,
    pub surface_container_highest: Color,
    pub surface_bright: Color,
    pub surface_dim: Color,
    pub surface_tint: Color,
    pub primary: Color,
    pub on_primary: Color,
    pub primary_container: Color,
    pub on_primary_container: Color,
    pub secondary: Color,
    pub on_secondary: Color,
    pub secondary_container: Color,
    pub on_secondary_container: Color,
    pub tertiary: Color,
    pub on_tertiary: Color,
    pub tertiary_container: Color,
    pub on_tertiary_container: Color,
    pub error: Color,
    pub on_error: Color,
    pub error_container: Color,
    pub on_error_container: Color,
    pub inverse_surface: Color,
    pub inverse_on_surface: Color,
    pub inverse_primary: Color,
    pub outline: Color,
    pub outline_variant: Color,
    pub scrim: Color,
    pub shadow: Color,
}

impl Default for Theme {
    fn default() -> Self {
        let colors = ColorScheme::default();
        Self {
            background: colors.background,
            on_background: colors.on_background,
            surface: colors.surface,
            surface_variant: colors.surface_variant,
            on_surface: colors.on_surface,
            on_surface_variant: colors.on_surface_variant,
            surface_container_lowest: colors.surface_container_lowest,
            surface_container_low: colors.surface_container_low,
            surface_container: colors.surface_container,
            surface_container_high: colors.surface_container_high,
            surface_container_highest: colors.surface_container_highest,
            surface_bright: colors.surface_bright,
            surface_dim: colors.surface_dim,
            surface_tint: colors.surface_tint,
            primary: colors.primary,
            on_primary: colors.on_primary,
            primary_container: colors.primary_container,
            on_primary_container: colors.on_primary_container,
            secondary: colors.secondary,
            on_secondary: colors.on_secondary,
            secondary_container: colors.secondary_container,
            on_secondary_container: colors.on_secondary_container,
            tertiary: colors.tertiary,
            on_tertiary: colors.on_tertiary,
            tertiary_container: colors.tertiary_container,
            on_tertiary_container: colors.on_tertiary_container,
            error: colors.error,
            on_error: colors.on_error,
            error_container: colors.error_container,
            on_error_container: colors.on_error_container,
            inverse_surface: colors.inverse_surface,
            inverse_on_surface: colors.inverse_on_surface,
            inverse_primary: colors.inverse_primary,
            outline: colors.outline,
            outline_variant: colors.outline_variant,
            scrim: colors.scrim,
            shadow: colors.shadow,
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
            button_bg_hover: colors.primary_container,
            button_bg_pressed: colors.on_primary_container,
        }
    }
}

impl Theme {
    pub fn apply_colors(&mut self) {
        self.background = self.colors.background;
        self.on_background = self.colors.on_background;
        self.surface = self.colors.surface;
        self.surface_variant = self.colors.surface_variant;
        self.on_surface = self.colors.on_surface;
        self.on_surface_variant = self.colors.on_surface_variant;
        self.surface_container_lowest = self.colors.surface_container_lowest;
        self.surface_container_low = self.colors.surface_container_low;
        self.surface_container = self.colors.surface_container;
        self.surface_container_high = self.colors.surface_container_high;
        self.surface_container_highest = self.colors.surface_container_highest;
        self.surface_bright = self.colors.surface_bright;
        self.surface_dim = self.colors.surface_dim;
        self.surface_tint = self.colors.surface_tint;
        self.primary = self.colors.primary;
        self.on_primary = self.colors.on_primary;
        self.primary_container = self.colors.primary_container;
        self.on_primary_container = self.colors.on_primary_container;
        self.secondary = self.colors.secondary;
        self.on_secondary = self.colors.on_secondary;
        self.secondary_container = self.colors.secondary_container;
        self.on_secondary_container = self.colors.on_secondary_container;
        self.tertiary = self.colors.tertiary;
        self.on_tertiary = self.colors.on_tertiary;
        self.tertiary_container = self.colors.tertiary_container;
        self.on_tertiary_container = self.colors.on_tertiary_container;
        self.error = self.colors.error;
        self.on_error = self.colors.on_error;
        self.error_container = self.colors.error_container;
        self.on_error_container = self.colors.on_error_container;
        self.inverse_surface = self.colors.inverse_surface;
        self.inverse_on_surface = self.colors.inverse_on_surface;
        self.inverse_primary = self.colors.inverse_primary;
        self.outline = self.colors.outline;
        self.outline_variant = self.colors.outline_variant;
        self.scrim = self.colors.scrim;
        self.shadow = self.colors.shadow;
        self.focus = self.colors.focus;
        self.button_bg = self.colors.primary;
        self.button_bg_hover = self.colors.primary_container;
        self.button_bg_pressed = self.colors.on_primary_container;
    }

    pub fn sync_colors_from_fields(&mut self) {
        self.colors.background = self.background;
        self.colors.on_background = self.on_background;
        self.colors.surface = self.surface;
        self.colors.surface_variant = self.surface_variant;
        self.colors.on_surface = self.on_surface;
        self.colors.on_surface_variant = self.on_surface_variant;
        self.colors.surface_container_lowest = self.surface_container_lowest;
        self.colors.surface_container_low = self.surface_container_low;
        self.colors.surface_container = self.surface_container;
        self.colors.surface_container_high = self.surface_container_high;
        self.colors.surface_container_highest = self.surface_container_highest;
        self.colors.surface_bright = self.surface_bright;
        self.colors.surface_dim = self.surface_dim;
        self.colors.surface_tint = self.surface_tint;
        self.colors.primary = self.primary;
        self.colors.on_primary = self.on_primary;
        self.colors.primary_container = self.primary_container;
        self.colors.on_primary_container = self.on_primary_container;
        self.colors.secondary = self.secondary;
        self.colors.on_secondary = self.on_secondary;
        self.colors.secondary_container = self.secondary_container;
        self.colors.on_secondary_container = self.on_secondary_container;
        self.colors.tertiary = self.tertiary;
        self.colors.on_tertiary = self.on_tertiary;
        self.colors.tertiary_container = self.tertiary_container;
        self.colors.on_tertiary_container = self.on_tertiary_container;
        self.colors.error = self.error;
        self.colors.on_error = self.on_error;
        self.colors.error_container = self.error_container;
        self.colors.on_error_container = self.on_error_container;
        self.colors.inverse_surface = self.inverse_surface;
        self.colors.inverse_on_surface = self.inverse_on_surface;
        self.colors.inverse_primary = self.inverse_primary;
        self.colors.outline = self.outline;
        self.colors.outline_variant = self.outline_variant;
        self.colors.scrim = self.scrim;
        self.colors.shadow = self.shadow;
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
