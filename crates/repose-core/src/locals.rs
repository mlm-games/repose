//! # Theming and locals
//!
//! Repose uses thread‑local “composition locals” for global UI parameters:
//!
//! - `Theme` - colors for surfaces, text, controls, etc.
//! - `Density` - dp->px device scale factor (platform sets this).
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
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

use parking_lot::RwLock;

use std::rc::Rc;

use crate::Color;
use crate::animation::{AnimationSpec, Easing};
use crate::indication::IndicationNodeFactory;
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

#[derive(Clone, Copy)]
pub(crate) enum LocalId {
    Theme,
    Density,
    UiScale,
    TextScale,
    TextDirection,
    WindowInsets,
    WindowSizeClass,
    ContainerWidth,
    ContainerHeight,
    ContentColor,
    TextSize,
    Indication,
    InputMode,
}

impl LocalId {
    fn index(self) -> usize {
        self as usize
    }
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
    /// Logical window container size in dp (defaults match a phone portrait).
    container_width: f32,
    container_height: f32,
}

static DEFAULTS: OnceLock<RwLock<Defaults>> = OnceLock::new();

fn defaults() -> &'static RwLock<Defaults> {
    DEFAULTS.get_or_init(|| {
        RwLock::new(Defaults {
            container_width: 360.0,
            container_height: 800.0,
            ..Default::default()
        })
    })
}

/// Set the global default theme used when no local Theme is active.
pub fn set_theme_default(t: Theme) {
    let changed = {
        let mut defaults = defaults().write();
        let changed = fingerprint_theme(&defaults.theme) != fingerprint_theme(&t);
        defaults.theme = t;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set the global default text direction used when no local TextDirection is active.
pub fn set_text_direction_default(d: TextDirection) {
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.text_direction != d;
        defaults.text_direction = d;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set the global default UI scale used when no local UiScale is active.
pub fn set_ui_scale_default(s: UiScale) {
    let value = UiScale(s.0.max(0.0));
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.ui_scale.0.to_bits() != value.0.to_bits();
        defaults.ui_scale = value;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set the global default text scale used when no local TextScale is active.
pub fn set_text_scale_default(s: TextScale) {
    let value = TextScale(s.0.max(0.0));
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.text_scale.0.to_bits() != value.0.to_bits();
        defaults.text_scale = value;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set the global default device density (dp->px) used when no local Density is active.
/// Platform runners should call this whenever the window scale factor changes.
pub fn set_density_default(d: Density) {
    let value = Density {
        scale: d.scale.max(0.0),
    };
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.density.scale.to_bits() != value.scale.to_bits();
        defaults.density = value;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

pub use crate::units::{Dp, DpOffset, DpRect, DpSize, Px, Sp, UnitExt};

/// Effective dp→px scale: device density × app UI scale.
#[inline]
pub fn effective_density_scale() -> f32 {
    (density().scale * ui_scale().0).max(0.0001)
}

/// Convert [`Dp`] into [`Px`] using current Density * UiScale.
#[inline]
pub fn dp_to_px(dp: Dp) -> Px {
    dp.to_px()
}

/// Convert [`Px`] into [`Dp`] using current Density * UiScale.
#[inline]
pub fn px_to_dp(px: Px) -> Dp {
    px.to_dp()
}

fn with_locals_frame<R>(f: impl FnOnce() -> R) -> R {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            let frame = LOCALS_STACK
                .try_with(|stack| {
                    stack
                        .try_borrow_mut()
                        .ok()
                        .and_then(|mut stack| stack.pop())
                })
                .ok()
                .flatten();
            drop(frame);
        }
    }
    LOCALS_STACK.with(|stack| stack.borrow_mut().push(HashMap::new()));
    let _guard = Guard;
    f()
}

fn set_local_boxed(t: TypeId, value: Box<dyn Any>) {
    let old = LOCALS_STACK
        .try_with(|stack| {
            let mut stack = stack.try_borrow_mut().ok()?;
            let old = stack.last_mut()?.insert(t, value);
            Some(old)
        })
        .ok()
        .flatten();
    drop(old);
}

fn get_local<T: 'static + Copy>() -> Option<T> {
    LOCALS_STACK.with(|stack| {
        for frame in stack.borrow().iter().rev() {
            if let Some(value) = frame.get(&TypeId::of::<T>())
                && let Some(value) = value.downcast_ref::<T>()
            {
                return Some(*value);
            }
        }
        None
    })
}

fn record_local<T: 'static>(id: LocalId, value: &T, fingerprint: impl FnOnce(&T) -> u64) {
    crate::scope_cache::record_scope_local_read(id.index(), fingerprint(value));
}

fn fingerprint_color(value: &Color) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.0.hash(&mut hasher);
    value.1.hash(&mut hasher);
    value.2.hash(&mut hasher);
    value.3.hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_theme(value: &Theme) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    format!("{value:?}").hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_text_size(value: Option<Sp>) -> u64 {
    value.map(|size| size.0.to_bits() as u64).unwrap_or(0)
}

fn fingerprint_input_mode(value: crate::input::InputMode) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(&value).hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_f32(value: f32) -> u64 {
    value.to_bits() as u64
}

fn fingerprint_window_insets(value: &WindowInsets) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.top.to_bits().hash(&mut hasher);
    value.bottom.to_bits().hash(&mut hasher);
    value.left.to_bits().hash(&mut hasher);
    value.right.to_bits().hash(&mut hasher);
    value.ime_bottom.to_bits().hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_window_size_class(value: &WindowSizeClass) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    value.width.hash(&mut hasher);
    value.height.hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_text_direction(value: &TextDirection) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::mem::discriminant(value).hash(&mut hasher);
    hasher.finish()
}

fn fingerprint_indication(value: &Option<Rc<dyn IndicationNodeFactory>>) -> u64 {
    value
        .as_ref()
        .map(|value| Rc::as_ptr(value) as *const () as usize as u64)
        .unwrap_or(0)
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

    /// Compose `ColorScheme.contentColorFor(backgroundColor)`.
    /// Maps a scheme container/background role to its paired on-* content color.
    pub fn content_color_for(self, background: Color) -> Option<Color> {
        if background == self.primary {
            Some(self.on_primary)
        } else if background == self.primary_container {
            Some(self.on_primary_container)
        } else if background == self.secondary {
            Some(self.on_secondary)
        } else if background == self.secondary_container {
            Some(self.on_secondary_container)
        } else if background == self.tertiary {
            Some(self.on_tertiary)
        } else if background == self.tertiary_container {
            Some(self.on_tertiary_container)
        } else if background == self.background {
            Some(self.on_background)
        } else if background == self.surface
            || background == self.surface_bright
            || background == self.surface_dim
            || background == self.surface_container
            || background == self.surface_container_low
            || background == self.surface_container_high
            || background == self.surface_container_highest
        {
            Some(self.on_surface)
        } else if background == self.inverse_surface {
            Some(self.inverse_on_surface)
        } else if background == self.error {
            Some(self.on_error)
        } else if background == self.error_container {
            Some(self.on_error_container)
        } else {
            None
        }
    }
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::dark()
    }
}

