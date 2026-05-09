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
    let W = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540540, 0.0162162);

    var col = textureSample(src, src_sampler, in.uv) * W[0];
    for (var i = 1; i < 5; i++) {
        let off = params.direction * texel * f32(i);
        col += textureSample(src, src_sampler, in.uv + off) * W[i];
        col += textureSample(src, src_sampler, in.uv - off) * W[i];
    }
    return col * params.intensity;
}
