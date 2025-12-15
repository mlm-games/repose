use repose_platform::run_desktop_app;

mod app;
mod pages;
mod ui;

fn main() -> anyhow::Result<()> {
    env_logger::init();
    run_desktop_app(app::app)
}
