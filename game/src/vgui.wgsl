// A VGUI ImagePanel's image (`0x4a94f0`): the surface's textured rect, tinted by the draw
// colour and clipped to the parent's clip rectangle, in the target's pixels as the RUI's quads.

#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var image_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var image_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> color: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> clip_rect: vec4<f32>;

struct Vertex {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) pixel: vec2<f32>,
};

struct Varyings {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vertex(v: Vertex) -> Varyings {
    var o: Varyings;
    let ndc = (v.pixel - view.viewport.xy) / view.viewport.zw * 2.0 - 1.0;
    o.clip = vec4(ndc.x, -ndc.y, 0.5, 1.0);
    o.uv = v.uv;
    return o;
}

@fragment
fn fragment(in: Varyings) -> @location(0) vec4<f32> {
    let p = in.clip.xy;
    if p.x < clip_rect.x || p.y < clip_rect.y || clip_rect.z <= p.x || clip_rect.w <= p.y {
        discard;
    }
    return textureSample(image_texture, image_sampler, in.uv) * color;
}
