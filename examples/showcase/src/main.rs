use repose_platform::run_desktop_app;

mod app;
mod pages;
mod ui;

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    run_desktop_app(|s, _rc| app::app(s))
}
