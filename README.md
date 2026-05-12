# matrix-screen-saver

Matrix rain screensaver written in Rust. Wayland/Linux daemon + Windows `.scr` native screensaver, sharing a common core library.

[![CI](https://github.com/p-o-d/matrix-screen-saver/actions/workflows/ci.yml/badge.svg)](https://github.com/p-o-d/matrix-screen-saver/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/p-o-d/matrix-screen-saver)](https://github.com/p-o-d/matrix-screen-saver/releases/latest)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

## Platform support

| Platform | Status | Download |
|----------|--------|----------|
| Linux (Wayland, KDE Plasma 6) | ✅ | [Latest release ↗](https://github.com/p-o-d/matrix-screen-saver/releases/latest) |
| Windows 10+ (native `.scr`) | ✅ | [Latest release ↗](https://github.com/p-o-d/matrix-screen-saver/releases/latest) |

## Features

- GPU-accelerated via wgpu (Vulkan on Linux, DX12/Vulkan on Windows)
- Depth layers — near/far rain planes at configurable scales and brightness
- Optional glow effect (two-pass Gaussian blur + blend)
- Scanline overlay and chromatic aberration
- Configurable charset: katakana / latin / binary / mixed
- Configurable speed, density, color, FPS, drop length
- Multi-monitor support (Linux: one renderer per output; Windows: spans virtual desktop)
- Native Win32 settings dialog (accessible from Windows Screen Saver Settings)
- Debug overlay with CPU/RAM/GPU stats (Linux)

## Install

### Linux (KDE Plasma 6 / Wayland)

```bash
bash install.sh
```

Installs to `~/.local/bin/matrix-screensaver` and registers a KDE autostart entry. Activates automatically when idle for `timeout_seconds` (default 120s).

Test immediately:
```bash
matrix-screensaver --test
```

### Windows

1. Download `matrix-screensaver.scr` from the [latest release](https://github.com/p-o-d/matrix-screen-saver/releases/latest)
2. Copy to `C:\Windows\System32\`
3. Right-click the `.scr` file → **Install**  
   — or open **Screen Saver Settings** → select **Matrix Screensaver** → click **Settings** to configure

See [INSTALL-WINDOWS.md](INSTALL-WINDOWS.md) for details.

## Build from source

### Linux

**Requirements:** Rust stable, `fontconfig`, `libwayland-dev`, `libxkbcommon-dev`

```bash
cargo build -p matrix-linux --release
cargo test -p matrix-core
```

### Windows (cross-compile from Linux)

**Requirements:** `mingw-w64` (`sudo apt install mingw-w64` / `sudo pacman -S mingw-w64-gcc`)

```bash
rustup target add x86_64-pc-windows-gnu
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release
# Output: target/x86_64-pc-windows-gnu/release/matrix-screensaver.exe
```

### Windows (native)

```bash
cargo build -p matrix-windows --release
```

## Configuration

Config file (all fields optional, fall back to defaults):

- Linux: `~/.config/matrix-screensaver/config.toml`
- Windows: `%APPDATA%\matrix-screensaver\config.toml`

```toml
[display]
font = "monospace"
font_size = 36        # base cell size in pixels (nearest depth plane)
fps = 60

[rain]
speed = 1.0
density = 0.05
charset = "mixed"     # mixed | katakana | latin | binary
drop_length_min = 5
drop_length_max = 25
depth_levels = 6
depth_scale_min = 0.35
depth_brightness_min = 0.25
cluster_strength = 0.2

[colors]
primary = "#00ff41"
background = "#000000"
glow = true
glow_intensity = 0.8

[idle]
timeout_seconds = 120
```

## Workspace layout

```
crates/
├── matrix-core/     # shared: RainSimulator, Renderer, GlyphAtlas, Config, shaders
├── matrix-linux/    # Wayland event loop, idle detection, fc-match font, /proc stats
└── matrix-windows/  # Win32 screensaver + config dialog + embedded font fallback
```

## Documentation

- [Architecture](docs/architecture.md) — crate boundaries, data flow, adding a new platform
- [Rendering pipeline](docs/rendering.md) — GPU passes, shaders, instance buffer, depth layers

## Requirements

### Linux
- KDE Plasma 6 (or any compositor with `wlr-layer-shell` + `ext-idle-notify-v1`)
- Vulkan-capable GPU
- `fontconfig` installed

### Windows
- Windows 10 or later
- DirectX 12 or Vulkan GPU
- No additional runtime dependencies

## License

This project is licensed under the [MIT License](LICENSE).

The Windows binary embeds [JetBrains Mono](https://github.com/JetBrains/JetBrainsMono) as a fallback font,
which is licensed under the [SIL Open Font License 1.1](LICENSE-OFL).
