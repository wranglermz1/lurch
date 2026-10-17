// The screen blur's pixel shaders (`blur.rs`), ported from their disassembly (`docs/ui.md`):
// `downsample_ps40`, four bilinear taps a texel from the middle of each 4 × 4 block, and
// `blurfilter_ps40`, a centre tap and packs of taps either side along one axis, each tap
// mirrored back into the texture's inner half texel.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Blur {
    step: vec2<f32>,
    packs: i32,
    centre: f32,
    bias: vec3<f32>,
    scale: f32,
    taps: array<vec4<f32>, 9>,
    min_uv: vec2<f32>,
    range2: vec2<f32>,
};

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var bilinear: sampler;
@group(0) @binding(2) var<uniform> constants: Blur;

@fragment
fn downsample(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    var c = textureSample(source, bilinear, in.uv, vec2(-1, -1)).xyz;
    c += textureSample(source, bilinear, in.uv, vec2(-1, 1)).xyz;
    c += textureSample(source, bilinear, in.uv, vec2(1, -1)).xyz;
    c += textureSample(source, bilinear, in.uv, vec2(1, 1)).xyz;
    c = c * 0.25;
    return vec4(c, dot(vec3(0.3, 0.59, 0.11), c));
}

fn mirror(p: vec4<f32>) -> vec4<f32> {
    let q = p - constants.min_uv.xyxy;
    return min(abs(q), constants.range2.xyxy - q) + constants.min_uv.xyxy;
}

@fragment
fn blur(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    var acc = textureSample(source, bilinear, in.uv) * constants.centre;
    for (var i = 0; i < constants.packs; i++) {
        let t = constants.taps[i];
        let back = mirror(in.uv.xyxy - t.xxzz * constants.step.xyxy);
        let ahead = mirror(t.xxzz * constants.step.xyxy + in.uv.xyxy);
        let first = (textureSample(source, bilinear, back.xy) + textureSample(source, bilinear, ahead.xy)) * t.y + acc;
        acc = (textureSample(source, bilinear, back.zw) + textureSample(source, bilinear, ahead.zw)) * t.w + first;
    }
    return vec4(acc.xyz * constants.scale + constants.bias, acc.w);
}
