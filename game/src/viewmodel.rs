//! The first-person viewmodel: the fists or an heirloom (`lurch::anim::viewmodel`) play their
//! sequences, and the legend's arms follow the weapon's bones, both read from the R5Reloaded
//! install when the game starts, and drawn again from scratch when the dev menu picks another
//! heirloom. An heirloom's own meshes are drawn on its bones. Each frame blends the main sequence, the sequences it is leaving and the
//! overlay layers, in StandardBlendingRules' order. The arms are skinned with every weight a
//! vertex has: on the GPU (Bevy's skinning, four weights) for a mesh none of whose vertices has
//! more, else on the CPU. A second camera draws them over the world with the viewmodel's FOV,
//! and the HUD over them: drawn by the first camera, the HUD would be painted over.
//!
//! Through a mantle or a wall climb the arms are the first-person proxy instead
//! (`lurch::anim::proxy`): the arms model plays its own mantle or climb sequence, placed in
//! the world on the ledge or at the wall, and the fists are put away for it (lowered, then
//! not drawn) and raised after.
//!
//! The materials are shaded with the game's pixel shader's inputs (`viewmodel.wgsl`): albedo
//! times its tint, the normal map as the game reads it (DirectX's green, flipped for Bevy),
//! roughness from gloss, the specular colour times cavity as F0, ambient occlusion on the
//! ambient light, and the emissive times its tint and a second texture scrolled with game time
//! and distorted by a third (the kunai's glow). Tangents are generated at the bind pose and
//! skinned with the normals.
//!
//! Not modelled yet: sway, bob and the view of the camera bone, the oracle's own bone
//! concatenation, and the game's own lighting and exposure (Bevy's lights the inputs, and
//! `EMISSIVE_NITS` scales the game's emissive into them). That a mantle puts the fists away is what R5Reloaded and retail show; which call
//! disables them (`0x7d0dc0`) is not settled.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::camera::{ClearColorConfig, RenderTarget};
use bevy::light::cluster::ClusterConfig;
use bevy::light::NotShadowCaster;
use bevy::math::{Affine3A, Vec3A};
use bevy::mesh::skinning::{SkinnedMesh, SkinnedMeshInverseBindposes};
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;
use bevy::camera::visibility::RenderLayers as Layers;
use lurch::anim::bones::{self, BoneList, Mat, IDENTITY};
use lurch::anim::proxy::Proxy;
use lurch::anim::viewmodel::{Blends, Viewmodel};
use lurch::assets::texture::Texture;
use lurch::sim::cue::Cue;
use lurch::sim::grapple::FP_ANIM_GRAPPLE;
use lurch::sim::math::V3;

use crate::convert::{dir, pos, rot, vfov, INCH};
use crate::data::{Data, GameData};
use crate::sim::{FrameCues, Pose as EyePose, Sim};
use crate::view::{sky_light, MainCamera, ViewImage};
use crate::{common, Frame, Respawn};

pub const VIEWMODEL_LAYER: usize = 1;
/// The viewmodel's FOV when the legend's settings do not give one: horizontal at 4:3, widened
/// for the aspect as the world FOV is (view setup, 0x5d4740, reads the player setting
/// `viewmodelfov`).
pub const DEFAULT_VIEWMODEL_FOV: f32 = 70.0;

#[derive(Resource)]
struct Vm {
    state: Viewmodel,
    proxy: Proxy,
    arms: Skinned,
    weapon: Skinned,
    /// The materials with an emissive, whose glow scrolls with game time.
    glowing: Vec<Handle<ViewmodelMaterial>>,
    /// The grapple had the weapons down after the last command.
    grapple_down: bool,
}

/// Where the rope leaves the hand this frame while the grapple's animation plays: the arms'
/// `grapple_rope` attachment, in the world.
#[derive(Resource, Default)]
pub struct GrappleHand(pub Option<V3>);

/// The proxy finished the grapple's last sequence: the next commands end the player's
/// first-person animation state (PostThink, `0x86ef40`).
#[derive(Resource, Default)]
pub struct GrappleAnimOver(pub bool);

