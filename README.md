# Repose

Repose is a small, composable UI toolkit for Rust with a Compose-like API, cross-platform runners for desktop, Android, and the web, and a WGPU renderer.

Its objective is to make declarative, state-driven UI practical in Rust while keeping the runtime, layout, text, input, and rendering layers inspectable and portable.

[![Crates.io](https://img.shields.io/crates/v/repose-core)](https://crates.io/crates/repose-core)
[![License](https://img.shields.io/github/license/mlm-games/repose)](LICENSE)
[![Demo](https://img.shields.io/badge/demo-live-blue)](https://mlm-games.github.io/repose/)

> **Status: pre-1.0.** The API may still change, especially in smaller areas.

Repose targets straightforward applications first and can grow into larger ones without requiring an embedded web view or separate native UI implementations.

## Features

- **Declarative composition** - `View` functions, reactive `Signal`s, `remember` / `remember_mutable`, derived state (`produce_state`), effects, and composition locals
- **Cross-platform runners** - Desktop (winit), Android (native activity), WebAssembly (canvas + WGPU/WebGL)
- **Layout** - Flexbox and Grid via [Taffy](https://github.com/DioxusLabs/taffy); modifiers for padding, gaps, alignment, clipping, borders, etc.
- **GPU rendering** - Rectangles, rounded clips, borders, ellipses, text, and images through a WGPU backend (atlases + pipelines)
- **Text** - Shaping, metrics, wrapping/ellipsis with caching (`repose-text` / Parley + font stack)
- **Input** - Pointer events, scrolling, focus traversal, IME, gestures
- **Widgets & building blocks** - Text, buttons, text fields, checkbox, switch, slider, `ScrollArea`, `LazyColumn` / lazy lists, pager, overlays & snackbars, color picker, selection, subcompose
- **Material-inspired components** - Material 3-style controls, ripples, symbols/icons (`repose-material`)
- **Navigation** - Typed back-stack navigation with transitions (`repose-navigation`)
- **Canvas** - Custom painting surface (`repose-canvas`)
- **Docking** - Dockable panels (`repose-docking`)
- **Accessibility** - AccessKit on desktop + semantic node pipeline
- **DevTools** - Inspector overlay (Ctrl/Cmd+Shift+I)
- **Animation** - Runtime animation clock and helpers
- **Lifecycle and composition** - Scoped effects, timers, reactive state, and host lifecycle listeners

### Non-Goals

- Full feature parity with mature toolkits; Repose prioritizes a small, maintainable toolkit that covers common application needs well

## Quick Start

### Prerequisites

- **Rust:** install the repository toolchain with `rustup toolchain install 1.98.0`.
- Desktop: install the WGPU system dependencies for your operating system.
- Web: install the `wasm32-unknown-unknown` target and `trunk` `0.21.14`:
  ```bash
  rustup target add wasm32-unknown-unknown
  cargo install trunk --version 0.21.14 --locked
  ```
- Android: use JDK `21`, Android build-tools `35.0.0`, the Android SDK/NDK, `adb`, and a connected device or emulator with USB debugging enabled. The documented local setup was tested with Android NDK `29.0.14206865`, `adb` `1.0.41`, and `cargo-rapk` `0.22.1`:
  ```bash
  cargo install cargo-rapk --version 0.22.1 --locked
  adb devices
  ```
  Set `ANDROID_HOME` (or `ANDROID_SDK_ROOT`), `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, and `ANDROID_BUILD_TOOLS` before building.

### Run the Showcase

**Desktop:**
```bash
cargo run -p showcase --bin showcase-desktop
```

**Web:**
```bash
cd examples/showcase
trunk serve
```

Or try the [hosted demo](https://mlm-games.github.io/repose/).

**Android:**
```bash
cd examples/showcase
cargo rapk run --target aarch64-linux-android --lib
```

`cargo rapk check` validates the manifest and `cargo rapk build` creates an APK; `run` additionally requires a connected device or emulator.

## Usage

```rust
use repose_core::prelude::*;
use repose_material::material3::{Button, ButtonConfig};
use repose_platform::{AppConfig, run_desktop_app_with_config};
use repose_ui::*;

fn Counter() -> View {
    let count = remember_mutable(|| 0);

    Column(Modifier::new().padding(16.0.dp())).child((
        Text(format!("Count: {}", *count.get())),
        Button(
            Modifier::new(),
            {
                let count = count.clone();
                move || count.update(|c| *c += 1)
            },
            ButtonConfig::default(),
            || Text("Increment"),
        ),
    ))
}

fn main() -> anyhow::Result<()> {
    run_desktop_app_with_config(
        |_sched, _render_context| Counter(),
        AppConfig::default(),
    )
}
```

**State management:**

```rust
// A Signal is shared state that can be read and updated across a composition.
let theme = signal(Theme::default());
theme.set(Theme::dark());

// Mutable state is convenient for component-local values.
let input = remember_mutable(|| String::new());

// Derived state recomputes when its dependencies change.
let full_name = produce_state("full", {
    let first = first_name.clone();
    let last = last_name.clone();
    move || format!("{} {}", first.get(), last.get())
});
```

**Layout:**
```rust
Row(Modifier::new().gap(8.0.dp())).child((
    Text("Left"),
    Spacer(),
    Text("Right"),
))
```

**Navigation:**
```rust
let stack = remember_back_stack(Route::Home);
let navigator = Navigator { stack: (*stack).clone() };

// Push a route.
navigator.push(Route::Details);

// Configure the platform back action.
back::set(Some(Rc::new(move || navigator.pop())));
```

## GIF

<img src="others/demo.gif" align="center">

<img width="2083" height="1326" alt="soredowe ui" src="https://github.com/user-attachments/assets/1f143ebd-5f24-47c8-9a95-3a09e762db0b" />

## Inspiration

Repose aims to be short and easy to understand by reading the code. Its declarative style is informed by Jetpack Compose and Iced, with an emphasis on keeping the Rust implementation understandable.

## Architecture

| Crate | Role |
|-------|------|
| `repose-macros` | Procedural macros used by Repose APIs |
| `repose-core` | Signals, effects, runtime, view model, locals, and animation |
| `repose-app` | Application lifecycle, input, and runtime integration |
| `repose-ui` | Widgets, Taffy layout, paint, hit regions, and semantics |
| `repose-render-wgpu` | WGPU renderer, atlases, and pipelines |
| `repose-platform` | Desktop, Android, and WASM runners |
| `repose-text` | Text shaping, metrics, and caches |
| `repose-material` | Material-inspired components and symbols |
| `repose-navigation` | Typed stack navigation and transitions |
| `repose-canvas` | Immediate-mode drawing and embedded callback surfaces |
| `repose-devtools` | Inspector HUD |
| `repose-docking` | Dockable panels |

### Lifecycle and composition

A Repose frame rebuilds the declarative view tree from lightweight values. Reactive signals, scoped effects, and timers own stateful work; layout is reconciled by Taffy, and the selected renderer paints the resulting tree. Platform runners own the window or activity and forward real lifecycle transitions such as foreground/background changes. Applications can register lifecycle listeners and remove them when their owning scope is disposed.

## Projects Using Repose

These projects exercise the toolkit in real applications:

- **[startpose](https://github.com/mlm-games/startpose)** - Web startpage
- **[wifi-exporter](https://github.com/mlm-games/wifi-exporter)** - Android WiFi importer/exporter
- **[soredowe](https://github.com/mlm-games/soredowe)** - Linux pacman/flathub/aur/appimage UI for install/updates
- **[renamite](https://github.com/mlm-games/renamite)** - Motion and vector animation editor built with Repose on desktop, Android, and the web
- **[my-ecosystem-template-bevy](https://github.com/mlm-games/my-ecosystem-template-bevy)** - 2D Bevy game template (uses repose-bevy for UI and inputs)
- **[ednitar-clap](https://github.com/mlm-games/ednitar-clap)** - Rust CLAP guitar-effect plugin demonstrating Repose Audio

## Contributing

Issues and PRs are welcome, especially for:
- Correctness bugs
- Performance regressions (include a repro)
- Platform gaps (except external issues like Android IME, if documented)

### Development Setup

```bash
git clone https://github.com/mlm-games/repose
cd repose
cargo test --workspace --locked
```

## Support

Consider supporting Repose's development if it is useful to you. Open an issue or a discussion for bugs and questions.

## Mentions

- [Taffy](https://github.com/DioxusLabs/taffy) for layout
- [wgpu](https://github.com/gfx-rs/wgpu) for cross-platform graphics
- [AccessKit](https://github.com/AccessKit/accesskit) for accessibility
- Heavily inspired by Jetpack Compose's API design

## License

MPL-2.0

See [LICENSE](LICENSE) for more info.
