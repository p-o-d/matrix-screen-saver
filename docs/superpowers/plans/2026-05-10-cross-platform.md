# Cross-Platform Matrix Screensaver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reorganize into a Cargo workspace with three crates — `matrix-core` (shared), `matrix-linux` (Wayland), `matrix-windows` (Win32 .scr) — so the screensaver builds on both Linux and Windows.

**Architecture:** Core-as-toolkit: platform crates own event loops and surface creation; `matrix-core` provides `RainSimulator`, `GlyphAtlas`, `Renderer`, and `Config`. `Renderer::new` is refactored to accept a pre-created `wgpu::Instance` + `wgpu::Surface<'static>` instead of raw Wayland pointers. Platform crates handle font lookup and wgpu surface creation before handing off to core.

**Tech Stack:** Rust, wgpu 0.20, fontdue 0.8, smithay-client-toolkit 0.18 (Linux), windows crate (Windows), raw-window-handle 0.6, pollster 0.3.

---

## File Map

| Action | Path | Responsibility |
|--------|------|----------------|
| **Create** | `Cargo.toml` | Workspace root, replaces package Cargo.toml |
| **Create** | `crates/matrix-core/Cargo.toml` | Core lib deps (wgpu, fontdue, serde, toml, dirs, rand, bytemuck, tracing) |
| **Create** | `crates/matrix-core/src/lib.rs` | pub mod re-exports |
| **Move+keep** | `crates/matrix-core/src/rain.rs` | RainSimulator (unchanged) |
| **Move+keep** | `crates/matrix-core/src/chars.rs` | Charset definitions (unchanged) |
| **Move+keep** | `crates/matrix-core/src/config.rs` | TOML config structs (unchanged) |
| **Move+refactor** | `crates/matrix-core/src/atlas.rs` | `GlyphAtlas::build(chars, size, font_bytes: &[u8])` — fc-match removed |
| **Move+refactor** | `crates/matrix-core/src/renderer.rs` | `Renderer::new(instance, surface, …)` — Wayland handle creation removed |
| **Create** | `crates/matrix-core/src/stats.rs` | `SystemStats` data struct only (no /proc reading) |
| **Move+keep** | `crates/matrix-core/src/shaders/` | All .wgsl files unchanged |
| **Create** | `crates/matrix-core/tests/rain_test.rs` | Moved from repo root tests/ |
| **Create** | `crates/matrix-core/tests/config_test.rs` | Moved from repo root tests/ |
| **Create** | `crates/matrix-linux/Cargo.toml` | Linux binary deps (smithay-client-toolkit, calloop, wayland-*, raw-window-handle, matrix-core) |
| **Create** | `crates/matrix-linux/src/font.rs` | `find_font(family: &str) -> Vec<u8>` via fc-match |
| **Move+adapt** | `crates/matrix-linux/src/main.rs` | Uses `font::find_font`, creates wgpu instance+surface, calls `Renderer::new` |
| **Move+keep** | `crates/matrix-linux/src/wayland_app.rs` | Wayland protocol handlers (unchanged) |
| **Move+adapt** | `crates/matrix-linux/src/stats.rs` | Poller logic; imports `matrix_core::stats::SystemStats`; keeps `GpuSpec` |
| **Create** | `crates/matrix-windows/Cargo.toml` | Windows binary deps (windows crate, raw-window-handle, matrix-core, pollster) |
| **Create** | `crates/matrix-windows/src/main.rs` | Parses /s /c /p args, dispatches |
| **Create** | `crates/matrix-windows/src/font.rs` | System font lookup + embedded fallback |
| **Create** | `crates/matrix-windows/src/screensaver.rs` | Fullscreen HWND, WndProc, wgpu surface, render loop |
| **Create** | `crates/matrix-windows/src/preview.rs` | Child HWND rendering for /p |
| **Create** | `crates/matrix-windows/src/config_dialog.rs` | Native Win32 dialog with all config fields |
| **Create** | `crates/matrix-windows/assets/JetBrainsMono-Regular.ttf` | Embedded fallback font (OFL) |
| **Delete** | `src/` | Replaced by crates/ |

---

## Phase 1: Workspace Skeleton

### Task 1: Create workspace root Cargo.toml

**Files:**
- Modify: `Cargo.toml` (full rewrite)

- [ ] **Step 1: Replace root Cargo.toml with workspace manifest**

```toml
# Cargo.toml
[workspace]
members = ["crates/matrix-core", "crates/matrix-linux", "crates/matrix-windows"]
resolver = "2"
```

- [ ] **Step 2: Create crate directories**

```bash
mkdir -p crates/matrix-core/src/shaders
mkdir -p crates/matrix-linux/src
mkdir -p crates/matrix-windows/src
mkdir -p crates/matrix-windows/assets
```

- [ ] **Step 3: Create matrix-core Cargo.toml**

```toml
# crates/matrix-core/Cargo.toml
[package]
name = "matrix-core"
version = "0.1.0"
edition = "2021"

[lib]
name = "matrix_core"
path = "src/lib.rs"

[dependencies]
wgpu = "0.20"
bytemuck = { version = "1", features = ["derive"] }
raw-window-handle = "0.6"
pollster = "0.3"
fontdue = "0.8"
serde = { version = "1", features = ["derive"] }
toml = "1"
dirs = "5"
rand = { version = "0.8", features = ["small_rng"] }
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create matrix-linux Cargo.toml**

```toml
# crates/matrix-linux/Cargo.toml
[package]
name = "matrix-linux"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "matrix-screensaver"
path = "src/main.rs"

