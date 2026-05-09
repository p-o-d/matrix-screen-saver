# Perspective Depth Layers — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add N-layer depth illusion to matrix rain: far drops are small and dim, close drops are large and bright, near overlaps far.

**Architecture:** Each depth level = independent `RainSimulator` on a scaled grid. All instances collected far→near into one buffer per frame. `Instance` gains a `scale` field; shader uses it for quad sizing. Config adds 3 new `[rain]` keys.

**Tech Stack:** Existing (Rust, wgpu 0.20, WGSL, serde/toml)

---

## File Map

```
src/config.rs          — add depth_levels, depth_scale_min, depth_brightness_min to RainConfig
config/default.toml    — add new keys
src/renderer.rs        — Instance gets scale: f32; render() takes &[DepthLayer]
src/shaders/rain.wgsl  — Instance input gets @location(4) scale: f32; shader uses it
src/main.rs            — Vec<Vec<RainSimulator>> per screen; collect layers; call render
```

---

## Task 1: Config — add depth fields

**Files:**
- Modify: `src/config.rs`
- Modify: `config/default.toml`

- [ ] **Step 1: Add fields to `RainConfig` in `src/config.rs`**

Find the `RainConfig` struct and add three fields:

```rust
pub struct RainConfig {
    pub speed: f32,
    pub density: f32,
    pub charset: CharsetKind,
    pub drop_length_min: usize,
    pub drop_length_max: usize,
    pub depth_levels: u8,
    pub depth_scale_min: f32,
    pub depth_brightness_min: f32,
}
```

Update `Default for RainConfig`:

```rust
impl Default for RainConfig {
    fn default() -> Self {
        Self {
            speed: 1.0,
            density: 0.05,
            charset: CharsetKind::Mixed,
            drop_length_min: 5,
            drop_length_max: 25,
            depth_levels: 3,
            depth_scale_min: 0.4,
            depth_brightness_min: 0.3,
        }
    }
}
```

- [ ] **Step 2: Update `config/default.toml`**

```toml
[rain]
speed = 1.0
density = 0.05
charset = "mixed"
drop_length_min = 5
drop_length_max = 25
depth_levels = 3
depth_scale_min = 0.4
depth_brightness_min = 0.3
```

- [ ] **Step 3: Verify tests still pass**

```bash
cargo test --test config_test
```

Expected: 4 tests pass (new fields get serde defaults, no breakage).

- [ ] **Step 4: Commit**

```bash
git add src/config.rs config/default.toml
git commit -m "feat: add depth_levels, depth_scale_min, depth_brightness_min to config"
```

---

## Task 2: Instance struct + WGSL shader

**Files:**
- Modify: `src/renderer.rs`
- Modify: `src/shaders/rain.wgsl`

- [ ] **Step 1: Add `scale` to `Instance` in `src/renderer.rs`**

Current `Instance`:
```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    pub position: [f32; 2],   // location 0
    pub atlas_rect: [f32; 4], // location 1
    pub brightness: f32,      // location 2
    pub is_head: u32,         // location 3
}
```

New `Instance` (add `scale` after `is_head`):
```rust
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Instance {
    pub position: [f32; 2],   // location 0
    pub atlas_rect: [f32; 4], // location 1
    pub brightness: f32,      // location 2
    pub is_head: u32,         // location 3
    pub scale: f32,           // location 4
}
```

Update `ATTRIBS` (add one entry):
```rust
const ATTRIBS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x4,
    2 => Float32,
    3 => Uint32,
    4 => Float32,
];
```

- [ ] **Step 2: Update `src/shaders/rain.wgsl`**

Add `scale: f32` to the `Instance` struct:
```wgsl
struct Instance {
    @location(0) position: vec2<f32>,
    @location(1) atlas_rect: vec4<f32>,
    @location(2) brightness: f32,
    @location(3) is_head: u32,
    @location(4) scale: f32,
}
```

