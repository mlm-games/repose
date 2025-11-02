#![cfg(target_os = "android")]
use repose_core::prelude::*;
use repose_platform::android::run_android_app;
use repose_ui::*;
use winit::platform::android::activity::AndroidApp;

fn app(_s: &mut Scheduler) -> View {
    let count = remember(|| signal(0i32));
    Surface(
        Modifier::new()
            .fill_max_size()
            .background(Color::from_hex("#121212")),
        Column(Modifier::new().padding(24.0)).with_children(vec![
            Text(format!("Count: {}", count.get())).modifier(Modifier::new().padding(12.0)),
            Button("Increment", {
                let count = count.clone();
                move || count.update(|c| *c += 1)
            })
            .modifier(Modifier::new().padding(4.0)),
            Button("Decrement", {
                let count = count.clone();
                move || count.update(|c| *c -= 1)
            })
            .modifier(Modifier::new().padding(4.0)),
        ]),
    )
}

#[no_mangle]
pub extern "C" fn android_main(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_min_level(log::Level::Info));
    let _ = run_android_app(app, app as fn(&mut Scheduler) -> View);
}
