# Matrix Screensaver Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a GPU-accelerated Matrix rain screensaver for KDE Plasma 6 on Wayland, written in Rust, with automated installer.

**Architecture:** Rust binary uses smithay-client-toolkit for Wayland (wlr-layer-shell overlay + ext-idle-notify-v1 idle detection), wgpu for GPU rendering, fontdue for glyph rasterization into a texture atlas. Binary runs as a KDE autostart daemon; activates fullscreen overlay on idle. Thin KDE KPackage plugin registers it in System Settings. `install.sh` automates build + install.

**Tech Stack:** Rust 2021, wgpu 0.20, smithay-client-toolkit 0.18, wayland-protocols 0.32, wayland-protocols-wlr 0.2, calloop 0.13, fontdue 0.8, serde+toml, rand, bytemuck

---

## File Map

```
src/
  main.rs           - CLI args, event loop bootstrap, ties all modules
  config.rs         - Config struct, TOML loading, color hex parsing
  chars.rs          - Charset generation per CharsetKind enum
  rain.rs           - Drop, CellState, RainSimulator (CPU sim)
  atlas.rs          - Glyph rasterization + GPU texture atlas
  renderer.rs       - wgpu device/queue/surface, pipelines, per-frame draw
  wayland_app.rs    - AppState, SCTK delegates, layer shell, idle monitor
  shaders/
    rain.wgsl       - Instanced character quad rendering
    blur.wgsl       - 1D Gaussian blur (two-pass glow)
kde-plugin/
  matrix-screensaver/
    metadata.json
    contents/ui/main.qml
config/
  default.toml
tests/
  config_test.rs
  rain_test.rs
install.sh
```

---

## Task 1: Project Scaffold

**Files:**
- Modify: `Cargo.toml`
- Create: stub `src/config.rs`, `src/chars.rs`, `src/rain.rs`, `src/atlas.rs`, `src/renderer.rs`, `src/wayland_app.rs`
- Create: `src/shaders/rain.wgsl`, `src/shaders/blur.wgsl`

- [ ] **Step 1: Fix and replace `Cargo.toml`**

```toml
[package]
name = "matrix-screensaver"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "matrix-screensaver"
path = "src/main.rs"

[dependencies]
# Wayland
smithay-client-toolkit = { version = "0.18", features = ["calloop"] }
wayland-client = "0.31"
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
wayland-protocols-wlr = { version = "0.2", features = ["client"] }
calloop = "0.13"
calloop-wayland-source = "0.3"

# GPU
wgpu = "0.20"
bytemuck = { version = "1", features = ["derive"] }
raw-window-handle = "0.6"
pollster = "0.3"

# Font
fontdue = "0.8"

# Config
serde = { version = "1", features = ["derive"] }
toml = "1"
dirs = "5"

# Util
rand = { version = "0.8", features = ["small_rng"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create module stub files**

```rust
// src/config.rs
pub struct Config;
```

```rust
// src/chars.rs
pub fn placeholder() {}
```

```rust
// src/rain.rs
pub struct RainSimulator;
```

```rust
// src/atlas.rs
pub struct GlyphAtlas;
```

```rust
// src/renderer.rs
pub struct Renderer;
```

```rust
// src/wayland_app.rs
pub struct AppState;
```

```rust
// src/shaders/rain.wgsl
// placeholder
```

```rust
// src/shaders/blur.wgsl
// placeholder
```

- [ ] **Step 3: Replace `src/main.rs` with module declarations**

```rust
mod config;
mod chars;
mod rain;
mod atlas;
mod renderer;
mod wayland_app;

fn main() {
    println!("matrix-screensaver starting");
}
```

- [ ] **Step 4: Verify project compiles**

```
cargo check
```

Expected: compiles with possible warnings, no errors.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/
git commit -m "chore: scaffold project with all module stubs"
```

---

## Task 2: Config Module

**Files:**
- Modify: `src/config.rs`
- Create: `tests/config_test.rs`, `config/default.toml`

- [ ] **Step 1: Write failing config tests**

Create `tests/config_test.rs`:

```rust
use matrix_screensaver::config::{Config, CharsetKind};

#[test]
fn default_config_has_expected_values() {
    let cfg = Config::default();
    assert_eq!(cfg.display.fps, 60);
    assert_eq!(cfg.display.font_size, 18.0);
    assert_eq!(cfg.rain.charset, CharsetKind::Mixed);
    assert!((cfg.rain.speed - 1.0).abs() < f32::EPSILON);
    assert!(cfg.colors.glow);
    assert_eq!(cfg.idle.timeout_seconds, 120);
}

#[test]
fn parse_color_green() {
    let [r, g, b, a] = Config::parse_color("#00ff41");
    assert_eq!(r, 0.0);
    assert!((g - 1.0).abs() < 0.01);
    assert!((b - 0.255).abs() < 0.01);
    assert_eq!(a, 1.0);
}

#[test]
fn load_from_toml_string() {
    let toml = r#"
        [display]
        fps = 30
        font_size = 24.0
        font = "JetBrains Mono"

        [rain]
        speed = 2.0
        charset = "katakana"

        [colors]
        primary = "#ff0000"
        glow = false

        [idle]
        timeout_seconds = 60
    "#;
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.display.fps, 30);
    assert_eq!(cfg.rain.charset, CharsetKind::Katakana);
    assert!(!cfg.colors.glow);
    assert_eq!(cfg.idle.timeout_seconds, 60);
}

#[test]
fn partial_toml_uses_defaults() {
    let toml = "[display]\nfps = 30";
    let cfg: Config = toml::from_str(toml).unwrap();
    assert_eq!(cfg.display.fps, 30);
    // Unspecified fields use defaults
    assert_eq!(cfg.rain.charset, CharsetKind::Mixed);
    assert!(cfg.colors.glow);
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test --test config_test 2>&1 | head -20
```

Expected: compile error — `Config` not a struct with fields.

- [ ] **Step 3: Implement `src/config.rs`**

```rust
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CharsetKind {
    Mixed,
    Katakana,
    Latin,
    Binary,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub font: String,
    pub font_size: f32,
    pub fps: u32,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        Self { font: "monospace".into(), font_size: 18.0, fps: 60 }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct RainConfig {
    pub speed: f32,
    pub density: f32,
    pub charset: CharsetKind,
    pub drop_length_min: usize,
    pub drop_length_max: usize,
}

impl Default for RainConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            density: 0.05,
            charset: CharsetKind::Mixed,
            drop_length_min: 5,
            drop_length_max: 25,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ColorsConfig {
    pub primary: String,
    pub background: String,
    pub glow: bool,
    pub glow_intensity: f32,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        Self {
            primary: "#00ff41".into(),
            background: "#000000".into(),
            glow: true,
            glow_intensity: 0.8,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IdleConfig {
    pub timeout_seconds: u64,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self { timeout_seconds: 120 }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub display: DisplayConfig,
    pub rain: RainConfig,
    pub colors: ColorsConfig,
    pub idle: IdleConfig,
}

impl Config {
    pub fn load() -> Self {
        let path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("matrix-screensaver/config.toml");

        if let Ok(content) = std::fs::read_to_string(&path) {
            toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!("config parse error: {e}, using defaults");
                Config::default()
            })
        } else {
            Config::default()
        }
    }

    /// Parse "#rrggbb" hex color → [r, g, b, 1.0] normalized floats.
    pub fn parse_color(hex: &str) -> [f32; 4] {
        let hex = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0) as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0) as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0) as f32 / 255.0;
        [r, g, b, 1.0]
    }
}
```

