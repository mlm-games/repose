use repose_platform::run_app_with_snackbar;
use repose_ui::overlay::SnackbarController;
use std::rc::Rc;

mod app;
mod pages;
mod ui;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    run_app_with_snackbar(
        |s, _rc| app::app(s),
        Rc::new(|ms| SnackbarController::tick_for_frame(ms)),
    )
}
