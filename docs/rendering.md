# Rendering Pipeline

## Overview

Two render passes per frame:

```
Rain instances → offscreen RGBA texture
                       ↓
              Gaussian blur (horizontal)
                       ↓
              Glow blend + scanline → swapchain
```

When glow is disabled the blur pass is skipped and rain renders directly to the swapchain.

---

## Instance buffer

Each visible character is one instance. The vertex shader positions a quad using per-instance data; no geometry is uploaded per frame.

Instance struct (matches `shaders/rain.wgsl` `VertexInput`):

```rust
struct RainInstance {
    position: [f32; 2],      // screen-space x, y (top-left of cell)
    uv_rect: [f32; 4],       // [u0, v0, u1, v1] into atlas texture
    color: [f32; 4],         // RGBA — brightness encodes depth + head/tail fade
    scale: f32,              // cell scale factor for depth layer
    _pad: [f32; 3],
}
```

Buffer is pre-allocated once for the worst-case cell count (sum across all depth levels at their smallest scale). The active instance count is written to a uniform and used as the draw count.

---

## Depth layers

`config.rain.depth_levels` independent `RainSimulator` instances run per screen. Layer 0 is farthest (scale = `depth_scale_min`, brightness = `depth_brightness_min`). Layer N-1 is nearest (scale = 1.0, brightness = 1.0).

Layers are collected into a single instance buffer in far→near order (painter's algorithm). Near layers overwrite far layers naturally via depth.

Instance count per layer: `ceil(w / (cell_w × scale)) × ceil(h / (cell_h × scale))`. Far layers have small cells → more instances than near layers.

---

## Pass 1 — Rain instances (`shaders/rain.wgsl`)

- **Vertex**: positions a unit quad, applies per-instance `position` + `scale`, looks up UV from atlas sampler
- **Fragment**: samples glyph atlas texture, multiplies by instance color
- **Target**: `offscreen_tex` (same format as swapchain, `TextureUsages::RENDER_ATTACHMENT | TEXTURE_BINDING`)
- **Blend**: standard alpha blend

Head character (brightest, usually white or near-white) is a separate instance at the drop's head position with a different color multiplier.

---

## Pass 2 — Blur + glow (`shaders/blur.wgsl`)

Separable Gaussian blur — horizontal pass writes to `blur_h_tex`, vertical pass reads from it.

**Why unrolled:** naga 0.20 rejects dynamic array indexing inside loops. The 9-tap Gaussian kernel is hardcoded as explicit variables:

```wgsl
let w0 = 0.227027; let w1 = 0.194595; let w2 = 0.121622;
let w3 = 0.054054; let w4 = 0.016216;
```

Blur radius scales with `glow_intensity` via a uniform.

---

## Pass 3 — Blend + scanlines (`shaders/scanline.wgsl`)

Composites the blurred glow texture over the original rain texture using additive blending, then applies a scanline darkening pattern and optional chromatic aberration (RGB channel offset).

**Scanlines:** every even row is darkened by `scanline_intensity`. Computed in the fragment shader from `floor(uv.y * screen_height) % 2`.

**Chromatic aberration:** red and blue channels are sampled at `uv ± aberration_offset` along the x axis. Green stays at center UV.

---

## Textures

| Name | Format | Size | Usage |
|------|--------|------|-------|
| `atlas_tex` | `Rgba8UnormSrgb` | `(cell_w × n_chars) × cell_h` | Glyph atlas, sampled in pass 1 |
| `offscreen_tex` | Swapchain format | Screen size | Pass 1 target, pass 3 source |
| `blur_h_tex` | Swapchain format | Screen size | Horizontal blur target |
| Swapchain | Platform format | Screen size | Final output |

`offscreen_tex` and `blur_h_tex` are recreated in `Renderer::resize` when the window resizes.

---

## Debug overlay (`shaders/rect.wgsl`)

Rendered as a separate pass after the scanline composite. Draws semi-transparent rectangles and bitmap text (using the debug atlas — a second `GlyphAtlas` built with a small fixed font). Shows:

- FPS counter
- CPU / RAM usage
- GPU name, VRAM usage, GPU load
- Renderer adapter info

The overlay drifts slowly (a few pixels per minute) and periodically glitches individual characters to prevent OLED burn-in.