Also expose from `src/main.rs`:
```rust
pub mod config;
pub mod chars;
pub mod rain;
pub mod atlas;
pub mod renderer;
pub mod wayland_app;
```

- [ ] **Step 4: Create `config/default.toml`**

```toml
[display]
font = "monospace"
font_size = 18
fps = 60

[rain]
speed = 1.0
density = 0.05
charset = "mixed"
drop_length_min = 5
drop_length_max = 25

[colors]
primary = "#00ff41"
background = "#000000"
glow = true
glow_intensity = 0.8

[idle]
timeout_seconds = 120
```

- [ ] **Step 5: Run tests to confirm passing**

```
cargo test --test config_test
```

Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add src/config.rs tests/config_test.rs config/default.toml src/main.rs
git commit -m "feat: add config module with TOML loading and color parsing"
```

---

## Task 3: Character Sets

**Files:**
- Modify: `src/chars.rs`
- Add tests to `tests/rain_test.rs`

- [ ] **Step 1: Write failing charset tests**

Create `tests/rain_test.rs`:

```rust
use matrix_screensaver::{chars::get_charset, config::CharsetKind};

#[test]
fn katakana_charset_has_correct_range() {
    let chars = get_charset(&CharsetKind::Katakana);
    assert!(!chars.is_empty());
    // Half-width katakana: U+FF66–U+FF9D
    assert!(chars.contains(&'ｦ')); // U+FF66
    assert!(chars.contains(&'ﾝ')); // U+FF9D
    // No latin
    assert!(!chars.contains(&'A'));
}

#[test]
fn latin_charset_has_az_and_digits() {
    let chars = get_charset(&CharsetKind::Latin);
    assert!(chars.contains(&'A'));
    assert!(chars.contains(&'z'));
    assert!(chars.contains(&'0'));
    assert!(chars.contains(&'9'));
    assert!(!chars.contains(&'ｦ'));
}

#[test]
fn binary_charset_only_zero_one() {
    let chars = get_charset(&CharsetKind::Binary);
    assert_eq!(chars, vec!['0', '1']);
}

