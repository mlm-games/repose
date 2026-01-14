# Repose

A small, composable UI toolkit/runtime in Rust with a Compose-like API, cross-platform runners (desktop/Android/web), and a WGPU renderer.

Status: **experimental / pre-1.0**. The API and internals may change. Currently have few working apps.

## Goals

- Declarative UI with simple state (`signal`, `remember`, effects)
- Cross-platform rendering via **wgpu**
- Practical widgets (text, buttons, scrolling, text field) + a small Material-ish component set
- Runs on **desktop (winit)**, **Android**, and **Web (wasm)**

## Non-goals (for now)

- Full parity with mature UI toolkits. (Should take atleast 1 or 2 years)
- Complex text editing features beyond single-line fields (rich selection handles, etc.)
- Accessibility coverage across all platforms (in progress)
- Multi-window + complex popup/overlay system

## What’s implemented

- Layout: Flexbox/Grid via **taffy**
- Rendering: rectangles, borders, rounded clipping, ellipses, text, images (wgpu)
- Text: shaping/metrics via **cosmic-text** with caching
- Input: pointer events, scrolling, basic focus traversal, IME hooks (platform dependent)
- Widgets: `Text`, `Button`, `TextField`, `Checkbox`, `Switch`, `Slider`, `ScrollArea`, `LazyColumn`
- Navigation: `repose-navigation` (typed stack + transitions)
- Devtools: HUD/inspector overlay
- Accessibility: **AccessKit** integration (desktop runner) + `SemNode` pipeline

## Try it

### Desktop
```bash
cargo run -p showcase --features desktop-bin
```

### Web (WASM)
A hosted demo (Showcase), check the github pages link 

(or)

Build locally:
```bash
cd examples/showcase
trunk serve
```

### Android
```bash
cd examples/showcase
# build instructions depend on your Android setup; see examples/showcase/Cargo.toml metadata for cargo-apk conf.
# for cargo-apk
cargo rapk run --target aarch64-linux-android --lib
```

## Examples / Demos

- Showcase app: `examples/showcase` (widgets/layout/scroll/text/canvas/navigation)
- Animation demo: `examples/animation_demo`, animations in showcase are of placeholder level
- Android counter: `examples/android_counter`

If you prefer “see it first”: the **Showcase web build** is the best entry point.

## Built with Repose (early)

These are small projects I’ve used to test the toolkit:

- startpose (web startpage): https://github.com/mlm-games/startpose
- wifi-exporter (Android Wifi Importer (Android 11+) and Exporter): https://github.com/mlm-games/wifi-exporter
- soredowe (Linux pacman UI): https://github.com/mlm-games/soredowe

(These work well, but are early and were initially for testing Repose in apps.)

## Architecture (high level)

- `repose-core`: signals/effects/runtime, view model (`View`, `Modifier`, `Scene`)
- `repose-ui`: widgets + layout/paint (Taffy -> Scene + HitRegions + Semantics)
- `repose-render-wgpu`: renderer backend (wgpu pipelines, atlases)
- `repose-platform`: platform runners (winit desktop / Android / wasm web)
- `repose-text`: text shaping + metrics + wrapping/ellipsis caches
- `repose-navigation`, `repose-material`, `repose-canvas`: higher-level components

## Contributing / Feedback

Issues and PRs are welcome, especially for:
- correctness bugs
- perf regressions (include a small repro)
- platform gaps (ignore external ones like Android IME/keyboard.)

## License

GPL-3.0-or-later (see `LICENSE`).

If you want to use Repose in a closed-source product, do open an issue.