/// Material type scale. All sizes are [`Sp`] (Compose `Typography` uses `TextUnit`).
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Typography {
    pub display_large: Sp,
    pub display_medium: Sp,
    pub display_small: Sp,
    pub headline_large: Sp,
    pub headline_medium: Sp,
    pub headline_small: Sp,
    pub title_large: Sp,
    pub title_medium: Sp,
    pub title_small: Sp,
    pub body_large: Sp,
    pub body_medium: Sp,
    pub body_small: Sp,
    pub label_large: Sp,
    pub label_medium: Sp,
    pub label_small: Sp,
}

impl Default for Typography {
    fn default() -> Self {
        Self {
            display_large: Sp(57.0),
            display_medium: Sp(45.0),
            display_small: Sp(36.0),
            headline_large: Sp(32.0),
            headline_medium: Sp(28.0),
            headline_small: Sp(24.0),
            title_large: Sp(22.0),
            title_medium: Sp(16.0),
            title_small: Sp(14.0),
            body_large: Sp(16.0),
            body_medium: Sp(14.0),
            body_small: Sp(12.0),
            label_large: Sp(14.0),
            label_medium: Sp(12.0),
            label_small: Sp(11.0),
        }
    }
}

/// Corner radii in [`Dp`].
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Shapes {
    pub extra_small: Dp,
    pub small: Dp,
    pub medium: Dp,
    pub large: Dp,
    pub extra_large: Dp,
}

impl Default for Shapes {
    fn default() -> Self {
        Self {
            extra_small: Dp(4.0),
            small: Dp(8.0),
            medium: Dp(12.0),
            large: Dp(16.0),
            extra_large: Dp(28.0),
        }
    }
}