#[test]
fn mixed_charset_contains_all() {
    let chars = get_charset(&CharsetKind::Mixed);
    assert!(chars.contains(&'ｦ'));
    assert!(chars.contains(&'A'));
    assert!(chars.contains(&'0'));
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test --test rain_test charset 2>&1 | head -10
```

Expected: compile error.

- [ ] **Step 3: Implement `src/chars.rs`**

```rust
use crate::config::CharsetKind;

/// Returns the set of characters for the given charset kind.
pub fn get_charset(kind: &CharsetKind) -> Vec<char> {
    // Half-width katakana: U+FF66–U+FF9D (56 characters)
    let katakana: Vec<char> = (0xFF66u32..=0xFF9Du32)
        .filter_map(char::from_u32)
        .collect();

    let uppercase: Vec<char> = (b'A'..=b'Z').map(|b| b as char).collect();
    let lowercase: Vec<char> = (b'a'..=b'z').map(|b| b as char).collect();
    let digits: Vec<char> = (b'0'..=b'9').map(|b| b as char).collect();

    match kind {
        CharsetKind::Katakana => katakana,
        CharsetKind::Latin => {
            uppercase.into_iter().chain(lowercase).chain(digits).collect()
        }
        CharsetKind::Binary => vec!['0', '1'],
        CharsetKind::Mixed => katakana
            .into_iter()
            .chain(uppercase)
            .chain(digits)
            .collect(),
    }
}
```

- [ ] **Step 4: Run charset tests**

```
cargo test --test rain_test charset
```

Expected: all 4 charset tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/chars.rs tests/rain_test.rs
git commit -m "feat: add charset generation for katakana/latin/binary/mixed"
```

---

## Task 4: Rain Simulator

**Files:**
- Modify: `src/rain.rs`, `tests/rain_test.rs`

- [ ] **Step 1: Add rain simulator tests to `tests/rain_test.rs`**

```rust
use matrix_screensaver::{
    config::{CharsetKind, RainConfig},
    rain::RainSimulator,
};

fn default_rain_config() -> RainConfig {
    RainConfig {
        speed: 1.0,
        density: 1.0, // max density for tests
        charset: CharsetKind::Latin,
        drop_length_min: 3,
        drop_length_max: 5,
    }
}

#[test]
fn simulator_initialises_empty() {
    let sim = RainSimulator::new(10, 20, vec!['A', 'B'], &default_rain_config());
    let all_dark = sim.cells.iter().flatten().all(|c| c.brightness == 0.0);
    assert!(all_dark);
}

#[test]
fn update_spawns_drops_at_max_density() {
    let mut sim = RainSimulator::new(10, 20, vec!['A', 'B', 'C'], &default_rain_config());
    // Run enough frames that at least some drops appear
    for _ in 0..30 {
        sim.update(1.0 / 60.0);
    }
    let any_lit = sim.cells.iter().flatten().any(|c| c.brightness > 0.0);
    assert!(any_lit, "no cells lit after 30 frames at max density");
}

#[test]
fn head_cell_has_maximum_brightness() {
    let cfg = RainConfig {
        density: 1.0,
        drop_length_min: 5,
        drop_length_max: 5,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(5, 40, vec!['X'], &cfg);
    // Advance until drops are on screen
    for _ in 0..120 {
        sim.update(1.0 / 60.0);
    }
    let has_head = sim.cells.iter().flatten().any(|c| c.is_head);
    assert!(has_head, "no head cell found after 120 frames");
    // Head brightness must equal 1.0
    let head_cells: Vec<_> = sim.cells.iter().flatten().filter(|c| c.is_head).collect();
    for cell in head_cells {
        assert!((cell.brightness - 1.0).abs() < f32::EPSILON);
    }
}

#[test]
fn brightness_decreases_from_head_to_tail() {
    let cfg = RainConfig {
        density: 1.0,
        speed: 0.5,
        drop_length_min: 8,
        drop_length_max: 8,
        ..default_rain_config()
    };
    let mut sim = RainSimulator::new(1, 40, vec!['A'], &cfg);
    for _ in 0..300 {
        sim.update(1.0 / 60.0);
    }
    // Find a column with a head and measure brightness descent
    let col: Vec<f32> = (0..sim.rows).map(|r| sim.cells[r][0].brightness).collect();
    let head_row = col.iter().position(|&b| b == 1.0);
    if let Some(head) = head_row {
        if head + 3 < sim.rows {
            assert!(col[head] > col[head + 1]);
            assert!(col[head + 1] >= col[head + 2]);
        }
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```
cargo test --test rain_test simulator 2>&1 | head -15
```

- [ ] **Step 3: Implement `src/rain.rs`**

```rust
use rand::{rngs::SmallRng, Rng, SeedableRng};
use crate::config::RainConfig;

#[derive(Clone)]
pub struct CellState {
    pub ch: char,
    pub brightness: f32,
    pub is_head: bool,
}

impl Default for CellState {
    fn default() -> Self {
        Self { ch: ' ', brightness: 0.0, is_head: false }
    }
}

struct Drop {
    column: usize,
    head_row: f32,
    length: usize,
    speed: f32,       // rows per second
    chars: Vec<char>,
    mutation_timer: f32,
}

pub struct RainSimulator {
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<Vec<CellState>>,
    drops: Vec<Drop>,
    charset: Vec<char>,
    density: f32,
    base_speed: f32,
    drop_length_min: usize,
    drop_length_max: usize,
    rng: SmallRng,
}

impl RainSimulator {
    pub fn new(columns: usize, rows: usize, charset: Vec<char>, config: &RainConfig) -> Self {
        let cells = vec![vec![CellState::default(); columns]; rows];
        Self {
            columns,
            rows,
            cells,
            drops: Vec::new(),
            charset,
            density: config.density,
            base_speed: config.speed * 8.0, // 8 cells/sec at speed=1.0
            drop_length_min: config.drop_length_min,
            drop_length_max: config.drop_length_max,
            rng: SmallRng::from_entropy(),
        }
    }

    pub fn update(&mut self, delta: f32) {
        // Clear
        for row in &mut self.cells {
            for cell in row.iter_mut() {
                cell.brightness = 0.0;
                cell.is_head = false;
            }
        }

        // Spawn: each column has density*delta probability per frame
        let spawn_prob = (self.density * delta * 60.0).min(1.0);
        for col in 0..self.columns {
            if self.rng.gen::<f32>() < spawn_prob {
                let length = self.rng.gen_range(self.drop_length_min..=self.drop_length_max);
                let speed = self.base_speed * self.rng.gen_range(0.5f32..1.5);
                let chars: Vec<char> = (0..length)
                    .map(|_| self.charset[self.rng.gen_range(0..self.charset.len())])
                    .collect();
                self.drops.push(Drop {
                    column: col,
                    head_row: -(length as f32),
                    length,
                    speed,
                    chars,
                    mutation_timer: 0.0,
                });
            }
        }

        // Advance and mutate
        let charset = &self.charset;
        let rng = &mut self.rng;
        self.drops.retain_mut(|drop| {
            drop.head_row += drop.speed * delta;
            drop.mutation_timer += delta;
            if drop.mutation_timer > 0.08 {
                drop.mutation_timer = 0.0;
                if !drop.chars.is_empty() {
                    let last = drop.chars.len() - 1;
                    drop.chars[last] = charset[rng.gen_range(0..charset.len())];
                }
            }
            // Remove when fully off screen
            drop.head_row < (drop.length + self.rows) as f32
        });

        // Write to cells
        let rows = self.rows;
        let cols = self.columns;
        for drop in &self.drops {
            let head = drop.head_row as i32;
            for i in 0..drop.length {
                let row = head - i as i32;
                if row < 0 || row >= rows as i32 { continue; }
                let row = row as usize;
                let col = drop.column;
                if col >= cols { continue; }

                let brightness = if i == 0 {
                    1.0
                } else {
                    1.0 - (i as f32 / drop.length as f32)
                };

                if brightness > self.cells[row][col].brightness {
                    let char_idx = (drop.length - 1 - i).min(drop.chars.len() - 1);
                    self.cells[row][col] = CellState {
                        ch: drop.chars[char_idx],
                        brightness,
                        is_head: i == 0,
                    };
                }
            }
        }
    }
}
```

- [ ] **Step 4: Run rain tests**

```
cargo test --test rain_test
```

Expected: all tests pass (charset + simulator).

- [ ] **Step 5: Commit**

```bash
git add src/rain.rs tests/rain_test.rs
git commit -m "feat: add rain simulator with drop lifecycle and brightness gradient"
```

---

## Task 5: Glyph Atlas

**Files:**
- Modify: `src/atlas.rs`

- [ ] **Step 1: Implement `src/atlas.rs`**

```rust
use fontdue::{Font, FontSettings};
use std::path::PathBuf;

pub struct GlyphAtlas {
    /// Raw R8 pixel data: one grayscale byte per pixel.
    pub data: Vec<u8>,
    pub atlas_width: u32,
    pub atlas_height: u32,
    /// Cell size on screen (all glyphs fit in this box).
    pub cell_width: u32,
    pub cell_height: u32,
    /// UV rects [u, v, w, h] in normalized [0,1] texture space per char.
    pub uvs: Vec<[f32; 4]>,
    /// The characters in atlas order.
    pub chars: Vec<char>,
}

impl GlyphAtlas {
    pub fn build(chars: &[char], font_size: f32, font_family: &str) -> Self {
        let font_path = Self::find_font(font_family);
        let font_bytes = std::fs::read(&font_path)
            .unwrap_or_else(|_| panic!("cannot read font: {}", font_path.display()));
        let font = Font::from_bytes(font_bytes.as_slice(), FontSettings::default())
            .expect("invalid font file");

        // Rasterize all chars, find max bounding box
        let rasterized: Vec<(fontdue::layout::GlyphRasterConfig, Vec<u8>)> = chars
            .iter()
            .map(|&ch| {
                let (metrics, bitmap) = font.rasterize(ch, font_size);
                (
                    fontdue::layout::GlyphRasterConfig {
                        glyph_index: font.lookup_glyph_index(ch),
                        render_hash: 0,
                        font_hash: 0,
                    },
                    // Store metrics inline — use a wrapper
                    {
                        let _ = metrics; // metrics captured in bitmap len
                        bitmap
                    },
                )
            })
            .collect();

        // Re-rasterize cleanly, capturing metrics
        let per_char: Vec<(fontdue::Metrics, Vec<u8>)> = chars
            .iter()
            .map(|&ch| font.rasterize(ch, font_size))
            .collect();

        let cell_width = per_char.iter().map(|(m, _)| m.width).max().unwrap_or(10) as u32;
        let cell_height = per_char.iter().map(|(m, _)| m.height).max().unwrap_or(18) as u32;

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
                    let dst = dst_y * atlas_width + dst_x;
                    if dst < atlas_data.len() as u32 {
                        atlas_data[dst as usize] = bitmap[src];
                    }
                }
            }

            let u = x_base as f32 / atlas_width as f32;
            let v = 0.0f32;
            let w = cell_width as f32 / atlas_width as f32;
            let h = 1.0f32;
            uvs.push([u, v, w, h]);
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

    /// Returns the UV rect for a character, or the first char's rect as fallback.
    pub fn uv_for_char(&self, ch: char) -> [f32; 4] {
        self.chars
            .iter()
            .position(|&c| c == ch)
            .map(|i| self.uvs[i])
            .unwrap_or_else(|| self.uvs.first().copied().unwrap_or([0.0; 4]))
    }

    fn find_font(family: &str) -> PathBuf {
        // Use fc-match to find a system font by family name.
        // For katakana support, callers should pass "monospace:lang=ja".
        let output = std::process::Command::new("fc-match")
            .args([family, "--format=%{file}"])
            .output()
            .expect("fc-match not found; install fontconfig");

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            panic!("fc-match returned no font for family '{family}'");
        }
        PathBuf::from(path)
    }
}
```

- [ ] **Step 2: Smoke-test atlas build compiles**

```
cargo check
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/atlas.rs
git commit -m "feat: add glyph atlas builder using fontdue + fc-match"
```

---

## Task 6: WGSL Shaders

**Files:**
- Modify: `src/shaders/rain.wgsl`, `src/shaders/blur.wgsl`

- [ ] **Step 1: Write `src/shaders/rain.wgsl`**

```wgsl
// Renders the Matrix rain: one instanced quad per visible character cell.

