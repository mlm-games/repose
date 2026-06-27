//! # Theming and locals
//!
//! Repose uses thread‑local “composition locals” for global UI parameters:
//!
//! - `Theme` - colors for surfaces, text, controls, etc.
//! - `Density` - dp→px device scale factor (platform sets this).
//! - `UiScale` - app-controlled UI scale multiplier (defaults to 1.0).
//! - `TextScale` - user text scaling (defaults to 1.0).
//! - `TextDirection` - LTR or RTL (defaults to LTR).
//!
//! Locals can be overridden for a subtree with `with_*`. If no local is set,
//! getters fall back to global defaults (which an app can set each frame).

use std::ops::Deref;

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use parking_lot::RwLock;

use crate::Color;
use crate::animation::{AnimationSpec, Easing};
use web_time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextDirection {
    #[default]
    Ltr,
    Rtl,
}

thread_local! {
    static LOCALS_STACK: RefCell<Vec<HashMap<TypeId, Box<dyn Any>>>> = RefCell::new(Vec::new());
}

#[derive(Clone, Copy, Debug, Default)]
struct Defaults {
    theme: Theme,
    text_direction: TextDirection,
    ui_scale: UiScale,
    text_scale: TextScale,
    density: Density,
    window_insets: WindowInsets,
    window_size_class: WindowSizeClass,
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

/// Convenience: convert a raw px scalar into dp using current Density * UiScale.
pub fn px_to_dp(px: f32) -> f32 {
    let scale = density().scale * ui_scale().0;
    if scale <= 0.0 { 0.0 } else { px / scale }
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
            primary: Color::from_hex("#69FDBE"),
            on_primary: Color::from_hex("#003020"),
            primary_container: Color::from_hex("#004D40"),
            on_primary_container: Color::from_hex("#6FF7F6"),

            secondary: Color::from_hex("#B3C9A7"),
            on_secondary: Color::from_hex("#1C3519"),
            secondary_container: Color::from_hex("#334D2E"),
            on_secondary_container: Color::from_hex("#CCE8B3"),

            tertiary: Color::from_hex("#FFC9C1"),
            on_tertiary: Color::from_hex("#3F1619"),
            tertiary_container: Color::from_hex("#5D1F22"),
            on_tertiary_container: Color::from_hex("#FFDBD8"),

            error: Color::from_hex("#F2B8B5"),
            on_error: Color::from_hex("#601410"),
            error_container: Color::from_hex("#8C1D18"),
            on_error_container: Color::from_hex("#F9DEDC"),

            background: Color::from_hex("#1A1C1E"),
            on_background: Color::from_hex("#E6E1E5"),
            surface: Color::from_hex("#1A1C1E"),
            on_surface: Color::from_hex("#E6E1E5"),
            surface_variant: Color::from_hex("#44474E"),
            on_surface_variant: Color::from_hex("#C4C6CE"),
            surface_container_lowest: Color::from_hex("#0A0A0C"),
            surface_container_low: Color::from_hex("#141115"),
            surface_container: Color::from_hex("#19131A"),
            surface_container_high: Color::from_hex("#1F1B22"),
            surface_container_highest: Color::from_hex("#2A2930"),
            surface_bright: Color::from_hex("#26292F"),
            surface_dim: Color::from_hex("#1A1C1E"),
            surface_tint: Color::from_hex("#69FDBE"),

            inverse_surface: Color::from_hex("#E6E1E5"),
            inverse_on_surface: Color::from_hex("#2A2930"),
            inverse_primary: Color::from_hex("#005048"),

            outline: Color::from_hex("#74777F"),
            outline_variant: Color::from_hex("#44474E"),

            scrim: Color::from_hex("#000000"),
            shadow: Color::from_hex("#000000"),
            focus: Color::from_hex("#006A6A"),
        }
    }

    pub fn light() -> Self {
        Self {
            primary: Color::from_hex("#006A6A"),
            on_primary: Color::WHITE,
            primary_container: Color::from_hex("#9EF0EC"),
            on_primary_container: Color::from_hex("#002020"),

            secondary: Color::from_hex("#586146"),
            on_secondary: Color::WHITE,
            secondary_container: Color::from_hex("#D8E3B8"),
            on_secondary_container: Color::from_hex("#161C0A"),

            tertiary: Color::from_hex("#744639"),
            on_tertiary: Color::WHITE,
            tertiary_container: Color::from_hex("#FFD9CD"),
            on_tertiary_container: Color::from_hex("#2C0E07"),

            error: Color::from_hex("#BA1A1A"),
            on_error: Color::WHITE,
            error_container: Color::from_hex("#FFDAD6"),
            on_error_container: Color::from_hex("#410002"),

            background: Color::from_hex("#FEF7FF"),
            on_background: Color::from_hex("#1A1C1E"),
            surface: Color::from_hex("#FEF7FF"),
            on_surface: Color::from_hex("#1A1C1E"),
            surface_variant: Color::from_hex("#E1E3DE"),
            on_surface_variant: Color::from_hex("#44474E"),
            surface_container_lowest: Color::WHITE,
            surface_container_low: Color::from_hex("#F4F5F0"),
            surface_container: Color::from_hex("#EEF0E9"),
            surface_container_high: Color::from_hex("#E9EAE4"),
            surface_container_highest: Color::from_hex("#E3E5DF"),
            surface_bright: Color::from_hex("#FEF7FF"),
            surface_dim: Color::from_hex("#DEDAD0"),
            surface_tint: Color::from_hex("#006A6A"),

            inverse_surface: Color::from_hex("#2F3033"),
            inverse_on_surface: Color::from_hex("#F1F0F4"),
            inverse_primary: Color::from_hex("#69FDBE"),

            outline: Color::from_hex("#74777F"),
            outline_variant: Color::from_hex("#C4C6CE"),

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

/// Centralized animation specs for M3 motion design.
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct MotionScheme {
    /// Shape / size / bounds transitions (e.g., indicator position, elevation).
    /// M3 standard: 200 ms FastOutSlowIn.
    pub shape: AnimationSpec,
    /// Color state transitions (e.g., label, tab text, selection).
    /// M3 standard: 150 ms FastOutSlowIn.
    pub color: AnimationSpec,
    /// Quick color changes (e.g., checkbox fill, switch track, radio ring).
    /// M3 standard: 100 ms FastOutSlowIn.
    pub color_fast: AnimationSpec,
    /// Overlay / popup enter‑exit (menus, dialogs, tooltips).
    /// M3 standard: 120 ms FastOutSlowIn.
    pub overlay: AnimationSpec,
    /// Spring‑based positional animation (sheets, drawers, swipe‑to‑dismiss).
    /// M3 standard: gentle spring (ζ = 0.5, k = 200).
    pub spring: AnimationSpec,
    /// Expanding containers (search bar, docked search suggestions).
    /// M3 standard: 250 ms FastOutSlowIn.
    pub expand: AnimationSpec,
    /// Large layout transitions (bottom sheet height, scaffold reflow).
    /// M3 standard: 300 ms EaseOut.
    pub layout: AnimationSpec,
}

impl Default for MotionScheme {
    fn default() -> Self {
        Self {
            shape: AnimationSpec::tween(Duration::from_millis(200), Easing::FastOutSlowIn),
            color: AnimationSpec::tween(Duration::from_millis(150), Easing::FastOutSlowIn),
            color_fast: AnimationSpec::tween(Duration::from_millis(100), Easing::FastOutSlowIn),
            overlay: AnimationSpec::tween(Duration::from_millis(120), Easing::FastOutSlowIn),
            spring: AnimationSpec::spring_gentle(),
            expand: AnimationSpec::tween(Duration::from_millis(250), Easing::FastOutSlowIn),
            layout: AnimationSpec::tween(Duration::from_millis(300), Easing::EaseOut),
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
    pub motion: MotionScheme,

    pub focus: Color,
    pub scrollbar_track: Color,
    pub scrollbar_thumb: Color,
    pub button_bg: Color,
    pub button_bg_hover: Color,
    pub button_bg_pressed: Color,
}

impl Deref for Theme {
    type Target = ColorScheme;
    fn deref(&self) -> &Self::Target {
        &self.colors
    }
}

impl Default for Theme {
    fn default() -> Self {
        let colors = ColorScheme::default();
        Self {
            colors,
            typography: Typography::default(),
            shapes: Shapes::default(),
            spacing: Spacing::default(),
            elevation: Elevation::default(),
            motion: MotionScheme::default(),
            focus: colors.focus,
            scrollbar_track: Color::TRANSPARENT,
            scrollbar_thumb: colors.outline.with_alpha(179),
            button_bg: colors.primary,
            button_bg_hover: colors.primary_container,
            button_bg_pressed: colors.on_primary_container,
        }
    }
}

impl Theme {
    pub fn with_colors(mut self, colors: ColorScheme) -> Self {
        self.colors = colors;
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

pub fn with_window_insets<R>(insets: WindowInsets, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<WindowInsets>(), Box::new(insets));
        f()
    })
}

#[derive(Clone, Copy, Debug)]
pub struct ContentColor(pub Color);

pub fn with_content_color<R>(color: Color, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<ContentColor>(), Box::new(ContentColor(color)));
        f()
    })
}

