// Single-axis Gaussian blur pass.
// Run twice: horizontal (direction=(1,0)) then vertical (direction=(0,1)).

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct BlurParams {
    direction: vec2<f32>,
    intensity: f32,
    _pad: f32,
}
@group(0) @binding(2) var<uniform> params: BlurParams;

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
    let texel = 1.0 / vec2<f32>(textureDimensions(src));
    // Unrolled: naga rejects dynamic array indexing in wgpu 0.20
    let w0 = 0.2270270;
    let w1 = 0.1945946;
    let w2 = 0.1216216;
    let w3 = 0.0540540;
    let w4 = 0.0162162;

    var col = textureSample(src, src_sampler, in.uv) * w0;

    let off1 = params.direction * texel;
    col += textureSample(src, src_sampler, in.uv + off1) * w1;
    col += textureSample(src, src_sampler, in.uv - off1) * w1;

    let off2 = params.direction * texel * 2.0;
    col += textureSample(src, src_sampler, in.uv + off2) * w2;
    col += textureSample(src, src_sampler, in.uv - off2) * w2;

    let off3 = params.direction * texel * 3.0;
    col += textureSample(src, src_sampler, in.uv + off3) * w3;
    col += textureSample(src, src_sampler, in.uv - off3) * w3;

    let off4 = params.direction * texel * 4.0;
    col += textureSample(src, src_sampler, in.uv + off4) * w4;
    col += textureSample(src, src_sampler, in.uv - off4) * w4;

    return col * params.intensity;
}