struct RainConfig {
    primary_color: vec4<f32>,     // bytes 0-15
    screen_size: vec2<f32>,       // bytes 16-23
    cell_size: vec2<f32>,         // bytes 24-31
}

@group(0) @binding(0) var<uniform> cfg: RainConfig;
@group(0) @binding(1) var glyph_atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct Instance {
    @location(0) position: vec2<f32>,    // top-left pixel of cell
    @location(1) atlas_rect: vec4<f32>,  // u, v, w, h in [0,1] texture space
    @location(2) brightness: f32,
    @location(3) is_head: u32,
}

struct VsOut {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) brightness: f32,
    @location(2) is_head: f32,
}

// Clockwise quad: two triangles covering [0,1]x[0,1]
var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let local = QUAD[vi];
    let px = inst.position + local * cfg.cell_size;
    // NDC: x in [-1,1], y in [-1,1] (y flipped)
    let ndc = vec2<f32>(
        px.x / cfg.screen_size.x * 2.0 - 1.0,
        1.0 - px.y / cfg.screen_size.y * 2.0,
    );

    var out: VsOut;
    out.clip_pos = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = inst.atlas_rect.xy + local * inst.atlas_rect.zw;
    out.brightness = inst.brightness;
    out.is_head = f32(inst.is_head);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    // Atlas is R8: coverage in red channel
    let coverage = textureSample(glyph_atlas, atlas_sampler, in.uv).r;

    // Head cell → nearly white; rest → primary color scaled by brightness
    let color = mix(cfg.primary_color.rgb, vec3<f32>(0.9, 1.0, 0.9), in.is_head * 0.85);
    let scaled = color * in.brightness;

    return vec4<f32>(scaled, coverage * max(in.brightness, 0.05));
}
```

- [ ] **Step 2: Write `src/shaders/blur.wgsl`**

```wgsl
// Single-axis Gaussian blur pass.
// Run twice: once horizontal (direction = (1,0)), once vertical (direction = (0,1)).
// Output is additively blended onto the main framebuffer for a glow effect.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct BlurParams {
    direction: vec2<f32>,  // (1,0) or (0,1)
    intensity: f32,
    _pad: f32,
}
@group(0) @binding(2) var<uniform> params: BlurParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Full-screen triangle trick
var<private> TRI: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, -1.0),
    vec2<f32>( 3.0, -1.0),
    vec2<f32>(-1.0,  3.0),
);

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var out: VsOut;
    out.pos = vec4<f32>(TRI[i], 0.0, 1.0);
    out.uv  = TRI[i] * vec2<f32>(0.5, -0.5) + 0.5;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(src));
    // 9-tap Gaussian weights
    let W = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540540, 0.0162162);

    var col = textureSample(src, src_sampler, in.uv) * W[0];
    for (var i = 1; i < 5; i++) {
        let off = params.direction * texel * f32(i);
        col += textureSample(src, src_sampler, in.uv + off) * W[i];
        col += textureSample(src, src_sampler, in.uv - off) * W[i];
    }
    return col * params.intensity;
}
```

- [ ] **Step 3: Verify cargo check still clean**

```
cargo check
```

- [ ] **Step 4: Commit**

```bash
git add src/shaders/
git commit -m "feat: add rain and bloom WGSL shaders"
```

---

## Task 7: Renderer

**Files:**
- Modify: `src/renderer.rs`

- [ ] **Step 1: Implement `src/renderer.rs`**

```rust
use std::sync::Arc;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;
use crate::{atlas::GlyphAtlas, config::Config, rain::CellState};

/// Per-instance data sent to the vertex shader.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    pub position: [f32; 2],
    pub atlas_rect: [f32; 4],
    pub brightness: f32,
    pub is_head: u32,
}

impl Instance {
    const ATTRIBS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x4,
        2 => Float32,
        3 => Uint32,
    ];

    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RainUniform {
    primary_color: [f32; 4],
    screen_size: [f32; 2],
    cell_size: [f32; 2],
}

pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub surface_config: wgpu::SurfaceConfiguration,

    atlas_texture: wgpu::Texture,
    rain_pipeline: wgpu::RenderPipeline,
    rain_bind_group: wgpu::BindGroup,
    rain_uniform_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    max_instances: usize,

    // Glow: offscreen texture → blur H → blur V → additive blend
    offscreen_tex: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    blur_h_tex: wgpu::Texture,
    blur_h_view: wgpu::TextureView,
    blur_pipeline: wgpu::RenderPipeline,
    blur_h_bind_group: wgpu::BindGroup,
    blur_v_bind_group: wgpu::BindGroup,
    blur_h_uniform_buf: wgpu::Buffer,
    blur_v_uniform_buf: wgpu::Buffer,
    blend_pipeline: wgpu::RenderPipeline,
    blend_bind_group: wgpu::BindGroup,

    primary_color: [f32; 4],
    glow: bool,
    pub width: u32,
    pub height: u32,
    pub atlas: Arc<GlyphAtlas>,
}

