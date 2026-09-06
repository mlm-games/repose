use repose_core::present_mode::PresentModePref;

/// Options common to all platforms.
#[derive(Clone, Copy, Debug)]
pub struct ReposeOptions {
    pub msaa_samples: u32,
    pub max_fps: Option<f32>,
    pub present_mode: PresentModePref,
}

impl Default for ReposeOptions {
    fn default() -> Self {
        Self {
            msaa_samples: 4,
            max_fps: None,
            present_mode: PresentModePref::Auto,
        }
    }
}

/// Configuration for desktop `run_desktop_app`.
#[derive(Clone, Debug)]
pub struct AppConfig {
    pub common: ReposeOptions,
    pub window_title: String,
    pub window_size: (u32, u32),
    pub enable_inspector: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            common: ReposeOptions::default(),
            window_title: "Repose".to_string(),
            window_size: (1280, 800),
            enable_inspector: true,
        }
    }
}

/// Options for the Android runner.
#[derive(Clone, Copy, Debug)]
#[derive(Default)]
pub struct AndroidOptions {
    pub continuous_redraw: bool,
    pub ime_height_px: Option<f32>,
    pub common: ReposeOptions,
}

