mod app;
mod pages;
mod ui;

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn wasm_start() {
    // If this doesn't show in the browser console, the wasm entrypoint isn't being run.
    web::start();
}

#[cfg(target_os = "android")]
use log::LevelFilter;
#[cfg(target_os = "android")]
use repose_core::prelude::*;
#[cfg(target_os = "android")]
use repose_platform::android::run_android_app;
#[cfg(target_os = "android")]
use winit::platform::android::activity::AndroidApp;

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));
    let _ = run_android_app(android_app, app::app as fn(&mut Scheduler) -> View);
}