impl Renderer {
    pub async fn new(
        display_ptr: *mut std::ffi::c_void,
        surface_ptr: *mut std::ffi::c_void,
        width: u32,
        height: u32,
        atlas: Arc<GlyphAtlas>,
        config: &Config,
    ) -> Self {
        use raw_window_handle::{
            RawDisplayHandle, RawWindowHandle,
            WaylandDisplayHandle, WaylandWindowHandle,
        };
        use std::ptr::NonNull;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            ..Default::default()
        });

        let display_handle = RawDisplayHandle::Wayland(
            WaylandDisplayHandle::new(NonNull::new(display_ptr).unwrap()),
        );
        let window_handle = RawWindowHandle::Wayland(
            WaylandWindowHandle::new(NonNull::new(surface_ptr).unwrap()),
        );

        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: display_handle,
                raw_window_handle: window_handle,
            })
        }.expect("failed to create wgpu surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no suitable GPU adapter");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("failed to get GPU device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter()
            .find(|&&f| f == wgpu::TextureFormat::Bgra8Unorm)
            .copied()
            .unwrap_or(caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let primary_color = Config::parse_color(&config.colors.primary);

        // ── Atlas texture ──────────────────────────────────────────────────────
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: wgpu::Extent3d {
                width: atlas.atlas_width,
                height: atlas.atlas_height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            atlas_texture.as_image_copy(),
            &atlas.data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(atlas.atlas_width),
                rows_per_image: Some(atlas.atlas_height),
            },
            wgpu::Extent3d {
                width: atlas.atlas_width,
                height: atlas.atlas_height,
                depth_or_array_layers: 1,
            },
        );
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        // ── Rain uniform ───────────────────────────────────────────────────────
        let rain_uniform = RainUniform {
            primary_color,
            screen_size: [width as f32, height as f32],
            cell_size: [atlas.cell_width as f32, atlas.cell_height as f32],
        };
        let rain_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rain_uniform"),
            contents: bytemuck::bytes_of(&rain_uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // ── Rain pipeline ──────────────────────────────────────────────────────
        let rain_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/rain.wgsl"));

        let rain_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rain_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let rain_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rain_bg"),
            layout: &rain_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: rain_uniform_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&atlas_sampler) },
            ],
        });

        let rain_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rain_pl"),
            bind_group_layouts: &[&rain_bgl],
            push_constant_ranges: &[],
        });

        let rain_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rain_pipeline"),
            layout: Some(&rain_pl),
            vertex: wgpu::VertexState {
                module: &rain_shader,
                entry_point: "vs_main",
                buffers: &[Instance::layout()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &rain_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        // ── Offscreen + blur textures ──────────────────────────────────────────
        let offscreen_desc = wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let offscreen_tex = device.create_texture(&offscreen_desc);
        let offscreen_view = offscreen_tex.create_view(&Default::default());
        let mut blur_desc = offscreen_desc.clone();
        blur_desc.label = Some("blur_h");
        let blur_h_tex = device.create_texture(&blur_desc);
        let blur_h_view = blur_h_tex.create_view(&Default::default());

        let blur_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let blur_shader = device.create_shader_module(wgpu::include_wgsl!("shaders/blur.wgsl"));

        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        #[repr(C)]
        #[derive(Copy, Clone, Pod, Zeroable)]
        struct BlurParams { direction: [f32; 2], intensity: f32, _pad: f32 }

        let blur_h_uniform = BlurParams { direction: [1.0, 0.0], intensity: config.colors.glow_intensity, _pad: 0.0 };
        let blur_v_uniform = BlurParams { direction: [0.0, 1.0], intensity: config.colors.glow_intensity, _pad: 0.0 };
        let blur_h_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur_h_uniform"), contents: bytemuck::bytes_of(&blur_h_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let blur_v_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blur_v_uniform"), contents: bytemuck::bytes_of(&blur_v_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let blur_h_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_h_bg"), layout: &blur_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&offscreen_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: blur_h_uniform_buf.as_entire_binding() },
            ],
        });
        let blur_v_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blur_v_bg"), layout: &blur_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&blur_h_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: blur_v_uniform_buf.as_entire_binding() },
            ],
        });

        let blur_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blur_pl"), bind_group_layouts: &[&blur_bgl], push_constant_ranges: &[],
        });
        let blur_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blur_pipeline"), layout: Some(&blur_pl_layout),
            vertex: wgpu::VertexState { module: &blur_shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: None, write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None,
        });

        // ── Blend pipeline (additive blend of blur result onto final) ──────────
        // Reuse blur shader vs_main with a full-screen triangle; only the blend mode differs
        let blend_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blend_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }, count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false,
                        min_binding_size: None,
                    }, count: None,
                },
            ],
        });
        let blend_uniform = BlurParams { direction: [0.0, 0.0], intensity: 1.0, _pad: 0.0 };
        let blend_uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blend_uniform"), contents: bytemuck::bytes_of(&blend_uniform),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let blend_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blend_bg"), layout: &blend_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&blur_h_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&blur_sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: blend_uniform_buf.as_entire_binding() },
            ],
        });
        let blend_pl_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("blend_pl"), bind_group_layouts: &[&blend_bgl], push_constant_ranges: &[],
        });
        let additive = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let blend_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blend_pipeline"), layout: Some(&blend_pl_layout),
            vertex: wgpu::VertexState { module: &blur_shader, entry_point: "vs_main", buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &blur_shader, entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format, blend: Some(additive), write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None, multisample: wgpu::MultisampleState::default(), multiview: None,
        });

        // ── Instance buffer (pre-allocate for max screen cells) ────────────────
        let max_instances = ((width / atlas.cell_width) * (height / atlas.cell_height)) as usize + 256;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buf"),
            size: (max_instances * std::mem::size_of::<Instance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            device, queue, surface, surface_config,
            atlas_texture, rain_pipeline, rain_bind_group, rain_uniform_buf,
            instance_buf, max_instances,
            offscreen_tex, offscreen_view, blur_h_tex, blur_h_view,
            blur_pipeline, blur_h_bind_group, blur_v_bind_group,
            blur_h_uniform_buf, blur_v_uniform_buf,
            blend_pipeline, blend_bind_group,
            primary_color, glow: config.colors.glow,
            width, height, atlas,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 { return; }
        self.width = width;
        self.height = height;
        self.surface_config.width = width;
        self.surface_config.height = height;
        self.surface.configure(&self.device, &self.surface_config);

        let uniform = RainUniform {
            primary_color: self.primary_color,
            screen_size: [width as f32, height as f32],
            cell_size: [self.atlas.cell_width as f32, self.atlas.cell_height as f32],
        };
        self.queue.write_buffer(&self.rain_uniform_buf, 0, bytemuck::bytes_of(&uniform));
    }

    pub fn render(&mut self, cells: &[Vec<CellState>], bg_color: [f32; 4]) {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => return,
        };
        let frame_view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());

        let cw = self.atlas.cell_width as f32;
        let ch = self.atlas.cell_height as f32;

        // Build instance buffer from cell grid
        let mut instances: Vec<Instance> = Vec::new();
        for (row_idx, row) in cells.iter().enumerate() {
            for (col_idx, cell) in row.iter().enumerate() {
                if cell.brightness < 0.01 { continue; }
                let uv = self.atlas.uv_for_char(cell.ch);
                instances.push(Instance {
                    position: [col_idx as f32 * cw, row_idx as f32 * ch],
                    atlas_rect: uv,
                    brightness: cell.brightness,
                    is_head: cell.is_head as u32,
                });
            }
        }
        instances.truncate(self.max_instances);
        if !instances.is_empty() {
            self.queue.write_buffer(
                &self.instance_buf, 0,
                bytemuck::cast_slice(&instances),
            );
        }

        let wgpu::Color { r, g, b, a } = wgpu::Color {
            r: bg_color[0] as f64, g: bg_color[1] as f64,
            b: bg_color[2] as f64, a: bg_color[3] as f64,
        };
        let clear = wgpu::Color { r, g, b, a };

        // Pass 1: render characters to offscreen (or directly to frame if no glow)
        let target_view = if self.glow { &self.offscreen_view } else { &frame_view };
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rain_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                ..Default::default()
            });
            if !instances.is_empty() {
                pass.set_pipeline(&self.rain_pipeline);
                pass.set_bind_group(0, &self.rain_bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buf.slice(..));
                pass.draw(0..6, 0..instances.len() as u32);
            }
        }

        if self.glow {
            // Pass 2: horizontal blur (offscreen → blur_h)
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blur_h"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &self.blur_h_view, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.blur_pipeline);
                pass.set_bind_group(0, &self.blur_h_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // Pass 3: copy offscreen to frame (main scene)
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("main_copy"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.blend_pipeline);
                pass.set_bind_group(0, &self.blend_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            // Pass 4: additive blend of blur_h (glow) onto frame
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("glow_blend"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &frame_view, resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.blend_pipeline);
                pass.set_bind_group(0, &self.blend_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }

        self.queue.submit([encoder.finish()]);
        frame.present();
    }
}
```

- [ ] **Step 2: Verify compilation**

```
cargo check
```

Expected: compiles (may have unused import warnings — OK).

- [ ] **Step 3: Commit**

```bash
git add src/renderer.rs
git commit -m "feat: add wgpu renderer with instanced character drawing and glow pass"
```

---

## Task 8: Wayland App State + Layer Shell + Idle Monitor

**Files:**
- Modify: `src/wayland_app.rs`

- [ ] **Step 1: Implement `src/wayland_app.rs`**

```rust
use std::sync::{Arc, mpsc};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_layer, delegate_output, delegate_registry, delegate_seat,
    delegate_keyboard, delegate_pointer,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{SeatHandler, SeatState, keyboard::{KeyboardHandler, KeyEvent}, pointer::{PointerHandler, PointerEvent}},
    shell::wlr_layer::{
        Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler,
        LayerSurface, LayerSurfaceConfigure,
    },
};
use wayland_client::{
    globals::GlobalList, protocol::{wl_output, wl_seat, wl_surface, wl_keyboard, wl_pointer},
    Connection, QueueHandle,
};
use wayland_protocols::ext::idle_notify::v1::client::{
    ext_idle_notification_v1::{self, ExtIdleNotificationV1},
    ext_idle_notifier_v1::ExtIdleNotifierV1,
};
use crate::config::Config;