/// Layout gaps in [`Dp`].
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Spacing {
    pub xs: Dp,
    pub sm: Dp,
    pub md: Dp,
    pub lg: Dp,
    pub xl: Dp,
    pub xxl: Dp,
}

impl Default for Spacing {
    fn default() -> Self {
        Self {
            xs: Dp(4.0),
            sm: Dp(8.0),
            md: Dp(12.0),
            lg: Dp(16.0),
            xl: Dp(24.0),
            xxl: Dp(32.0),
        }
    }
}

/// Elevation levels in [`Dp`].
#[derive(Clone, Copy, Debug)]
#[must_use]
pub struct Elevation {
    pub level0: Dp,
    pub level1: Dp,
    pub level2: Dp,
    pub level3: Dp,
    pub level4: Dp,
    pub level5: Dp,
}

impl Default for Elevation {
    fn default() -> Self {
        Self {
            level0: Dp(0.0),
            level1: Dp(1.0),
            level2: Dp(3.0),
            level3: Dp(6.0),
            level4: Dp(8.0),
            level5: Dp(12.0),
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
            button_bg_pressed: colors.secondary_container,
        }
    }
}

impl Theme {
    pub fn with_colors(mut self, colors: ColorScheme) -> Self {
        self.colors = colors;
        self
    }

    /// A dark theme around the default dark color scheme.
    pub fn dark() -> Self {
        Self::default().with_colors(ColorScheme::dark())
    }

    /// A light theme: default colors swapped for the light scheme plus light
    /// chrome/button derivatives.
    pub fn light() -> Self {
        let colors = ColorScheme::light();
        Self {
            focus: colors.focus,
            scrollbar_thumb: colors.outline.with_alpha(179),
            button_bg: colors.primary,
            button_bg_hover: colors.primary_container,
            button_bg_pressed: colors.secondary_container,
            colors,
            ..Self::default()
        }
    }

    /// Whether the theme's background reads as dark, for syncing OS window
    /// chrome (titlebar / caption buttons) to the app theme.
    pub fn is_dark(&self) -> bool {
        self.colors.background.is_dark()
    }
}

/// Platform/device scale (dp->px multiplier). Platform runner should set this.
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
    let value = get_local::<ContentColor>()
        .map(|color| color.0)
        .unwrap_or_else(|| {
            get_local::<Theme>()
                .unwrap_or_else(|| defaults().read().theme)
                .on_surface
        });
    record_local(LocalId::ContentColor, &value, fingerprint_color);
    value
}

/// Compose `@Composable contentColorFor(backgroundColor)`.
/// Scheme role pair if known. Otherwise current `LocalContentColor`, then a
/// luminance fallback so custom fills never inherit a same-luma icon color.
pub fn content_color_for(background: Color) -> Color {
    if let Some(c) = theme().colors.content_color_for(background) {
        return c;
    }
    let local = content_color();
    // If ambient content would sit on a similarly light/dark fill, flip.
    if (background.relative_luminance() - local.relative_luminance()).abs() < 0.25 {
        if background.is_dark() {
            Color::WHITE
        } else {
            Color::BLACK
        }
    } else {
        local
    }
}

/// Composition-local default text size ([`Sp`]). Bare `Text(...)`
/// children inherit the container's typography instead of the global default.
#[derive(Clone, Copy, Debug)]
pub struct TextSize(pub Sp);

pub fn with_text_size<R>(size: Sp, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(TypeId::of::<TextSize>(), Box::new(TextSize(size)));
        f()
    })
}

pub fn text_size() -> Option<Sp> {
    let value = get_local::<TextSize>().map(|size| size.0);
    record_local(LocalId::TextSize, &value, |value| {
        fingerprint_text_size(*value)
    });
    value
}

/// Composition-local default indication (ripple/highlight) factory.
/// Components like `Button` read this to get the default press feedback.
/// Mirrors Compose's `LocalIndication`.
#[derive(Clone, Debug, Default)]
pub struct LocalIndication(pub Option<Rc<dyn IndicationNodeFactory>>);

pub fn with_local_indication<R>(
    indication: Option<Rc<dyn IndicationNodeFactory>>,
    f: impl FnOnce() -> R,
) -> R {
    with_locals_frame(|| {
        set_local_boxed(
            std::any::TypeId::of::<LocalIndication>(),
            Box::new(LocalIndication(indication)),
        );
        f()
    })
}

/// Optional composition-local override for [`crate::input::InputMode`].
/// Mirrors Compose `LocalInputModeManager` for tests and nested hosts.
#[derive(Clone, Copy, Debug)]
struct LocalInputMode(pub crate::input::InputMode);

