fn main() {
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
    showcase::desktop_main();
}