#[derive(Debug)]
pub enum AppEvent {
    Idle,
    Resume,
    Dismiss,    // user input while screensaver active
    Resize(u32, u32),
}

pub struct AppState {
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub seat_state: SeatState,
    pub layer_shell: LayerShell,

    pub layer_surface: Option<LayerSurface>,
    pub wl_surface: Option<wl_surface::WlSurface>,
    pub configured: bool,

    pub idle_notifier: Option<ExtIdleNotifierV1>,
    pub idle_notification: Option<ExtIdleNotificationV1>,
    pub timeout_ms: u32,

    pub event_tx: mpsc::Sender<AppEvent>,
    pub qh: QueueHandle<Self>,
    pub conn: Connection,
}

impl AppState {
    pub fn new(
        conn: Connection,
        globals: &GlobalList,
        qh: QueueHandle<Self>,
        config: &Config,
        event_tx: mpsc::Sender<AppEvent>,
    ) -> Self {
        let compositor_state = CompositorState::bind(globals, &qh).expect("compositor not available");
        let layer_shell = LayerShell::bind(globals, &qh).expect("wlr-layer-shell not available");
        let output_state = OutputState::new(globals, &qh);
        let seat_state = SeatState::new(globals, &qh);

        // Bind ext-idle-notifier-v1
        let idle_notifier: Option<ExtIdleNotifierV1> = globals.bind(&qh, 1..=1, ()).ok();

        Self {
            registry_state: RegistryState::new(globals),
            compositor_state,
            output_state,
            seat_state,
            layer_shell,
            layer_surface: None,
            wl_surface: None,
            configured: false,
            idle_notifier,
            idle_notification: None,
            timeout_ms: (config.idle.timeout_seconds * 1000) as u32,
            event_tx,
            qh,
            conn,
        }
    }

    /// Create idle notification subscription on the first available seat.
    pub fn subscribe_idle(&mut self, seat: &wl_seat::WlSeat) {
        if let (Some(notifier), None) = (&self.idle_notifier, &self.idle_notification) {
            let notification = notifier.get_idle_notification(
                self.timeout_ms,
                seat,
                &self.qh,
                (),
            );
            self.idle_notification = Some(notification);
        }
    }

    /// Create the fullscreen layer surface over the given output.
    pub fn create_layer_surface(&mut self, output: Option<&wl_output::WlOutput>) {
        if self.layer_surface.is_some() { return; }
        let wl_surface = self.compositor_state.create_surface(&self.qh);
        let layer_surface = self.layer_shell.create_layer_surface(
            &self.qh,
            wl_surface.clone(),
            Layer::Overlay,
            Some("matrix-screensaver"),
            output,
        );
        layer_surface.set_anchor(Anchor::all());
        layer_surface.set_exclusive_zone(-1);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer_surface.set_size(0, 0);
        wl_surface.commit();
        self.wl_surface = Some(wl_surface);
        self.layer_surface = Some(layer_surface);
    }

    pub fn destroy_layer_surface(&mut self) {
        if let Some(ls) = self.layer_surface.take() {
            ls.destroy();
        }
        self.wl_surface = None;
        self.configured = false;
    }
}

// ── SCTK delegate impls ────────────────────────────────────────────────────

impl CompositorHandler for AppState {
    fn scale_factor_changed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface, _new_factor: i32) {}
    fn transform_changed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface, _new_transform: wl_output::Transform) {}
    fn frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface, _time: u32) {}
    fn surface_enter(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface, _output: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface, _output: &wl_output::WlOutput) {}
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState { &mut self.output_state }
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl LayerShellHandler for AppState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.layer_surface = None;
        self.wl_surface = None;
    }

    fn configure(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        layer: &LayerSurface, configure: LayerSurfaceConfigure, serial: u32)
    {
        layer.ack_configure(serial);
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            let _ = self.event_tx.send(AppEvent::Resize(w, h));
        }
        self.configured = true;
    }
}