/// Override input mode for a composition subtree.
pub fn with_input_mode<R>(mode: crate::input::InputMode, f: impl FnOnce() -> R) -> R {
    with_locals_frame(|| {
        set_local_boxed(
            TypeId::of::<LocalInputMode>(),
            Box::new(LocalInputMode(mode)),
        );
        f()
    })
}

/// Read a composition-local input mode override, if any.
pub(crate) fn local_input_mode() -> Option<crate::input::InputMode> {
    let value = get_local::<LocalInputMode>().map(|mode| mode.0);
    let effective = value.unwrap_or_else(crate::input::default_input_mode);
    record_local(LocalId::InputMode, &effective, |value| {
        fingerprint_input_mode(*value)
    });
    value
}

fn raw_local_indication() -> Option<Rc<dyn IndicationNodeFactory>> {
    LOCALS_STACK.with(|stack| {
        for frame in stack.borrow().iter().rev() {
            if let Some(value) = frame.get(&TypeId::of::<LocalIndication>())
                && let Some(indication) = value.downcast_ref::<LocalIndication>()
            {
                return indication.0.clone();
            }
        }
        None
    })
}

pub fn local_indication() -> Option<Rc<dyn IndicationNodeFactory>> {
    let value = raw_local_indication();
    record_local(LocalId::Indication, &value, fingerprint_indication);
    value
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
    let changed = {
        let mut defaults = defaults().write();
        let old = defaults.window_insets;
        let changed = old.top.to_bits() != insets.top.to_bits()
            || old.bottom.to_bits() != insets.bottom.to_bits()
            || old.left.to_bits() != insets.left.to_bits()
            || old.right.to_bits() != insets.right.to_bits()
            || old.ime_bottom.to_bits() != insets.ime_bottom.to_bits();
        defaults.window_insets = insets;
        changed
    };
    set_local_boxed(TypeId::of::<WindowInsets>(), Box::new(insets));
    if changed {
        crate::request_frame();
    }
}

/// Update just the IME bottom inset (keyboard height in px). Platform runners
/// call this when the soft keyboard opens/closes.
pub fn set_ime_inset(height_px: f32) {
    let insets = {
        let mut defaults = defaults().write();
        let changed = defaults.window_insets.ime_bottom.to_bits() != height_px.to_bits();
        defaults.window_insets.ime_bottom = height_px;
        (changed, defaults.window_insets)
    };
    set_local_boxed(TypeId::of::<WindowInsets>(), Box::new(insets.1));
    if insets.0 {
        crate::request_frame();
    }
}

/// Query current window insets.
pub fn window_insets() -> WindowInsets {
    let value = get_local::<WindowInsets>().unwrap_or_else(|| defaults().read().window_insets);
    record_local(LocalId::WindowInsets, &value, fingerprint_window_insets);
    value
}

