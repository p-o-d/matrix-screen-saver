// Full-screen post-process: darkens every other horizontal row to simulate CRT scanlines.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct ScanlineParams {
    intensity: f32,
}
@group(0) @binding(2) var<uniform> params: ScanlineParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

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
    var color = textureSample(src, src_sampler, in.uv);
    let dark = 1.0 - params.intensity * f32(u32(in.pos.y) % 2u);
    return vec4<f32>(color.rgb * dark, color.a);
}