pub fn content_color() -> Color {
    get_local::<ContentColor>()
        .map(|c| c.0)
        .unwrap_or_else(|| theme().on_surface)
}

/// System window insets (status bar, navigation bar, IME keyboard, etc.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WindowInsets {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    /// Soft keyboard (IME) inset from bottom of screen. Set by platform runner
    /// when the keyboard opens/closes. Used by `imePadding()` modifier.
    pub ime_bottom: f32,
}

/// Set the global default window insets (platform should call this when insets change).
pub fn set_window_insets_default(insets: WindowInsets) {
    defaults().write().window_insets = insets;
    set_local_boxed(TypeId::of::<WindowInsets>(), Box::new(insets));
}

/// Update just the IME bottom inset (keyboard height in px). Platform runners
/// call this when the soft keyboard opens/closes.
pub fn set_ime_inset(height_px: f32) {
    let mut insets = defaults().write().window_insets;
    insets.ime_bottom = height_px;
    // Also immediately set the thread-local so it's visible to the current frame
    set_local_boxed(TypeId::of::<WindowInsets>(), Box::new(insets));
}

/// Query current window insets.
pub fn window_insets() -> WindowInsets {
    get_local::<WindowInsets>().unwrap_or_else(|| defaults().read().window_insets)
}

/// Coarse width category for a window, computed from its current size.
///
/// Thresholds (in dp) match the Material 3 adaptive spec:
///
/// - [`WidthClass::Compact`]  : width < 600 dp
/// - [`WidthClass::Medium`]   : 600 dp <= width < 840 dp
/// - [`WidthClass::Expanded`] : width >= 840 dp
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum WidthClass {
    #[default]
    Compact,
    Medium,
    Expanded,
}

