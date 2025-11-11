#![cfg(target_os = "android")]
use log::LevelFilter;
use repose_core::prelude::*;
use repose_platform::android::run_android_app;
use winit::platform::android::activity::AndroidApp;

mod app;
mod pages;
mod ui;

#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));
    let _ = run_android_app(android_app, app::app as fn(&mut Scheduler) -> View);
}