/// Set the logical window container size (in dp). The `LayoutEngine` calls
/// this on every layout from the physical viewport + density.
pub fn set_window_container_size(width_dp: f32, height_dp: f32) {
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.container_width.to_bits() != width_dp.to_bits()
            || defaults.container_height.to_bits() != height_dp.to_bits();
        defaults.container_width = width_dp;
        defaults.container_height = height_dp;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set just the logical window container width (in dp). Prefer
/// [`set_window_container_size`]. Kept for hosts that update one axis at a time.
pub fn set_window_container_width(w_dp: f32) {
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.container_width.to_bits() != w_dp.to_bits();
        defaults.container_width = w_dp;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// Set just the logical window container height (in dp). Prefer
/// [`set_window_container_size`]. Kept for hosts that update one axis at a time.
pub fn set_window_container_height(h_dp: f32) {
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.container_height.to_bits() != h_dp.to_bits();
        defaults.container_height = h_dp;
        changed
    };
    if changed {
        crate::request_frame();
    }
}

/// The logical window container width in dp (used by Material dropdowns).
pub fn get_window_container_width() -> f32 {
    let value = defaults().read().container_width;
    record_local(LocalId::ContainerWidth, &value, |value| {
        fingerprint_f32(*value)
    });
    value
}

/// The logical window container height in dp (used by Material search bars).
pub fn get_window_container_height() -> f32 {
    let value = defaults().read().container_height;
    record_local(LocalId::ContainerHeight, &value, |value| {
        fingerprint_f32(*value)
    });
    value
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
/// the current dp->px density scale (`Density.scale * UiScale.0`).
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
    let changed = {
        let mut defaults = defaults().write();
        let changed = defaults.window_size_class != class;
        defaults.window_size_class = class;
        changed
    };
    if changed {
        crate::request_frame();
    }
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
    let value =
        get_local::<WindowSizeClass>().unwrap_or_else(|| defaults().read().window_size_class);
    record_local(
        LocalId::WindowSizeClass,
        &value,
        fingerprint_window_size_class,
    );
    value
}

pub fn theme() -> Theme {
    let value = get_local::<Theme>().unwrap_or_else(|| defaults().read().theme);
    record_local(LocalId::Theme, &value, fingerprint_theme);
    value
}

pub fn theme_fingerprint() -> u64 {
    fingerprint_theme(&theme())
}

/// Same as [`theme_fingerprint`], but reuses a [`Theme`] the caller already
/// read. Fingerprinting formats the whole theme, so caching code should take
/// this overload rather than reading the local twice.
pub fn theme_fingerprint_of(theme: &Theme) -> u64 {
    fingerprint_theme(theme)
}

pub fn density() -> Density {
    let value = get_local::<Density>().unwrap_or_else(|| defaults().read().density);
    record_local(LocalId::Density, &value, |value| {
        fingerprint_f32(value.scale)
    });
    value
}

pub fn ui_scale() -> UiScale {
    let value = get_local::<UiScale>().unwrap_or_else(|| defaults().read().ui_scale);
    record_local(LocalId::UiScale, &value, |value| fingerprint_f32(value.0));
    value
}

pub fn text_scale() -> TextScale {
    let value = get_local::<TextScale>().unwrap_or_else(|| defaults().read().text_scale);
    record_local(LocalId::TextScale, &value, |value| fingerprint_f32(value.0));
    value
}

pub fn text_direction() -> TextDirection {
    let value = get_local::<TextDirection>().unwrap_or_else(|| defaults().read().text_direction);
    record_local(LocalId::TextDirection, &value, fingerprint_text_direction);
    value
}

pub(crate) fn local_fingerprint(id: usize) -> u64 {
    match id {
        id if id == LocalId::Theme.index() => {
            fingerprint_theme(&get_local::<Theme>().unwrap_or_else(|| defaults().read().theme))
        }
        id if id == LocalId::Density.index() => fingerprint_f32(
            get_local::<Density>()
                .unwrap_or_else(|| defaults().read().density)
                .scale,
        ),
        id if id == LocalId::UiScale.index() => fingerprint_f32(
            get_local::<UiScale>()
                .unwrap_or_else(|| defaults().read().ui_scale)
                .0,
        ),
        id if id == LocalId::TextScale.index() => fingerprint_f32(
            get_local::<TextScale>()
                .unwrap_or_else(|| defaults().read().text_scale)
                .0,
        ),
        id if id == LocalId::TextDirection.index() => fingerprint_text_direction(
            &get_local::<TextDirection>().unwrap_or_else(|| defaults().read().text_direction),
        ),
        id if id == LocalId::WindowInsets.index() => fingerprint_window_insets(
            &get_local::<WindowInsets>().unwrap_or_else(|| defaults().read().window_insets),
        ),
        id if id == LocalId::WindowSizeClass.index() => fingerprint_window_size_class(
            &get_local::<WindowSizeClass>().unwrap_or_else(|| defaults().read().window_size_class),
        ),
        id if id == LocalId::ContainerWidth.index() => {
            fingerprint_f32(defaults().read().container_width)
        }
        id if id == LocalId::ContainerHeight.index() => {
            fingerprint_f32(defaults().read().container_height)
        }
        id if id == LocalId::ContentColor.index() => fingerprint_color(
            &get_local::<ContentColor>()
                .map(|value| value.0)
                .unwrap_or_else(|| {
                    get_local::<Theme>()
                        .unwrap_or_else(|| defaults().read().theme)
                        .on_surface
                }),
        ),
        id if id == LocalId::TextSize.index() => {
            fingerprint_text_size(get_local::<TextSize>().map(|value| value.0))
        }
        id if id == LocalId::Indication.index() => fingerprint_indication(&raw_local_indication()),
        id if id == LocalId::InputMode.index() => {
            fingerprint_input_mode(crate::input::input_mode())
        }
        _ => 0,
    }
}

pub fn locals_stamp() -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for id in 0..=LocalId::InputMode.index() {
        id.hash(&mut hasher);
        local_fingerprint(id).hash(&mut hasher);
    }
    hasher.finish()
}

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
