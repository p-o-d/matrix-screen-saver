# Cross-Platform Matrix Screensaver — Design Spec

**Date:** 2026-05-10  
**Status:** Approved

## Context

Current codebase is Wayland/Linux-only. Adding Windows as a second target with:
- Native `.scr` screensaver format (PE binary responding to `/s` `/c` `/p` args)
- Native Win32 config dialog (accessible from Screen Saver Settings via `/c`)
- Maximum shared code via Cargo workspace

## Architecture

**Core as toolkit, platform drives.** Platform crates own event loops entirely. No shared loop abstraction — Linux (calloop/Wayland) and Windows (Win32 GetMessage) are structurally incompatible. Core exposes data types and GPU APIs; platforms wire everything together.

## Workspace Layout

```
matrix-screen-saver/
├── Cargo.toml                    # [workspace] members = ["crates/*"]
├── crates/
│   ├── matrix-core/              # shared: rain, renderer, atlas, config, chars, shaders
│   ├── matrix-linux/             # Wayland event loop + idle detection + fc-match font
│   └── matrix-windows/           # Win32 window + event loop + config dialog
└── install.sh                    # Linux only, unchanged
```

### matrix-core

```
crates/matrix-core/src/
├── lib.rs
├── rain.rs           # RainSimulator — moved unchanged
├── chars.rs          # charset definitions — moved unchanged
├── config.rs         # TOML config structs — moved unchanged
├── atlas.rs          # GlyphAtlas::new(font_bytes: &[u8], config) — font-finding removed
├── renderer.rs       # Renderer::new(device, queue, surface, w, h, atlas, config) — surface creation removed
└── shaders/          # all .wgsl files
```

**Dependencies:** wgpu, fontdue, serde, toml, dirs, rand, bytemuck, tracing

### matrix-linux

```
crates/matrix-linux/src/
├── main.rs           # event loop (calloop), idle/resume lifecycle
├── wayland_app.rs    # Wayland protocol handlers — moved unchanged
├── font.rs           # fc-match font resolution → Vec<u8>
└── stats.rs          # /proc CPU/RAM/GPU monitoring — moved unchanged
```

**Dependencies:** smithay-client-toolkit, calloop, calloop-wayland-source, wayland-*, raw-window-handle, matrix-core

### matrix-windows

```
crates/matrix-windows/src/
├── main.rs           # parses /s /c /p args, dispatches
├── screensaver.rs    # fullscreen HWND + WndProc + render loop
├── preview.rs        # child HWND rendering for /p preview
├── config_dialog.rs  # native Win32 dialog via DialogBoxParam
└── font.rs           # Windows font resolution; embedded fallback via include_bytes!
```

**Dependencies:** windows (with Win32 features), raw-window-handle, matrix-core

## Core Refactors

### atlas.rs

Remove `find_font()`. Platform provides font bytes.

```rust
// Before
pub fn new(family: &str, config: &Config) -> Self

// After
pub fn new(font_bytes: &[u8], config: &Config) -> Self
```

### renderer.rs

Remove Wayland raw-pointer surface creation. Caller provides wgpu objects.

```rust
// Before
pub fn new(display_ptr: *mut c_void, surface_ptr: *mut c_void, w, h, atlas, config) -> Self

// After
pub fn new(
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface,
    w: u32, h: u32,
    atlas: &GlyphAtlas,
    config: &Config,
) -> Self
```

Platform sequence: create wgpu instance → request adapter (compatible with platform surface) → create device/queue → create surface from platform handle → call `Renderer::new()`.

## Linux Platform Details

Minimal changes — files move, one new file added:

- `font.rs`: extract `find_font(family) -> Vec<u8>` (fc-match invocation) from current `atlas.rs`
- `main.rs`: call `font::find_font()` → bytes → `GlyphAtlas::new(bytes, config)`. Extract wgpu surface creation from Wayland raw pointers (`WaylandDisplayHandle` + `WaylandWindowHandle`) before calling `Renderer::new()`.
- All Wayland event loop logic, idle detection, resize handling: unchanged.

## Windows Platform Details

### Entry point

```rust
// main.rs
match args {
    ["/s"] | ["-s"] => screensaver::run(),
    ["/p", hwnd]    => preview::run(parse_hwnd(hwnd)),
    ["/c"] | ["-c"] | [] => config_dialog::run(),
}
```

### Screensaver mode (`screensaver.rs`)

- `CreateWindowEx(WS_EX_TOPMOST, WS_POPUP | WS_VISIBLE)` fullscreen on primary monitor
- WndProc exits on: `WM_KEYDOWN`, `WM_MOUSEMOVE` (past threshold), `WM_LBUTTONDOWN`, `WM_RBUTTONDOWN`, `WM_DESTROY`
- wgpu surface via `raw-window-handle` `Win32WindowHandle` (HWND + HINSTANCE)
- Frame loop: `PeekMessage` non-blocking → `rain.step()` → `renderer.render()` → sleep to FPS target
- No idle detection needed — Windows OS launches `.scr /s` at screensaver timeout

### Preview mode (`preview.rs`)

- Create child window inside parent HWND bounds (from `/p HWND` arg)
- Same render loop, sized to parent

### Config dialog (`config_dialog.rs`)

- `DialogBoxParam` with programmatically created controls (no .rc file)
- Controls:
  - Trackbars: speed, density, glow_intensity
  - Comboboxes: fps (list), charset (mixed/katakana/latin/binary)
  - Color button: `ChooseColor` for primary color
  - Checkbox: glow enabled
- On OK: serialize to `%APPDATA%\matrix-screensaver\config.toml` via `matrix_core::Config`
- Config file path: `dirs::config_dir()` handles cross-platform path correctly

### Font resolution (`font.rs`)

1. Try `EnumFontFamiliesEx` to locate a system monospace font file path
2. Fallback: `include_bytes!()` an embedded OFL-licensed monospace font

## Config File

Same TOML schema on both platforms. `dirs::config_dir()` returns:
- Linux: `~/.config/matrix-screensaver/config.toml`
- Windows: `%APPDATA%\matrix-screensaver\config.toml`

No schema changes. Existing tests (`rain_test.rs`, `config_test.rs`) move to `matrix-core/tests/` and run unchanged on both platforms.

## Verification

```bash
# Linux — no regression
cargo build -p matrix-linux --release
cargo test -p matrix-core
~/.local/bin/matrix-screensaver --test

# Cross-compile Windows
rustup target add x86_64-pc-windows-gnu
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release

# Windows runtime (on Windows VM)
matrix-screensaver.scr /c    # config dialog opens, settings persist to %APPDATA%
matrix-screensaver.scr /s    # fullscreen rain, exits on keypress/mouse
matrix-screensaver.scr /p 0  # preview renders into HWND
```

## Files Changed

| Action | Path |
|--------|------|
| Rewrite | `Cargo.toml` → workspace root |
| Create | `crates/matrix-core/` (rain, chars, config, atlas refactored, renderer refactored, shaders) |
| Create | `crates/matrix-linux/` (main adapted, wayland_app moved, font.rs new, stats moved) |
| Create | `crates/matrix-windows/` (all new) |
| Delete | `src/` |
| Unchanged | `install.sh`, `config/default.toml`, `kde-plugin/` |