In `vs_main`, replace `cfg.cell_size` with `cfg.cell_size * inst.scale`:
```wgsl
@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let local = QUAD[vi];
    let actual_cell = cfg.cell_size * inst.scale;
    let px = inst.position + local * actual_cell;
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
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/renderer.rs src/shaders/rain.wgsl
git commit -m "feat: add per-instance scale to Instance struct and WGSL shader"
```

---

## Task 3: Renderer `render()` API — accept depth layers

**Files:**
- Modify: `src/renderer.rs`

- [ ] **Step 1: Add `DepthLayer` struct**

Add near the top of `src/renderer.rs` (after imports, before `Instance`):

```rust
/// One depth level's data for a single render call.
pub struct DepthLayer<'a> {
    pub cells: &'a [Vec<crate::rain::CellState>],
    pub scale: f32,
    pub brightness_mult: f32,
}
```

- [ ] **Step 2: Update `render()` signature and instance building**

Change:
```rust
pub fn render(&mut self, cells: &[Vec<CellState>])
```
To:
```rust
pub fn render(&mut self, layers: &[DepthLayer<'_>])
```

Replace the instance-building block. Old code builds instances from `cells` with `scale: 1.0` (not present yet — we're adding it now). New code iterates layers far→near:

```rust
let cw = self.atlas.cell_width as f32;
let ch = self.atlas.cell_height as f32;

let mut instances: Vec<Instance> = Vec::new();
// layers[0] = farthest → rendered first (painter's algorithm)
for layer in layers {
    let lcw = cw * layer.scale;
    let lch = ch * layer.scale;
    for (row_idx, row) in layer.cells.iter().enumerate() {
        for (col_idx, cell) in row.iter().enumerate() {
            if cell.brightness < 0.01 { continue; }
            let uv = self.atlas.uv_for_char(cell.ch);
            instances.push(Instance {
                position: [col_idx as f32 * lcw, row_idx as f32 * lch],
                atlas_rect: uv,
                brightness: cell.brightness * layer.brightness_mult,
                is_head: cell.is_head as u32,
                scale: layer.scale,
            });
        }
    }
}
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check
```

Expected: only callers of `render()` will break (main.rs — fixed in Task 4).

- [ ] **Step 4: Commit**

```bash
git add src/renderer.rs
git commit -m "feat: renderer render() accepts depth layers, builds instances far-to-near"
```

---

## Task 4: Main loop — multiple simulators per screen

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Add depth helper function**

Add after `get_surface_ptr`:

```rust
/// Compute per-level scale and brightness multipliers, far (index 0) → near (index N-1).
fn depth_levels(config: &matrix_screensaver::config::RainConfig) -> Vec<(f32, f32)> {
    let n = config.depth_levels.max(1) as usize;
    (0..n).map(|i| {
        let t = if n == 1 { 1.0 } else { i as f32 / (n - 1) as f32 };
        let scale = config.depth_scale_min + (1.0 - config.depth_scale_min) * t;
        let brightness = config.depth_brightness_min + (1.0 - config.depth_brightness_min) * t;
        (scale, brightness)
    }).collect()
}
```

- [ ] **Step 2: Change rain storage to `Vec<Vec<Option<RainSimulator>>>`**

Replace:
```rust
let mut rains: Vec<Option<RainSimulator>> = Vec::new();
```
With:
```rust
// Outer: one entry per screen surface. Inner: one per depth level.
let mut rains: Vec<Vec<RainSimulator>> = Vec::new();
```

- [ ] **Step 3: Update Idle handler**

Replace the rain resize_with in the Idle arm:
```rust
AppEvent::Idle if !screensaver_active => {
    tracing::info!("idle: activating screensaver");
    app_state.create_layer_surfaces_all();
    let n = app_state.surfaces.len();
    renderers.resize_with(n, || None);
    rains.resize_with(n, Vec::new);
    screensaver_active = true;
}
```

- [ ] **Step 4: Update Resize handler**

Replace the entire `AppEvent::Resize(idx, w, h)` arm:

```rust
AppEvent::Resize(idx, w, h) if screensaver_active => {
    if idx >= renderers.len() {
        renderers.resize_with(idx + 1, || None);
        rains.resize_with(idx + 1, Vec::new);
    }
    let levels = depth_levels(&config.rain);
    let cw = atlas.cell_width;
    let ch = atlas.cell_height;
    if let Some(r) = &mut renderers[idx] {
        r.resize(w, h);
        rains[idx] = levels.iter().map(|&(scale, _)| {
            let cols = ((w as f32 / (cw as f32 * scale)) as usize).max(1);
            let rows = ((h as f32 / (ch as f32 * scale)) as usize).max(1);
            RainSimulator::new(cols, rows, charset.clone(), &config.rain)
        }).collect();
    } else if app_state.surfaces.get(idx).map_or(false, |s| s.configured) {
        let display_ptr = get_display_ptr(&conn);
        let surface_ptr = get_surface_ptr(&app_state.surfaces[idx].wl_surface);
        let r = pollster::block_on(Renderer::new(
            display_ptr, surface_ptr, w, h, atlas.clone(), &config,
        ));
        rains[idx] = levels.iter().map(|&(scale, _)| {
            let cols = ((w as f32 / (cw as f32 * scale)) as usize).max(1);
            let rows = ((h as f32 / (ch as f32 * scale)) as usize).max(1);
            RainSimulator::new(cols, rows, charset.clone(), &config.rain)
        }).collect();
        renderers[idx] = Some(r);
    }
}
```

- [ ] **Step 5: Update Resume/Dismiss handler**

Replace:
```rust
rains.clear();
```
(Already `rains: Vec<Vec<RainSimulator>>` — `clear()` still works.)

- [ ] **Step 6: Update render loop**

Replace:
```rust
for i in 0..renderers.len() {
    if let (Some(r), Some(sim)) = (&mut renderers[i], &mut rains[i]) {
        sim.update(delta);
        r.render(&sim.cells);
        rendered_any = true;
    }
}
```

With:
```rust
let levels = depth_levels(&config.rain);
for i in 0..renderers.len() {
    if let Some(r) = &mut renderers[i] {
        if rains[i].is_empty() { continue; }
        for sim in &mut rains[i] {
            sim.update(delta);
        }
        let depth_layers: Vec<matrix_screensaver::renderer::DepthLayer<'_>> = rains[i]
            .iter()
            .zip(levels.iter())
            .map(|(sim, &(scale, brightness_mult))| {
                matrix_screensaver::renderer::DepthLayer {
                    cells: &sim.cells,
                    scale,
                    brightness_mult,
                }
            })
            .collect();
        r.render(&depth_layers);
        rendered_any = true;
    }
}
```

- [ ] **Step 7: Update --test mode**

The `--test` mode resend logic and wait loop don't touch rains/renderers directly — no change needed there. The state machine handles it through Resize events.

- [ ] **Step 8: Build release**

```bash
cargo build --release 2>&1 | tail -5
```

Expected: `Finished release profile`.

- [ ] **Step 9: Run all tests**

```bash
cargo test
```

Expected: all 12 tests pass (depth changes don't touch config/rain unit tests).

- [ ] **Step 10: Commit**

```bash
git add src/main.rs
git commit -m "feat: multi-depth rain simulators per screen with perspective scaling"
```

---

## Verification

After all tasks:

1. `cargo build --release` — clean build
2. `cargo test` — all 12 tests pass
3. Run with 5s test config — depth layers visually visible: far drops smaller/dimmer, near drops larger/brighter, near overlaps far
4. Set `depth_levels = 1` in config → flat (no perspective), single simulator
5. Set `depth_levels = 5` → five depth planes render correctly

---

## Config Reference (after this feature)

| Key | Default | Effect |
|---|---|---|
| `rain.depth_levels` | `3` | Number of depth planes (1 = flat) |
| `rain.depth_scale_min` | `0.4` | Cell scale of farthest plane |
| `rain.depth_brightness_min` | `0.3` | Brightness multiplier of farthest plane |