/// Material texture slots, the skinned pixel shader's resource order
/// (`lurch::assets::texture::material`).
const ALBEDO: usize = 0;
const NORMAL: usize = 1;
const GLOSS: usize = 2;
const SPEC: usize = 3;
const EMISSIVE: usize = 4;
const AO: usize = 5;
const CAVITY: usize = 6;
const EMISSIVE_MULTIPLY: usize = 10;
const UV_DISTORTION: usize = 11;

/// `CBufUberStatic` offsets, from the skinned pixel shader's reflection data.
const C_UV1_ROT_SCALE_X: usize = 0x00;
const C_UV1_ROT_SCALE_Y: usize = 0x08;
const C_UV1_TRANSLATE: usize = 0x10;
const C_UV2_ROT_SCALE_X: usize = 0x18;
const C_UV2_ROT_SCALE_Y: usize = 0x20;
const C_UV2_TRANSLATE: usize = 0x28;
const C_UV_DISTORTION_INTENSITY: usize = 0x78;
const C_L0_ALBEDO_TINT: usize = 0x164;
const C_L0_EMISSIVE_TINT: usize = 0x174;

/// The shell's own scale from the game's emissive units to Bevy's luminance: one unit is
/// about a white surface in the shell's sun. The game's exposure is not modelled.
const EMISSIVE_NITS: f32 = 3000.0;

pub type ViewmodelMaterial = ExtendedMaterial<StandardMaterial, GameShading>;

/// What the game's pixel shader adds to the base material (`viewmodel.wgsl`). A texture the
/// material lacks is left out of `present`, and the shader uses a neutral value.
#[derive(Asset, TypePath, AsBindGroup, Clone, Default)]
pub struct GameShading {
    #[uniform(100)]
    constants: ShadingConstants,
    #[texture(101)]
    #[sampler(102)]
    normal: Option<Handle<Image>>,
    #[texture(103)]
    #[sampler(104)]
    gloss: Option<Handle<Image>>,
    #[texture(105)]
    #[sampler(106)]
    spec: Option<Handle<Image>>,
    #[texture(107)]
    #[sampler(108)]
    cavity: Option<Handle<Image>>,
    #[texture(109)]
    #[sampler(110)]
    emissive: Option<Handle<Image>>,
    #[texture(111)]
    #[sampler(112)]
    multiply: Option<Handle<Image>>,
    #[texture(113)]
    #[sampler(114)]
    distortion: Option<Handle<Image>>,
}

#[derive(ShaderType, Clone, Default)]
struct ShadingConstants {
    uv1_x: Vec2,
    uv1_y: Vec2,
    uv1_translate: Vec2,
    uv2_x: Vec2,
    uv2_y: Vec2,
    uv2_translate: Vec2,
    distortion: Vec2,
    emissive_tint: Vec3,
    game_time: f32,
    emissive_scale: f32,
    present: u32,
}

impl MaterialExtension for GameShading {
    fn fragment_shader() -> ShaderRef {
        "embedded://lurch_game/viewmodel.wgsl".into()
    }
}