/// Coarse height category for a window, computed from its current size.
///
/// Thresholds (in dp) match the Material 3 adaptive spec:
///
/// - [`HeightClass::Compact`]  : height < 480 dp
/// - [`HeightClass::Medium`]   : 480 dp <= height < 900 dp
/// - [`HeightClass::Expanded`] : height >= 900 dp
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum HeightClass {
    #[default]
    Compact,
    Medium,
    Expanded,
}

/// Snapshot of the current window's size category.
///
/// The `LayoutEngine` updates the `WindowSizeClass` default local every time
/// the window is resized, so UI can read it via [`window_size_class()`] during
/// composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct WindowSizeClass {
    pub width: WidthClass,
    pub height: HeightClass,
}

impl WindowSizeClass {
    /// `true` when there is enough horizontal space for multi-pane layouts.
    pub fn is_expanded_width(&self) -> bool {
        matches!(self.width, WidthClass::Expanded)
    }
    /// `true` when there is enough horizontal space for a two-pane layout
    /// (list + detail). Per M3 this is Medium or wider.
    pub fn is_at_least_medium_width(&self) -> bool {
        matches!(self.width, WidthClass::Medium | WidthClass::Expanded)
    }
}

/// Compute a [`WindowSizeClass`] from a window size in physical pixels and
/// the current dp→px density scale (`Density.scale * UiScale.0`).
pub fn calculate_window_size_class(
    width_px: u32,
    height_px: u32,
    density_scale: f32,
) -> WindowSizeClass {
    let density = density_scale.max(0.0001);
    let width_dp = (width_px as f32) / density;
    let height_dp = (height_px as f32) / density;

    let width = if width_dp < 600.0 {
        WidthClass::Compact
    } else if width_dp < 840.0 {
        WidthClass::Medium
    } else {
        WidthClass::Expanded
    };
    let height = if height_dp < 480.0 {
        HeightClass::Compact
    } else if height_dp < 900.0 {
        HeightClass::Medium
    } else {
        HeightClass::Expanded
    };

    WindowSizeClass { width, height }
}

