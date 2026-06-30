use repose_core::*;

/// Default values for surface component.
pub struct SurfaceDefaults;

impl SurfaceDefaults {
    pub fn color() -> Color {
        theme().surface
    }
    pub fn content_color() -> Color {
        theme().on_surface
    }
    pub const SHAPE_RADIUS: f32 = 0.0;
    pub const TONAL_ELEVATION: f32 = 0.0;
    pub const SHADOW_ELEVATION: f32 = 0.0;
}

/// Default values for toggle button components.
pub struct ToggleButtonDefaults;

impl ToggleButtonDefaults {
    pub const HEIGHT: f32 = 40.0;
    pub const HORIZONTAL_PADDING: f32 = 24.0;
    pub const SHAPE_RADIUS: f32 = 20.0;
    pub fn content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn checked_container_color() -> Color {
        theme().primary
    }
    pub fn checked_content_color() -> Color {
        theme().on_primary
    }
    pub fn tonal_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn tonal_checked_container_color() -> Color {
        theme().secondary_container
    }
    pub fn tonal_checked_content_color() -> Color {
        theme().on_secondary_container
    }
    pub fn outlined_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn outlined_border_color() -> Color {
        theme().outline
    }
    pub fn outlined_checked_container_color() -> Color {
        theme().secondary_container
    }
    pub fn outlined_checked_content_color() -> Color {
        theme().on_secondary_container
    }
    pub fn elevated_content_color() -> Color {
        theme().primary
    }
    pub fn elevated_checked_container_color() -> Color {
        theme().surface_container_low
    }
    pub fn elevated_checked_content_color() -> Color {
        theme().primary
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        }
    }
    pub fn state_elevation_default() -> StateElevation {
        StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 0.0,
            disabled: 0.0,
        }
    }
    pub fn elevated_state_elevation() -> StateElevation {
        let th = theme();
        StateElevation {
            default: th.elevation.level1,
            hovered: th.elevation.level2,
            pressed: th.elevation.level1,
            disabled: 0.0,
        }
    }
}

/// Default values for progress indicator components.
pub struct ProgressIndicatorDefaults;

impl ProgressIndicatorDefaults {
    pub fn linear_color() -> Color {
        theme().primary
    }
    pub fn linear_track_color() -> Color {
        theme().secondary_container
    }
    pub const LINEAR_INDICATOR_HEIGHT: f32 = 4.0;
    pub const LINEAR_INDICATOR_GAP_SIZE: f32 = 4.0;
    pub const LINEAR_TRACK_STOP_SIZE: f32 = 4.0;

    pub fn circular_color() -> Color {
        theme().primary
    }
    pub fn circular_track_color() -> Color {
        theme().secondary_container
    }
    pub const CIRCULAR_INDICATOR_SIZE: f32 = 40.0;
    pub const CIRCULAR_STROKE_WIDTH: f32 = 4.0;

    /// M3 `ActiveHandleLeadingSpace` / `ActiveHandleTrailingSpace`
    pub const SLIDER_THUMB_TRACK_GAP: f32 = 6.0;
}

/// Default values for button components.
pub struct ButtonDefaults;

impl ButtonDefaults {
    pub fn content_color() -> Color {
        theme().on_primary
    }
    pub fn container_color() -> Color {
        theme().primary
    }
    pub fn tonal_container_color() -> Color {
        theme().secondary_container
    }
    pub fn tonal_content_color() -> Color {
        theme().on_secondary_container
    }
    pub fn elevated_container_color() -> Color {
        theme().surface_container_low
    }
    pub fn elevated_content_color() -> Color {
        theme().primary
    }
    pub fn outlined_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn outlined_border_color() -> Color {
        theme().outline_variant
    }
    pub fn text_content_color() -> Color {
        theme().primary
    }
    pub const HEIGHT: f32 = 40.0;
    pub const HORIZONTAL_PADDING: f32 = 24.0;
    pub const TEXT_HORIZONTAL_PADDING: f32 = 12.0;
    pub const SHAPE_RADIUS: f32 = 20.0;
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: th.on_surface.with_alpha_f32(0.12),
        }
    }
    pub fn state_elevation_default() -> StateElevation {
        StateElevation {
            default: 0.0,
            hovered: 1.0,
            pressed: 0.0,
            disabled: 0.0,
        }
    }
    pub fn elevated_state_elevation() -> StateElevation {
        let th = theme();
        StateElevation {
            default: th.elevation.level1,
            hovered: th.elevation.level2,
            pressed: th.elevation.level1,
            disabled: 0.0,
        }
    }
}