fn f32_at(b: &[u8], at: usize) -> f32 {
    b.get(at..at + 4).map_or(0.0, |x| f32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}

fn vec2_at(b: &[u8], at: usize) -> Vec2 {
    Vec2::new(f32_at(b, at), f32_at(b, at + 4))
}

fn vec3_at(b: &[u8], at: usize) -> Vec3 {
    Vec3::new(f32_at(b, at), f32_at(b, at + 4), f32_at(b, at + 8))
}

/// A texture as the pak holds it, its blocks handed to the GPU as they are.
pub fn gpu_image(t: &Texture) -> Option<Image> {
    use TextureFormat::*;
    let format = if t.format == 53 { &R8Unorm } else { [Bc1RgbaUnorm, Bc1RgbaUnormSrgb, Bc2RgbaUnorm, Bc2RgbaUnormSrgb, Bc3RgbaUnorm, Bc3RgbaUnormSrgb, Bc4RUnorm, Bc4RSnorm, Bc5RgUnorm, Bc5RgSnorm, Bc6hRgbUfloat, Bc6hRgbFloat, Bc7RgbaUnorm, Bc7RgbaUnormSrgb].get(t.format as usize)? };
    let size = Extent3d { width: t.width, height: t.height, depth_or_array_layers: 1 };
    let mut image = Image::new(size, TextureDimension::D2, t.mips.concat(), *format, RenderAssetUsages::RENDER_WORLD);
    image.texture_descriptor.mip_level_count = t.mips.len() as u32;
    let wrap = ImageAddressMode::Repeat;
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor { address_mode_u: wrap, address_mode_v: wrap, anisotropy_clamp: 8, ..ImageSamplerDescriptor::linear() });
    Some(image)
}

/// The viewmodel's animation, which the sound plays after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Animate;

/// This frame's sequence events on the viewmodel and the first-person proxy, as (event,
/// options), for the sound layer.
#[derive(Resource, Default)]
pub struct VmEvents(pub Vec<(u16, String)>);

#[derive(Component)]
pub struct ViewmodelCamera;

#[derive(Component)]
struct ViewmodelMesh {
    /// The weapon's own, put away while the proxy draws the arms.
    weapon: bool,
}

/// A model's bones as its GPU-skinned meshes' joints, and its meshes skinned on the CPU.
struct Skinned {
    joints: Vec<Entity>,
    cpu: Vec<CpuSkinned>,
}

pub struct ViewmodelPlugin;

