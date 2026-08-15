// intentionally snake_case
#![allow(non_upper_case_globals)]

mod app;
mod pages;
mod ui;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() -> Result<(), JsValue> {
    repose_platform::web::run_web_app(
        |s, _rc| app::app(s),
        repose_platform::web::WebOptions::new(None),
    )
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn desktop_main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let _ = repose_platform::run_desktop_app_with_config(
        |s, _rc| app::app(s),
        repose_platform::AppConfig::default(),
    );
}

#[cfg(target_os = "android")]
use log::LevelFilter;

#[cfg(target_os = "android")]
use repose_core::prelude::*;

#[cfg(target_os = "android")]
use repose_platform::android::{AndroidOptions, run_android_app_with_options};

#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Info));
    let _ =
        run_android_app_with_options(android_app, |s, _rc| app::app(s), AndroidOptions::default());
}