impl SeatHandler for AppState {
    fn seat_state(&mut self) -> &mut SeatState { &mut self.seat_state }
    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.subscribe_idle(&seat);
    }
    fn new_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat, capability: smithay_client_toolkit::seat::Capability)
    {
        // Keyboard and pointer capabilities handled by SCTK delegates
    }
    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat, _capability: smithay_client_toolkit::seat::Capability) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: wl_seat::WlSeat) {}
}

impl KeyboardHandler for AppState {
    fn enter(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface, _serial: u32, _raw: &[u32], _keysyms: &[u32]) {}
    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface, _serial: u32) {}
    fn press_key(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32, _event: KeyEvent)
    {
        let _ = self.event_tx.send(AppEvent::Dismiss);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard,
        _: u32, _: smithay_client_toolkit::seat::keyboard::Modifiers) {}
}

impl PointerHandler for AppState {
    fn pointer_frame(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer, _events: &[PointerEvent])
    {
        let _ = self.event_tx.send(AppEvent::Dismiss);
    }
}

// Idle notification dispatch
impl wayland_client::Dispatch<ExtIdleNotificationV1, ()> for AppState {
    fn event(state: &mut Self, _proxy: &ExtIdleNotificationV1,
        event: ext_idle_notification_v1::Event, _: &(), _conn: &Connection, _qh: &QueueHandle<Self>)
    {
        match event {
            ext_idle_notification_v1::Event::Idled => {
                let _ = state.event_tx.send(AppEvent::Idle);
            }
            ext_idle_notification_v1::Event::Resumed => {
                let _ = state.event_tx.send(AppEvent::Resume);
            }
            _ => {}
        }
    }
}

impl wayland_client::Dispatch<ExtIdleNotifierV1, ()> for AppState {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(AppState);
delegate_layer!(AppState);
delegate_output!(AppState);
delegate_registry!(AppState);
delegate_seat!(AppState);
delegate_keyboard!(AppState);
delegate_pointer!(AppState);
```

- [ ] **Step 2: Check compilation**

```
cargo check
```

Expected: no errors (some warnings about unused fields OK).

- [ ] **Step 3: Commit**

```bash
git add src/wayland_app.rs
git commit -m "feat: add Wayland app state with layer shell and idle monitor delegates"
```

---

## Task 9: Main Event Loop

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement `src/main.rs`**

```rust
pub mod config;
pub mod chars;
pub mod rain;
pub mod atlas;
pub mod renderer;
pub mod wayland_app;

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{globals::registry_queue_init, Connection};
use config::Config;
use chars::get_charset;
use rain::RainSimulator;
use atlas::GlyphAtlas;
use renderer::Renderer;
use wayland_app::{AppEvent, AppState};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let test_mode = std::env::args().any(|a| a == "--test");
    let config = Config::load();

    // Font family: use lang=ja for katakana support
    let font_family = match &config.rain.charset {
        config::CharsetKind::Katakana | config::CharsetKind::Mixed => {
            format!("{}:lang=ja", config.display.font)
        }
        _ => config.display.font.clone(),
    };

    let charset = get_charset(&config.rain.charset);
    let atlas = Arc::new(
        GlyphAtlas::build(&charset, config.display.font_size, &font_family)
    );

    let bg_color = Config::parse_color(&config.colors.background);
    let frame_duration = Duration::from_secs_f64(1.0 / config.display.fps as f64);

    // Wayland setup
    let conn = Connection::connect_to_env().expect("Wayland connection failed");
    let (globals, event_queue) = registry_queue_init(&conn).expect("registry init failed");
    let qh = event_queue.handle();

    let (event_tx, event_rx) = mpsc::channel::<AppEvent>();

    let mut app_state = AppState::new(conn.clone(), &globals, qh.clone(), &config, event_tx);

    let mut event_loop: EventLoop<AppState> =
        EventLoop::try_new().expect("event loop failed");
    WaylandSource::new(conn.clone(), event_queue)
        .insert(event_loop.handle())
        .expect("wayland source insert failed");

    // State machine
    let mut renderer: Option<Renderer> = None;
    let mut rain: Option<RainSimulator> = None;
    let mut screensaver_active = false;
    let mut last_frame = Instant::now();

    // Run initial roundtrip so seat/output globals are ready
    event_loop
        .dispatch(Some(Duration::from_millis(100)), &mut app_state)
        .unwrap();