impl Plugin for ViewmodelPlugin {
    fn build(&self, app: &mut App) {
        common(app);
        bevy::asset::embedded_asset!(app, "viewmodel.wgsl");
        app.add_plugins(MaterialPlugin::<ViewmodelMaterial>::default());
        app.init_resource::<VmEvents>().init_resource::<GrappleHand>().init_resource::<GrappleAnimOver>().add_systems(
            Update,
            (spawn_meshes.run_if(resource_exists_and_changed::<Data>), respawned.run_if(resource_exists::<Vm>), animate.run_if(resource_exists::<Vm>).in_set(Animate), sync_camera, game_time.run_if(resource_exists::<Vm>))
                .chain()
                .in_set(Frame::View),
        );
    }
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn spawn_meshes(
    mut commands: Commands,
    data: Res<Data>,
    sim: Res<Sim>,
    pose: Res<EyePose>,
    image: Option<Res<ViewImage>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ViewmodelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut bindposes: ResMut<Assets<SkinnedMeshInverseBindposes>>,
    main: Query<Entity, With<MainCamera>>,
    rui: Query<(), With<crate::rui::RuiCamera>>,
    cams: Query<(), With<ViewmodelCamera>>,
    meshes_before: Query<Entity, With<ViewmodelMesh>>,
    joints_before: Query<Entity, With<Joint>>,
) {
    for e in meshes_before.iter().chain(&joints_before) {
        commands.entity(e).despawn();
    }
    if cams.is_empty() {
        let host_ui = rui.is_empty();
        if host_ui {
            for e in &main {
                commands.entity(e).remove::<IsDefaultUiCamera>();
            }
        }
        let mut camera = commands.spawn((
            Camera3d::default(),
            Camera { order: 1, clear_color: ClearColorConfig::None, ..default() },
            Projection::Perspective(PerspectiveProjection { near: 0.01, far: 100.0, fov: vfov(data.0.legend.float("viewmodelfov").unwrap_or(DEFAULT_VIEWMODEL_FOV)), ..default() }),
            Transform::from_translation(pos(pose.0.origin)).with_rotation(rot(pose.0.angles)),
            RenderLayers::from_layers(&[VIEWMODEL_LAYER, crate::rui::UI_LAYER]),
            ViewmodelCamera,
            crate::blur::BlurSource,
            ClusterConfig::Single,
            sky_light(&mut images),
        ));
        if let Some(image) = image {
            camera.insert(RenderTarget::Image(image.0.clone().into()));
        }
        if host_ui {
            camera.insert(IsDefaultUiCamera);
        }
    }
    let mut glowing = Vec::new();
    let mut spawn = |src: &[lurch::assets::model::Mesh], textures: &[Vec<Option<Texture>>], constants: &[Option<Vec<u8>>], bones: usize, weapon: bool| {
        let joints: Vec<Entity> = (0..bones).map(|_| commands.spawn((Joint, GlobalTransform::IDENTITY)).id()).collect();
        let skin = SkinnedMesh { inverse_bindposes: bindposes.add(SkinnedMeshInverseBindposes::from(vec![Mat4::IDENTITY; joints.len()])), joints: joints.clone() };
        let mut cpu = Vec::new();
        for (index, (m, textures)) in src.iter().zip(textures).enumerate() {
            let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, m.positions.iter().map(|&p| (dir(p) * INCH).to_array()).collect::<Vec<_>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, m.normals.iter().map(|&n| dir(n).to_array()).collect::<Vec<_>>())
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, m.uvs.clone())
                .with_inserted_indices(Indices::U16(m.indices.chunks(3).flat_map(|t| [t[0], t[2], t[1]]).collect()));
            if let Err(e) = mesh.generate_tangents() {
                warn!("viewmodel mesh {}: no tangents: {e}", m.material);
            }
            let bind: Vec<[f32; 4]> = match mesh.attribute(Mesh::ATTRIBUTE_TANGENT) {
                Some(VertexAttributeValues::Float32x4(t)) => t.iter().map(|t| [-t[2], -t[0], t[1], t[3]]).collect(),
                _ => Vec::new(),
            };
            let mut texture = |slot: usize| textures.get(slot)?.as_ref().and_then(gpu_image).map(|i| images.add(i));
            let c = constants.get(index).cloned().flatten().unwrap_or_default();
            let mut shading = GameShading {
                normal: if bind.is_empty() { None } else { texture(NORMAL) },
                gloss: texture(GLOSS),
                spec: texture(SPEC),
                cavity: texture(CAVITY),
                emissive: texture(EMISSIVE),
                multiply: texture(EMISSIVE_MULTIPLY),
                distortion: texture(UV_DISTORTION),
                constants: ShadingConstants {
                    uv1_x: vec2_at(&c, C_UV1_ROT_SCALE_X),
                    uv1_y: vec2_at(&c, C_UV1_ROT_SCALE_Y),
                    uv1_translate: vec2_at(&c, C_UV1_TRANSLATE),
                    uv2_x: vec2_at(&c, C_UV2_ROT_SCALE_X),
                    uv2_y: vec2_at(&c, C_UV2_ROT_SCALE_Y),
                    uv2_translate: vec2_at(&c, C_UV2_TRANSLATE),
                    distortion: vec2_at(&c, C_UV_DISTORTION_INTENSITY),
                    emissive_tint: vec3_at(&c, C_L0_EMISSIVE_TINT),
                    emissive_scale: EMISSIVE_NITS,
                    ..default()
                },
            };
            let present = [&shading.normal, &shading.gloss, &shading.spec, &shading.cavity, &shading.emissive, &shading.multiply, &shading.distortion];
            shading.constants.present = present.iter().enumerate().fold(0, |m, (bit, t)| m | (t.is_some() as u32) << bit);
            let tint = if c.is_empty() { Vec3::ONE } else { vec3_at(&c, C_L0_ALBEDO_TINT) };
            let lit = shading.emissive.is_some() && shading.constants.emissive_tint != Vec3::ZERO;
            let material = materials.add(ViewmodelMaterial {
                base: StandardMaterial { base_color: Color::linear_rgb(tint.x, tint.y, tint.z), base_color_texture: texture(ALBEDO), occlusion_texture: texture(AO), ..default() },
                extension: shading,
            });
            if lit {
                glowing.push(material.clone());
            }
            let gpu = m.weights.iter().all(|w| w.len() <= 4);
            if gpu {
                let slot = |w: &Vec<(u16, f32)>, k: usize| w.get(k).copied().unwrap_or((0, 0.0));
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_INDEX, VertexAttributeValues::Uint16x4(m.weights.iter().map(|w| [0, 1, 2, 3].map(|k| slot(w, k).0)).collect()));
                // The stored weights sum to 1 only to their precision. A joint here also carries
                // the root's placement, which a sum short of 1 would pull toward the origin.
                let sum = |w: &Vec<(u16, f32)>| w.iter().map(|&(_, x)| x).sum::<f32>();
                mesh.insert_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, m.weights.iter().map(|w| [0, 1, 2, 3].map(|k| slot(w, k).1 / sum(w))).collect::<Vec<_>>());
            }
            let h = meshes.add(mesh);
            let mut e = commands.spawn((Mesh3d(h.clone()), MeshMaterial3d(material), RenderLayers::layer(VIEWMODEL_LAYER), NoFrustumCulling, NotShadowCaster, ViewmodelMesh { weapon }, Transform::default()));
            if gpu {
                e.insert(skin.clone());
            } else {
                cpu.push(CpuSkinned { index, mesh: h, tangents: bind });
            }
        }
        Skinned { joints, cpu }
    };
    let d = &data.0;
    let arms = spawn(&d.arms.meshes, &d.arms.textures, &d.arms.constants, d.arms.bones.len(), false);
    let weapon = spawn(&d.weapon_meshes, &d.weapon_textures, &d.weapon_constants, d.weapon_bones.len(), true);
    let (state, proxy) = drawn(d, &sim);
    info!("viewmodel: {} sequences, {} arms meshes, {} skinned on the CPU, {} weapon meshes", data.0.anims.seqs.len(), data.0.arms.meshes.len(), arms.cpu.len(), data.0.weapon_textures.len());
    commands.insert_resource(Vm { state, proxy, arms, weapon, glowing, grapple_down: false });
}