/// Set the global default window size class used when no local is active.
/// Called by the `LayoutEngine` on resize.
pub fn set_window_size_class_default(class: WindowSizeClass) {
    defaults().write().window_size_class = class;
}

/// Override the window size class for a subtree of the composition.
pub fn with_window_size_class<R>(class: WindowSizeClass, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<WindowSizeClass>(), Box::new(class));
        f()
    })
}

/// Query current window size class. Returns a default-initialized
/// `WindowSizeClass` (Compact/Compact) if nothing has been set yet.
pub fn window_size_class() -> WindowSizeClass {
    get_local::<WindowSizeClass>().unwrap_or_else(|| defaults().read().window_size_class)
}

macro_rules! def_local_getter {
    ($fn_name:ident, $ty:ty, $default_field:ident) => {
        pub fn $fn_name() -> $ty {
            get_local::<$ty>().unwrap_or_else(|| defaults().read().$default_field)
        }
    };
}

def_local_getter!(theme, Theme, theme);
def_local_getter!(density, Density, density);
def_local_getter!(ui_scale, UiScale, ui_scale);
def_local_getter!(text_scale, TextScale, text_scale);
def_local_getter!(text_direction, TextDirection, text_direction);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_class_thresholds_match_m3() {
        // Density 1.0 (1 dp = 1 px) for clarity.
        assert_eq!(
            calculate_window_size_class(100, 100, 1.0).width,
            WidthClass::Compact
        );
        assert_eq!(
            calculate_window_size_class(599, 100, 1.0).width,
            WidthClass::Compact
        );
        assert_eq!(
            calculate_window_size_class(600, 100, 1.0).width,
            WidthClass::Medium
        );
        assert_eq!(
            calculate_window_size_class(839, 100, 1.0).width,
            WidthClass::Medium
        );
        assert_eq!(
            calculate_window_size_class(840, 100, 1.0).width,
            WidthClass::Expanded
        );
        assert_eq!(
            calculate_window_size_class(2000, 100, 1.0).width,
            WidthClass::Expanded
        );
    }

    #[test]
    fn height_class_thresholds_match_m3() {
        assert_eq!(
            calculate_window_size_class(100, 100, 1.0).height,
            HeightClass::Compact
        );
        assert_eq!(
            calculate_window_size_class(100, 479, 1.0).height,
            HeightClass::Compact
        );
        assert_eq!(
            calculate_window_size_class(100, 480, 1.0).height,
            HeightClass::Medium
        );
        assert_eq!(
            calculate_window_size_class(100, 899, 1.0).height,
            HeightClass::Medium
        );
        assert_eq!(
            calculate_window_size_class(100, 900, 1.0).height,
            HeightClass::Expanded
        );
    }

    #[test]
    fn density_scales_thresholds() {
        // 2.0x density: 600 dp = 1200 px.
        let c = calculate_window_size_class(1199, 100, 2.0);
        assert_eq!(c.width, WidthClass::Compact);
        let c = calculate_window_size_class(1200, 100, 2.0);
        assert_eq!(c.width, WidthClass::Medium);
    }

    #[test]
    fn is_at_least_medium_width() {
        let c = WindowSizeClass {
            width: WidthClass::Compact,
            height: HeightClass::Compact,
        };
        assert!(!c.is_at_least_medium_width());
        let c = WindowSizeClass {
            width: WidthClass::Medium,
            height: HeightClass::Compact,
        };
        assert!(c.is_at_least_medium_width());
        let c = WindowSizeClass {
            width: WidthClass::Expanded,
            height: HeightClass::Compact,
        };
        assert!(c.is_at_least_medium_width());
    }
}
