#![allow(non_snake_case)]

use std::sync::atomic::AtomicU64;

use repose_core::*;

use crate::ripple::{RippleConfig, ripple};

/// Generic component id counter (first used by filter chips).
pub(crate) static FILTERCHIP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Apply tonal elevation as a translucent primary overlay when the container
/// color matches the surface color. This mirrors CK's Surface tonalElevation:
/// the overlay is composited over the container so the base tint is preserved.
pub(crate) fn apply_tonal_elevation(m: Modifier, elevation: f32, container: Color) -> Modifier {
    if elevation <= 0.0 {
        return m;
    }
    let th = theme();
    if container != th.surface {
        return m;
    }
    let overlay_alpha = match elevation {
        e if e < 0.5 => 0.0,
        e if e < 1.5 => 0.05,
        e if e < 2.5 => 0.08,
        e if e < 3.5 => 0.11,
        e if e < 4.5 => 0.12,
        _ => 0.14,
    };
    if overlay_alpha <= 0.0 {
        return m;
    }
    m.background(
        th.colors
            .primary
            .with_alpha_f32(overlay_alpha)
            .composite_over(container),
    )
}

pub(crate) fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color(
        (a.0 as f32 + (b.0 as f32 - a.0 as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8,
        (a.1 as f32 + (b.1 as f32 - a.1 as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8,
        (a.2 as f32 + (b.2 as f32 - a.2 as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8,
        (a.3 as f32 + (b.3 as f32 - a.3 as f32) * t)
            .round()
            .clamp(0.0, 255.0) as u8,
    )
}
/// Disable a control's interactions without attaching a clickable. Use on any
/// interactive modifier chain so a future "disabled still receives hover/focus"
/// bug cannot reappear.
pub(crate) fn apply_enabled(m: Modifier, enabled: bool) -> Modifier {
    if enabled { m } else { m.enabled(false) }
}

/// Wire clickable when enabled, or disable the control when not. Centralizes
/// the "still clickable when disabled" class of bugs across M3 controls.
pub(crate) fn apply_enabled_click(
    m: Modifier,
    enabled: bool,
    on_click: impl Fn() + 'static,
) -> Modifier {
    if enabled {
        m.clickable().on_click(on_click)
    } else {
        apply_enabled(m, enabled)
    }
}

/// Wire source + M3 ripple (content-tinted) + clickable in one consistent path.
/// Use this for every interactive M3 control so hover/press/focus feedback is
/// uniform across buttons, chips, icon buttons, FABs, toggles, etc.
///
/// `bounded`/`radius` let selection controls pass unbounded 20dp (Compose parity).
pub(crate) fn apply_m3_clickable(
    m: Modifier,
    source: &MutableInteractionSource,
    ripple_color: Color,
    enabled: bool,
    on_click: impl Fn() + 'static,
) -> Modifier {
    apply_m3_clickable_ex(m, source, ripple_color, enabled, on_click, true, None)
}

pub(crate) fn apply_m3_clickable_ex(
    mut m: Modifier,
    source: &MutableInteractionSource,
    ripple_color: Color,
    enabled: bool,
    on_click: impl Fn() + 'static,
    bounded: bool,
    radius: Option<f32>,
) -> Modifier {
    m = m.interaction_source(source);
    m = m.indication(ripple(RippleConfig {
        color: Some(ripple_color),
        bounded,
        radius,
        ..Default::default()
    }));
    apply_enabled_click(m, enabled, on_click)
}

pub(crate) fn apply_m3_clickable_without_indication(
    mut m: Modifier,
    source: &MutableInteractionSource,
    enabled: bool,
    on_click: impl Fn() + 'static,
) -> Modifier {
    m = m.interaction_source(source);
    if enabled {
        m = m.clickable().on_click(on_click);
        m.indication = None;
    } else {
        m = m.enabled(false);
    }
    m
}

/// Attach Compose-equivalent action semantics (role + enabled) to a control's
/// modifier.
pub(crate) fn with_button_semantics(m: Modifier, enabled: bool) -> Modifier {
    m.semantics(Semantics {
        role: Role::Button,
        enabled,
        ..Default::default()
    })
}

/// Compose IconButton/FAB/Surface provide `LocalContentColor` *before* content
/// composes. Repose `Text`/`Icon` bake color at construction, so an eager `View`
/// passed into IconButton never sees `with_content_color`.
pub(crate) fn force_content_color_on_view(view: &mut View, to: Color) {
    // Material icons are font glyphs (Text). Leave Image tints alone.
    if let ViewKind::Text { color, .. } = &mut view.kind {
        *color = to;
    }
    for child in &mut view.children {
        force_content_color_on_view(child, to);
    }
}

/// Build icon content under the correct LocalContentColor, then harden baked
/// text colors so eager `View` call sites (repadio-style) stay correct.
pub(crate) fn icon_content_with_color(content_color: Color, icon: View) -> View {
    let mut icon = with_content_color(content_color, move || icon);
    force_content_color_on_view(&mut icon, content_color);
    icon
}