/// The weapon just drawn and the arms at rest, as a pilot spawns with them.
fn drawn(d: &GameData, sim: &Sim) -> (Viewmodel, Proxy) {
    let class = d.legend.str("classActivityModifier").filter(|m| !m.is_empty()).map(str::to_string);
    let mut state = Viewmodel::new(class.into_iter().collect(), d.times);
    state.idles = lurch::anim::viewmodel::idles(d.weapon_class);
    state.deploy(&d.anims, sim.pilot.player.sprinting, sim.view_now);
    let mut proxy = Proxy::default();
    proxy.poses = d.arms_anims.sk.default_poses();
    (state, proxy)
}

/// A respawn draws the weapon afresh, so a mantle or a grapple cut short leaves nothing behind
/// in the view.
fn respawned(mut respawns: MessageReader<Respawn>, mut vm: ResMut<Vm>, data: Res<Data>, sim: Res<Sim>) {
    if respawns.read().count() > 0 {
        (vm.state, vm.proxy) = drawn(&data.0, &sim);
        vm.grapple_down = false;
    }
}

fn mul_point(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2] + m[r][3])
}

fn mul_dir(m: &Mat, v: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

#[allow(clippy::too_many_arguments)] // Bevy's system parameters
fn animate(
    mut vm: ResMut<Vm>,
    data: Res<Data>,
    sim: Res<Sim>,
    cues: Res<FrameCues>,
    mut eye: ResMut<EyePose>,
    mut events: ResMut<VmEvents>,
    mut hand: ResMut<GrappleHand>,
    mut over: ResMut<GrappleAnimOver>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut roots: Query<(&mut Transform, &mut Layers, &mut Visibility, &ViewmodelMesh)>,
    mut joints: Query<&mut GlobalTransform, With<Joint>>,
) {
    let vm = &mut *vm;
    let d = &*data.0;
    let grapple_lowers = d.legend.bool("grapple_disableWeaponsWhenActive").unwrap_or(false);
    for step in &cues.0 {
        let now = step.sound.curtime;
        let was = vm.proxy.visible;
        for cue in &step.cues {
            if let Cue::GrappleAnim(g) = cue {
                vm.proxy.grapple(&d.arms_anims, *g, now);
            }
        }
        if (step.traversal.is_some() || step.climb.is_some()) && !was {
            vm.state.holster(&d.anims, now);
        }
        let down = grapple_lowers && step.grapple_down;
        if down && !vm.grapple_down {
            vm.state.holster(&d.anims, now);
        } else if !down && vm.grapple_down {
            vm.state.unholster(&d.anims, now);
        }
        vm.grapple_down = down;
        vm.state.command(&d.anims, &step.cues, step.sprinting, step.modifiers, now);
        let drawn = !vm.state.hidden(now);
        vm.proxy.command(&d.arms_anims, step.traversal, drawn, sim.settings.mantle.jumpoff_duration, now);
        vm.proxy.climb(&d.arms_anims, step.climb, now);
        if was && !vm.proxy.visible {
            vm.state.unholster(&d.anims, now);
        }
        over.0 |= vm.proxy.grapple_think(&d.arms_anims, now);
    }
    let (blends, fired) = vm.state.frame(&d.anims, sim.view_now);
    events.0.extend(fired);
    let (proxy_blends, fired) = vm.proxy.frame(&d.arms_anims, sim.view_now);
    events.0.extend(fired);
    let own = arms_pose(d, &proxy_blends, &vm.proxy.poses);
    let at = |a: Option<(usize, Mat)>| a.map_or(IDENTITY, |(b, local)| bones::concat(&own[b], &local));
    let reference = at(d.arms.reference);
    let pinned = vm.proxy.pinned(&reference, 1.0);
    let player = &sim.pilot.player;
    let view_angles = player.view_angles;
    let stand = &sim.settings.stances[0];
    let half_hull = (stand.hull_max[2] - stand.hull_min[2]) * 0.5;
    let a = eye.0.angles;
    let base_eye = eye.0.origin;
    vm.proxy.slide_frame(player.view_entity, player.view_entity_blend);
    let (origin, angles) = vm.proxy.climb_view(&at(d.arms.camera), &at(d.arms.camera_base), player.view_entity, player.view_entity_blend, eye.0.origin, view_angles);
    let (origin, angles) = vm.proxy.view(&d.arms_anims, &pinned, &at(d.arms.camera), &at(d.arms.camera_base), half_hull, origin, angles, sim.view_now);
    eye.0.origin = origin;
    eye.0.angles = [angles[0] + (a[0] - view_angles[0]), angles[1] + (a[1] - view_angles[1]), angles[2] + (a[2] - view_angles[2])];
    if vm.proxy.climbing() {
        let place = vm.proxy.climb_place(sim.pilot.origin, a, player.view_offset[2]);
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.arms, &skin, &Transform::IDENTITY, &mut meshes, &mut joints);
        for (mut t, mut layers, mut shown, m) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = if m.weapon { Visibility::Hidden } else { Visibility::Inherited };
        }
        return;
    }
    hand.0 = None;
    if vm.proxy.grappling() {
        let aims = player.first_person_anim == FP_ANIM_GRAPPLE && player.view_entity && (player.grappling || player.grapple_retracting);
        let (place, own) = if aims {
            let place = vm.proxy.grapple_place(&at(d.arms.camera_base), eye.0.origin, eye.0.angles);
            let frametime = cues.0.last().map_or(0.0, |s| s.frametime);
            vm.proxy.grapple_aim(&d.arms_anims, player.grapple_points[0], eye.0.origin, eye.0.angles, frametime);
            (place, arms_pose(d, &proxy_blends, &vm.proxy.poses))
        } else {
            (vm.proxy.eye_place(&at(d.arms.camera), base_eye, view_angles), own.clone())
        };
        let at = |a: Option<(usize, Mat)>| a.map_or(IDENTITY, |(b, local)| bones::concat(&own[b], &local));
        if d.arms.grapple_rope.is_some() {
            let r = bones::concat(&place, &at(d.arms.grapple_rope));
            hand.0 = Some([r[0][3], r[1][3], r[2][3]]);
        }
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.arms, &skin, &Transform::IDENTITY, &mut meshes, &mut joints);
        for (mut t, mut layers, mut shown, m) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = if m.weapon { Visibility::Hidden } else { Visibility::Inherited };
        }
        return;
    }
    if vm.proxy.hands_in(sim.view_now) {
        let forward = lurch::sim::math::angle_vectors(view_angles).0;
        let place = vm.proxy.place(&reference, 1.0, forward, sim.view_now);
        let skin: Vec<Mat> = d.arms.bones.iter().zip(&own).map(|(b, m)| bones::concat(&bones::concat(&place, m), &b.pose_to_bone)).collect();
        skin_meshes(&d.arms.meshes, &vm.arms, &skin, &Transform::IDENTITY, &mut meshes, &mut joints);
        for (mut t, mut layers, mut shown, m) in &mut roots {
            *t = Transform::IDENTITY;
            *layers = Layers::layer(0);
            *shown = if m.weapon { Visibility::Hidden } else { Visibility::Inherited };
        }
        return;
    }
    if vm.state.hidden(sim.view_now) {
        for (_, _, mut shown, _) in &mut roots {
            *shown = Visibility::Hidden;
        }
        return;
    }
    let sk = &d.anims.sk;
    let all = BoneList::all(sk.names.len());
    let poses = sk.default_poses();
    let mut pose = d.defaults.clone();
    if let Some((s, c)) = blends.main {
        bones::accumulate_pose(sk, &mut pose, s, c, 1.0, &poses, &all);
    }
    for (s, c, w) in blends.transitions.into_iter().chain(blends.layers) {
        bones::accumulate_pose(sk, &mut pose, s, c, w, &poses, &all);
    }
    let weapon = bones::build_transforms(sk, &pose, &IDENTITY);
    let arms = &d.arms;
    let mut am: Vec<Mat> = Vec::with_capacity(arms.bones.len());
    for (i, b) in arms.bones.iter().enumerate() {
        let m = match arms.merge[i] {
            Some(j) => weapon[j],
            None => {
                let local = bones::quat_matrix(b.quat, b.pos);
                if b.parent < 0 { local } else { bones::concat(&am[b.parent as usize], &local) }
            }
        };
        am.push(m);
    }
    let skin: Vec<Mat> = arms.bones.iter().zip(&am).map(|(b, m)| bones::concat(m, &b.pose_to_bone)).collect();
    let t = Transform::from_translation(pos(eye.0.origin)).with_rotation(rot(eye.0.angles));
    skin_meshes(&arms.meshes, &vm.arms, &skin, &t, &mut meshes, &mut joints);
    let skin: Vec<Mat> = d.weapon_bones.iter().zip(&weapon).map(|(b, m)| bones::concat(m, &b.pose_to_bone)).collect();
    skin_meshes(&d.weapon_meshes, &vm.weapon, &skin, &t, &mut meshes, &mut joints);
    for (mut r, mut layers, mut shown, _) in &mut roots {
        *r = t;
        *layers = Layers::layer(VIEWMODEL_LAYER);
        *shown = Visibility::Inherited;
    }
}