/// Default values for snackbar components.
pub struct SnackbarDefaults;

impl SnackbarDefaults {
    pub const MIN_HEIGHT: f32 = 48.0;
    pub const MIN_WIDTH: f32 = 280.0;
    pub const MAX_WIDTH: f32 = 600.0;
    pub const SHAPE_RADIUS: f32 = 4.0;
    pub fn container_color() -> Color {
        theme().inverse_surface
    }
    pub fn content_color() -> Color {
        theme().inverse_on_surface
    }
    pub fn action_color() -> Color {
        theme().inverse_primary
    }
    pub fn dismiss_action_content_color() -> Color {
        theme().inverse_on_surface
    }
}

/// Default values for card components.
pub struct CardDefaults;

impl CardDefaults {
    pub fn filled_container_color() -> Color {
        theme().surface_container_highest
    }
    pub fn elevated_container_color() -> Color {
        theme().surface_container_low
    }
    pub fn outlined_container_color() -> Color {
        theme().surface
    }
    pub fn outlined_border_color() -> Color {
        theme().outline_variant
    }
    pub fn filled_content_color() -> Color {
        theme().on_surface
    }
    pub fn elevated_content_color() -> Color {
        theme().on_surface
    }
    pub fn outlined_content_color() -> Color {
        theme().on_surface
    }
    pub fn disabled_container_color() -> Color {
        theme().on_surface.with_alpha_f32(0.04)
    }
    pub fn disabled_content_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub const SHAPE_RADIUS: f32 = 12.0;
    pub const ELEVATION: f32 = 0.0;
}

/// Default values for dialog components.
pub struct DialogDefaults;

impl DialogDefaults {
    pub fn container_color() -> Color {
        theme().surface_container_high
    }
    pub const SHAPE_RADIUS: f32 = 28.0;
    pub const MIN_WIDTH: f32 = 280.0;
    pub const MAX_WIDTH: f32 = 560.0;
    pub const HORIZONTAL_PADDING: f32 = 24.0;
}

/// Default values for icon button components.
pub struct IconButtonDefaults;

