# Matrix Screensaver — Design Spec
Date: 2026-05-09

## Context

Build a Matrix-movie-style rain screensaver for KDE Plasma 6 on Wayland. Written in Rust. GPU-accelerated, fully customizable via config file, with automated installer.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│  Rust Binary (matrix-screensaver)                   │
│                                                     │
│  ┌──────────────┐  ┌───────────────┐  ┌──────────┐ │
│  │ Idle Monitor │  │ Rain Simulator│  │ Renderer │ │
│  │ ext-idle-    │→ │ CPU: columns, │→ │  wgpu    │ │
│  │ notify-v1    │  │ drops, chars  │  │ + glyphon│ │
│  └──────────────┘  └───────────────┘  └──────────┘ │
│                                                     │
│  ┌────────────────────────────────────────────────┐ │
│  │ Config loader (TOML via serde)                 │ │
│  └────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────┘
         │ wlr-layer-shell-unstable-v1
         ↓
┌─────────────────────────────────────────────────────┐
│  KDE KPackage Plugin                                │
│  ~/.local/share/kscreenlocker/wallpapers/           │
│  matrix-screensaver/                                │
│    metadata.json  (registers in System Settings)    │
│    contents/ui/main.qml  (invokes Rust binary)      │
└─────────────────────────────────────────────────────┘
         │
┌─────────────────────────────────────────────────────┐
│  Installer (install.sh)                             │
│  1. cargo build --release                           │
│  2. install binary → ~/.local/bin/                  │
│  3. install KPackage → ~/.local/share/kscreenlocker │
│  4. write default config → ~/.config/matrix-ss/     │
│  5. install KDE autostart .desktop entry            │
└─────────────────────────────────────────────────────┘
```

## Library Stack

| Crate | Purpose |
|---|---|
| `wgpu` | GPU rendering (Vulkan backend on Linux) |
| `winit` | Wayland window/surface creation |
| `smithay-client-toolkit` | wlr-layer-shell protocol, Wayland client |
| `wayland-protocols` | `ext-idle-notify-v1` idle detection |
| `glyphon` | GPU text rendering — glyph atlas → wgpu textures |
| `serde` + `toml` | Config file deserialization |
| `rand` | Rain drop randomization |
| `tracing` + `tracing-subscriber` | Structured logging |

## Components

### 1. Config (`src/config.rs`)
- Reads `~/.config/matrix-screensaver/config.toml`
- Falls back to embedded defaults if file missing
- All fields optional (partial overrides work)

```toml
[display]
font = "monospace"
font_size = 18
fps = 60

[rain]
speed = 1.0           # animation speed multiplier
density = 0.05        # probability new drop spawns per column per frame
charset = "mixed"     # mixed | katakana | latin | binary
drop_length_min = 5
drop_length_max = 25

[colors]
primary = "#00ff41"   # main rain color
background = "#000000"
glow = true
glow_intensity = 0.8

[idle]
timeout_seconds = 120
```

### 2. Idle Monitor (`src/idle.rs`)
- Connects to Wayland compositor via `wayland-protocols`
- Binds `ext-idle-notify-v1` global
- Creates idle/resume notifications at configured timeout
- Sends events to main loop via channel

### 3. Rain Simulator (`src/rain.rs`)
- Stores array of Drop structs: `{column, head_row, length, speed, chars[]}`
- Each frame: advance drops by speed, spawn new drops at density probability
- Produces per-cell brightness values (head=1.0, gradient to 0.0 at tail)
- Character mutation: random char changes each frame at head position

### 4. Renderer (`src/renderer.rs`)
- Init: rasterize full charset → GPU texture atlas via `glyphon`
- Per frame:
  1. Build instance buffer from rain simulator output
  2. Render pass: draw instanced quads sampling atlas, apply brightness + color
  3. Glow pass: render to offscreen texture → Gaussian blur → additive blend
- Uses `wgpu` with Vulkan backend, surface created by `winit`

### 5. Window/Surface (`src/window.rs`)
- Uses `smithay-client-toolkit` for `wlr-layer-shell-unstable-v1`
- Layer: `Overlay`, anchor: all edges (fullscreen)
- Exclusive zone: -1 (don't shift other surfaces)
- Keyboard interactivity: on activation, grab keyboard to dismiss on any input
- Mouse: dismiss on move/click

### 6. Main Loop (`src/main.rs`)
- Initializes config, renderer, rain simulator, idle monitor
- Event loop: winit drives rendering, idle monitor signals on/off
- On idle signal: create/show layer surface, start render loop
- On input/resume signal: hide/destroy layer surface, pause simulation

## KDE Plugin

**Path:** `kde-plugin/matrix-screensaver/`

```
metadata.json          # KPackage metadata, registers as kscreenlocker wallpaper
contents/ui/main.qml  # Minimal QML — shows name/preview in System Settings
```

The Rust binary runs as a background process (via KDE autostart) and manages its own idle detection + Wayland surface. It activates **before** kscreenlocker (idle screensaver phase). The KDE plugin is for discoverability in System Settings only — it does not launch or control the binary. The autostart `.desktop` entry handles binary lifecycle.

## Installer (`install.sh`)

```bash
#!/usr/bin/env bash
set -euo pipefail

# 1. Build
cargo build --release

# 2. Install binary
install -Dm755 target/release/matrix-screensaver ~/.local/bin/matrix-screensaver

# 3. Install KDE plugin
PLUGIN_DIR=~/.local/share/kscreenlocker/wallpapers/matrix-screensaver
mkdir -p "$PLUGIN_DIR"
cp -r kde-plugin/matrix-screensaver/. "$PLUGIN_DIR/"

# 4. Default config
CONFIG_DIR=~/.config/matrix-screensaver
mkdir -p "$CONFIG_DIR"
if [[ ! -f "$CONFIG_DIR/config.toml" ]]; then
  cp config/default.toml "$CONFIG_DIR/config.toml"
fi

# 5. KDE autostart
AUTOSTART_DIR=~/.config/autostart
mkdir -p "$AUTOSTART_DIR"
cat > "$AUTOSTART_DIR/matrix-screensaver.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Matrix Screensaver
Exec=$HOME/.local/bin/matrix-screensaver
X-KDE-autostart-phase=2
EOF

echo "Installed. Run: ~/.local/bin/matrix-screensaver"
```

## Rendering Detail

**Glyph atlas:** Built at startup. Charset = half-width katakana (ｦ-ﾝ, ~56 chars) + A-Z + 0-9. Rasterized at configured font size to a single RGBA texture atlas. UV coordinates stored per character index.

**Per-character brightness:**
- Head of drop: white (`#ffffff`) with glow
- 1-3 cells below head: bright primary color (`#00ff41`)
- Rest of tail: gradient from primary to dim (`#003b10`)
- Fade to background over last 3 cells

**Glow (bloom):**
- Render to MSAA texture
- Two-pass separable Gaussian blur (horizontal + vertical) on bright channel
- Additive blend back onto main output

## Customizable Parameters

Font, font size, FPS cap, rain speed, column density, drop length range, charset, primary color, background color, glow on/off, glow intensity, idle timeout.

## Verification

1. `cargo build --release` — compiles clean, no warnings
2. `./target/release/matrix-screensaver --test` — renders one frame, exits 0
3. `bash install.sh` — all files installed to correct locations
4. Leave system idle for `timeout_seconds` → screensaver activates fullscreen
5. Move mouse or press key → screensaver dismisses
6. System Settings → Screen Locking → Wallpaper → plugin appears in list