/// The arms' bones in the model's space as the proxy plays `blends` with pose parameters
/// `poses`.
fn arms_pose(d: &GameData, blends: &Blends, poses: &[f32]) -> Vec<Mat> {
    let sk = &d.arms_anims.sk;
    let all = BoneList::all(sk.names.len());
    let mut pose = d.arms.defaults.clone();
    if let Some((s, c)) = blends.main {
        bones::accumulate_pose(sk, &mut pose, s, c, 1.0, poses, &all);
    }
    for &(s, c, w) in &blends.transitions {
        bones::accumulate_pose(sk, &mut pose, s, c, w, poses, &all);
    }
    bones::build_transforms(sk, &pose, &IDENTITY)
}

/// A bone of the arms or the weapon as a joint of their GPU-skinned meshes, placed by
/// `skin_meshes` alone.
#[derive(Component)]
struct Joint;

/// A mesh with more than four weights on a vertex: its index in its model, its mesh and
/// its tangents at the bind pose, in the model's space, with their handedness.
struct CpuSkinned {
    index: usize,
    mesh: Handle<Mesh>,
    tangents: Vec<[f32; 4]>,
}

/// Skins a model's meshes: `skin` takes each bone from the bind pose to where it is drawn, in
/// the space `root` places. The joints get it in Bevy's space; a CPU-skinned mesh is skinned
/// with every weight a vertex has and placed by its entity's transform.
fn skin_meshes(src: &[lurch::assets::model::Mesh], s: &Skinned, skin: &[Mat], root: &Transform, meshes: &mut Assets<Mesh>, joints: &mut Query<&mut GlobalTransform, With<Joint>>) {
    let root = root.compute_affine();
    for (&e, m) in s.joints.iter().zip(skin) {
        if let Ok(mut g) = joints.get_mut(e) {
            *g = GlobalTransform::from(root * joint(m));
        }
    }
    for c in &s.cpu {
        let (mesh, bind) = (&src[c.index], &c.tangents);
        let mut positions = Vec::with_capacity(mesh.positions.len());
        let mut normals = Vec::with_capacity(mesh.positions.len());
        let mut tangents = Vec::with_capacity(bind.len());
        for (k, v) in mesh.positions.iter().enumerate() {
            let (mut p, mut n, mut t) = ([0.0f32; 3], [0.0f32; 3], [0.0f32; 3]);
            let bt = bind.get(k).map_or([0.0; 3], |t| [t[0], t[1], t[2]]);
            for &(b, w) in &mesh.weights[k] {
                let s = &skin[b as usize];
                let (pv, nv, tv) = (mul_point(s, *v), mul_dir(s, mesh.normals[k]), mul_dir(s, bt));
                for c in 0..3 {
                    p[c] += w * pv[c];
                    n[c] += w * nv[c];
                    t[c] += w * tv[c];
                }
            }
            positions.push((dir(p) * INCH).to_array());
            normals.push(dir(n).normalize_or_zero().to_array());
            if let Some(b) = bind.get(k) {
                tangents.push(dir(t).normalize_or_zero().extend(b[3]).to_array());
            }
        }
        if let Some(mut m) = meshes.get_mut(&c.mesh) {
            m.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
            m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
            if !bind.is_empty() {
                m.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tangents);
            }
        }
    }
}

