// The material inputs of the game's skinned-model pixel shader (`viewmodel.rs`), ported from
// its disassembly (docs/assets.md, Materials and textures): the normal map, gloss, specular
// colour, cavity and the scrolling, distorted emissive. Bevy's PBR lighting does the rest, but
// over another game's world, where the host's light shades them as it shades its own hand.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    forward_io::{VertexOutput, FragmentOutput},
}

const HAS_NORMAL: u32 = 1u;
const HAS_GLOSS: u32 = 2u;
const HAS_SPEC: u32 = 4u;
const HAS_CAVITY: u32 = 8u;
const HAS_EMISSIVE: u32 = 16u;
const HAS_MULTIPLY: u32 = 32u;
const HAS_DISTORTION: u32 = 64u;

struct GameShading {
    uv1_x: vec2<f32>,
    uv1_y: vec2<f32>,
    uv1_translate: vec2<f32>,
    uv2_x: vec2<f32>,
    uv2_y: vec2<f32>,
    uv2_translate: vec2<f32>,
    distortion: vec2<f32>,
    emissive_tint: vec3<f32>,
    game_time: f32,
    emissive_scale: f32,
    present: u32,
    host_light: vec4<f32>,
    host_dir0: vec3<f32>,
    host_dir1: vec3<f32>,
}

// Minecraft's entity shading (`light.glsl`): two fixed diffuse lights and an ambient term.
const HOST_LIGHT_POWER: f32 = 0.6;
const HOST_AMBIENT_LIGHT: f32 = 0.4;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3<f32>(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> shading: GameShading;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var normal_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var gloss_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var gloss_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(105) var spec_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(106) var spec_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(107) var cavity_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(108) var cavity_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(109) var emissive_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(110) var emissive_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(111) var multiply_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(112) var multiply_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(113) var distortion_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(114) var distortion_sampler: sampler;

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    let has = shading.present;
#ifdef VERTEX_UVS_A
    let uv = in.uv;
#ifdef VERTEX_TANGENTS
    if (has & HAS_NORMAL) != 0u {
        let t = textureSample(normal_texture, normal_sampler, uv).rg * 2.867661 - 1.439453;
        let n = pbr_input.world_normal;
        let tangent = normalize(in.world_tangent.xyz);
        let bitangent = in.world_tangent.w * cross(n, tangent);
        pbr_input.N = normalize(t.x * tangent - t.y * bitangent + n);
    }
#endif
    var gloss = 0.2;
    if (has & HAS_GLOSS) != 0u {
        gloss = textureSample(gloss_texture, gloss_sampler, uv).r;
    }
    var spec = vec3<f32>(0.04);
    if (has & HAS_SPEC) != 0u {
        spec = textureSample(spec_texture, spec_sampler, uv).rgb;
    }
    var cavity = 1.0;
    if (has & HAS_CAVITY) != 0u {
        cavity = textureSample(cavity_texture, cavity_sampler, uv).r;
    }
    pbr_input.material.metallic = 0.0;
    pbr_input.material.perceptual_roughness = max(1.0 - gloss, 0.04);
    pbr_input.material.reflectance = sqrt(spec * cavity / 0.16);
    var glow = vec3<f32>(0.0);
    if (has & HAS_EMISSIVE) != 0u {
        let time = shading.game_time;
        let uv2 = vec2<f32>(dot(uv, shading.uv2_x), dot(uv, shading.uv2_y)) + shading.uv2_translate * time;
        var distort = vec2<f32>(0.0);
        if (has & HAS_DISTORTION) != 0u {
            distort = (textureSample(distortion_texture, distortion_sampler, uv2).rg * 2.0 - 1.0) * shading.distortion;
        }
        let uv1 = vec2<f32>(dot(uv, shading.uv1_x), dot(uv, shading.uv1_y)) + shading.uv1_translate * time
            + distort * vec2<f32>(shading.uv1_x.x, shading.uv1_y.y);
        var multiply = vec3<f32>(1.0);
        if (has & HAS_MULTIPLY) != 0u {
            multiply = textureSample(multiply_texture, multiply_sampler, uv1).rgb;
        }
        glow = textureSample(emissive_texture, emissive_sampler, uv).rgb * shading.emissive_tint * multiply;
        pbr_input.material.emissive = vec4<f32>(glow * shading.emissive_scale, 1.0);
    }
#endif
    var out: FragmentOutput;
    if shading.host_light.w > 0.0 {
        // The host does its lighting on gamma-encoded colours, so it is done on them here.
        let n = pbr_input.N;
        let diffuse = max(dot(shading.host_dir0, n), 0.0) + max(dot(shading.host_dir1, n), 0.0);
        let lit = min(1.0, diffuse * HOST_LIGHT_POWER + HOST_AMBIENT_LIGHT);
        let albedo = to_srgb(pbr_input.material.base_color.rgb);
        out.color = vec4<f32>(to_linear(albedo * lit * shading.host_light.rgb) + glow, pbr_input.material.base_color.a);
        return out;
    }
    out.color = apply_pbr_lighting(pbr_input);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
