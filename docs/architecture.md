# Architecture

## Workspace layout

Three crates share a Cargo workspace. Platform crates own their event loops; core is a pure library.

```
matrix-core/     shared library — no platform I/O
matrix-linux/    Wayland binary
matrix-windows/  Win32 .scr binary
```

**Core-as-toolkit principle:** `matrix-core` never creates windows, opens files for fonts, or reads `/proc`. Platform crates do all of that, then hand pre-built objects into core APIs.

---

## matrix-core

### Config (`config.rs`)

`Config::load()` reads `{config_dir}/matrix-screensaver/config.toml` via `dirs::config_dir()` — resolves to `~/.config/` on Linux, `%APPDATA%` on Windows. All fields are optional; missing ones fall back to hardcoded defaults. Derives both `Serialize` and `Deserialize`.

### Characters (`chars.rs`)

`get_charset(kind)` returns a `&'static [char]` slice. Four variants: `Mixed` (katakana + latin), `Katakana`, `Latin`, `Binary`. Used by `GlyphAtlas::build` and `RainSimulator::new`.

### Glyph atlas (`atlas.rs`)

`GlyphAtlas::build(chars, font_size, font_bytes)` rasterizes every character with `fontdue` into a single row texture atlas. Layout: one cell per character, all cells equal size (`cell_width × cell_height`). Returns UV coordinates per character. Stores a `HashMap<char, usize>` for O(1) UV lookup at render time.

The atlas texture is uploaded to the GPU in `Renderer::new`; `GlyphAtlas` itself is CPU-side only.

### Rain simulation (`rain.rs`)

`RainSimulator` owns a 2-D grid of `Cell` structs (one per column/row position). Each column has an active drop (head position, speed, length, character sequence). A heat map drives clustering — nearby active drops raise heat; heat decays each frame and biases new drop spawning toward hot zones.

`sim.update(dt)` advances state. `sim.cells` is a flat `Vec<Cell>` read by the renderer each frame.

### Renderer (`renderer.rs`)

See [rendering.md](rendering.md) for GPU pipeline detail.

`Renderer::new(instance, surface, width, height, atlas, config, debug_atlas, stats)` — caller provides an already-created `wgpu::Instance` and `wgpu::Surface<'static>`. The renderer creates adapter/device/queue internally (so it can store `adapter_info` for the debug overlay).

`Renderer::resize(w, h)` reconfigures the swapchain **and** recreates offscreen + blur textures and all dependent bind groups.

### SystemStats (`stats.rs`)

Plain data struct — `cpu_pct`, `ram_used_gb`, `ram_total_gb`, `vram_used_gb`, `vram_total_gb`, `gpu_pct`. No I/O. Populated by the platform poller and read by the renderer's debug overlay.

---

## matrix-linux

### Entry point (`main.rs`)

`calloop` event loop. Manages a `Vec<SurfaceSlot>` — one slot per Wayland output. On `AppEvent::Idle`: create surfaces, build `GlyphAtlas`, start `Renderer` per screen. On `AppEvent::Resume`: destroy surfaces, drop renderers. On `AppEvent::Resize(idx, w, h)`: call `renderer.resize(w, h)`.

Frame rendering is driven by a `calloop` timer at the configured FPS.

### Wayland (`wayland_app.rs`)

Smithay-client-toolkit (SCTK 0.18) handles `wlr-layer-shell` surface creation, `ext-idle-notify-v1` idle/resume events, and output enumeration. Converts protocol events to `AppEvent` and sends them over a channel to the main loop.

Surface type: `Layer::Background`, `ZLayer::Bottom` — sits behind all windows.

### Font (`font.rs`)

`find_font(family) -> Vec<u8>` — runs `fc-match <family> --format=%{file}`, reads the resulting path. Panics with a user-facing message if `fontconfig` is unavailable.

### Stats (`stats.rs`)

Spawns a background thread that polls `/proc/stat` (CPU), `/proc/meminfo` (RAM), and vendor-specific GPU sysfs paths (NVIDIA via `nvidia-smi`, AMD via `/sys/class/drm/`). Writes into a `Arc<Mutex<SystemStats>>` shared with the renderer.

### wgpu surface creation

Wayland surface → `wgpu::Surface<'static>` via `raw-window-handle`:
```rust
WaylandWindowHandle::new(wl_surface.id().as_ptr())
WaylandDisplayHandle::new(conn.backend().display_ptr())
```

---

## matrix-windows

### Entry point (`main.rs`)

Parses command-line args and dispatches:

| Arg | Mode |
|-----|------|
| `/s` or `-s` | Fullscreen screensaver |
| `/p <HWND>` | Preview inside parent window |
| `/c`, `-c`, or none | Config dialog |

`#![windows_subsystem = "windows"]` suppresses the console window.

### Screensaver (`screensaver.rs`)

Creates a `WS_EX_TOPMOST | WS_POPUP` window spanning the full virtual desktop (`SM_XVIRTUALSCREEN`/`SM_YVIRTUALSCREEN` origin, `SM_CXVIRTUALSCREEN`/`SM_CYVIRTUALSCREEN` size) — covers all monitors in multi-display setups. WndProc exits on `WM_KEYDOWN`, `WM_LBUTTONDOWN`, `WM_RBUTTONDOWN`, and mouse movement past a 10-pixel threshold (tracked with a separate `MOUSE_INITIALIZED` flag to avoid false-quit when the cursor starts at 0,0).

Frame loop: `PeekMessage` (non-blocking) → `rain.update(dt)` → `renderer.render()` → `thread::sleep` to hit FPS target.

### Preview (`preview.rs`)

Creates a `WS_CHILD | WS_VISIBLE` window inside the parent HWND (passed via `/p`). Same render loop as screensaver. `WndProc` only calls `DefWindowProcW` — no exit on input.

### Config dialog (`config_dialog.rs`)

Programmatic Win32 dialog (no `.rc` resource file). Controls created in `WM_CREATE`:

| Control | Type | Config field |
|---------|------|-------------|
| Speed | Trackbar 0–200 | `rain.speed` × 100 |
| Density | Trackbar 0–100 | `rain.density` × 100 |
| FPS | Combobox 30/60/120 | `display.fps` |
| Charset | Combobox | `rain.charset` |
| Primary color | Button → `ChooseColorW` | `colors.primary` |
| Glow | Checkbox | `colors.glow` |

OK reads all controls, calls `toml::to_string(&config)`, writes to `%APPDATA%\matrix-screensaver\config.toml`. All config fields are preserved (no partial write).

### Font (`font.rs`)

Probes common system monospace font paths (Consolas, Courier New, MS Gothic). Falls back to `include_bytes!("../assets/JetBrainsMono-Regular.ttf")` — OFL 1.1 licensed, always available.

### wgpu surface creation

```rust
instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
    raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
    raw_window_handle: RawWindowHandle::Win32(
        Win32WindowHandle::new(NonZeroIsize::new(hwnd.0 as isize).unwrap())
    ),
})
```

---

## Adding a new platform

1. Create `crates/matrix-<platform>/`
2. Resolve a font → `Vec<u8>`
3. Create `wgpu::Instance` + `wgpu::Surface<'static>` from your window handle
4. Call `GlyphAtlas::build(chars, font_size, &font_bytes)`
5. Call `Renderer::new(instance, surface, w, h, atlas, &config, None, None).await`
6. Run your event loop: `rain.update(dt)` → `renderer.render(&depth_layers)` each frame
7. Call `renderer.resize(w, h)` on window resize
