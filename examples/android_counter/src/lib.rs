#![cfg(target_os = "android")]
use log::LevelFilter;
use repose_core::prelude::*;
use repose_material::material3::Button;
use repose_platform::RenderContext;
use repose_platform::android::run_android_app;
use repose_ui::*;
use winit::platform::android::activity::AndroidApp;

fn app(_s: &mut Scheduler, _rc: &RenderContext) -> View {
    let count = remember(|| signal(0i32));
    Box(
        Modifier::new()
            .fill_max_size()
            .background(theme().background),
    )
    .child(
        Column(Modifier::new().padding(24.0).fill_max_size()).with_children(vec![
            Spacer(),
            Text(format!("Count: {}", count.get())).modifier(Modifier::new().padding(12.0)),
            Button(Modifier::new().padding(16.0), {
                let count = count.clone();
                move || count.update(|c| *c += 1)
            }, || Text("Increment")),
            Button(Modifier::new().padding(16.0), {
                let count = count.clone();
                move || count.update(|c| *c -= 1)
            }, || Text("Decrement")),
            Spacer(),
        ]),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn android_main(android_app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(LevelFilter::Trace));
    let _ = run_android_app(android_app, app);
}
