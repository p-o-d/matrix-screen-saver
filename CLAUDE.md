# CLAUDE.md — matrix-screen-saver

## What this is

Matrix movie-style rain screensaver for KDE Plasma 6 on Wayland. Written in Rust. Runs as a background daemon; activates on idle via `ext-idle-notify-v1`.

## Build & install

```bash
cargo build --release          # build
cargo test                     # run unit tests
bash install.sh                # build + install binary + KDE autostart entry
```

Manual install steps (install.sh does all of these):
- Binary → `~/.local/bin/matrix-screensaver`
- Config (first run only) → `~/.config/matrix-screensaver/config.toml`
- Autostart → `~/.config/autostart/matrix-screensaver.desktop`

## Test without idle wait

```bash
matrix-screensaver --test      # forces activation immediately, exits after first rendered frame
```

## Architecture

```
main.rs          — event loop, idle/resume logic, per-screen rain+renderer lifecycle
wayland_app.rs   — Wayland state, layer-shell surfaces, idle-notify dispatch, AppEvent channel
config.rs        — TOML config (serde), Config::load() reads ~/.config/matrix-screensaver/config.toml
chars.rs         — character sets (katakana / latin / binary / mixed)
atlas.rs         — fontdue glyph atlas; one texture atlas per session
rain.rs          — RainSimulator: columnar drop state, heat map clustering
renderer.rs      — wgpu renderer: instanced quads, depth layers, optional glow (blur + blend passes)
shaders/rain.wgsl  — vertex/fragment for rain instances
shaders/blur.wgsl  — Gaussian blur for glow effect (loop unrolled — naga rejects dynamic array indexing)
```

## Key invariants

**Depth layers**: `config.rain.depth_levels` independent `RainSimulator`s per screen. Far (index 0, scale=`depth_scale_min`) → near (index N-1, scale=1.0). Drawn far→near (painter's algorithm) into one GPU instance buffer.

**Instance buffer sizing**: Must sum `ceil(w/(cw*s)) * ceil(h/(ch*s))` across all depth levels — far level (small scale) has the most cells. Sizing for base level only fills the buffer and truncates near layers.

**Wayland surface matching**: `LayerSurface::PartialEq` uses `Arc::ptr_eq` — safe to compare with `==`. `LayerSurface::wl_surface()` is private in SCTK 0.18; don't try to call it.

**blur.wgsl**: naga 0.20 rejects dynamic array indexing in loops. Gaussian kernel weights are unrolled into explicit variables.

**Multi-monitor**: one `SurfaceSlot` per output in `app_state.surfaces`. `AppEvent::Resize(usize, u32, u32)` carries the surface index.

## Config file

`~/.config/matrix-screensaver/config.toml` — all fields optional, missing ones fall back to defaults.

```toml
[display]
font = "monospace"
font_size = 36        # base cell size (nearest depth plane)
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

## Wayland requirements

- `wlr-layer-shell` (KDE Plasma 6 supports this)
- `ext-idle-notify-v1` (Plasma 6.1+)
- `WAYLAND_DISPLAY` must be set

## GPU

wgpu 0.20, Vulkan backend. Two render passes: rain instances → offscreen; blur + blend → swapchain.
