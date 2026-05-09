// Renders Matrix rain: one instanced quad per visible character cell.

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

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, inst: Instance) -> VsOut {
    let local = QUAD[vi];
    let px = inst.position + local * cfg.cell_size;
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
    let coverage = textureSample(glyph_atlas, atlas_sampler, in.uv).r;
    let color = mix(cfg.primary_color.rgb, vec3<f32>(0.9, 1.0, 0.9), in.is_head * 0.85);
    let scaled = color * in.brightness;
    return vec4<f32>(scaled, coverage * max(in.brightness, 0.05));
}