    loop {
        // Dispatch Wayland events (non-blocking)
        event_loop
            .dispatch(Some(Duration::from_millis(1)), &mut app_state)
            .unwrap();

        // Process app events
        while let Ok(event) = event_rx.try_recv() {
            match event {
                AppEvent::Idle if !screensaver_active => {
                    tracing::info!("idle: activating screensaver");
                    app_state.create_layer_surface(None);
                    screensaver_active = true;
                }

                AppEvent::Resume | AppEvent::Dismiss if screensaver_active => {
                    tracing::info!("resume: hiding screensaver");
                    app_state.destroy_layer_surface();
                    renderer = None;
                    rain = None;
                    screensaver_active = false;
                }

                AppEvent::Resize(w, h) => {
                    if let Some(r) = &mut renderer {
                        r.resize(w, h);
                        // Rebuild rain simulator for new dimensions
                        let cols = (w / atlas.cell_width).max(1) as usize;
                        let rows = (h / atlas.cell_height).max(1) as usize;
                        rain = Some(RainSimulator::new(cols, rows, charset.clone(), &config.rain));
                    } else if screensaver_active && app_state.configured {
                        // First configure: create renderer
                        if let Some(wl_surface) = &app_state.wl_surface {
                            let display_ptr = conn.backend().display_ptr() as *mut std::ffi::c_void;
                            let surface_ptr = wl_surface.id().as_ptr() as *mut std::ffi::c_void;
                            let r = pollster::block_on(Renderer::new(
                                display_ptr, surface_ptr, w, h, atlas.clone(), &config,
                            ));
                            let cols = (w / atlas.cell_width).max(1) as usize;
                            let rows = (h / atlas.cell_height).max(1) as usize;
                            rain = Some(RainSimulator::new(cols, rows, charset.clone(), &config.rain));
                            renderer = Some(r);
                        }
                    }
                }

                _ => {}
            }
        }

        // Render frame if active
        if screensaver_active {
            if let (Some(r), Some(sim)) = (&mut renderer, &mut rain) {
                let now = Instant::now();
                let delta = now.duration_since(last_frame).as_secs_f32();
                last_frame = now;

                sim.update(delta);
                r.render(&sim.cells, bg_color);

                if test_mode {
                    tracing::info!("--test: rendered one frame, exiting");
                    return;
                }

                // Cap frame rate
                let elapsed = Instant::now().duration_since(now);
                if elapsed < frame_duration {
                    std::thread::sleep(frame_duration - elapsed);
                }
            }
        } else {
            last_frame = Instant::now();
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
```

- [ ] **Step 2: Build release binary**

```
cargo build --release 2>&1 | tail -20
```

Expected: compiles successfully. Fix any type mismatches that arise (API specifics may need minor adjustments).

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: implement main event loop with idle detection and render loop"
```

---

## Task 10: KDE Plugin + Default Config

**Files:**
- Create: `kde-plugin/matrix-screensaver/metadata.json`
- Create: `kde-plugin/matrix-screensaver/contents/ui/main.qml`

- [ ] **Step 1: Create `kde-plugin/matrix-screensaver/metadata.json`**

```json
{
    "KPackageStructure": "KDE/WallpaperPlugin",
    "KPlugin": {
        "Authors": [{ "Name": "matrix-screensaver" }],
        "Description": "Matrix rain screensaver",
        "Id": "com.github.matrix-screensaver",
        "License": "MIT",
        "Name": "Matrix Screensaver",
        "Version": "1.0"
    },
    "X-KDE-PluginInfo-Name": "com.github.matrix-screensaver"
}
```

- [ ] **Step 2: Create `kde-plugin/matrix-screensaver/contents/ui/main.qml`**

```qml
import QtQuick 2.15

Rectangle {
    id: root
    color: "black"

    Text {
        anchors.centerIn: parent
        text: "Matrix Screensaver\nRun: ~/.local/bin/matrix-screensaver"
        color: "#00ff41"
        font.family: "monospace"
        font.pointSize: 14
        horizontalAlignment: Text.AlignHCenter
    }
}
```

Note: The QML plugin appears in System Settings for discoverability. The actual rendering is done by the Rust binary running as a background daemon via autostart.

- [ ] **Step 3: Commit**

```bash
git add kde-plugin/
git commit -m "feat: add KDE wallpaper plugin for System Settings registration"
```

---

## Task 11: Installer Script

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Create `install.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="matrix-screensaver"
BINARY_SRC="target/release/${BINARY_NAME}"
BINARY_DST="${HOME}/.local/bin/${BINARY_NAME}"
PLUGIN_SRC="kde-plugin/matrix-screensaver"
PLUGIN_DST="${HOME}/.local/share/kscreenlocker/wallpapers/matrix-screensaver"
CONFIG_DIR="${HOME}/.config/matrix-screensaver"
AUTOSTART_DIR="${HOME}/.config/autostart"

echo "==> Building release binary..."
cargo build --release

echo "==> Installing binary to ${BINARY_DST}..."
install -Dm755 "${BINARY_SRC}" "${BINARY_DST}"

echo "==> Installing KDE plugin to ${PLUGIN_DST}..."
mkdir -p "${PLUGIN_DST}"
cp -r "${PLUGIN_SRC}/." "${PLUGIN_DST}/"

echo "==> Writing default config (skipped if already exists)..."
mkdir -p "${CONFIG_DIR}"
if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    cp config/default.toml "${CONFIG_DIR}/config.toml"
    echo "    Created ${CONFIG_DIR}/config.toml"
else
    echo "    Config already exists — not overwritten."
fi

echo "==> Installing KDE autostart entry..."
mkdir -p "${AUTOSTART_DIR}"
cat > "${AUTOSTART_DIR}/${BINARY_NAME}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Matrix Screensaver
Comment=Matrix rain screensaver daemon
Exec=${BINARY_DST}
X-KDE-autostart-phase=2
X-KDE-autostart-after=panel
Hidden=false
EOF

echo ""
echo "Installation complete."
echo ""
echo "  Binary:    ${BINARY_DST}"
echo "  Config:    ${CONFIG_DIR}/config.toml"
echo "  Autostart: ${AUTOSTART_DIR}/${BINARY_NAME}.desktop"
echo ""
echo "Log out and back in (or run '${BINARY_DST}' manually) to start."
echo "Screensaver activates after \$(grep timeout ${CONFIG_DIR}/config.toml | head -1 | grep -o '[0-9]*') seconds idle."
```

- [ ] **Step 2: Make executable**

```bash
chmod +x install.sh
```

- [ ] **Step 3: Verify script syntax**

```bash
bash -n install.sh && echo "syntax OK"
```

Expected: `syntax OK`

- [ ] **Step 4: Commit**

```bash
git add install.sh config/default.toml
git commit -m "feat: add automated installer script"
```

---

## Task 12: Initialize CLAUDE.md + .gitignore

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Update `.gitignore`**

```
/target
.superpowers/
*.orig
```

- [ ] **Step 2: Commit everything remaining**

```bash
git add .gitignore docs/
git commit -m "chore: add gitignore and project docs"
```

---

## Task 13: Integration Verification

**Files:** None (read-only verification)

- [ ] **Step 1: Full release build**

```
cargo build --release
```

Expected: success, `target/release/matrix-screensaver` exists.

- [ ] **Step 2: Run all unit tests**

```
cargo test
```

Expected: all tests pass.

- [ ] **Step 3: Verify `--test` flag works**

```bash
# Requires a running Wayland compositor (KDE session)
WAYLAND_DISPLAY=wayland-0 ./target/release/matrix-screensaver --test
echo "exit: $?"
```

Expected: exits 0 after rendering one frame (or logs "no Wayland display" if outside compositor).

- [ ] **Step 4: Run installer dry-run**

```bash
# Verify install.sh touches correct paths without actually running
grep -E '(BINARY_DST|PLUGIN_DST|CONFIG_DIR|AUTOSTART_DIR)=' install.sh
```

Expected: 4 path variables shown.

- [ ] **Step 5: Final commit**

```bash
git add -A
git status  # verify nothing unexpected
git commit -m "chore: complete matrix screensaver implementation"
```

---

## Customization Reference

After install, edit `~/.config/matrix-screensaver/config.toml`:

| Key | Default | Effect |
|---|---|---|
| `display.font` | `"monospace"` | Font family (fc-match name) |
| `display.font_size` | `18` | Character height in px |
| `display.fps` | `60` | Frame cap |
| `rain.speed` | `1.0` | Drop fall speed multiplier |
| `rain.density` | `0.05` | New-drop probability per column per frame |
| `rain.charset` | `"mixed"` | `mixed/katakana/latin/binary` |
| `rain.drop_length_min/max` | `5/25` | Drop length range |
| `colors.primary` | `"#00ff41"` | Rain color |
| `colors.background` | `"#000000"` | Background |
| `colors.glow` | `true` | Bloom glow pass |
| `colors.glow_intensity` | `0.8` | Glow strength |
| `idle.timeout_seconds` | `120` | Idle time before activation |
