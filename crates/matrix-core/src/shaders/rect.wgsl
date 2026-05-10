// Renders a solid-color rectangle. Used for the debug overlay background panel.

struct RectParams {
    rect: vec4<f32>,         // x, y, w, h in pixels
    screen_size: vec2<f32>,
    _pad: vec2<f32>,
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> p: RectParams;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
}

var<private> QUAD: array<vec2<f32>, 6> = array<vec2<f32>, 6>(
    vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
);

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    let local = QUAD[vi];
    let px = p.rect.xy + local * p.rect.zw;
    let ndc = vec2<f32>(
        px.x / p.screen_size.x * 2.0 - 1.0,
        1.0 - px.y / p.screen_size.y * 2.0,
    );
    var out: VsOut;
    out.pos = vec4<f32>(ndc, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return p.color;
}
