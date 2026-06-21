use repose_core::*;

/// Default values for progress indicator components.
pub struct ProgressIndicatorDefaults;

impl ProgressIndicatorDefaults {
    pub fn linear_color() -> Color {
        theme().primary
    }
    pub fn linear_track_color() -> Color {
        theme().secondary_container
    }
    /// Height of the linear indicator bar in dp.
    pub const LINEAR_INDICATOR_HEIGHT: f32 = 4.0;
    /// Gap between the indicator fill and the track in dp.
    pub const LINEAR_INDICATOR_GAP_SIZE: f32 = 4.0;
    /// Diameter of the stop indicator dot in dp.
    pub const LINEAR_TRACK_STOP_SIZE: f32 = 4.0;

    pub fn circular_color() -> Color {
        theme().primary
    }
    pub fn circular_track_color() -> Color {
        theme().secondary_container
    }
    /// Size of the circular indicator's bounding box in dp.
    pub const CIRCULAR_INDICATOR_SIZE: f32 = 40.0;
    /// Stroke width for the circular indicator ring in dp.
    pub const CIRCULAR_STROKE_WIDTH: f32 = 4.0;
}

/// Default values for button components.
pub struct ButtonDefaults;

impl ButtonDefaults {
    pub fn content_color() -> Color {
        theme().on_surface
    }
    pub fn container_color() -> Color {
        theme().primary
    }
    pub const HEIGHT: f32 = 40.0;
    pub const HORIZONTAL_PADDING: f32 = 24.0;
    pub const SHAPE_RADIUS: f32 = 20.0;
}

/// Default values for snackbar components.
pub struct SnackbarDefaults;

impl SnackbarDefaults {
    pub fn container_color() -> Color {
        theme().inverse_surface
    }
    pub fn content_color() -> Color {
        theme().inverse_on_surface
    }
    pub fn action_color() -> Color {
        theme().inverse_primary
    }
    pub const SHAPE_RADIUS: f32 = 4.0;
}

/// Default values for card components.
pub struct CardDefaults;

impl CardDefaults {
    pub fn container_color() -> Color {
        theme().surface_container_highest
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