/// A bone's skinning matrix in Bevy's space: with `A` the axes `dir` maps, `A·R·Aᵀ` and the
/// translation through `dir` in metres, for vertices already placed by `dir` at the bind pose.
fn joint(m: &Mat) -> Affine3A {
    let source = |b: [f32; 3]| [-b[2], -b[0], b[1]];
    let axis = |b: [f32; 3]| Vec3A::from(dir(mul_dir(m, source(b))));
    Affine3A::from_cols(axis([1.0, 0.0, 0.0]), axis([0.0, 1.0, 0.0]), axis([0.0, 0.0, 1.0]), Vec3A::from(dir([m[0][3], m[1][3], m[2][3]]) * INCH))
}

/// `c_gameTime` for the emissive's scroll: the view's clock.
fn game_time(vm: Res<Vm>, sim: Res<Sim>, mut materials: ResMut<Assets<ViewmodelMaterial>>) {
    for h in &vm.glowing {
        if let Some(mut m) = materials.get_mut(h) {
            m.extension.constants.game_time = sim.view_now;
        }
    }
}

fn sync_camera(eye: Res<EyePose>, mut cams: Query<&mut Transform, With<ViewmodelCamera>>) {
    for mut t in &mut cams {
        *t = Transform::from_translation(pos(eye.0.origin)).with_rotation(rot(eye.0.angles));
    }
}
