#![allow(non_snake_case)]

use std::sync::atomic::AtomicU64;

use repose_core::*;

use crate::ripple::{RippleConfig, ripple};

/// Generic component id counter (first used by filter chips).
pub(crate) static FILTERCHIP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Apply tonal elevation as a translucent primary overlay when the container
/// color matches the surface color. This mirrors CK's Surface tonalElevation.
pub(crate) fn apply_tonal_elevation(m: Modifier, elevation: f32, container: Color) -> Modifier {
    if elevation > 0.0 {
        let th = theme();
        if container == th.colors.surface {
            let overlay_alpha = (elevation * 4.0 + 4.0).min(24.0) / 100.0;
            return m.background(th.colors.primary.with_alpha_f32(overlay_alpha));
        }
    }
    m
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
/// Wire source + M3 ripple (content-tinted) + clickable in one consistent path.
/// Use this for every interactive M3 control so hover/press/focus feedback is
/// uniform across buttons, chips, icon buttons, FABs, toggles, etc.
pub(crate) fn apply_m3_clickable(
    mut m: Modifier,
    source: &MutableInteractionSource,
    ripple_color: Color,
    enabled: bool,
    on_click: impl Fn() + 'static,
) -> Modifier {
    m = m.interaction_source(source);
    m = m.indication(ripple(RippleConfig {
        color: Some(ripple_color),
        bounded: true,
        ..Default::default()
    }));
    if enabled {
        m = m.clickable().on_click(on_click);
    }
    m
}