impl IconButtonDefaults {
    pub const CONTAINER_SIZE: f32 = 48.0;
    pub const FILLED_CONTAINER_SIZE: f32 = 40.0;
    pub fn content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn disabled_content_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn filled_content_color() -> Color {
        theme().on_primary
    }
    pub fn filled_container_color() -> Color {
        theme().primary
    }
    pub fn filled_tonal_content_color() -> Color {
        theme().on_secondary_container
    }
    pub fn filled_tonal_container_color() -> Color {
        theme().secondary_container
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for checkbox.
pub struct CheckboxDefaults;

impl CheckboxDefaults {
    pub const TOUCH_TARGET_SIZE: f32 = 40.0;
    pub const BOX_SIZE: f32 = 18.0;
    pub const STROKE_WIDTH: f32 = 2.0;
    pub const CORNER_RADIUS: f32 = 2.0;
    pub const CHECK_ICON_SIZE: f32 = 14.0;
    pub fn checked_color() -> Color {
        theme().primary
    }
    pub fn unchecked_color() -> Color {
        theme().on_surface_variant
    }
    pub fn checkmark_color() -> Color {
        theme().on_primary
    }
    pub fn disabled_checked_box_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_checkmark_color() -> Color {
        theme().surface
    }
    pub fn disabled_unchecked_border_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for radio button.
pub struct RadioButtonDefaults;

impl RadioButtonDefaults {
    pub const TOUCH_TARGET_SIZE: f32 = 40.0;
    pub const OUTER_RADIUS: f32 = 10.0;
    pub const DOT_RADIUS: f32 = 5.0;
    pub const STROKE_WIDTH: f32 = 2.0;
    pub fn selected_color() -> Color {
        theme().primary
    }
    pub fn unselected_color() -> Color {
        theme().on_surface_variant
    }
    pub fn disabled_selected_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_unselected_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for switch.
pub struct SwitchDefaults;

impl SwitchDefaults {
    pub const TRACK_WIDTH: f32 = 52.0;
    pub const TRACK_HEIGHT: f32 = 32.0;
    pub const THUMB_CHECKED_SIZE: f32 = 24.0;
    pub const THUMB_UNCHECKED_SIZE: f32 = 16.0;
    pub fn checked_track_color() -> Color {
        theme().primary
    }
    pub fn unchecked_track_color() -> Color {
        theme().surface_container_highest
    }
    pub fn checked_thumb_color() -> Color {
        theme().on_primary
    }
    pub fn unchecked_thumb_color() -> Color {
        theme().outline
    }
    pub fn checked_icon_color() -> Color {
        theme().on_primary
    }
    pub fn unchecked_icon_color() -> Color {
        theme().outline
    }
    pub fn unchecked_border_color() -> Color {
        theme().outline
    }
    pub fn disabled_checked_thumb_color() -> Color {
        theme().surface
    }
    pub fn disabled_checked_track_color() -> Color {
        theme().on_surface.with_alpha_f32(0.12)
    }
    pub fn disabled_checked_icon_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_unchecked_thumb_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_unchecked_track_color() -> Color {
        theme().surface_container_highest.with_alpha_f32(0.12)
    }
    pub fn disabled_unchecked_border_color() -> Color {
        theme().on_surface.with_alpha_f32(0.12)
    }
    pub fn disabled_unchecked_icon_color() -> Color {
        theme().surface_container_highest.with_alpha_f32(0.38)
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for slider.
pub struct SliderDefaults;

impl SliderDefaults {
    pub const TRACK_HEIGHT: f32 = 4.0;
    pub const THUMB_SIZE: f32 = 20.0;
    pub fn active_track_color() -> Color {
        theme().primary
    }
    pub fn inactive_track_color() -> Color {
        theme().secondary_container
    }
    pub fn thumb_color() -> Color {
        theme().primary
    }
    pub fn active_tick_color() -> Color {
        theme().secondary_container
    }
    pub fn inactive_tick_color() -> Color {
        theme().primary
    }
    pub fn disabled_thumb_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_active_track_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_inactive_track_color() -> Color {
        theme().on_surface.with_alpha_f32(0.12)
    }
    pub fn disabled_active_tick_color() -> Color {
        theme().on_surface.with_alpha_f32(0.12)
    }
    pub fn disabled_inactive_tick_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for divider.
pub struct DividerDefaults;

impl DividerDefaults {
    pub const THICKNESS: f32 = 1.0;
    pub fn color() -> Color {
        theme().outline_variant
    }
}

/// Default values for badge.
pub struct BadgeDefaults;

impl BadgeDefaults {
    pub const DOT_SIZE: f32 = 6.0;
    pub const LABEL_MIN_WIDTH: f32 = 16.0;
    pub const LABEL_HEIGHT: f32 = 16.0;
    pub const DOT_OFFSET_X: f32 = 6.0;
    pub const DOT_OFFSET_Y: f32 = 6.0;
    pub const CONTENT_OFFSET_X: f32 = 12.0;
    pub const CONTENT_OFFSET_Y: f32 = 14.0;
    pub fn container_color() -> Color {
        theme().error
    }
    pub fn content_color() -> Color {
        theme().on_error
    }
}

/// Default values for list item.
pub struct ListItemDefaults;

impl ListItemDefaults {
    pub const ONE_LINE_HEIGHT: f32 = 56.0;
    pub const TWO_LINE_HEIGHT: f32 = 72.0;
    pub const THREE_LINE_HEIGHT: f32 = 88.0;
    pub const HORIZONTAL_PADDING: f32 = 16.0;
    pub const TRAILING_PADDING: f32 = 24.0;
    pub fn headline_color() -> Color {
        theme().on_surface
    }
    pub fn supporting_color() -> Color {
        theme().on_surface_variant
    }
    pub fn overline_color() -> Color {
        theme().on_surface_variant
    }
    pub fn leading_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn trailing_icon_color() -> Color {
        theme().on_surface_variant
    }
    // disabled
    pub fn disabled_container_color() -> Color {
        theme().on_surface.with_alpha_f32(0.04)
    }
    pub fn disabled_headline_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_supporting_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_overline_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_leading_icon_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_trailing_icon_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    // selected
    pub fn selected_container_color() -> Color {
        theme().surface_container_high
    }
    pub fn selected_headline_color() -> Color {
        theme().on_surface
    }
    pub fn selected_supporting_color() -> Color {
        theme().on_surface_variant
    }
    pub fn selected_overline_color() -> Color {
        theme().on_surface_variant
    }
    pub fn selected_leading_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn selected_trailing_icon_color() -> Color {
        theme().on_surface_variant
    }
    // dragged
    pub fn dragged_container_color() -> Color {
        theme().surface_container_high
    }
    pub fn dragged_headline_color() -> Color {
        theme().on_surface
    }
    pub fn dragged_supporting_color() -> Color {
        theme().on_surface_variant
    }
    pub fn dragged_overline_color() -> Color {
        theme().on_surface_variant
    }
    pub fn dragged_leading_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn dragged_trailing_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for top app bar.
pub struct TopAppBarDefaults;

impl TopAppBarDefaults {
    pub const HEIGHT: f32 = 64.0;
    pub fn container_color() -> Color {
        theme().surface
    }
    pub fn scrolled_container_color() -> Color {
        theme().surface_container
    }
    pub fn title_content_color() -> Color {
        theme().on_surface
    }
    pub fn subtitle_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn navigation_icon_content_color() -> Color {
        theme().on_surface
    }
    pub fn action_icon_content_color() -> Color {
        theme().on_surface_variant
    }
}

/// Default values for tab row.
pub struct TabDefaults;

impl TabDefaults {
    pub const HEIGHT: f32 = 48.0;
    pub const INDICATOR_HEIGHT: f32 = 3.0;
    pub const INDICATOR_CORNER: f32 = 1.5;
    pub fn container_color() -> Color {
        theme().surface
    }
    pub fn selected_content_color() -> Color {
        theme().primary
    }
    pub fn unselected_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn indicator_color() -> Color {
        theme().primary
    }
}

/// Default values for navigation bar.
pub struct NavigationBarDefaults;

impl NavigationBarDefaults {
    pub const HEIGHT: f32 = 80.0;
    pub const TONAL_ELEVATION: f32 = 0.0;
    pub const ITEM_ACTIVE_INDICATOR_OPACITY: f32 = 1.0;
    pub const ITEM_SPACING: f32 = 8.0;
    pub const ACTIVE_INDICATOR_WIDTH: f32 = 56.0;
    pub const ACTIVE_INDICATOR_HEIGHT: f32 = 32.0;
    pub fn container_color() -> Color {
        theme().surface_container
    }
    pub fn content_color() -> Color {
        theme().on_surface
    }
    pub fn selected_icon_color() -> Color {
        theme().on_secondary_container
    }
    pub fn selected_text_color() -> Color {
        theme().secondary
    }
    pub fn unselected_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn unselected_text_color() -> Color {
        theme().on_surface_variant
    }
    pub fn indicator_color() -> Color {
        theme().secondary_container
    }
    pub const INDICATOR_RADIUS: f32 = 16.0;
}

/// Default values for navigation rail.
pub struct NavigationRailDefaults;

impl NavigationRailDefaults {
    pub const WIDTH: f32 = 80.0;
    pub const ITEM_RADIUS: f32 = 16.0;
    pub const ITEM_MIN_HEIGHT: f32 = 56.0;
    pub const ITEM_ACTIVE_INDICATOR_OPACITY: f32 = 1.0;
    pub const ITEM_SPACING: f32 = 4.0;
    pub const ACTIVE_INDICATOR_WIDTH: f32 = 56.0;
    pub const ACTIVE_INDICATOR_HEIGHT: f32 = 32.0;
    pub fn container_color() -> Color {
        theme().surface
    }
    pub fn selected_icon_color() -> Color {
        theme().on_secondary_container
    }
    pub fn selected_text_color() -> Color {
        theme().secondary
    }
    pub fn unselected_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn unselected_text_color() -> Color {
        theme().on_surface_variant
    }
    pub fn indicator_color() -> Color {
        theme().secondary_container
    }
}

/// Default values for segmented button.
pub struct SegmentedButtonDefaults;

impl SegmentedButtonDefaults {
    pub const HEIGHT: f32 = 40.0;
    pub const SHAPE_RADIUS: f32 = 20.0;
    pub fn border_color() -> Color {
        theme().outline
    }
    pub fn selected_container_color() -> Color {
        theme().secondary_container
    }
    pub fn selected_content_color() -> Color {
        theme().on_secondary_container
    }
    pub fn unselected_content_color() -> Color {
        theme().on_surface
    }
    pub fn state_colors_default() -> StateColors {
        let th = theme();
        StateColors {
            default: Color::TRANSPARENT,
            hovered: th.on_surface.with_alpha_f32(0.08),
            pressed: th.on_surface.with_alpha_f32(0.12),
            disabled: Color::TRANSPARENT,
        }
    }
}

/// Default values for FAB.
pub struct FABDefaults;

impl FABDefaults {
    pub const SMALL_SIZE: f32 = 40.0;
    pub const SMALL_SHAPE_RADIUS: f32 = 12.0;
    pub const SIZE: f32 = 56.0;
    pub const LARGE_SIZE: f32 = 96.0;
    pub const SHAPE_RADIUS: f32 = 28.0;
    pub const LARGE_SHAPE_RADIUS: f32 = 28.0;
    pub fn container_color() -> Color {
        theme().primary_container
    }
    pub fn content_color() -> Color {
        theme().on_primary_container
    }
    pub fn state_elevation() -> StateElevation {
        StateElevation {
            default: 6.0,
            hovered: 8.0,
            pressed: 12.0,
            disabled: 0.0,
        }
    }
}

/// Default values for chip components.
pub struct ChipDefaults;

impl ChipDefaults {
    pub const HEIGHT: f32 = 32.0;
    pub const HORIZONTAL_PADDING: f32 = 16.0;
    pub const SHAPE_RADIUS: f32 = 8.0;
    pub const BORDER_WIDTH: f32 = 1.0;

    // Colors for non-selected state
    pub fn container_color() -> Color {
        Color::TRANSPARENT
    }
    pub fn label_color() -> Color {
        theme().on_surface_variant
    }
    pub fn leading_icon_color() -> Color {
        theme().primary
    }
    pub fn trailing_icon_color() -> Color {
        theme().primary
    }

    // Disabled colors (non-selected)
    pub fn disabled_container_color() -> Color {
        Color::TRANSPARENT
    }
    pub fn disabled_label_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_leading_icon_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn disabled_trailing_icon_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }

    // Colors for selected state
    pub fn selected_container_color() -> Color {
        theme().secondary_container
    }
    pub fn selected_label_color() -> Color {
        theme().on_secondary_container
    }
    pub fn selected_leading_icon_color() -> Color {
        theme().primary
    }
    pub fn selected_trailing_icon_color() -> Color {
        theme().on_secondary_container
    }

    // Disabled selected container
    pub fn disabled_selected_container_color() -> Color {
        theme()
            .on_surface
            .with_alpha_f32(0.12)
            .composite_over(theme().secondary_container)
    }

    // Border colors
    pub fn border_color() -> Color {
        theme().outline_variant
    }
    pub fn selected_border_color() -> Color {
        Color::TRANSPARENT
    }
    pub fn disabled_border_color() -> Color {
        theme().on_surface.with_alpha_f32(0.12)
    }
    pub fn disabled_selected_border_color() -> Color {
        Color::TRANSPARENT
    }

    // Elevation defaults (flat chip - no elevation)
    pub fn elevation_default() -> f32 {
        0.0
    }
    pub fn elevation_hovered() -> f32 {
        0.0
    }
    pub fn elevation_focused() -> f32 {
        0.0
    }
    pub fn elevation_pressed() -> f32 {
        0.0
    }
    pub fn elevation_dragged() -> f32 {
        0.0
    }
    pub fn elevation_disabled() -> f32 {
        0.0
    }

    // Elevated chip defaults
    pub fn elevated_elevation_default() -> f32 {
        theme().elevation.level1
    }
    pub fn elevated_elevation_hovered() -> f32 {
        theme().elevation.level2
    }
    pub fn elevated_elevation_focused() -> f32 {
        theme().elevation.level1
    }
    pub fn elevated_elevation_pressed() -> f32 {
        theme().elevation.level1
    }
    pub fn elevated_elevation_dragged() -> f32 {
        theme().elevation.level3
    }
    pub fn elevated_elevation_disabled() -> f32 {
        0.0
    }

    // Elevated chip container colors
    pub fn elevated_container_color() -> Color {
        theme().surface_container_low
    }
    pub fn elevated_selected_container_color() -> Color {
        theme().secondary_container
    }
    pub fn disabled_elevated_container_color() -> Color {
        theme()
            .on_surface
            .with_alpha_f32(0.12)
            .composite_over(theme().surface_container_low)
    }
}

/// Default values for scaffold layout.
pub struct ScaffoldDefaults;

impl ScaffoldDefaults {
    pub fn container_color() -> Color {
        theme().background
    }
    pub const TOP_BAR_HEIGHT: f32 = 64.0;
    pub const BOTTOM_BAR_HEIGHT: f32 = 80.0;
    pub const FAB_MARGIN: f32 = 16.0;
}

/// Default values for navigation drawer.
pub struct NavigationDrawerDefaults;

impl NavigationDrawerDefaults {
    pub const WIDTH: f32 = 300.0;
    pub const TONAL_ELEVATION: f32 = 0.0;
    pub fn container_color() -> Color {
        theme().surface_container_low
    }
    pub fn content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn scrim_color() -> Color {
        theme().scrim.with_alpha(82)
    }
    pub const SHAPE_RADIUS: f32 = 16.0;
}

/// Default values for bottom sheet / modal bottom sheet.
pub struct BottomSheetDefaults;

impl BottomSheetDefaults {
    pub const TONAL_ELEVATION: f32 = 0.0;
    pub fn container_color() -> Color {
        theme().surface_container_low
    }
    pub fn content_color() -> Color {
        theme().on_surface
    }
    pub fn drag_handle_color() -> Color {
        theme().on_surface_variant
    }
    pub fn scrim_color() -> Color {
        theme().scrim.with_alpha(85)
    }
    pub const DRAG_HANDLE_WIDTH: f32 = 32.0;
    pub const DRAG_HANDLE_HEIGHT: f32 = 4.0;
    pub const SHAPE_RADIUS: f32 = 16.0;
    pub const PEEK_HEIGHT: f32 = 56.0;
    pub const MAX_WIDTH: f32 = 640.0;
}

/// Default values for search bar.
pub struct SearchBarDefaults;

impl SearchBarDefaults {
    pub const HEIGHT: f32 = 56.0;
    pub const EXPANDED_WIDTH: f32 = 360.0;
    pub const COLLAPSED_WIDTH: f32 = 240.0;
    pub const DOCKED_HEIGHT: f32 = 400.0;
    pub fn container_color() -> Color {
        theme().surface_container
    }
    pub fn active_container_color() -> Color {
        theme().surface_container_high
    }
    pub fn content_color() -> Color {
        theme().on_surface
    }
    pub fn placeholder_color() -> Color {
        theme().on_surface_variant
    }
}

/// Default values for dropdown menu.
pub struct DropdownMenuDefaults;

impl DropdownMenuDefaults {
    pub const MIN_WIDTH: f32 = 112.0;
    pub const ITEM_HEIGHT: f32 = 40.0;
    pub fn container_color() -> Color {
        theme().surface_container
    }
    pub fn item_text_color() -> Color {
        theme().on_surface
    }
    pub fn disabled_item_text_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn divider_color() -> Color {
        theme().outline_variant
    }
}

/// Default values for tooltip.
pub struct TooltipDefaults;

impl TooltipDefaults {
    pub const OFFSET_Y: f32 = -28.0;
    pub const HORIZONTAL_PADDING: f32 = 8.0;
    pub const VERTICAL_PADDING: f32 = 4.0;
    pub fn container_color() -> Color {
        theme().inverse_surface
    }
    pub fn content_color() -> Color {
        theme().inverse_on_surface
    }
}

/// Default values for pull-to-refresh.
pub struct PullToRefreshDefaults;

impl PullToRefreshDefaults {
    pub const THRESHOLD: f32 = 64.0;
    pub fn indicator_color() -> Color {
        theme().primary
    }
    pub fn container_color() -> Color {
        Color::TRANSPARENT
    }
}

/// Default values for alert dialog.
pub struct AlertDialogDefaults;

impl AlertDialogDefaults {
    pub const MIN_WIDTH: f32 = 280.0;
    pub const MAX_WIDTH: f32 = 560.0;
    pub const HORIZONTAL_PADDING: f32 = 24.0;
    pub fn scrim_color() -> Color {
        theme().scrim.with_alpha(82)
    }
}

/// Default values for outlined text field.
pub struct OutlinedTextFieldDefaults;

impl OutlinedTextFieldDefaults {
    pub const MIN_HEIGHT: f32 = 56.0;
    pub const MIN_WIDTH: f32 = 280.0;
    pub const UNFOCUSED_BORDER_THICKNESS: f32 = 1.0;
    pub const FOCUSED_BORDER_THICKNESS: f32 = 2.0;
    pub const TEXT_FIELD_PADDING: f32 = 16.0;
    pub const VERTICAL_PADDING_WITH_LABEL: f32 = 8.0;

    pub fn label_color() -> Color {
        theme().on_surface_variant
    }
    pub fn focused_label_color() -> Color {
        theme().primary
    }
    pub fn error_label_color() -> Color {
        theme().error
    }
    pub fn input_color() -> Color {
        theme().on_surface
    }
    pub fn disabled_input_color() -> Color {
        theme().on_surface.with_alpha_f32(0.38)
    }
    pub fn supporting_color() -> Color {
        theme().on_surface_variant
    }
    pub fn placeholder_color() -> Color {
        theme().on_surface_variant
    }
    pub fn leading_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn trailing_icon_color() -> Color {
        theme().on_surface_variant
    }
    pub fn unfocused_border_color() -> Color {
        theme().outline
    }
    pub fn focused_border_color() -> Color {
        theme().primary
    }
    pub fn error_border_color() -> Color {
        theme().error
    }
    pub fn cursor_color() -> Color {
        theme().primary
    }
    pub fn container_color() -> Color {
        theme().surface
    }
}

/// Default values for date picker.
pub struct DatePickerDefaults;

impl DatePickerDefaults {
    pub const CONTAINER_WIDTH: f32 = 360.0;
    pub const CONTAINER_HEIGHT: f32 = 568.0;
    pub const HEADER_CONTAINER_HEIGHT: f32 = 120.0;
    pub const DATE_CELL_SIZE: f32 = 40.0;
    pub const YEAR_CELL_HEIGHT: f32 = 36.0;
    pub const YEAR_CELL_WIDTH: f32 = 72.0;
    pub const TODAY_BORDER_WIDTH: f32 = 1.0;
    pub const HORIZONTAL_PADDING: f32 = 12.0;

    pub fn container_color() -> Color {
        theme().surface_container_high
    }
    pub fn header_color() -> Color {
        theme().on_surface_variant
    }
    pub fn weekday_color() -> Color {
        theme().on_surface
    }
    pub fn day_color() -> Color {
        theme().on_surface
    }
    pub fn selected_day_color() -> Color {
        theme().on_primary
    }
    pub fn selected_day_container_color() -> Color {
        theme().primary
    }
    pub fn today_content_color() -> Color {
        theme().primary
    }
    pub fn today_border_color() -> Color {
        theme().primary
    }
    pub fn year_selected_container_color() -> Color {
        theme().primary
    }
    pub fn year_selected_content_color() -> Color {
        theme().on_primary
    }
    pub fn year_unselected_content_color() -> Color {
        theme().on_surface_variant
    }
}

/// Default values for time picker.
pub struct TimePickerDefaults;

impl TimePickerDefaults {
    pub const CLOCK_DIAL_CONTAINER_SIZE: f32 = 256.0;
    pub const CLOCK_DIAL_MIN_CONTAINER_SIZE: f32 = 200.0;
    pub const PERIOD_SELECTOR_HORIZONTAL_WIDTH: f32 = 216.0;
    pub const PERIOD_SELECTOR_HORIZONTAL_HEIGHT: f32 = 38.0;
    pub const PERIOD_SELECTOR_VERTICAL_WIDTH: f32 = 52.0;
    pub const PERIOD_SELECTOR_VERTICAL_HEIGHT: f32 = 80.0;
    pub const TIME_SELECTOR_CONTAINER_WIDTH: f32 = 96.0;
    pub const TIME_SELECTOR_CONTAINER_HEIGHT: f32 = 80.0;
    pub const TIME_SELECTOR_24H_WIDTH: f32 = 114.0;
    pub const MAX_HEIGHT: f32 = 384.0;

    pub fn clock_dial_color() -> Color {
        theme().surface_container_highest
    }
    pub fn clock_dial_selected_content_color() -> Color {
        theme().on_primary
    }
    pub fn clock_dial_unselected_content_color() -> Color {
        theme().on_surface
    }
    pub fn selector_color() -> Color {
        theme().primary
    }
    pub fn container_color() -> Color {
        theme().surface_container_high
    }
    pub fn period_selector_selected_container_color() -> Color {
        theme().tertiary_container
    }
    pub fn period_selector_selected_content_color() -> Color {
        theme().on_tertiary_container
    }
    pub fn period_selector_unselected_content_color() -> Color {
        theme().on_surface_variant
    }
    pub fn period_selector_border_color() -> Color {
        theme().outline
    }
    pub fn time_selector_selected_container_color() -> Color {
        theme().primary_container
    }
    pub fn time_selector_selected_content_color() -> Color {
        theme().on_primary_container
    }
    pub fn time_selector_unselected_container_color() -> Color {
        theme().surface_container_highest
    }
    pub fn time_selector_unselected_content_color() -> Color {
        theme().on_surface
    }
    pub fn time_selector_separator_color() -> Color {
        theme().on_surface
    }
}

/// Default values for swipe-to-dismiss.
pub struct SwipeToDismissDefaults;

impl SwipeToDismissDefaults {
    pub const POSITIONAL_THRESHOLD: f32 = 56.0;
    pub const DISMISS_THRESHOLD: f32 = 150.0;
    pub const DISMISSED_OFFSET: f32 = 300.0;
    pub const MAX_WIDTH: f32 = 400.0;
}

/// Default values for carousel.
pub struct CarouselDefaults;

impl CarouselDefaults {
    pub const MIN_SMALL_ITEM_SIZE: f32 = 40.0;
    pub const MAX_SMALL_ITEM_SIZE: f32 = 56.0;
    pub const ANCHOR_SIZE: f32 = 10.0;
}
