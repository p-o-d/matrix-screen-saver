# CLAUDE.md — matrix-screen-saver

## What this is

Matrix rain screensaver. Rust, Cargo workspace. Two targets:
- **Linux**: Wayland layer-shell daemon, activates on idle via `ext-idle-notify-v1` (KDE Plasma 6)
- **Windows**: native `.scr` screensaver (PE binary, `/s` fullscreen / `/c` config dialog / `/p` preview)

## Workspace layout

```
crates/
├── matrix-core/     # shared: RainSimulator, Renderer, GlyphAtlas, Config, chars, shaders, SystemStats
├── matrix-linux/    # Wayland event loop, idle detection, fc-match font, /proc stats poller
└── matrix-windows/  # Win32 screensaver, config dialog, embedded font fallback
```

## Build & install

```bash
# Linux
cargo build -p matrix-linux --release
cargo test -p matrix-core
bash install.sh                        # build + install binary + KDE autostart

# Windows cross-compile (requires mingw-w64: sudo pacman -S mingw-w64-gcc)
rustup target add x86_64-pc-windows-gnu
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release
# Output: target/x86_64-pc-windows-gnu/release/matrix-screensaver.exe → rename to .scr
```

Manual Linux install steps (install.sh does all of these):
- Binary → `~/.local/bin/matrix-screensaver`
- Config (first run only) → `~/.config/matrix-screensaver/config.toml`
- Autostart → `~/.config/autostart/matrix-screensaver.desktop`

## Test without idle wait (Linux)

```bash
matrix-screensaver --test      # forces activation immediately, exits after first rendered frame
```

## Architecture

### matrix-core

```
config.rs        — TOML config (serde Serialize+Deserialize), Config::load() uses dirs::config_dir()
chars.rs         — character sets (katakana / latin / binary / mixed)
atlas.rs         — fontdue glyph atlas; GlyphAtlas::build(chars, font_size, font_bytes)
rain.rs          — RainSimulator: columnar drop state, heat map clustering
renderer.rs      — wgpu renderer: Renderer::new(instance, surface, w, h, atlas, config, ...)
stats.rs         — SystemStats data struct (pure data; no platform I/O)
shaders/rain.wgsl    — vertex/fragment for rain instances
shaders/blur.wgsl    — Gaussian blur for glow (kernel unrolled — naga rejects dynamic array indexing)
shaders/rect.wgsl    — debug overlay rects
shaders/scanline.wgsl — scanline effect pass
```

### matrix-linux

```
main.rs          — calloop event loop, idle/resume lifecycle, per-screen rain+renderer lifecycle
wayland_app.rs   — Wayland protocol handlers, layer-shell surfaces, AppEvent channel
font.rs          — find_font(family) → Vec<u8> via fc-match
stats.rs         — /proc CPU/RAM poller, GpuSpec GPU stats; imports SystemStats from matrix-core
```

### matrix-windows

```
main.rs           — arg dispatch: /s → screensaver, /p → preview, /c (default) → config dialog
screensaver.rs    — fullscreen WS_EX_TOPMOST WS_POPUP, WndProc, PeekMessage render loop
preview.rs        — child HWND rendering for /p preview mode
config_dialog.rs  — programmatic Win32 dialog (no .rc), saves via toml::to_string
font.rs           — system font path probe + include_bytes! JetBrains Mono fallback (OFL 1.1)
assets/JetBrainsMono-Regular.ttf  — embedded fallback font (OFL 1.1, 264KB)
```

## Key invariants

**Renderer::new**: caller creates `wgpu::Instance` + `wgpu::Surface<'static>` from platform handle; core creates adapter/device/queue internally.

**Depth layers**: `config.rain.depth_levels` independent `RainSimulator`s per screen. Far (index 0, scale=`depth_scale_min`) → near (index N-1, scale=1.0). Drawn far→near (painter's algorithm) into one GPU instance buffer.

**Instance buffer sizing**: Must sum `ceil(w/(cw*s)) * ceil(h/(ch*s))` across all depth levels — far level (small scale) has the most cells.

**Wayland surface matching**: `LayerSurface::PartialEq` uses `Arc::ptr_eq`. `LayerSurface::wl_surface()` is private in SCTK 0.18.

**blur.wgsl**: naga 0.20 rejects dynamic array indexing in loops. Gaussian kernel weights are unrolled.

**Multi-monitor (Linux)**: one `SurfaceSlot` per output in `app_state.surfaces`. `AppEvent::Resize(usize, u32, u32)` carries the surface index.

**Renderer::resize**: recreates offscreen and blur textures + all dependent bind groups. Must be called on every surface resize.

**Config serialization**: all Config structs derive both `Serialize` and `Deserialize`. Windows config dialog saves via `toml::to_string(&config)` to preserve all fields.

## Config file

Same TOML schema on both platforms. Path via `dirs::config_dir()`:
- Linux: `~/.config/matrix-screensaver/config.toml`
- Windows: `%APPDATA%\matrix-screensaver\config.toml`

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

## Platform requirements

### Linux
- `wlr-layer-shell` (KDE Plasma 6 supports this)
- `ext-idle-notify-v1` (Plasma 6.1+)
- `WAYLAND_DISPLAY` must be set
- `fontconfig` + `fc-match` for font resolution

### Windows
- Windows 10+
- DirectX 12 or Vulkan capable GPU
- No runtime dependencies (font embedded, wgpu statically linked)

## GPU

wgpu 0.20. Linux: Vulkan backend. Windows: DX12 preferred, Vulkan fallback.
Two render passes: rain instances → offscreen texture; blur + blend → swapchain.