[dependencies]
matrix-core = { path = "../matrix-core" }
smithay-client-toolkit = { version = "0.18", features = ["calloop"] }
wayland-client = "0.31"
wayland-backend = { version = "0.3", features = ["client_system"] }
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
wayland-protocols-wlr = { version = "0.2", features = ["client"] }
calloop = "0.13"
calloop-wayland-source = "0.3"
raw-window-handle = "0.6"
wgpu = "0.20"
pollster = "0.3"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/matrix-core/Cargo.toml crates/matrix-linux/Cargo.toml
git commit -m "chore: convert to cargo workspace with matrix-core and matrix-linux crates"
```

---

### Task 2: Populate matrix-core with shared source files

**Files:**
- Create: `crates/matrix-core/src/lib.rs`
- Create: `crates/matrix-core/src/rain.rs` (copy of `src/rain.rs`)
- Create: `crates/matrix-core/src/chars.rs` (copy of `src/chars.rs`)
- Create: `crates/matrix-core/src/config.rs` (copy of `src/config.rs`)
- Create: `crates/matrix-core/src/atlas.rs` (copy of `src/atlas.rs` — unchanged for now)
- Create: `crates/matrix-core/src/renderer.rs` (copy of `src/renderer.rs` — unchanged for now)
- Create: `crates/matrix-core/src/stats.rs` (new — just the SystemStats struct)
- Create: `crates/matrix-core/src/shaders/*` (copies of `src/shaders/*`)

- [ ] **Step 1: Copy unchanged source files**

```bash
cp src/rain.rs crates/matrix-core/src/rain.rs
cp src/chars.rs crates/matrix-core/src/chars.rs
cp src/config.rs crates/matrix-core/src/config.rs
cp src/atlas.rs crates/matrix-core/src/atlas.rs
cp src/renderer.rs crates/matrix-core/src/renderer.rs
cp src/shaders/*.wgsl crates/matrix-core/src/shaders/
```

- [ ] **Step 2: Create matrix-core/src/stats.rs with just the data struct**

```rust
// crates/matrix-core/src/stats.rs
#[derive(Debug, Clone, Default)]
pub struct SystemStats {
    pub ram_used_gb: f32,
    pub ram_total_gb: f32,
    pub cpu_pct: f32,
    pub vram_used_gb: f32,
    pub vram_total_gb: f32,
    pub gpu_pct: f32,
}
```

- [ ] **Step 3: Create matrix-core/src/lib.rs**

```rust
// crates/matrix-core/src/lib.rs
pub mod config;
pub mod chars;
pub mod rain;
pub mod atlas;
pub mod renderer;
pub mod stats;
```

- [ ] **Step 4: Fix renderer.rs — update the import of SystemStats to use local path**

In `crates/matrix-core/src/renderer.rs`, line 1 currently has:
```rust
use crate::{atlas::GlyphAtlas, config::Config, rain::CellState, stats::SystemStats};
```
This already uses `crate::` so it is correct within matrix-core. No change needed.

- [ ] **Step 5: Verify matrix-core compiles (will fail due to Wayland handles in renderer — that's OK for now)**

```bash
cargo check -p matrix-core 2>&1 | head -40
```

Expected: errors about `WaylandDisplayHandle`, `WaylandWindowHandle` — because those are in renderer.rs which we haven't refactored yet. That is expected at this stage.

- [ ] **Step 6: Commit**

```bash
git add crates/matrix-core/
git commit -m "chore: populate matrix-core with shared source files"
```

---

### Task 3: Populate matrix-linux and verify it builds

**Files:**
- Create: `crates/matrix-linux/src/main.rs` (copy of `src/main.rs` — import paths updated)
- Create: `crates/matrix-linux/src/wayland_app.rs` (copy of `src/wayland_app.rs`)
- Create: `crates/matrix-linux/src/stats.rs` (poller logic from `src/stats.rs`, imports SystemStats from matrix-core)

- [ ] **Step 1: Copy wayland_app.rs unchanged**

```bash
cp src/wayland_app.rs crates/matrix-linux/src/wayland_app.rs
```

- [ ] **Step 2: Create crates/matrix-linux/src/stats.rs**

Copy everything from `src/stats.rs` **except** the `SystemStats` struct definition (which now lives in matrix-core). Replace the struct with an import, and keep `GpuSpec` and all the poller logic.

```rust
// crates/matrix-linux/src/stats.rs
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use matrix_core::stats::SystemStats;

/// PCI vendor+device IDs of the wgpu-selected GPU adapter.
#[derive(Debug, Clone)]
pub struct GpuSpec {
    pub vendor: u32,
    pub device: u32,
}

// --- all remaining code from src/stats.rs verbatim below this line ---
// (enum GpuKind, struct CpuSnapshot, fn read_cpu_snapshot, fn detect_gpu,
//  fn start_stats_poller, etc. — copy-paste unchanged)
```

Open `src/stats.rs` and copy every line except the `SystemStats` struct block (lines 5-13) into `crates/matrix-linux/src/stats.rs`, appending after the header above.

- [ ] **Step 3: Create crates/matrix-linux/src/main.rs**

Copy `src/main.rs` verbatim then update the two `use` declarations at the top:

```rust
// crates/matrix-linux/src/main.rs
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};

use matrix_core::config::Config;
use matrix_core::chars::get_charset;
use matrix_core::rain::RainSimulator;
use matrix_core::atlas::GlyphAtlas;
use matrix_core::renderer::Renderer;
use crate::stats::{GpuSpec, SystemStats, start_stats_poller};
use crate::wayland_app::{AppEvent, AppState};

mod stats;
mod wayland_app;
```

Replace all subsequent occurrences of `matrix_screensaver::` with `matrix_core::` throughout the file. The helper functions `depth_levels`, `make_rains`, `try_subscribe_idle`, `get_display_ptr`, `get_surface_ptr` can be copied verbatim.

- [ ] **Step 4: Check matrix-linux compiles**

```bash
cargo check -p matrix-linux 2>&1 | head -60
```

Expected: errors about `Renderer::new` signature mismatch (we haven't updated it yet). We will fix this in Phase 2.

- [ ] **Step 5: Commit**

```bash
git add crates/matrix-linux/
git commit -m "chore: populate matrix-linux crate (pre-refactor, compilation errors expected)"
```

---

## Phase 2: Core Refactors — Clean Platform Boundary

### Task 4: Refactor GlyphAtlas to accept font bytes

**Files:**
- Modify: `crates/matrix-core/src/atlas.rs`
- Create: `crates/matrix-linux/src/font.rs`
- Modify: `crates/matrix-linux/src/main.rs`

- [ ] **Step 1: Write a failing test for the new atlas interface**

```rust
// crates/matrix-core/tests/atlas_test.rs  (new file)
#[test]
fn build_atlas_from_bytes() {
    // Minimal valid TTF: JetBrains Mono embedded in test via include_bytes
    // For now use a system font that we know exists on Linux CI
    let font_bytes = std::fs::read("/usr/share/fonts/TTF/DejaVuSansMono.ttf")
        .or_else(|_| std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"))
        .expect("DejaVu Sans Mono not found — install fonts-dejavu-core");
    let chars: Vec<char> = "ABCabc".chars().collect();
    let atlas = matrix_core::atlas::GlyphAtlas::build(&chars, 14.0, &font_bytes);
    assert!(atlas.atlas_width > 0);
    assert!(atlas.atlas_height > 0);
    assert_eq!(atlas.chars.len(), 6);
}
```

- [ ] **Step 2: Run the test — verify it fails**

```bash
cargo test -p matrix-core atlas_test 2>&1
```

Expected: `FAILED` — `build` still takes `&str` not `&[u8]`.

- [ ] **Step 3: Refactor GlyphAtlas::build in crates/matrix-core/src/atlas.rs**

Replace the `build` signature and body — remove `find_font`, accept bytes directly:

```rust
// crates/matrix-core/src/atlas.rs  (full file)
use fontdue::{Font, FontSettings};

pub struct GlyphAtlas {
    pub data: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    pub cell_width: u32,
    pub cell_height: u32,
    pub uvs: Vec<[f32; 4]>,
    pub chars: Vec<char>,
}

impl GlyphAtlas {
    pub fn build(chars: &[char], font_size: f32, font_bytes: &[u8]) -> Self {
        let font = Font::from_bytes(font_bytes, FontSettings::default())
            .expect("invalid font bytes");

        let per_char: Vec<(fontdue::Metrics, Vec<u8>)> = chars
            .iter()
            .map(|&ch| font.rasterize(ch, font_size))
            .collect();

        let cell_width = per_char.iter().map(|(m, _)| m.width).max().unwrap_or(10) as u32;
        let cell_height = per_char.iter().map(|(m, _)| m.height).max().unwrap_or(18) as u32;
        let cell_width = cell_width.max(1);
        let cell_height = cell_height.max(1);

        let num_chars = chars.len() as u32;
        let atlas_width = cell_width * num_chars;
        let atlas_height = cell_height;
        let mut atlas_data = vec![0u8; (atlas_width * atlas_height) as usize];
        let mut uvs = Vec::with_capacity(chars.len());

        for (i, (metrics, bitmap)) in per_char.iter().enumerate() {
            let x_base = i as u32 * cell_width;
            let x_pad = (cell_width.saturating_sub(metrics.width as u32)) / 2;
            let y_pad = (cell_height.saturating_sub(metrics.height as u32)) / 2;
            for row in 0..metrics.height {
                for col in 0..metrics.width {
                    let src = row * metrics.width + col;
                    let dst_x = x_base + x_pad + col as u32;
                    let dst_y = y_pad + row as u32;
                    let dst = (dst_y * atlas_width + dst_x) as usize;
                    if dst < atlas_data.len() && src < bitmap.len() {
                        atlas_data[dst] = bitmap[src];
                    }
                }
            }
            let u = x_base as f32 / atlas_width as f32;
            uvs.push([u, 0.0f32, cell_width as f32 / atlas_width as f32, 1.0f32]);
        }

        Self {
            data: atlas_data,
            atlas_width,
            atlas_height,
            cell_width,
            cell_height,
            uvs,
            chars: chars.to_vec(),
        }
    }

    pub fn uv_for_char(&self, ch: char) -> [f32; 4] {
        self.chars
            .iter()
            .position(|&c| c == ch)
            .map(|i| self.uvs[i])
            .unwrap_or_else(|| self.uvs.first().copied().unwrap_or([0.0; 4]))
    }
}
```

- [ ] **Step 4: Run the atlas test — verify it passes**

```bash
cargo test -p matrix-core atlas_test 2>&1
```

Expected: `test build_atlas_from_bytes ... ok`

- [ ] **Step 5: Create crates/matrix-linux/src/font.rs**

```rust
// crates/matrix-linux/src/font.rs
use std::path::PathBuf;

/// Resolve a font family name to font file bytes using fc-match.
pub fn find_font(family: &str) -> Vec<u8> {
    let path = resolve_path(family);
    std::fs::read(&path)
        .unwrap_or_else(|e| panic!("cannot read font '{}': {e}", path.display()))
}

fn resolve_path(family: &str) -> PathBuf {
    let output = std::process::Command::new("fc-match")
        .args([family, "--format=%{file}"])
        .output()
        .expect("fc-match not found — install fontconfig (pacman -S fontconfig)");
    if !output.status.success() {
        panic!("fc-match failed for family '{family}'");
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        panic!("fc-match returned empty path for family '{family}'");
    }
    PathBuf::from(path)
}
```

- [ ] **Step 6: Update crates/matrix-linux/src/main.rs to use font::find_font**

Add `mod font;` at the top of main.rs and replace:
```rust
// OLD (around line 33)
let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_family));
```
with:
```rust
// NEW
let font_bytes = font::find_font(&font_family);
let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_bytes));
```

Also replace the debug atlas build (around line 43):
```rust
// OLD
let da = Arc::new(GlyphAtlas::build(&debug_chars, 14.0, &config.display.font));
```
with:
```rust
// NEW
let debug_font_bytes = font::find_font(&config.display.font);
let da = Arc::new(GlyphAtlas::build(&debug_chars, 14.0, &debug_font_bytes));
```

- [ ] **Step 7: Check matrix-linux compiles (atlas part)**

```bash
cargo check -p matrix-linux 2>&1 | grep "atlas\|font\|GlyphAtlas"
```

Expected: no atlas-related errors.

- [ ] **Step 8: Commit**

```bash
git add crates/matrix-core/src/atlas.rs crates/matrix-core/tests/atlas_test.rs \
        crates/matrix-linux/src/font.rs crates/matrix-linux/src/main.rs
git commit -m "refactor: GlyphAtlas::build accepts font bytes; add font.rs to matrix-linux"
```

---

### Task 5: Refactor Renderer::new — remove Wayland surface creation

**Files:**
- Modify: `crates/matrix-core/src/renderer.rs`
- Modify: `crates/matrix-linux/src/main.rs`

- [ ] **Step 1: Update Renderer::new signature in crates/matrix-core/src/renderer.rs**

Find the `impl Renderer` block and replace `pub async fn new(...)` (lines ~211–241 of the original). Remove the Wayland handle creation code. New signature:

```rust
pub async fn new(
    instance: wgpu::Instance,
    surface: wgpu::Surface<'static>,
    width: u32,
    height: u32,
    atlas: Arc<GlyphAtlas>,
    config: &Config,
    debug_atlas: Option<Arc<GlyphAtlas>>,
    stats: Option<Arc<Mutex<SystemStats>>>,
) -> Self {
    // Remove the entire block that was:
    //   use raw_window_handle::{...WaylandDisplayHandle, WaylandWindowHandle};
    //   use std::ptr::NonNull;
    //   let instance = wgpu::Instance::new(...);
    //   let surface = unsafe { instance.create_surface_unsafe(...) };
    //
    // Keep everything from "let adapter = instance.request_adapter..." onward.
    // The `instance` and `surface` variables are now the parameters above.
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        })
        .await
        .expect("no suitable GPU adapter found");

    let adapter_info = adapter.get_info();

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default(), None)
        .await
        .expect("GPU device request failed");

    // Keep everything from "let caps = surface.get_capabilities(&adapter);" onward
    // verbatim — no other changes in Renderer::new.
```

The complete diff is: delete lines 221–241 (the `use raw_window_handle` block through `.expect("wgpu surface creation failed")`) and add `instance` + `surface` as parameters.

- [ ] **Step 2: Remove unused imports from renderer.rs**

Remove these imports from the top of `crates/matrix-core/src/renderer.rs` if present:
```rust
// DELETE these lines if they exist at the top:
use raw_window_handle::{...};
use std::ptr::NonNull;
use std::ffi::c_void;
```

- [ ] **Step 3: Update matrix-linux/src/main.rs — create wgpu surface before calling Renderer::new**

Add a helper function at the bottom of `crates/matrix-linux/src/main.rs`:

```rust
/// Create a wgpu Instance and Wayland surface for the given wl_surface.
fn create_wgpu_surface(
    conn: &Connection,
    wl_surface: &wayland_client::protocol::wl_surface::WlSurface,
) -> (wgpu::Instance, wgpu::Surface<'static>) {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle,
        WaylandDisplayHandle, WaylandWindowHandle,
    };
    use std::ptr::NonNull;

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
        ..Default::default()
    });

    let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
    let surface_ptr = {
        use wayland_client::Proxy;
        wl_surface.id().as_ptr() as *mut std::ffi::c_void
    };

    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Wayland(
                WaylandDisplayHandle::new(NonNull::new(display_ptr).unwrap()),
            ),
            raw_window_handle: RawWindowHandle::Wayland(
                WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()),
            ),
        })
    }
    .expect("wgpu surface creation failed");

    (instance, surface)
}
```

Then replace the `Renderer::new` call site (around the `AppEvent::Resize` handler):

```rust
// OLD
let display_ptr = get_display_ptr(&conn);
let surface_ptr = get_surface_ptr(&app_state.surfaces[idx].wl_surface);
let r = pollster::block_on(Renderer::new(
    display_ptr, surface_ptr, w, h, atlas.clone(), &config,
    debug_atlas.clone(), debug_stats.clone(),
));
```

```rust
// NEW
let (wgpu_instance, wgpu_surface) = create_wgpu_surface(
    &conn,
    &app_state.surfaces[idx].wl_surface,
);
let r = pollster::block_on(Renderer::new(
    wgpu_instance, wgpu_surface, w, h, atlas.clone(), &config,
    debug_atlas.clone(), debug_stats.clone(),
));
```

Delete the old `get_display_ptr` and `get_surface_ptr` helper functions from main.rs.

- [ ] **Step 4: Move tests to matrix-core**

```bash
cp tests/rain_test.rs crates/matrix-core/tests/rain_test.rs
cp tests/config_test.rs crates/matrix-core/tests/config_test.rs
```

In each copied test file, update the crate import:
```rust
// OLD
use matrix_screensaver::...;
// NEW
use matrix_core::...;
```

- [ ] **Step 5: Build and test**

```bash
cargo build -p matrix-linux --release 2>&1
cargo test -p matrix-core 2>&1
```

Expected: both pass with zero errors.

- [ ] **Step 6: Smoke test the Linux binary**

```bash
./target/release/matrix-screensaver --test
```

Expected: `--test: rendered one frame, exiting 0` then process exits 0.

- [ ] **Step 7: Delete old src/ directory**

```bash
rm -rf src/ tests/
```

- [ ] **Step 8: Verify everything still builds after deletion**

```bash
cargo build -p matrix-linux --release && cargo test -p matrix-core
```

Expected: all pass.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor: Renderer::new accepts wgpu instance+surface; workspace fully wired; Linux verified"
```

---

## Phase 3: Windows Crate

### Task 6: matrix-windows Cargo.toml and skeleton

**Files:**
- Create: `crates/matrix-windows/Cargo.toml`
- Create: `crates/matrix-windows/src/main.rs`

- [ ] **Step 1: Create crates/matrix-windows/Cargo.toml**

```toml
# crates/matrix-windows/Cargo.toml
[package]
name = "matrix-windows"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "matrix-screensaver"
path = "src/main.rs"

[dependencies]
matrix-core = { path = "../matrix-core" }
wgpu = "0.20"
raw-window-handle = "0.6"
pollster = "0.3"
dirs = "5"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_Graphics_Gdi",
    "Win32_System_LibraryLoader",
    "Win32_UI_WindowsAndMessaging",
    "Win32_UI_Controls",
    "Win32_UI_ColorDialog",
    "Win32_UI_Input_KeyboardAndMouse",
] }
```

- [ ] **Step 2: Create crates/matrix-windows/src/main.rs**

```rust
// crates/matrix-windows/src/main.rs
#![windows_subsystem = "windows"]

mod font;
mod screensaver;
mod preview;
mod config_dialog;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.to_ascii_lowercase());

    match mode.as_deref() {
        Some("/s") | Some("-s") => screensaver::run(),
        Some("/p") => {
            let hwnd = args.get(2)
                .and_then(|s| s.parse::<isize>().ok())
                .unwrap_or(0);
            preview::run(hwnd);
        }
        Some("/c") | Some("-c") | None => config_dialog::run(),
        _ => config_dialog::run(),
    }
}
```

- [ ] **Step 3: Create stub modules so it compiles**

```rust
// crates/matrix-windows/src/font.rs
pub fn load_font(charset: &matrix_core::config::CharsetKind) -> Vec<u8> {
    load_embedded()
}

fn load_embedded() -> Vec<u8> {
    include_bytes!("../assets/JetBrainsMono-Regular.ttf").to_vec()
}
```

```rust
// crates/matrix-windows/src/screensaver.rs
pub fn run() { todo!("screensaver::run") }
```

```rust
// crates/matrix-windows/src/preview.rs
pub fn run(_parent_hwnd: isize) { todo!("preview::run") }
```

```rust
// crates/matrix-windows/src/config_dialog.rs
pub fn run() { todo!("config_dialog::run") }
```

- [ ] **Step 4: Download JetBrains Mono font**

Download `JetBrainsMono-Regular.ttf` from https://github.com/JetBrains/JetBrainsMono/releases (OFL 1.1 license) and save to `crates/matrix-windows/assets/JetBrainsMono-Regular.ttf`.

```bash
# Or via curl if you have internet access:
curl -L "https://github.com/JetBrains/JetBrainsMono/raw/master/fonts/ttf/JetBrainsMono-Regular.ttf" \
     -o crates/matrix-windows/assets/JetBrainsMono-Regular.ttf
```

- [ ] **Step 5: Cross-compile check**

```bash
rustup target add x86_64-pc-windows-gnu
cargo check -p matrix-windows --target x86_64-pc-windows-gnu 2>&1
```

Expected: compiles (stubs). No Windows API errors yet since windows crate features only activate on `cfg(windows)`.

- [ ] **Step 6: Commit**

```bash
git add crates/matrix-windows/
git commit -m "feat(windows): add matrix-windows crate skeleton with stub modules"
```

---

### Task 7: Implement font.rs — system lookup + embedded fallback

**Files:**
- Modify: `crates/matrix-windows/src/font.rs`

- [ ] **Step 1: Implement system font lookup with embedded fallback**

```rust
// crates/matrix-windows/src/font.rs
use matrix_core::config::CharsetKind;

/// Load font bytes for the given charset.
/// Tries system monospace fonts first; falls back to embedded JetBrains Mono.
pub fn load_font(charset: &CharsetKind) -> Vec<u8> {
    // For katakana/mixed: prefer a CJK-capable system font (MS Gothic, Yu Gothic)
    let candidates: &[&str] = match charset {
        CharsetKind::Katakana | CharsetKind::Mixed => &[
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "C:\\Windows\\Fonts\\yugothm.ttf",
            "C:\\Windows\\Fonts\\YuGothM.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\consola.ttf",
        ],
        _ => &[
            "C:\\Windows\\Fonts\\consola.ttf",
            "C:\\Windows\\Fonts\\cour.ttf",
            "C:\\Windows\\Fonts\\lucon.ttf",
        ],
    };

    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return bytes;
        }
    }

    load_embedded()
}

fn load_embedded() -> Vec<u8> {
    include_bytes!("../assets/JetBrainsMono-Regular.ttf").to_vec()
}
```

- [ ] **Step 2: Cross-compile check**

```bash
cargo check -p matrix-windows --target x86_64-pc-windows-gnu 2>&1 | grep -E "error|warning" | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/matrix-windows/src/font.rs
git commit -m "feat(windows): implement font loader with system lookup and embedded fallback"
```

---

### Task 8: Implement screensaver.rs — fullscreen render loop

**Files:**
- Modify: `crates/matrix-windows/src/screensaver.rs`

- [ ] **Step 1: Implement screensaver.rs**

```rust
// crates/matrix-windows/src/screensaver.rs
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use matrix_core::{
    atlas::GlyphAtlas,
    chars::get_charset,
    config::Config,
    rain::RainSimulator,
    renderer::{DepthLayer, Renderer},
};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITORINFO, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
        GetClientRect, GetSystemMetrics, PeekMessageW, PostQuitMessage,
        RegisterClassExW, ShowWindow, TranslateMessage, CS_HREDRAW, CS_VREDRAW,
        MSG, PM_REMOVE, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW,
        WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_RBUTTONDOWN,
        WM_QUIT, WNDCLASSEXW, WS_EX_TOPMOST, WS_POPUP, WS_VISIBLE,
    },
};
use windows::core::PCWSTR;
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::num::NonZeroIsize;

static mut MOUSE_START: Option<(i32, i32)> = None;
const MOUSE_THRESHOLD: i32 = 10;

pub fn run() {
    let config = Config::load();
    let charset = get_charset(&config.rain.charset);
    let font_bytes = crate::font::load_font(&config.rain.charset);
    let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_bytes));
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    let hwnd = create_fullscreen_window();
    let (width, height) = get_window_size(hwnd);

    let (instance, surface) = create_wgpu_surface(hwnd);
    let mut renderer = pollster::block_on(Renderer::new(
        instance, surface, width, height,
        atlas.clone(), &config, None, None,
    ));

    let levels = depth_levels(&config.rain);
    let mut rains = make_rains(width, height, &atlas, &levels, &charset, &config.rain);
    let mut last_frame = Instant::now();

    loop {
        let mut msg = MSG::default();
        // drain all pending messages
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT {
                return;
            }
            unsafe {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let now = Instant::now();
        let delta = now.duration_since(last_frame).as_secs_f32().min(0.1);
        last_frame = now;

        for sim in &mut rains {
            sim.update(delta);
        }

        let depth_layers: Vec<DepthLayer<'_>> = rains
            .iter()
            .zip(levels.iter())
            .map(|(sim, &(scale, brightness_mult))| DepthLayer {
                cells: &sim.cells,
                scale,
                brightness_mult,
            })
            .collect();
        renderer.render(&depth_layers);

        let elapsed = Instant::now().duration_since(now);
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}

fn create_fullscreen_window() -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        let class_name: Vec<u16> = "MatrixSCR\0".encode_utf16().collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);

        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(std::ptr::null()),
            WS_POPUP | WS_VISIBLE,
            0, 0, w, h,
            None, None,
            hinstance,
            None,
        ).unwrap();

        ShowWindow(hwnd, SW_SHOW);
        hwnd
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_KEYDOWN | WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            DestroyWindow(hwnd).ok();
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lparam.0 & 0xffff) as i16 as i32;
            let y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
            match MOUSE_START {
                None => MOUSE_START = Some((x, y)),
                Some((sx, sy)) => {
                    if (x - sx).abs() > MOUSE_THRESHOLD || (y - sy).abs() > MOUSE_THRESHOLD {
                        DestroyWindow(hwnd).ok();
                    }
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn create_wgpu_surface(hwnd: HWND) -> (wgpu::Instance, wgpu::Surface<'static>) {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        ..Default::default()
    });

    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            raw_window_handle: RawWindowHandle::Win32(
                Win32WindowHandle::new(NonZeroIsize::new(hwnd.0 as isize).unwrap()),
            ),
        })
    }
    .expect("wgpu surface creation failed");

    (instance, surface)
}

fn get_window_size(hwnd: HWND) -> (u32, u32) {
    let mut rect = windows::Win32::Foundation::RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect).ok() };
    ((rect.right - rect.left) as u32, (rect.bottom - rect.top) as u32)
}

fn depth_levels(config: &matrix_core::config::RainConfig) -> Vec<(f32, f32)> {
    let n = (config.depth_levels as usize).max(1);
    (0..n).map(|i| {
        let t = if n == 1 { 1.0 } else { i as f32 / (n - 1) as f32 };
        let scale = config.depth_scale_min + (1.0 - config.depth_scale_min) * t;
        let bri = config.depth_brightness_min + (1.0 - config.depth_brightness_min) * t;
        (scale, bri)
    }).collect()
}

fn make_rains(
    w: u32, h: u32,
    atlas: &GlyphAtlas,
    levels: &[(f32, f32)],
    charset: &[char],
    config: &matrix_core::config::RainConfig,
) -> Vec<RainSimulator> {
    let cw = atlas.cell_width as f32;
    let ch = atlas.cell_height as f32;
    levels.iter().map(|&(scale, _)| {
        let cols = ((w as f32 / (cw * scale)) as usize).max(1);
        let rows = ((h as f32 / (ch * scale)) as usize).max(1);
        RainSimulator::new(cols, rows, charset.to_vec(), config)
    }).collect()
}
```

- [ ] **Step 2: Cross-compile check**

```bash
cargo check -p matrix-windows --target x86_64-pc-windows-gnu 2>&1 | grep "^error" | head -20
```

Expected: no errors (or only minor ones to fix).

- [ ] **Step 3: Commit**

```bash
git add crates/matrix-windows/src/screensaver.rs
git commit -m "feat(windows): implement fullscreen screensaver with wgpu DX12/Vulkan surface"
```

---

### Task 9: Implement preview.rs — child HWND rendering

**Files:**
- Modify: `crates/matrix-windows/src/preview.rs`

- [ ] **Step 1: Implement preview.rs**

```rust
// crates/matrix-windows/src/preview.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use matrix_core::{
    atlas::GlyphAtlas, chars::get_charset, config::Config,
    rain::RainSimulator, renderer::{DepthLayer, Renderer},
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::GetClientRect,
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, PeekMessageW,
        RegisterClassExW, ShowWindow, TranslateMessage,
        CS_HREDRAW, CS_VREDRAW, MSG, PM_REMOVE, SW_SHOW,
        WM_QUIT, WNDCLASSEXW, WS_CHILD, WS_VISIBLE,
        HWND_MESSAGE, LPARAM, LRESULT, WPARAM,
    },
};
use windows::core::PCWSTR;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use std::num::NonZeroIsize;

pub fn run(parent_hwnd_raw: isize) {
    if parent_hwnd_raw == 0 { return; }
    let parent = HWND(parent_hwnd_raw as _);

    let config = Config::load();
    let charset = get_charset(&config.rain.charset);
    let font_bytes = crate::font::load_font(&config.rain.charset);
    let atlas = Arc::new(GlyphAtlas::build(&charset, config.display.font_size, &font_bytes));
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    // Get parent size
    let mut rect = RECT::default();
    unsafe { windows::Win32::Graphics::Gdi::GetClientRect(parent, &mut rect).ok() };
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;

    // Create child window inside parent
    let hwnd = create_child_window(parent, width, height);

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::DX12 | wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            raw_window_handle: RawWindowHandle::Win32(
                Win32WindowHandle::new(NonZeroIsize::new(hwnd.0 as isize).unwrap()),
            ),
        })
    }.expect("preview surface failed");

    let mut renderer = pollster::block_on(Renderer::new(
        instance, surface, width, height, atlas.clone(), &config, None, None,
    ));

    let levels: Vec<(f32, f32)> = {
        let n = (config.rain.depth_levels as usize).max(1);
        (0..n).map(|i| {
            let t = if n == 1 { 1.0 } else { i as f32 / (n - 1) as f32 };
            (config.rain.depth_scale_min + (1.0 - config.rain.depth_scale_min) * t,
             config.rain.depth_brightness_min + (1.0 - config.rain.depth_brightness_min) * t)
        }).collect()
    };
    let cw = atlas.cell_width as f32;
    let ch = atlas.cell_height as f32;
    let mut rains: Vec<RainSimulator> = levels.iter().map(|&(scale, _)| {
        let cols = ((width as f32 / (cw * scale)) as usize).max(1);
        let rows = ((height as f32 / (ch * scale)) as usize).max(1);
        RainSimulator::new(cols, rows, charset.to_vec(), &config.rain)
    }).collect();

    let mut last_frame = Instant::now();
    loop {
        let mut msg = MSG::default();
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            if msg.message == WM_QUIT { return; }
            unsafe { TranslateMessage(&msg); DispatchMessageW(&msg); }
        }
        let now = Instant::now();
        let delta = now.duration_since(last_frame).as_secs_f32().min(0.1);
        last_frame = now;
        for sim in &mut rains { sim.update(delta); }
        let depth_layers: Vec<DepthLayer<'_>> = rains.iter().zip(levels.iter())
            .map(|(sim, &(scale, brightness_mult))| DepthLayer { cells: &sim.cells, scale, brightness_mult })
            .collect();
        renderer.render(&depth_layers);
        let elapsed = Instant::now().duration_since(now);
        if elapsed < frame_duration { std::thread::sleep(frame_duration - elapsed); }
    }
}

fn create_child_window(parent: HWND, w: u32, h: u32) -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        let class_name: Vec<u16> = "MatrixPreview\0".encode_utf16().collect();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(preview_wnd_proc),
            hInstance: hinstance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(std::ptr::null()),
            WS_CHILD | WS_VISIBLE,
            0, 0, w as i32, h as i32,
            parent, None, hinstance, None,
        ).unwrap();
        ShowWindow(hwnd, SW_SHOW);
        hwnd
    }
}

unsafe extern "system" fn preview_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
```

- [ ] **Step 2: Cross-compile check**

```bash
cargo check -p matrix-windows --target x86_64-pc-windows-gnu 2>&1 | grep "^error" | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add crates/matrix-windows/src/preview.rs
git commit -m "feat(windows): implement preview mode (child HWND rendering)"
```

---

### Task 10: Implement config_dialog.rs — native Win32 settings dialog

**Files:**
- Modify: `crates/matrix-windows/src/config_dialog.rs`

- [ ] **Step 1: Implement config_dialog.rs**

```rust
// crates/matrix-windows/src/config_dialog.rs
//
// Creates a modal dialog window programmatically (no .rc file).
// Controls: speed trackbar, density trackbar, fps combobox,
//           charset combobox, primary color button, glow checkbox.
// On OK: saves config to %APPDATA%/matrix-screensaver/config.toml.

use matrix_core::config::{CharsetKind, Config, ColorsConfig, DisplayConfig, RainConfig};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{GetStockObject, SetBkColor, DEFAULT_GUI_FONT, WHITE_BRUSH},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        ColorDialog::{ChooseColorW, CHOOSECOLORW, CC_FULLOPEN, CC_RGBINIT},
        Controls::{
            SendMessageW, TBM_GETPOS, TBM_SETPOS, TBM_SETRANGE, TBM_SETTICFREQ,
            TBSTYLE_AUTOTICKS, TBSTYLE_HORZ, WM_USER, TBS_TOOLTIPS,
        },
        WindowsAndMessaging::{
            CB_ADDSTRING, CB_GETCURSEL, CB_SETCURSEL, CreateWindowExW, DefWindowProcW,
            DestroyWindow, DispatchMessageW, EndDialog, GetDlgItem, GetMessageW,
            MessageBoxW, RegisterClassExW, SendDlgItemMessageW, SetDlgItemTextW,
            ShowWindow, TranslateMessage, BM_GETCHECK, BM_SETCHECK, BST_CHECKED,
            BST_UNCHECKED, BS_AUTOCHECKBOX, CB_GETCURSEL as CBS_GETCURSEL,
            HWND_DESKTOP, IDCANCEL, IDOK, MB_OK, MB_ICONERROR,
            SW_SHOW, WM_COMMAND, WM_DESTROY, WM_INITDIALOG, WS_BORDER,
            WS_CHILD, WS_CLIPSIBLINGS, WS_TABSTOP, WS_VISIBLE, WNDCLASSEXW,
            CS_HREDRAW, CS_VREDRAW, WS_CAPTION, WS_SYSMENU, WS_OVERLAPPED,
            MSG, LPARAM as _LPARAM,
        },
    },
};
use windows::core::PCWSTR;

const IDC_SPEED: i32      = 1001;
const IDC_DENSITY: i32    = 1002;
const IDC_FPS: i32        = 1003;
const IDC_CHARSET: i32    = 1004;
const IDC_COLOR: i32      = 1005;
const IDC_GLOW: i32       = 1006;
const IDC_OK: i32         = IDOK.0 as i32;
const IDC_CANCEL: i32     = IDCANCEL.0 as i32;

static mut CUSTOM_COLORS: [u32; 16] = [0u32; 16];

struct DialogState {
    config: Config,
    chosen_color: u32, // COLORREF
}

pub fn run() {
    let config = Config::load();
    let chosen_color = color_to_colorref(&config.colors.primary);

    // Create a regular overlapped window (dialog substitute without .rc)
    let hwnd = create_dialog_window();
    
    unsafe {
        // Load current config into controls
        init_controls(hwnd, &config, chosen_color);
        ShowWindow(hwnd, SW_SHOW);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn create_dialog_window() -> HWND {
    unsafe {
        let hinstance = GetModuleHandleW(None).unwrap();
        let class_name: Vec<u16> = "MatrixConfig\0".encode_utf16().collect();
        let title: Vec<u16> = "Matrix Screensaver Settings\0".encode_utf16().collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(dialog_wnd_proc),
            hInstance: hinstance.into(),
            hbrBackground: GetStockObject(WHITE_BRUSH).into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        CreateWindowExW(
            Default::default(),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
            100, 100, 400, 380,
            None, None, hinstance, None,
        ).unwrap()
    }
}

unsafe fn init_controls(hwnd: HWND, config: &Config, chosen_color: u32) {
    let hinstance = GetModuleHandleW(None).unwrap();

    macro_rules! label {
        ($x:expr, $y:expr, $text:literal) => {{
            let t: Vec<u16> = concat!($text, "\0").encode_utf16().collect();
            let s: Vec<u16> = "STATIC\0".encode_utf16().collect();
            CreateWindowExW(Default::default(), PCWSTR(s.as_ptr()), PCWSTR(t.as_ptr()),
                WS_CHILD | WS_VISIBLE, $x, $y, 80, 20, hwnd, None, hinstance, None).ok();
        }};
    }

    // Speed trackbar (0–200, representing 0.0–2.0)
    label!(10, 20, "Speed:");
    let s: Vec<u16> = "msctls_trackbar32\0".encode_utf16().collect();
    let tb_speed = CreateWindowExW(Default::default(), PCWSTR(s.as_ptr()), PCWSTR(std::ptr::null()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | TBSTYLE_HORZ | TBSTYLE_AUTOTICKS,
        100, 18, 240, 30, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_SPEED as _),
        hinstance, None).unwrap();
    SendMessageW(tb_speed, TBM_SETRANGE as u32, WPARAM(1), LPARAM(((0u16 as u32) | ((200u16 as u32) << 16)) as isize));
    SendMessageW(tb_speed, TBM_SETPOS as u32, WPARAM(1), LPARAM((config.rain.speed * 100.0) as isize));

    // Density trackbar (0–100, representing 0.0–1.0)
    label!(10, 60, "Density:");
    let tb_density = CreateWindowExW(Default::default(), PCWSTR(s.as_ptr()), PCWSTR(std::ptr::null()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | TBSTYLE_HORZ | TBSTYLE_AUTOTICKS,
        100, 58, 240, 30, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_DENSITY as _),
        hinstance, None).unwrap();
    SendMessageW(tb_density, TBM_SETRANGE as u32, WPARAM(1), LPARAM(((0u16 as u32) | ((100u16 as u32) << 16)) as isize));
    SendMessageW(tb_density, TBM_SETPOS as u32, WPARAM(1), LPARAM((config.rain.density * 100.0) as isize));

    // FPS combobox
    label!(10, 100, "FPS:");
    let c: Vec<u16> = "COMBOBOX\0".encode_utf16().collect();
    let cb_fps = CreateWindowExW(Default::default(), PCWSTR(c.as_ptr()), PCWSTR(std::ptr::null()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | windows::Win32::UI::WindowsAndMessaging::CBS_DROPDOWNLIST,
        100, 100, 120, 100, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_FPS as _),
        hinstance, None).unwrap();
    for fps_str in &["30\0", "60\0", "120\0"] {
        let ws: Vec<u16> = fps_str.encode_utf16().collect();
        SendMessageW(cb_fps, CB_ADDSTRING as u32, WPARAM(0), LPARAM(ws.as_ptr() as isize));
    }
    let fps_idx = match config.display.fps { 30 => 0, 120 => 2, _ => 1 };
    SendMessageW(cb_fps, CB_SETCURSEL as u32, WPARAM(fps_idx), LPARAM(0));

    // Charset combobox
    label!(10, 140, "Charset:");
    let cb_charset = CreateWindowExW(Default::default(), PCWSTR(c.as_ptr()), PCWSTR(std::ptr::null()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | windows::Win32::UI::WindowsAndMessaging::CBS_DROPDOWNLIST,
        100, 140, 120, 100, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_CHARSET as _),
        hinstance, None).unwrap();
    for cs in &["Mixed\0", "Katakana\0", "Latin\0", "Binary\0"] {
        let ws: Vec<u16> = cs.encode_utf16().collect();
        SendMessageW(cb_charset, CB_ADDSTRING as u32, WPARAM(0), LPARAM(ws.as_ptr() as isize));
    }
    let cs_idx = match config.rain.charset { CharsetKind::Mixed => 0, CharsetKind::Katakana => 1, CharsetKind::Latin => 2, CharsetKind::Binary => 3 };
    SendMessageW(cb_charset, CB_SETCURSEL as u32, WPARAM(cs_idx), LPARAM(0));

    // Color button
    label!(10, 180, "Color:");
    let b: Vec<u16> = "BUTTON\0".encode_utf16().collect();
    let color_label: Vec<u16> = "Pick Color...\0".encode_utf16().collect();
    CreateWindowExW(Default::default(), PCWSTR(b.as_ptr()), PCWSTR(color_label.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        100, 178, 120, 26, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_COLOR as _),
        hinstance, None).ok();

    // Glow checkbox
    let glow_label: Vec<u16> = "Enable glow effect\0".encode_utf16().collect();
    let cb_glow = CreateWindowExW(Default::default(), PCWSTR(b.as_ptr()), PCWSTR(glow_label.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX,
        100, 220, 200, 24, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_GLOW as _),
        hinstance, None).unwrap();
    let glow_check = if config.colors.glow { BST_CHECKED } else { BST_UNCHECKED };
    SendMessageW(cb_glow, BM_SETCHECK as u32, WPARAM(glow_check.0 as usize), LPARAM(0));

    // OK / Cancel buttons
    let ok_label: Vec<u16> = "OK\0".encode_utf16().collect();
    CreateWindowExW(Default::default(), PCWSTR(b.as_ptr()), PCWSTR(ok_label.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        100, 300, 80, 30, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_OK as _),
        hinstance, None).ok();
    let cancel_label: Vec<u16> = "Cancel\0".encode_utf16().collect();
    CreateWindowExW(Default::default(), PCWSTR(b.as_ptr()), PCWSTR(cancel_label.as_ptr()),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP,
        200, 300, 80, 30, hwnd,
        windows::Win32::UI::WindowsAndMessaging::HMENU(IDC_CANCEL as _),
        hinstance, None).ok();
}

unsafe extern "system" fn dialog_wnd_proc(
    hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            windows::Win32::UI::WindowsAndMessaging::PostQuitMessage(0);
            LRESULT(0)
        }
        WM_COMMAND => {
            let ctrl_id = (wparam.0 & 0xffff) as i32;
            match ctrl_id {
                IDC_OK => {
                    if let Some(config) = read_controls(hwnd) {
                        save_config(&config);
                    }
                    DestroyWindow(hwnd).ok();
                    LRESULT(0)
                }
                IDC_CANCEL => {
                    DestroyWindow(hwnd).ok();
                    LRESULT(0)
                }
                IDC_COLOR => {
                    show_color_picker(hwnd);
                    LRESULT(0)
                }
                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn read_controls(hwnd: HWND) -> Option<Config> {
    let get_hwnd = |id: i32| GetDlgItem(hwnd, id).ok()?;

    let speed_pos = SendMessageW(get_hwnd(IDC_SPEED)?, TBM_GETPOS as u32, WPARAM(0), LPARAM(0)).0;
    let density_pos = SendMessageW(get_hwnd(IDC_DENSITY)?, TBM_GETPOS as u32, WPARAM(0), LPARAM(0)).0;
    let fps_sel = SendMessageW(get_hwnd(IDC_FPS)?, CB_GETCURSEL as u32, WPARAM(0), LPARAM(0)).0;
    let cs_sel = SendMessageW(get_hwnd(IDC_CHARSET)?, CB_GETCURSEL as u32, WPARAM(0), LPARAM(0)).0;
    let glow_check = SendMessageW(get_hwnd(IDC_GLOW)?, BM_GETCHECK as u32, WPARAM(0), LPARAM(0)).0;

    let mut config = Config::load();
    config.rain.speed = speed_pos as f32 / 100.0;
    config.rain.density = density_pos as f32 / 100.0;
    config.display.fps = match fps_sel { 0 => 30, 2 => 120, _ => 60 };
    config.rain.charset = match cs_sel { 1 => CharsetKind::Katakana, 2 => CharsetKind::Latin, 3 => CharsetKind::Binary, _ => CharsetKind::Mixed };
    config.colors.glow = glow_check == BST_CHECKED.0 as isize;
    // Color is updated globally in show_color_picker via CUSTOM_COLORS[0]
    config.colors.primary = colorref_to_color(CUSTOM_COLORS[0]);
    Some(config)
}

unsafe fn show_color_picker(hwnd: HWND) {
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: hwnd,
        rgbResult: CUSTOM_COLORS[0],
        lpCustColors: CUSTOM_COLORS.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };
    if ChooseColorW(&mut cc).as_bool() {
        CUSTOM_COLORS[0] = cc.rgbResult;
    }
}

fn save_config(config: &Config) {
    let dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("matrix-screensaver");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("config.toml");
    // Serialize manually (toml doesn't impl Serialize by default here; use toml::to_string)
    // Config derives Deserialize but not Serialize — add Serialize derive in matrix-core first.
    // For now write key fields directly:
    let content = format!(
        "[display]\nfps = {}\n\n[rain]\nspeed = {:.2}\ndensity = {:.3}\ncharset = \"{}\"\n\n[colors]\nprimary = \"{}\"\nglow = {}\n",
        config.display.fps,
        config.rain.speed,
        config.rain.density,
        charset_str(&config.rain.charset),
        config.colors.primary,
        config.colors.glow,
    );
    std::fs::write(path, content).ok();
}

fn charset_str(c: &CharsetKind) -> &'static str {
    match c { CharsetKind::Mixed => "mixed", CharsetKind::Katakana => "katakana", CharsetKind::Latin => "latin", CharsetKind::Binary => "binary" }
}

fn color_to_colorref(hex: &str) -> u32 {
    let [r, g, b, _] = matrix_core::config::Config::parse_color(hex);
    ((r * 255.0) as u32) | (((g * 255.0) as u32) << 8) | (((b * 255.0) as u32) << 16)
}

fn colorref_to_color(cr: u32) -> String {
    format!("#{:02x}{:02x}{:02x}", cr & 0xff, (cr >> 8) & 0xff, (cr >> 16) & 0xff)
}
```

- [ ] **Step 2: Cross-compile check**

```bash
cargo check -p matrix-windows --target x86_64-pc-windows-gnu 2>&1 | grep "^error" | head -30
```

Expected: resolve any remaining type errors. Common ones to fix:
- `CBS_DROPDOWNLIST` may be `CBS_DROPDOWNLIST` from `Win32_UI_WindowsAndMessaging` — adjust imports.
- `HMENU` constructor — use `windows::Win32::UI::WindowsAndMessaging::HMENU(id as isize)`.

- [ ] **Step 3: Commit**

```bash
git add crates/matrix-windows/src/config_dialog.rs
git commit -m "feat(windows): implement native Win32 config dialog"
```

---

## Phase 4: Polish

### Task 11: Final cleanup and build verification

**Files:**
- Modify: `install.sh` (ensure it builds `matrix-linux`, not old package)
- Modify: `crates/matrix-windows/Cargo.toml` (set binary output name for .scr)

- [ ] **Step 1: Update install.sh to reference the workspace crate**

Open `install.sh`. Find the `cargo build` line and update:
```bash
# OLD
cargo build --release

# NEW
cargo build -p matrix-linux --release
```

The binary path also changes from `target/release/matrix-screensaver` — this stays the same (workspace bins still go to `target/release/`).

- [ ] **Step 2: Verify Linux install still works**

```bash
bash install.sh
matrix-screensaver --test
```

Expected: exits 0 after rendering one frame.

- [ ] **Step 3: Full cross-compile of Windows target**

```bash
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release 2>&1
```

Expected: `target/x86_64-pc-windows-gnu/release/matrix-screensaver.exe`

- [ ] **Step 4: Create Windows install instructions**

Create `INSTALL-WINDOWS.md`:

```markdown
# Installing on Windows

## Requirements
- Windows 10 or later
- DirectX 12 capable GPU (DX12 preferred; falls back to Vulkan)

## Install

1. Copy `matrix-screensaver.exe` to `C:\Windows\System32\matrix-screensaver.scr`
   (rename from `.exe` to `.scr`)

2. Right-click the `.scr` file → **Install**
   — or —
   Open **Screen Saver Settings** (right-click desktop → Personalize → Lock screen → Screen saver),
   select **Matrix Screensaver** from the dropdown.

3. Click **Settings** to configure speed, density, charset, color, and glow.

## Config file

Settings are saved to `%APPDATA%\matrix-screensaver\config.toml`.
Same format as the Linux config — you can copy settings between platforms.

## Build from source (cross-compile from Linux)

```bash
rustup target add x86_64-pc-windows-gnu
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release
# Rename output:
cp target/x86_64-pc-windows-gnu/release/matrix-screensaver.exe matrix-screensaver.scr
```
```

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p matrix-core 2>&1
```

Expected: all tests pass.

- [ ] **Step 6: Final commit**

```bash
git add install.sh INSTALL-WINDOWS.md
git commit -m "chore: update install.sh for workspace; add Windows install guide"
```

---

## Verification Checklist

```bash
# Linux — no regression
cargo build -p matrix-linux --release
cargo test -p matrix-core
~/.local/bin/matrix-screensaver --test    # exits 0

# Cross-compile Windows
cargo build -p matrix-windows --target x86_64-pc-windows-gnu --release

# Windows runtime (on Windows VM)
matrix-screensaver.scr /c    # config dialog appears, settings persist
matrix-screensaver.scr /s    # fullscreen rain, exits on keypress
matrix-screensaver.scr /p 0  # preview renders
```
